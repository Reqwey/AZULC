//! Modern Forge installation, with launcher-owned dependency and processor steps.

use super::{
    DownloadContext, InstallError,
    forge_processors::{self, PlannedProcessor},
    forge_profile::{self, invalid},
    minecraft, progress,
};
use crate::{
    domain::{InstallStage, PipelineEvent},
    services::{
        download::{file_ops, integrity},
        java,
    },
};
use reqwest::Client;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
use tokio::sync::mpsc::UnboundedSender;

struct WorkDirectory(PathBuf);

impl Drop for WorkDirectory {
    fn drop(&mut self) {
        // This is a unique directory created by this attempt, never a supplied path.
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub(super) async fn install(
    client: &Client,
    root: &Path,
    base: &minecraft::VersionJson,
    installer: &Path,
    tx: UnboundedSender<PipelineEvent>,
    downloads: DownloadContext<'_>,
) -> Result<String, InstallError> {
    let root = std::path::absolute(root)?;
    let installer = std::path::absolute(installer)?;
    progress(
        &tx,
        InstallStage::InstallingLoader,
        "Reading Forge processor and dependency manifests",
    );
    let prepare_root = root.clone();
    let minecraft_version = base.id.clone();
    let (prepared, _work) = tokio::task::spawn_blocking(move || {
        let work = WorkDirectory(
            prepare_root
                .join("installers")
                .join(format!(".forge-{}", uuid::Uuid::new_v4())),
        );
        std::fs::create_dir_all(&work.0)?;
        let profile =
            forge_profile::prepare(&installer, &prepare_root, &minecraft_version, &work.0)?;
        Ok::<_, InstallError>((profile, work))
    })
    .await
    .map_err(|error| invalid(error.to_string()))??;

    let mut planned = HashMap::<PathBuf, minecraft::DownloadItem>::new();
    for library in prepared
        .profile
        .libraries
        .iter()
        .chain(&prepared.version.libraries)
    {
        if !minecraft::rules_allow(&library.rules) {
            continue;
        }
        if let Some(item) = super::loader_library_download(&root, library, downloads.router)? {
            planned.entry(item.path.clone()).or_insert(item);
        }
    }
    let mut processors = Vec::new();
    for processor in prepared.profile.processors.iter().filter(|p| p.is_client()) {
        let mut processor_plan = PlannedProcessor::new(processor, &prepared.variables, &root)?;
        // Like SJMCL, route Mojang mapping downloads through our own downloader.
        // Keep the step in the count; its validated output marks it as completed.
        if processor
            .jar
            .starts_with("net.minecraftforge:installertools:")
            && processor_plan
                .args
                .windows(2)
                .any(|pair| pair == ["--task", "DOWNLOAD_MOJMAPS"])
        {
            let item = mappings_download(base, &root, &processor_plan.args)?;
            let hash = item
                .sha1
                .clone()
                .ok_or_else(|| invalid("client mappings has no SHA-1"))?;
            processor_plan.outputs.push((item.path.clone(), hash));
            processor_plan.mappings_prepared = true;
            planned.insert(item.path.clone(), item);
        }
        processors.push(processor_plan);
    }
    minecraft::download_batch_with_policy(
        client.clone(),
        planned.into_values().collect(),
        InstallStage::DownloadingLoader,
        "Forge runtime and processor dependencies",
        tx.clone(),
        downloads.policy,
    )
    .await?;

    if !processors.is_empty() {
        progress(
            &tx,
            InstallStage::InstallingLoader,
            "Selecting Java for Forge processors",
        );
        let required = java::required_major(
            &base.id,
            base.java_version.as_ref().map(|v| v.major_version),
        );
        let runtime = java::select(&java::detect().await, required)
            .ok_or(InstallError::MissingJava(required))?;
        forge_processors::execute(&runtime.path, &root, &processors, &tx).await?;
    }

    progress(
        &tx,
        InstallStage::InstallingLoader,
        "Verifying Forge runtime libraries and writing the version profile",
    );
    for library in &prepared.version.libraries {
        if !minecraft::rules_allow(&library.rules) {
            continue;
        }
        let artifact = library.downloads.as_ref().and_then(|d| d.artifact.as_ref());
        let path = if let Some(artifact) = artifact.filter(|a| !a.path.is_empty()) {
            root.join("libraries")
                .join(super::checked_library_path(&artifact.path)?)
        } else {
            forge_profile::library_path(&root, &library.name)?
        };
        if !path.is_file() {
            return Err(invalid(format!(
                "missing Forge runtime library {}",
                path.display()
            )));
        }
        if let Some(hash) = artifact
            .map(|a| a.sha1.as_str())
            .filter(|hash| !hash.is_empty())
            && !forge_processors::outputs_valid(&[(path.clone(), hash.to_owned())]).await?
        {
            return Err(invalid(format!(
                "Forge runtime library checksum mismatch: {}",
                path.display()
            )));
        }
    }
    let id = prepared.version.id;
    file_ops::write_atomic(
        &root.join("versions").join(&id).join(format!("{id}.json")),
        &prepared.version_json,
    )
    .await?;
    Ok(id)
}

fn mappings_download(
    base: &minecraft::VersionJson,
    root: &Path,
    args: &[String],
) -> Result<minecraft::DownloadItem, InstallError> {
    let option = |key: &str| {
        args.windows(2)
            .find(|pair| pair[0] == key)
            .map(|pair| pair[1].as_str())
    };
    if option("--side") != Some("client") || option("--version") != Some(base.id.as_str()) {
        return Err(invalid(
            "DOWNLOAD_MOJMAPS requested a different side or Minecraft version",
        ));
    }
    let path = forge_profile::output_path(
        root,
        option("--output").ok_or_else(|| invalid("DOWNLOAD_MOJMAPS has no output"))?,
    )?;
    let artifact = base
        .downloads
        .get("client_mappings")
        .ok_or_else(|| invalid("Minecraft metadata has no client mappings"))?;
    let sha1 = integrity::normalized_hex::<20>(&artifact.sha1)
        .ok_or_else(|| invalid("client mappings has invalid SHA-1"))?;
    Ok(minecraft::DownloadItem {
        url: artifact.url.clone(),
        path,
        sha1: Some(sha1),
        size: artifact.size,
        label: "Minecraft client mappings".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mappings_are_taken_from_minecraft_metadata_and_output_is_confined() {
        let root = std::env::temp_dir().join("forge-mappings");
        let base: minecraft::VersionJson = serde_json::from_value(serde_json::json!({
            "id":"1.20.1", "downloads":{"client_mappings":{
                "url":"https://example.invalid/mappings", "size":3,
                "sha1":integrity::sha1_hex(b"abc")
            }}
        }))
        .unwrap();
        let mut args = vec![
            "--task".into(),
            "DOWNLOAD_MOJMAPS".into(),
            "--version".into(),
            "1.20.1".into(),
            "--side".into(),
            "client".into(),
            "--output".into(),
            root.join("libraries/mappings.txt")
                .to_string_lossy()
                .into_owned(),
        ];
        let download = mappings_download(&base, &root, &args).unwrap();
        assert_eq!(download.sha1, Some(integrity::sha1_hex(b"abc")));
        args[5] = "server".into();
        assert!(mappings_download(&base, &root, &args).is_err());
    }

    /// Set AZULC_FORGE_SMOKE_ROOT and AZULC_FORGE_SMOKE_INSTALLER to isolated
    /// absolute paths; optional AZULC_FORGE_SMOKE_VERSION defaults to 1.20.1.
    #[tokio::test]
    #[ignore = "downloads real Minecraft/Forge dependencies and requires a compatible Java"]
    async fn real_modern_forge_install() {
        use crate::{domain::DownloadPolicy, services::download::source::SourceRouter};
        let root =
            PathBuf::from(std::env::var_os("AZULC_FORGE_SMOKE_ROOT").expect("isolated smoke root"));
        let installer =
            PathBuf::from(std::env::var_os("AZULC_FORGE_SMOKE_INSTALLER").expect("installer JAR"));
        assert!(root.is_absolute() && installer.is_absolute());
        let version =
            std::env::var("AZULC_FORGE_SMOKE_VERSION").unwrap_or_else(|_| "1.20.1".into());
        let policy = DownloadPolicy::default();
        let router = SourceRouter::from_policy(&policy);
        let client = Client::new();
        let (base, raw) = minecraft::fetch_version_with_router(&client, &version, router)
            .await
            .unwrap();
        let directory = root.join("versions").join(&base.id);
        file_ops::write_atomic(&directory.join(format!("{}.json", base.id)), &raw)
            .await
            .unwrap();
        let artifact = &base.downloads["client"];
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let logger = tokio::spawn(async move {
            let mut last = String::new();
            while let Some(event) = rx.recv().await {
                match event {
                    PipelineEvent::Progress(p) if p.detail != last => {
                        eprintln!("{}: {}", p.stage.label(), p.detail);
                        last = p.detail;
                    }
                    PipelineEvent::Log(line) => eprintln!("{line}"),
                    _ => {}
                }
            }
        });
        minecraft::download_batch_with_policy(
            client.clone(),
            vec![minecraft::DownloadItem {
                url: artifact.url.clone(),
                path: directory.join(format!("{}.jar", base.id)),
                sha1: Some(artifact.sha1.clone()),
                size: artifact.size,
                label: "Minecraft client".into(),
            }],
            InstallStage::DownloadingMinecraft,
            "Smoke client",
            tx.clone(),
            &policy,
        )
        .await
        .unwrap();
        let result = install(
            &client,
            &root,
            &base,
            &installer,
            tx.clone(),
            DownloadContext {
                router,
                policy: &policy,
            },
        )
        .await;
        drop(tx);
        logger.await.unwrap();
        let id = result.unwrap();
        assert!(
            root.join("versions")
                .join(&id)
                .join(format!("{id}.json"))
                .is_file()
        );
    }
}
