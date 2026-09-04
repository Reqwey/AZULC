use crate::{
    domain::{DownloadPolicy, InstallProgress, InstallStage, PipelineEvent},
    services::{
        download::{self, DownloadSpec, file_ops, integrity, source::SourceRouter},
        path_safety,
    },
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};
use tokio::sync::mpsc::UnboundedSender;

pub const VERSION_MANIFEST: &str =
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

#[derive(Debug, thiserror::Error)]
pub enum MinecraftError {
    #[error("network request failed: {0}")]
    Network(#[from] reqwest::Error),
    #[error("file operation failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid version metadata: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Minecraft {0} was not found")]
    VersionNotFound(String),
    #[error("file checksum failed: {0}")]
    Checksum(String),
    #[error("invalid SHA-1 digest in {0}")]
    InvalidSha1(String),
    #[error("unsafe library path in metadata: {0:?}")]
    UnsafeLibraryPath(String),
    #[error("unsafe Minecraft version id in metadata: {0:?}")]
    UnsafeVersionId(String),
    #[error("version metadata id {actual:?} does not match requested version {requested:?}")]
    VersionIdMismatch { requested: String, actual: String },
    #[error("unsafe asset index id in metadata: {0:?}")]
    UnsafeAssetIndexId(String),
    #[error(transparent)]
    Download(#[from] download::DownloadError),
    #[error("version metadata is missing a required field: {0}")]
    Missing(&'static str),
}

#[derive(Debug, Clone, Deserialize)]
pub struct VersionManifest {
    pub latest: LatestVersions,
    pub versions: Vec<VersionEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LatestVersions {
    #[serde(default)]
    pub release: String,
    #[serde(default)]
    pub snapshot: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VersionEntry {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(rename = "releaseTime", default)]
    pub release_time: String,
    pub url: String,
    #[serde(default)]
    pub sha1: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct VersionJson {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub inherits_from: Option<String>,
    #[serde(default)]
    pub main_class: Option<String>,
    #[serde(default)]
    pub assets: Option<String>,
    #[serde(default)]
    pub asset_index: Option<AssetIndexRef>,
    #[serde(default)]
    pub downloads: HashMap<String, DownloadRef>,
    #[serde(default)]
    pub libraries: Vec<Library>,
    #[serde(default)]
    pub arguments: Option<Arguments>,
    #[serde(default)]
    pub minecraft_arguments: Option<String>,
    #[serde(default)]
    pub java_version: Option<JavaVersion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaVersion {
    pub major_version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Arguments {
    #[serde(default)]
    pub game: Vec<serde_json::Value>,
    #[serde(default)]
    pub jvm: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetIndexRef {
    pub id: String,
    pub url: String,
    #[serde(default)]
    pub sha1: String,
    #[serde(default)]
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DownloadRef {
    #[serde(default)]
    pub path: String,
    pub url: String,
    #[serde(default)]
    pub sha1: String,
    #[serde(default)]
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LibraryDownloads {
    pub artifact: Option<DownloadRef>,
    #[serde(default)]
    pub classifiers: HashMap<String, DownloadRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Library {
    pub name: String,
    #[serde(default)]
    pub downloads: Option<LibraryDownloads>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub natives: HashMap<String, String>,
    #[serde(default)]
    pub extract: Option<ExtractRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtractRule {
    #[serde(default)]
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub action: String,
    pub os: Option<OsRule>,
    #[serde(default)]
    pub features: HashMap<String, bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsRule {
    pub name: Option<String>,
    pub arch: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AssetIndex {
    objects: HashMap<String, AssetObject>,
}

#[derive(Debug, Deserialize)]
struct AssetObject {
    hash: String,
    size: u64,
}

#[derive(Debug, Clone)]
pub struct DownloadItem {
    pub url: String,
    pub path: PathBuf,
    pub sha1: Option<String>,
    pub size: u64,
    pub label: String,
}

pub async fn fetch_manifest_with_router(
    client: &Client,
    router: SourceRouter,
) -> Result<VersionManifest, MinecraftError> {
    Ok(client
        .get(router.rewrite(VERSION_MANIFEST))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?)
}

pub async fn fetch_version_with_router(
    client: &Client,
    version: &str,
    router: SourceRouter,
) -> Result<(VersionJson, Vec<u8>), MinecraftError> {
    let manifest = fetch_manifest_with_router(client, router).await?;
    let entry = manifest
        .versions
        .into_iter()
        .find(|v| v.id == version)
        .ok_or_else(|| MinecraftError::VersionNotFound(version.into()))?;
    let bytes = client
        .get(router.rewrite(&entry.url))
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?
        .to_vec();
    if !entry.sha1.is_empty() && !integrity::hex_matches::<20>(&entry.sha1, &digest(&bytes)) {
        return Err(MinecraftError::Checksum(format!("{version}.json")));
    }
    let json: VersionJson = serde_json::from_slice(&bytes)?;
    ensure_version_id(version, &json.id)?;
    Ok((json, bytes))
}

fn ensure_version_id(requested: &str, actual: &str) -> Result<(), MinecraftError> {
    if requested == actual {
        Ok(())
    } else {
        Err(MinecraftError::VersionIdMismatch {
            requested: requested.to_owned(),
            actual: actual.to_owned(),
        })
    }
}

pub async fn plan_vanilla_with_router(
    client: &Client,
    root: &Path,
    version: &VersionJson,
    raw_json: &[u8],
    router: SourceRouter,
) -> Result<Vec<DownloadItem>, MinecraftError> {
    let version_id = path_safety::exact_component(&version.id)
        .ok_or_else(|| MinecraftError::UnsafeVersionId(version.id.clone()))?;
    let version_dir = root.join("versions").join(version_id);
    tokio::fs::create_dir_all(&version_dir).await?;
    file_ops::write_atomic(&version_dir.join(format!("{version_id}.json")), raw_json).await?;

    let mut items = Vec::new();
    let client_jar = version
        .downloads
        .get("client")
        .ok_or(MinecraftError::Missing("downloads.client"))?;
    items.push(DownloadItem {
        url: router.rewrite(&client_jar.url),
        path: version_dir.join(format!("{version_id}.jar")),
        sha1: nonempty(&client_jar.sha1),
        size: client_jar.size,
        label: format!("Minecraft {version_id} client"),
    });

    for library in &version.libraries {
        if !rules_allow(&library.rules) {
            continue;
        }
        if !is_legacy_native_container(library) {
            if let Some(artifact) = library.downloads.as_ref().and_then(|d| d.artifact.as_ref()) {
                items.push(download_for_library(root, artifact, &library.name, router)?);
            } else if let Some(path) = maven_path(&library.name) {
                let base_url = library
                    .url
                    .as_deref()
                    .unwrap_or("https://libraries.minecraft.net/");
                items.push(DownloadItem {
                    url: router.rewrite(&format!("{}/{}", base_url.trim_end_matches('/'), path)),
                    path: root.join("libraries").join(&path),
                    sha1: None,
                    size: 0,
                    label: library.name.clone(),
                });
            }
        }
        if let Some(classifier) = native_classifier(library)
            && let Some(native) = library
                .downloads
                .as_ref()
                .and_then(|d| d.classifiers.get(&classifier))
        {
            items.push(download_for_library(
                root,
                native,
                &format!("{} native", library.name),
                router,
            )?);
        }
    }

    if let Some(index_ref) = &version.asset_index {
        let index_id = path_safety::exact_component(&index_ref.id)
            .ok_or_else(|| MinecraftError::UnsafeAssetIndexId(index_ref.id.clone()))?;
        let bytes = client
            .get(router.rewrite(&index_ref.url))
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?
            .to_vec();
        if !index_ref.sha1.is_empty()
            && !integrity::hex_matches::<20>(&index_ref.sha1, &digest(&bytes))
        {
            return Err(MinecraftError::Checksum(format!(
                "asset index {}",
                index_ref.id
            )));
        }
        let index_path = root.join("assets/indexes").join(format!("{index_id}.json"));
        file_ops::write_atomic(&index_path, &bytes).await?;
        let index: AssetIndex = serde_json::from_slice(&bytes)?;
        for (name, object) in index.objects {
            let hash = normalized_asset_hash(&name, &object.hash)?;
            let prefix = &hash[..2];
            items.push(DownloadItem {
                url: router.rewrite(&format!(
                    "https://resources.download.minecraft.net/{prefix}/{}",
                    hash
                )),
                path: root.join("assets/objects").join(prefix).join(&hash),
                sha1: Some(hash),
                size: object.size,
                label: name,
            });
        }
    }
    Ok(deduplicate_download_items(items))
}

fn normalized_asset_hash(name: &str, value: &str) -> Result<String, MinecraftError> {
    integrity::normalized_hex::<20>(value)
        .ok_or_else(|| MinecraftError::InvalidSha1(format!("asset object {name}")))
}

fn deduplicate_download_items(items: Vec<DownloadItem>) -> Vec<DownloadItem> {
    let mut destinations = HashSet::with_capacity(items.len());
    items
        .into_iter()
        .filter(|item| destinations.insert(item.path.clone()))
        .collect()
}

fn download_for_library(
    root: &Path,
    download: &DownloadRef,
    label: &str,
    router: SourceRouter,
) -> Result<DownloadItem, MinecraftError> {
    let path = if download.path.is_empty() {
        return Err(MinecraftError::Missing("library.downloads.artifact.path"));
    } else {
        path_safety::relative_path(&download.path)
            .ok_or_else(|| MinecraftError::UnsafeLibraryPath(download.path.clone()))?
    };
    Ok(DownloadItem {
        url: router.rewrite(&download.url),
        path: root.join("libraries").join(path),
        sha1: nonempty(&download.sha1),
        size: download.size,
        label: label.into(),
    })
}

pub async fn download_batch_with_policy(
    client: Client,
    items: Vec<DownloadItem>,
    stage: InstallStage,
    detail: &str,
    tx: UnboundedSender<PipelineEvent>,
    policy: &DownloadPolicy,
) -> Result<(), MinecraftError> {
    let router = SourceRouter::from_policy(policy);
    let total = if items.iter().all(|item| item.size > 0) {
        items.iter().map(|item| item.size).sum()
    } else {
        0
    };
    let files_total = items.len();
    let specs = items
        .into_iter()
        .map(|item| DownloadSpec {
            urls: vec![router.rewrite(&item.url)],
            destination: item.path,
            size: item.size,
            sha1: item.sha1,
            sha512: None,
            label: item.label,
        })
        .collect();

    let progress_tx = tx.clone();
    let progress_detail = detail.to_owned();
    download::download_batch(client, specs, policy.concurrency, move |snapshot| {
        let _ = progress_tx.send(PipelineEvent::Progress(InstallProgress {
            stage,
            current: snapshot.current,
            total: snapshot.total,
            detail: progress_detail.clone(),
            files_done: snapshot.files_done,
            files_total: snapshot.files_total,
            bytes_per_second: snapshot.bytes_per_second,
        }));
    })
    .await?;

    let _ = tx.send(PipelineEvent::Progress(InstallProgress {
        stage,
        current: if total == 0 { 1 } else { total },
        total: if total == 0 { 1 } else { total },
        detail: format!("{detail} complete"),
        files_done: files_total,
        files_total,
        bytes_per_second: 0.0,
    }));
    Ok(())
}

pub fn rules_allow(rules: &[Rule]) -> bool {
    if rules.is_empty() {
        return true;
    }
    let mut allowed = false;
    for rule in rules {
        let matches = rule.os.as_ref().is_none_or(os_matches) && rule.features.is_empty();
        if matches {
            allowed = rule.action == "allow";
        }
    }
    allowed
}

fn os_matches(rule: &OsRule) -> bool {
    let current = if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "osx"
    } else {
        "linux"
    };
    rule.name.as_deref().is_none_or(|name| name == current)
        && rule.arch.as_deref().is_none_or(|arch| match arch {
            "x86" => cfg!(target_arch = "x86"),
            "x86_64" | "amd64" => cfg!(target_arch = "x86_64"),
            "arm64" | "aarch64" => cfg!(target_arch = "aarch64"),
            _ => true,
        })
}

pub fn native_classifier(library: &Library) -> Option<String> {
    let key = if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "osx"
    } else {
        "linux"
    };
    library.natives.get(key).map(|value| {
        value.replace(
            "${arch}",
            if cfg!(target_pointer_width = "64") {
                "64"
            } else {
                "32"
            },
        )
    })
}

pub fn is_legacy_native_container(library: &Library) -> bool {
    !library.natives.is_empty()
}

pub fn maven_path(name: &str) -> Option<String> {
    let (coordinate, ext) = name.split_once('@').unwrap_or((name, "jar"));
    let parts: Vec<_> = coordinate.split(':').collect();
    let (group, artifact, version) = (*parts.first()?, *parts.get(1)?, *parts.get(2)?);
    let classifier = parts.get(3).copied();
    let classifier = classifier.map(|v| format!("-{v}")).unwrap_or_default();
    let path = format!(
        "{}/{}/{}/{}-{}{}.{}",
        group.replace('.', "/"),
        artifact,
        version,
        artifact,
        version,
        classifier,
        ext
    );
    path_safety::relative_path(&path)?;
    Some(path)
}

fn nonempty(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_string())
}
fn digest(bytes: &[u8]) -> String {
    integrity::sha1_hex(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_maven_coordinates() {
        assert_eq!(
            maven_path("net.fabricmc:fabric-loader:0.16.10"),
            Some("net/fabricmc/fabric-loader/0.16.10/fabric-loader-0.16.10.jar".into())
        );
        assert_eq!(
            maven_path("org.lwjgl:lwjgl:3.3.3:natives-windows"),
            Some("org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-windows.jar".into())
        );
        assert_eq!(
            maven_path("com.example:tool:1.2.3:all@zip"),
            Some("com/example/tool/1.2.3/tool-1.2.3-all.zip".into())
        );
        assert_eq!(maven_path("../outside:tool:1.0"), None);
    }

    #[test]
    fn version_metadata_id_must_match_the_manifest_request() {
        assert!(ensure_version_id("1.21.1", "1.21.1").is_ok());
        assert!(matches!(
            ensure_version_id("1.21.1", "../other"),
            Err(MinecraftError::VersionIdMismatch { .. })
        ));
    }

    #[tokio::test]
    async fn planning_rejects_unsafe_version_and_asset_index_ids_before_network_access() {
        let client = Client::new();
        let root = std::env::temp_dir().join(format!("azulc-plan-{}", uuid::Uuid::new_v4()));
        let unsafe_version = VersionJson {
            id: "../outside".into(),
            ..Default::default()
        };
        assert!(matches!(
            plan_vanilla_with_router(
                &client,
                &root,
                &unsafe_version,
                b"{}",
                SourceRouter::new(crate::domain::DownloadSource::Official)
            )
            .await,
            Err(MinecraftError::UnsafeVersionId(_))
        ));

        let mut downloads = HashMap::new();
        downloads.insert(
            "client".into(),
            DownloadRef {
                url: "https://example.invalid/client.jar".into(),
                ..Default::default()
            },
        );
        let unsafe_index = VersionJson {
            id: "1.21.1".into(),
            downloads,
            asset_index: Some(AssetIndexRef {
                id: "../outside".into(),
                url: "https://example.invalid/index.json".into(),
                sha1: String::new(),
                size: 0,
            }),
            ..Default::default()
        };
        assert!(matches!(
            plan_vanilla_with_router(
                &client,
                &root,
                &unsafe_index,
                b"{}",
                SourceRouter::new(crate::domain::DownloadSource::Official)
            )
            .await,
            Err(MinecraftError::UnsafeAssetIndexId(_))
        ));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn applies_os_rules_in_order() {
        let current = if cfg!(windows) {
            "windows"
        } else if cfg!(target_os = "macos") {
            "osx"
        } else {
            "linux"
        };
        assert!(rules_allow(&[Rule {
            action: "allow".into(),
            os: Some(OsRule {
                name: Some(current.into()),
                arch: None,
                version: None
            }),
            features: HashMap::new(),
        }]));
    }

    #[test]
    fn recognizes_legacy_lwjgl_native_container() {
        let library = Library {
            name: "org.lwjgl.lwjgl:lwjgl-platform:2.9.1".into(),
            natives: HashMap::from([
                ("windows".into(), "natives-windows".into()),
                ("linux".into(), "natives-linux".into()),
            ]),
            ..Library::default()
        };
        assert!(is_legacy_native_container(&library));
        let expected = if cfg!(windows) {
            Some("natives-windows")
        } else if cfg!(target_os = "macos") {
            None
        } else {
            Some("natives-linux")
        };
        assert_eq!(native_classifier(&library).as_deref(), expected);
    }

    #[test]
    fn legacy_metadata_duplicate_destinations_are_downloaded_once() {
        let item = |label: &str, url: &str| DownloadItem {
            url: url.into(),
            path: PathBuf::from("libraries/jinput-platform-natives-windows.jar"),
            sha1: Some("385ee093e01f587f30ee1c8a2ee7d408fd732e16".into()),
            size: 155_179,
            label: label.into(),
        };
        let items = deduplicate_download_items(vec![
            item("first declaration", "https://example.test/first.jar"),
            item("duplicate declaration", "https://example.test/second.jar"),
        ]);

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].label, "first declaration");
    }

    #[test]
    fn malformed_asset_hash_returns_an_error_instead_of_panicking() {
        assert!(matches!(
            normalized_asset_hash("minecraft/sounds/example.ogg", "f"),
            Err(MinecraftError::InvalidSha1(_))
        ));
    }
}
