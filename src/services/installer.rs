use crate::{
    domain::{
        DownloadPolicy, InstallProgress, InstallRequest, InstallStage, Instance, InstanceOrigin,
        LoaderKind, ModpackInstallSpec, ModpackSource, PipelineEvent,
    },
    services::{
        download::{self, DownloadSnapshot, DownloadSpec, file_ops, source::SourceRouter},
        java, loader_catalog,
        minecraft::{self, DownloadItem, VersionJson},
        modpack::{self, ModpackFile, ModpackFormat, ModpackPlan},
        path_safety,
        providers::{
            curseforge::{self, ResourceClass},
            modrinth::{self, ContentType as ModrinthContentType, ModrinthClient},
        },
    },
    storage::Paths,
};
use futures::{StreamExt, stream};
use reqwest::Client;
use serde::Deserialize;
use std::{
    collections::HashMap,
    io::Read,
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
    sync::mpsc::UnboundedSender,
};

#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    #[error(transparent)]
    Minecraft(#[from] minecraft::MinecraftError),
    #[error("network request failed: {0}")]
    Network(#[from] reqwest::Error),
    #[error("file operation failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid metadata: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Java {0} was not found; install a compatible runtime first")]
    MissingJava(u32),
    #[error("could not resolve a loader build for {0}")]
    LoaderVersion(String),
    #[error("loader installer failed with exit code {0}")]
    InstallerExit(i32),
    #[error("the loader installer did not create a version profile")]
    MissingInstalledProfile,
    #[error("modpack import failed: {0}")]
    Modpack(String),
    #[error(transparent)]
    CurseForge(#[from] curseforge::CurseForgeError),
    #[error(transparent)]
    Modrinth(#[from] modrinth::ModrinthError),
    #[error(transparent)]
    Download(#[from] download::DownloadError),
    #[error("CurseForge project {0} is not a modpack")]
    NotModpack(u64),
    #[error("CurseForge file {file_id} has no SHA-1 hash")]
    MissingCurseForgeHash { file_id: u64 },
    #[error("unsafe or empty CurseForge file name: {0:?}")]
    UnsafeFileName(String),
    #[error("a modpack cannot contain another modpack as a dependency (project {0})")]
    NestedModpack(u64),
    #[error("Modrinth project {0} is not a modpack")]
    NotModrinthModpack(String),
    #[error("Modrinth version {0} does not provide a primary .mrpack archive")]
    NotMrpack(String),
}

pub async fn run(request: InstallRequest, paths: Paths, tx: UnboundedSender<PipelineEvent>) {
    let result = run_inner(request, paths, tx.clone()).await;
    if let Err((stage, error)) = result {
        let _ = tx.send(PipelineEvent::Failed {
            stage,
            message: error.to_string(),
        });
    }
}

async fn run_inner(
    mut request: InstallRequest,
    paths: Paths,
    tx: UnboundedSender<PipelineEvent>,
) -> Result<(), (InstallStage, InstallError)> {
    let client = Client::builder()
        .user_agent("AZULC/0.1.0")
        .build()
        .map_err(|e| (InstallStage::ResolvingMinecraft, InstallError::Network(e)))?;
    let router = SourceRouter::from_policy(&request.download_policy);
    let concurrency = request
        .download_policy
        .concurrency
        .clamp(1, crate::domain::cpu_thread_count());
    paths
        .prepare()
        .map_err(|e| (InstallStage::Queued, InstallError::Io(e)))?;

    let prepared_modpack = match request.modpack.clone() {
        Some(spec) => {
            let prepared = prepare_modpack(&spec, &paths, concurrency, tx.clone()).await?;
            request.minecraft_version = prepared.plan.metadata.minecraft_version.clone();
            request.loader = prepared.plan.metadata.loader.clone();
            let _ = tx.send(PipelineEvent::ResolvedMetadata {
                minecraft_version: request.minecraft_version.clone(),
                loader: request.loader.clone(),
            });
            let loader = request
                .loader
                .version
                .as_deref()
                .map(|version| format!("{} {version}", request.loader.kind))
                .unwrap_or_else(|| request.loader.kind.to_string());
            let _ = tx.send(PipelineEvent::Log(format!(
                "Modpack manifest selected Minecraft {} with {loader}",
                request.minecraft_version
            )));
            Some(prepared)
        }
        None => None,
    };

    progress(
        &tx,
        InstallStage::ResolvingMinecraft,
        "Connecting to the Minecraft version service",
    );
    let (base, raw) =
        minecraft::fetch_version_with_router(&client, &request.minecraft_version, router)
            .await
            .map_err(|e| (InstallStage::ResolvingMinecraft, e.into()))?;
    safe_profile_id(&base.id).map_err(|error| (InstallStage::PlanningMinecraft, error))?;
    let _ = tx.send(PipelineEvent::Log(format!(
        "Retrieved Minecraft {} metadata",
        base.id
    )));

    progress(
        &tx,
        InstallStage::PlanningMinecraft,
        "Planning the client, libraries, natives, and assets",
    );
    let downloads =
        minecraft::plan_vanilla_with_router(&client, &paths.minecraft, &base, &raw, router)
            .await
            .map_err(|e| (InstallStage::PlanningMinecraft, e.into()))?;
    let count = downloads.len();
    let _ = tx.send(PipelineEvent::Log(format!(
        "Download plan contains {count} files with {concurrency} workers"
    )));

    minecraft::download_batch_with_policy(
        client.clone(),
        downloads,
        InstallStage::DownloadingMinecraft,
        "Minecraft files",
        tx.clone(),
        &request.download_policy,
    )
    .await
    .map_err(|e| (InstallStage::DownloadingMinecraft, e.into()))?;
    progress(
        &tx,
        InstallStage::VerifyingMinecraft,
        "Every Minecraft base file is present and verified",
    );

    let (version_id, resolved_loader_version) = match request.loader.kind {
        LoaderKind::Vanilla => (base.id.clone(), None),
        LoaderKind::Fabric => install_fabric(
            &client,
            &paths.minecraft,
            &base,
            request.loader.version.as_deref(),
            tx.clone(),
            router,
            &request.download_policy,
        )
        .await
        .map_err(|e| (loader_stage(&e), e))?,
        LoaderKind::Forge | LoaderKind::NeoForge => install_forge_family(
            &client,
            &paths.minecraft,
            &base,
            request.loader.kind,
            request.loader.version.as_deref(),
            tx.clone(),
            DownloadContext {
                router,
                policy: &request.download_policy,
            },
        )
        .await
        .map_err(|e| (loader_stage(&e), e))?,
    };

    let game_dir = paths.instance_dir(request.instance_id);
    tokio::fs::create_dir_all(game_dir.join("mods"))
        .await
        .map_err(|e| (InstallStage::Finalizing, InstallError::Io(e)))?;

    if let Some(prepared) = prepared_modpack.as_ref() {
        install_modpack_content(
            &prepared.plan,
            &game_dir,
            client.clone(),
            concurrency,
            tx.clone(),
        )
        .await?;
        progress(
            &tx,
            InstallStage::ApplyingModpackOverrides,
            "Applying the modpack override files",
        );
        modpack::apply_overrides(
            prepared.archive.clone(),
            game_dir.clone(),
            prepared.plan.overrides_prefix.clone(),
        )
        .await
        .map_err(|error| {
            (
                InstallStage::ApplyingModpackOverrides,
                InstallError::Modpack(error),
            )
        })?;
        let _ = tx.send(PipelineEvent::Log(
            "Modpack override files were applied".into(),
        ));
    }

    progress(
        &tx,
        InstallStage::Finalizing,
        "Creating the isolated game directory and instance record",
    );
    materialize_instance_version_files(&paths.minecraft, &game_dir, &base.id, &version_id)
        .await
        .map_err(|error| (InstallStage::Finalizing, InstallError::Io(error)))?;
    let _ = tx.send(PipelineEvent::Log(format!(
        "Linked Minecraft {} client and version metadata into the instance directory",
        base.id
    )));
    let instance = Instance {
        id: request.instance_id,
        name: request.name,
        minecraft_version: request.minecraft_version,
        version_id,
        loader: crate::domain::LoaderSpec {
            kind: request.loader.kind,
            version: resolved_loader_version,
        },
        game_dir,
        installed: true,
        description: request.description,
        color: request.color,
        favorite: false,
        play_time_seconds: 0,
        last_played_unix: None,
        settings: request.settings,
        origin: prepared_modpack
            .map(|prepared| prepared.origin)
            .unwrap_or(InstanceOrigin::Custom),
    };
    let _ = tx.send(PipelineEvent::Progress(InstallProgress {
        stage: InstallStage::Complete,
        current: 1,
        total: 1,
        detail: "The instance is ready to launch".into(),
        files_done: 1,
        files_total: 1,
        bytes_per_second: 0.0,
    }));
    let _ = tx.send(PipelineEvent::Finished(Box::new(instance)));
    Ok(())
}

pub(crate) async fn materialize_instance_version_files(
    minecraft_root: &Path,
    instance_dir: &Path,
    minecraft_version: &str,
    profile_id: &str,
) -> Result<(), std::io::Error> {
    let minecraft_version = portable_component(minecraft_version)?;
    let profile_id = portable_component(profile_id)?;
    let shared_base = minecraft_root.join("versions").join(&minecraft_version);
    link_or_copy(
        &shared_base.join(format!("{minecraft_version}.jar")),
        &instance_dir.join(format!("{minecraft_version}.jar")),
    )
    .await?;
    link_or_copy(
        &shared_base.join(format!("{minecraft_version}.json")),
        &instance_dir.join(format!("{minecraft_version}.json")),
    )
    .await?;

    if profile_id != minecraft_version {
        let profile = minecraft_root
            .join("versions")
            .join(&profile_id)
            .join(format!("{profile_id}.json"));
        link_or_copy(&profile, &instance_dir.join(format!("{profile_id}.json"))).await?;
    }
    Ok(())
}

fn portable_component(value: &str) -> Result<String, std::io::Error> {
    path_safety::exact_component(value)
        .map(str::to_owned)
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("unsafe file or profile name: {value:?}"),
            )
        })
}

async fn link_or_copy(source: &Path, destination: &Path) -> Result<(), std::io::Error> {
    let staging = file_ops::staging_path(destination)?;

    if tokio::fs::hard_link(source, &staging).await.is_err()
        && let Err(error) = tokio::fs::copy(source, &staging).await
    {
        let _ = tokio::fs::remove_file(&staging).await;
        return Err(error);
    }
    if let Err(error) = file_ops::replace_file(&staging, destination).await {
        let _ = tokio::fs::remove_file(&staging).await;
        return Err(error);
    }
    Ok(())
}

struct PreparedModpack {
    archive: PathBuf,
    plan: ModpackPlan,
    origin: InstanceOrigin,
}

async fn prepare_modpack(
    spec: &ModpackInstallSpec,
    paths: &Paths,
    concurrency: usize,
    tx: UnboundedSender<PipelineEvent>,
) -> Result<PreparedModpack, (InstallStage, InstallError)> {
    let (archive, remote_origin) = match &spec.source {
        ModpackSource::Local { archive } => {
            progress(
                &tx,
                InstallStage::ResolvingModpack,
                "Opening the local modpack archive",
            );
            let _ = tx.send(PipelineEvent::Log(format!(
                "Using local modpack archive {}",
                archive.display()
            )));
            (archive.clone(), None)
        }
        ModpackSource::CurseForge {
            project_id,
            file_id,
            file_name,
        } => {
            progress(
                &tx,
                InstallStage::ResolvingModpack,
                "Resolving the CurseForge modpack archive",
            );
            let api = curseforge::CurseForgeClient::from_env()
                .map_err(|error| (InstallStage::ResolvingModpack, error.into()))?;
            let project = api
                .get_project(*project_id)
                .await
                .map_err(|error| (InstallStage::ResolvingModpack, error.into()))?;
            if project.resource_class() != Some(ResourceClass::Modpack) {
                return Err((
                    InstallStage::ResolvingModpack,
                    InstallError::NotModpack(*project_id),
                ));
            }
            let file = api
                .get_file(*project_id, *file_id)
                .await
                .map_err(|error| (InstallStage::ResolvingModpack, error.into()))?;
            // CurseForge can suppress downloadUrl for otherwise valid pack
            // files. Match SJMCL's modpack-only ForgeCDN fallback here.
            let url = api
                .resolve_modpack_file_url(&project, &file)
                .map_err(|error| (InstallStage::ResolvingModpack, error.into()))?;
            let sha1 = file
                .sha1()
                .ok_or((
                    InstallStage::ResolvingModpack,
                    InstallError::MissingCurseForgeHash { file_id: *file_id },
                ))?
                .to_owned();
            let archive_name = if file.file_name.trim().is_empty() {
                safe_file_name(file_name)
            } else {
                safe_file_name(&file.file_name)
            }
            .map_err(|error| (InstallStage::ResolvingModpack, error))?;
            let destination = paths
                .data
                .join("downloads")
                .join("modpacks")
                .join(format!("{project_id}-{file_id}-{archive_name}"));
            let label = if file.display_name.trim().is_empty() {
                archive_name
            } else {
                file.display_name.clone()
            };
            let progress_tx = tx.clone();
            let progress_label = label.clone();
            download::download_batch(
                api.download_client(),
                vec![DownloadSpec {
                    urls: vec![url.to_string()],
                    destination: destination.clone(),
                    size: file.file_length,
                    sha1: Some(sha1),
                    sha512: None,
                    label,
                }],
                concurrency,
                move |snapshot| {
                    send_download_progress(
                        &progress_tx,
                        InstallStage::DownloadingModpack,
                        &format!("Downloading {progress_label}"),
                        snapshot,
                    );
                },
            )
            .await
            .map_err(|error| (InstallStage::DownloadingModpack, error.into()))?;
            let _ = tx.send(PipelineEvent::Log(format!(
                "Downloaded CurseForge modpack archive: {}",
                destination.display()
            )));

            let project_name = nonempty(&project.name)
                .or_else(|| nonempty(&spec.project_name))
                .unwrap_or("CurseForge modpack")
                .to_owned();
            let version_name = spec
                .version_name
                .as_deref()
                .and_then(nonempty)
                .or_else(|| nonempty(&file.display_name))
                .map(str::to_owned);
            (
                destination,
                Some(InstanceOrigin::Modpack {
                    provider: "CurseForge".into(),
                    project_id: Some(project.id.to_string()),
                    project_name,
                    version_id: Some(file.id.to_string()),
                    version_name,
                }),
            )
        }
        ModpackSource::Modrinth {
            project_id,
            version_id,
            file_name: _,
        } => {
            progress(
                &tx,
                InstallStage::ResolvingModpack,
                "Resolving the Modrinth modpack archive",
            );
            let api = ModrinthClient::new()
                .map_err(|error| (InstallStage::ResolvingModpack, error.into()))?;
            let resolved = api
                .get_install_plan(project_id, version_id)
                .await
                .map_err(|error| (InstallStage::ResolvingModpack, error.into()))?;
            if resolved.project_type != ModrinthContentType::Modpack {
                return Err((
                    InstallStage::ResolvingModpack,
                    InstallError::NotModrinthModpack(project_id.clone()),
                ));
            }
            if !resolved.install.is_mrpack() {
                return Err((
                    InstallStage::ResolvingModpack,
                    InstallError::NotMrpack(version_id.clone()),
                ));
            }

            let archive_name = safe_file_name(&resolved.install.file_name)
                .map_err(|error| (InstallStage::ResolvingModpack, error))?;
            let project_id = safe_profile_id(&resolved.project.id)
                .map_err(|error| (InstallStage::ResolvingModpack, error))?;
            let version_id = safe_profile_id(&resolved.version.id)
                .map_err(|error| (InstallStage::ResolvingModpack, error))?;
            let cache_directory = safe_file_name(&format!("{project_id}-{version_id}"))
                .map_err(|error| (InstallStage::ResolvingModpack, error))?;
            let destination = paths
                .data
                .join("downloads")
                .join("modpacks")
                .join(cache_directory)
                .join(&archive_name);
            let progress_tx = tx.clone();
            let progress_label = resolved.version.name.clone();
            download::download_batch(
                api.download_client(),
                vec![DownloadSpec {
                    urls: vec![resolved.install.url.clone()],
                    destination: destination.clone(),
                    size: resolved.install.size,
                    sha1: resolved.install.sha1.clone(),
                    sha512: resolved.install.sha512.clone(),
                    label: archive_name,
                }],
                concurrency,
                move |snapshot| {
                    send_download_progress(
                        &progress_tx,
                        InstallStage::DownloadingModpack,
                        &format!("Downloading {progress_label}"),
                        snapshot,
                    );
                },
            )
            .await
            .map_err(|error| (InstallStage::DownloadingModpack, error.into()))?;
            let _ = tx.send(PipelineEvent::Log(format!(
                "Downloaded Modrinth modpack archive: {}",
                destination.display()
            )));

            let project_name = nonempty(&resolved.project.title)
                .or_else(|| nonempty(&spec.project_name))
                .unwrap_or("Modrinth modpack")
                .to_owned();
            let version_name = spec
                .version_name
                .as_deref()
                .and_then(nonempty)
                .or_else(|| nonempty(&resolved.version.name))
                .map(str::to_owned);
            (
                destination,
                Some(InstanceOrigin::Modpack {
                    provider: "Modrinth".into(),
                    project_id: Some(project_id),
                    project_name,
                    version_id: Some(version_id),
                    version_name,
                }),
            )
        }
    };

    progress(
        &tx,
        InstallStage::InspectingModpack,
        "Reading and validating the modpack manifest",
    );
    let plan = modpack::inspect_archive(archive.clone())
        .await
        .map_err(|error| {
            (
                InstallStage::InspectingModpack,
                InstallError::Modpack(error),
            )
        })?;
    let _ = tx.send(PipelineEvent::Log(format!(
        "Validated {} modpack manifest with {} required content entries",
        modpack_format_label(plan.format),
        plan.files
            .iter()
            .filter(|file| !matches!(
                file,
                ModpackFile::CurseForge {
                    required: false,
                    ..
                }
            ))
            .count()
    )));

    let origin = remote_origin.unwrap_or_else(|| InstanceOrigin::Modpack {
        provider: modpack_format_label(plan.format).into(),
        project_id: None,
        project_name: nonempty(&spec.project_name)
            .unwrap_or(&plan.metadata.name)
            .to_owned(),
        version_id: None,
        version_name: spec
            .version_name
            .as_deref()
            .and_then(nonempty)
            .map(str::to_owned)
            .or_else(|| plan.metadata.version.clone()),
    });
    Ok(PreparedModpack {
        archive,
        plan,
        origin,
    })
}

async fn install_modpack_content(
    plan: &ModpackPlan,
    game_dir: &Path,
    download_client: Client,
    concurrency: usize,
    tx: UnboundedSender<PipelineEvent>,
) -> Result<(), (InstallStage, InstallError)> {
    progress(
        &tx,
        InstallStage::DownloadingModpackContent,
        "Resolving modpack content files",
    );
    let mut direct_specs = Vec::with_capacity(plan.files.len());
    let mut curseforge_specs = Vec::new();
    let mut curseforge_client = None;
    let mut curseforge_files = Vec::new();
    let mut optional_files = 0_usize;
    for file in &plan.files {
        match file {
            ModpackFile::CurseForge {
                project_id,
                file_id,
                required: true,
            } => curseforge_files.push((*project_id, *file_id)),
            ModpackFile::CurseForge {
                required: false, ..
            } => optional_files += 1,
            ModpackFile::Direct {
                path,
                urls,
                sha1,
                sha512,
                size,
            } => direct_specs.push(DownloadSpec {
                urls: urls.clone(),
                destination: game_dir.join(path),
                size: *size,
                sha1: Some(sha1.clone()),
                sha512: Some(sha512.clone()),
                label: path.display().to_string(),
            }),
        }
    }
    if optional_files > 0 {
        let _ = tx.send(PipelineEvent::Log(format!(
            "Skipped {optional_files} optional CurseForge content files"
        )));
    }

    if !curseforge_files.is_empty() {
        let api = Arc::new(curseforge::CurseForgeClient::from_env().map_err(|error| {
            (
                InstallStage::DownloadingModpackContent,
                InstallError::from(error),
            )
        })?);
        curseforge_client = Some(api.download_client());
        let root = game_dir.to_path_buf();
        let resolved = stream::iter(curseforge_files.into_iter().map(|(project_id, file_id)| {
            let api = Arc::clone(&api);
            let root = root.clone();
            async move { resolve_curseforge_content(api, root, project_id, file_id).await }
        }))
        .buffer_unordered(concurrency.clamp(1, crate::domain::cpu_thread_count()))
        .collect::<Vec<_>>()
        .await;
        for result in resolved {
            curseforge_specs
                .push(result.map_err(|error| (InstallStage::DownloadingModpackContent, error))?);
        }
    }

    let count = direct_specs.len() + curseforge_specs.len();
    let _ = tx.send(PipelineEvent::Log(format!(
        "Downloading {count} required modpack content files with {} workers",
        concurrency.clamp(1, crate::domain::cpu_thread_count())
    )));
    if !direct_specs.is_empty() {
        let progress_tx = tx.clone();
        download::download_batch(
            download_client,
            direct_specs,
            concurrency,
            move |snapshot| {
                send_download_progress(
                    &progress_tx,
                    InstallStage::DownloadingModpackContent,
                    "Downloading direct modpack content",
                    snapshot,
                );
            },
        )
        .await
        .map_err(|error| (InstallStage::DownloadingModpackContent, error.into()))?;
    }
    if !curseforge_specs.is_empty() {
        let progress_tx = tx.clone();
        let client = curseforge_client.expect("CurseForge specs always have a scoped CDN client");
        download::download_batch(client, curseforge_specs, concurrency, move |snapshot| {
            send_download_progress(
                &progress_tx,
                InstallStage::DownloadingModpackContent,
                "Downloading CurseForge modpack content",
                snapshot,
            );
        })
        .await
        .map_err(|error| (InstallStage::DownloadingModpackContent, error.into()))?;
    }
    Ok(())
}

async fn resolve_curseforge_content(
    api: Arc<curseforge::CurseForgeClient>,
    game_dir: PathBuf,
    project_id: u64,
    file_id: u64,
) -> Result<DownloadSpec, InstallError> {
    let project = api.get_project(project_id).await?;
    let file = api.get_file(project_id, file_id).await?;
    // Pack manifests use SJMCL's ForgeCDN fallback when CurseForge suppresses
    // downloadUrl; ordinary interactive resource installs remain strict.
    let url = api.resolve_modpack_file_url(&project, &file)?;
    let sha1 = file
        .sha1()
        .ok_or(InstallError::MissingCurseForgeHash { file_id })?
        .to_owned();
    let file_name = safe_file_name(&file.file_name)?;
    let directory = curseforge_content_directory(project.resource_class(), project_id)?;
    Ok(DownloadSpec {
        urls: vec![url.to_string()],
        destination: game_dir.join(directory).join(&file_name),
        size: file.file_length,
        sha1: Some(sha1),
        sha512: None,
        label: file_name,
    })
}

fn send_download_progress(
    tx: &UnboundedSender<PipelineEvent>,
    stage: InstallStage,
    detail: &str,
    snapshot: DownloadSnapshot,
) {
    let _ = tx.send(PipelineEvent::Progress(InstallProgress {
        stage,
        current: snapshot.current,
        total: snapshot.total,
        detail: detail.into(),
        files_done: snapshot.files_done,
        files_total: snapshot.files_total,
        bytes_per_second: snapshot.bytes_per_second,
    }));
}

fn safe_file_name(value: &str) -> Result<String, InstallError> {
    path_safety::file_name(value).ok_or_else(|| InstallError::UnsafeFileName(value.into()))
}

fn safe_profile_id(value: &str) -> Result<String, InstallError> {
    path_safety::exact_component(value)
        .map(str::to_owned)
        .ok_or_else(|| InstallError::UnsafeFileName(value.into()))
}

fn nonempty(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

fn curseforge_content_directory(
    class: Option<ResourceClass>,
    project_id: u64,
) -> Result<&'static str, InstallError> {
    match class {
        Some(ResourceClass::ResourcePack) => Ok("resourcepacks"),
        Some(ResourceClass::ShaderPack) => Ok("shaderpacks"),
        Some(ResourceClass::Modpack) => Err(InstallError::NestedModpack(project_id)),
        // CurseForge manifests traditionally omit the destination path. Match
        // the established launcher behavior and treat every other class as a
        // mod instead of guessing a world-specific datapack destination.
        _ => Ok("mods"),
    }
}

fn modpack_format_label(format: ModpackFormat) -> &'static str {
    match format {
        ModpackFormat::CurseForge => "CurseForge",
        ModpackFormat::Modrinth => "Modrinth",
        ModpackFormat::MultiMc => "MultiMC/Prism",
    }
}

fn loader_stage(error: &InstallError) -> InstallStage {
    match error {
        InstallError::Minecraft(_) => InstallStage::DownloadingLoader,
        InstallError::InstallerExit(_) => InstallStage::RunningProcessors,
        InstallError::MissingJava(_)
        | InstallError::MissingInstalledProfile
        | InstallError::Io(_) => InstallStage::InstallingLoader,
        InstallError::Network(_) | InstallError::Json(_) | InstallError::LoaderVersion(_) => {
            InstallStage::ResolvingLoader
        }
        InstallError::Modpack(_)
        | InstallError::CurseForge(_)
        | InstallError::Modrinth(_)
        | InstallError::Download(_)
        | InstallError::NotModpack(_)
        | InstallError::MissingCurseForgeHash { .. }
        | InstallError::UnsafeFileName(_)
        | InstallError::NestedModpack(_)
        | InstallError::NotModrinthModpack(_)
        | InstallError::NotMrpack(_) => InstallStage::ResolvingLoader,
    }
}

async fn install_fabric(
    client: &Client,
    root: &Path,
    base: &VersionJson,
    requested: Option<&str>,
    tx: UnboundedSender<PipelineEvent>,
    router: SourceRouter,
    policy: &DownloadPolicy,
) -> Result<(String, Option<String>), InstallError> {
    progress(
        &tx,
        InstallStage::ResolvingLoader,
        "Resolving a compatible Fabric Loader build",
    );
    let loader_version = match requested.filter(|v| !v.trim().is_empty()) {
        Some(version) => version.to_string(),
        None => {
            let url = format!("https://meta.fabricmc.net/v2/versions/loader/{}", base.id);
            let versions: Vec<FabricLoaderEntry> = client
                .get(router.rewrite(&url))
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            versions
                .iter()
                .find(|v| v.loader.stable)
                .or_else(|| versions.first())
                .map(|v| v.loader.version.clone())
                .ok_or_else(|| InstallError::LoaderVersion("Fabric".into()))?
        }
    };
    let url = format!(
        "https://meta.fabricmc.net/v2/versions/loader/{}/{}/profile/json",
        base.id, loader_version
    );
    let bytes = client
        .get(router.rewrite(&url))
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?
        .to_vec();
    let profile: VersionJson = serde_json::from_slice(&bytes)?;
    let version_id = safe_profile_id(&profile.id)?;
    let version_dir = root.join("versions").join(&version_id);
    tokio::fs::create_dir_all(&version_dir).await?;
    file_ops::write_atomic(&version_dir.join(format!("{version_id}.json")), &bytes).await?;

    let mut downloads = Vec::new();
    for library in &profile.libraries {
        let path = minecraft::maven_path(&library.name)
            .ok_or_else(|| InstallError::LoaderVersion(library.name.clone()))?;
        let base_url = library
            .url
            .as_deref()
            .unwrap_or("https://maven.fabricmc.net/");
        downloads.push(DownloadItem {
            url: router.rewrite(&format!("{}/{}", base_url.trim_end_matches('/'), path)),
            path: root.join("libraries").join(&path),
            sha1: None,
            size: 0,
            label: library.name.clone(),
        });
    }
    minecraft::download_batch_with_policy(
        client.clone(),
        downloads,
        InstallStage::DownloadingLoader,
        "Fabric libraries",
        tx.clone(),
        policy,
    )
    .await?;
    progress(
        &tx,
        InstallStage::InstallingLoader,
        "Fabric version profile written",
    );
    Ok((version_id, Some(loader_version)))
}

#[derive(Debug, Deserialize)]
struct FabricLoaderEntry {
    loader: FabricLoader,
}
#[derive(Debug, Deserialize)]
struct FabricLoader {
    version: String,
    stable: bool,
}

#[derive(Clone, Copy)]
struct DownloadContext<'a> {
    router: SourceRouter,
    policy: &'a DownloadPolicy,
}

async fn install_forge_family(
    client: &Client,
    root: &Path,
    base: &VersionJson,
    kind: LoaderKind,
    requested: Option<&str>,
    tx: UnboundedSender<PipelineEvent>,
    downloads: DownloadContext<'_>,
) -> Result<(String, Option<String>), InstallError> {
    let name = kind.label();
    progress(
        &tx,
        InstallStage::ResolvingLoader,
        &format!("Resolving a compatible {name} build"),
    );
    let version = match requested.filter(|v| !v.trim().is_empty()) {
        requested if kind == LoaderKind::Forge => {
            resolve_forge(client, &base.id, requested, downloads.router).await?
        }
        Some(v) => normalize_forge_version(kind, &base.id, v),
        None => resolve_neoforge(client, &base.id, downloads.router).await?,
    };
    let (url, filename) = forge_installer_url(kind, &base.id, &version);
    safe_file_name(&filename)?;
    let installer = root.join("installers").join(&filename);
    minecraft::download_batch_with_policy(
        client.clone(),
        vec![DownloadItem {
            url: downloads.router.rewrite(&url),
            path: installer.clone(),
            sha1: None,
            size: 0,
            label: filename,
        }],
        InstallStage::DownloadingLoader,
        &format!("{name} installer"),
        tx.clone(),
        downloads.policy,
    )
    .await?;

    let expected_profile_id = match kind {
        LoaderKind::NeoForge => Some(
            prepare_neoforge_dependencies(
                client,
                root,
                &installer,
                tx.clone(),
                downloads.router,
                downloads.policy,
            )
            .await?,
        ),
        LoaderKind::Forge => read_modern_installer_profile_id(&installer)?,
        _ => None,
    }
    .map(|profile_id| safe_profile_id(&profile_id))
    .transpose()?;
    if let Some(profile_id) = expected_profile_id.as_deref() {
        let _ = tx.send(PipelineEvent::Log(format!(
            "[pipeline] {name} installer will create version profile {profile_id}"
        )));
    }

    if kind == LoaderKind::Forge
        && let Some(profile_id) = install_legacy_forge(
            client,
            root,
            &installer,
            tx.clone(),
            downloads.router,
            downloads.policy,
        )
        .await?
    {
        return Ok((profile_id, Some(version)));
    }

    progress(
        &tx,
        InstallStage::InstallingLoader,
        &format!("Selecting Java for the {name} installer"),
    );
    let runtimes = java::detect().await;
    let required = java::required_major(
        &base.id,
        base.java_version.as_ref().map(|v| v.major_version),
    );
    let runtime = java::select(&runtimes, required).ok_or(InstallError::MissingJava(required))?;
    ensure_launcher_profile(root).await?;
    let _ = tx.send(PipelineEvent::Log(format!(
        "Running {} -jar {} --installClient",
        runtime.path.display(),
        installer.display()
    )));
    progress(
        &tx,
        InstallStage::RunningProcessors,
        &format!("{name} is downloading dependencies and running processors"),
    );

    let mut command = Command::new(&runtime.path);
    command
        .arg("-jar")
        .arg(&installer)
        .arg("--installClient")
        .arg(root)
        .current_dir(root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(java::CREATE_NO_WINDOW);
    let mut child = command.spawn()?;
    let stdout = child.stdout.take().map(BufReader::new);
    let stderr = child.stderr.take().map(BufReader::new);
    let out_task = tokio::spawn(pipe_lines(stdout, tx.clone(), "installer"));
    let err_task = tokio::spawn(pipe_stderr(stderr, tx.clone(), "installer!"));
    let status = child.wait().await?;
    let _ = tokio::join!(out_task, err_task);
    if !status.success() {
        return Err(InstallError::InstallerExit(status.code().unwrap_or(-1)));
    }
    let profile_id =
        find_installed_profile(root, &base.id, &version, expected_profile_id.as_deref())
            .await?
            .ok_or(InstallError::MissingInstalledProfile)?;
    Ok((profile_id, Some(version)))
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ModernForgeInstallProfile {
    #[serde(default)]
    libraries: Vec<minecraft::Library>,
}

async fn prepare_neoforge_dependencies(
    client: &Client,
    root: &Path,
    installer: &Path,
    tx: UnboundedSender<PipelineEvent>,
    router: SourceRouter,
    policy: &DownloadPolicy,
) -> Result<String, InstallError> {
    progress(
        &tx,
        InstallStage::DownloadingLoader,
        "Reading NeoForge dependency manifests",
    );
    let installer = installer.to_path_buf();
    let libraries_root = root.join("libraries");
    let (libraries, profile_id) = tokio::task::spawn_blocking(move || {
        extract_embedded_maven_and_read_libraries(&installer, &libraries_root)
    })
    .await
    .map_err(|error| std::io::Error::other(error.to_string()))??;

    let mut planned = HashMap::<PathBuf, DownloadItem>::new();
    for library in libraries {
        if let Some(item) = neoforge_library_download(root, &library, router)? {
            planned.entry(item.path.clone()).or_insert(item);
        }
    }
    let _ = tx.send(PipelineEvent::Log(format!(
        "[pipeline] Prepared {} NeoForge libraries before starting the Java installer",
        planned.len()
    )));
    minecraft::download_batch_with_policy(
        client.clone(),
        planned.into_values().collect(),
        InstallStage::DownloadingLoader,
        "NeoForge installer libraries",
        tx.clone(),
        policy,
    )
    .await?;
    Ok(profile_id)
}

fn read_modern_installer_profile_id(installer: &Path) -> Result<Option<String>, std::io::Error> {
    let file = std::fs::File::open(installer)?;
    let mut archive = zip::ZipArchive::new(file).map_err(zip_io_error)?;
    let version = match archive.by_name("version.json") {
        Ok(mut entry) => {
            let mut json = String::new();
            entry.read_to_string(&mut json)?;
            serde_json::from_str::<VersionJson>(&json)
                .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?
        }
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(error) => return Err(zip_io_error(error)),
    };
    if version.id.trim().is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "loader installer version.json has no profile id",
        ));
    }
    Ok(Some(version.id))
}

fn extract_embedded_maven_and_read_libraries(
    installer: &Path,
    libraries_root: &Path,
) -> Result<(Vec<minecraft::Library>, String), std::io::Error> {
    let file = std::fs::File::open(installer)?;
    let mut archive = zip::ZipArchive::new(file).map_err(zip_io_error)?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(zip_io_error)?;
        let Some(enclosed) = entry.enclosed_name() else {
            continue;
        };
        let Ok(relative) = enclosed.strip_prefix("maven") else {
            continue;
        };
        if !entry.is_file() || relative.as_os_str().is_empty() {
            continue;
        }
        let destination = libraries_root.join(relative);
        file_ops::copy_reader_atomic(&destination, &mut entry)?;
    }

    let profile =
        read_installer_json::<ModernForgeInstallProfile>(&mut archive, "install_profile.json")?;
    let version = read_installer_json::<VersionJson>(&mut archive, "version.json")?;
    let libraries = profile
        .libraries
        .into_iter()
        .chain(version.libraries)
        .collect();
    if version.id.trim().is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "NeoForge installer version.json has no profile id",
        ));
    }
    Ok((libraries, version.id))
}

fn read_installer_json<T: serde::de::DeserializeOwned>(
    archive: &mut zip::ZipArchive<std::fs::File>,
    name: &str,
) -> Result<T, std::io::Error> {
    let mut entry = archive.by_name(name).map_err(zip_io_error)?;
    let mut json = String::new();
    entry.read_to_string(&mut json)?;
    serde_json::from_str(&json)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
}

fn neoforge_library_download(
    root: &Path,
    library: &minecraft::Library,
    router: SourceRouter,
) -> Result<Option<DownloadItem>, InstallError> {
    let artifact = library
        .downloads
        .as_ref()
        .and_then(|downloads| downloads.artifact.as_ref());
    let relative = artifact
        .filter(|artifact| !artifact.path.trim().is_empty())
        .map(|artifact| artifact.path.clone())
        .or_else(|| minecraft::maven_path(&library.name));
    let Some(relative) = relative else {
        return Ok(None);
    };
    let relative_path = checked_library_path(&relative)?;
    let url = artifact
        .filter(|artifact| !artifact.url.trim().is_empty())
        .map(|artifact| artifact.url.clone())
        .or_else(|| {
            library.url.as_ref().map(|base| {
                format!(
                    "{}/{}",
                    base.trim_end_matches('/'),
                    relative.replace('\\', "/")
                )
            })
        });
    let Some(url) = url else {
        return Ok(None);
    };
    Ok(Some(DownloadItem {
        url: router.rewrite(&url),
        path: root.join("libraries").join(relative_path),
        sha1: artifact
            .and_then(|artifact| (!artifact.sha1.trim().is_empty()).then(|| artifact.sha1.clone())),
        size: artifact.map_or(0, |artifact| artifact.size),
        label: library.name.clone(),
    }))
}

fn checked_library_path(value: &str) -> Result<PathBuf, InstallError> {
    path_safety::relative_path(value).ok_or_else(|| {
        InstallError::LoaderVersion(format!("unsafe NeoForge library path {value:?}"))
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyForgeInstallProfile {
    install: LegacyForgeInstall,
    version_info: VersionJson,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyForgeInstall {
    path: String,
    file_path: String,
}

async fn install_legacy_forge(
    client: &Client,
    root: &Path,
    installer: &Path,
    tx: UnboundedSender<PipelineEvent>,
    router: SourceRouter,
    policy: &DownloadPolicy,
) -> Result<Option<String>, InstallError> {
    let profile = {
        let file = std::fs::File::open(installer)?;
        let mut archive = zip::ZipArchive::new(file).map_err(zip_io_error)?;
        if archive.file_names().any(|name| name == "version.json") {
            return Ok(None);
        }

        progress(
            &tx,
            InstallStage::DownloadingLoader,
            "Reading the legacy Forge install profile",
        );
        let mut json = String::new();
        archive
            .by_name("install_profile.json")
            .map_err(zip_io_error)?
            .read_to_string(&mut json)?;
        serde_json::from_str::<LegacyForgeInstallProfile>(&json)?
    };

    let profile_id = safe_profile_id(&profile.version_info.id)?;
    let forge_library_path = minecraft::maven_path(&profile.install.path)
        .ok_or_else(|| InstallError::LoaderVersion(profile.install.path.clone()))?;

    let mut downloads = Vec::new();
    for library in &profile.version_info.libraries {
        let relative = minecraft::maven_path(&library.name)
            .ok_or_else(|| InstallError::LoaderVersion(library.name.clone()))?;
        if relative == forge_library_path {
            continue;
        }
        let base_url = library
            .url
            .as_deref()
            .unwrap_or("https://libraries.minecraft.net/");
        downloads.push(DownloadItem {
            url: router.rewrite(&format!("{}/{}", base_url.trim_end_matches('/'), relative)),
            path: root.join("libraries").join(&relative),
            sha1: None,
            size: 0,
            label: library.name.clone(),
        });
    }
    let _ = tx.send(PipelineEvent::Log(format!(
        "Detected a legacy Forge installer; using the legacy path for {profile_id}"
    )));
    minecraft::download_batch_with_policy(
        client.clone(),
        downloads,
        InstallStage::DownloadingLoader,
        "Legacy Forge libraries",
        tx.clone(),
        policy,
    )
    .await?;
    progress(
        &tx,
        InstallStage::InstallingLoader,
        "Extracting the legacy Forge core and writing its version profile",
    );

    let output = root.join("libraries").join(&forge_library_path);
    if let Some(parent) = output.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    {
        let file = std::fs::File::open(installer)?;
        let mut archive = zip::ZipArchive::new(file).map_err(zip_io_error)?;
        let mut input = archive
            .by_name(&profile.install.file_path)
            .map_err(zip_io_error)?;
        file_ops::copy_reader_atomic(&output, &mut input)?;
    }
    let version_dir = root.join("versions").join(&profile_id);
    tokio::fs::create_dir_all(&version_dir).await?;
    file_ops::write_atomic(
        &version_dir.join(format!("{profile_id}.json")),
        &serde_json::to_vec_pretty(&profile.version_info)?,
    )
    .await?;
    let _ = tx.send(PipelineEvent::Log(format!(
        "Legacy Forge core and version profile written: {profile_id}"
    )));
    Ok(Some(profile_id))
}

fn zip_io_error(error: zip::result::ZipError) -> std::io::Error {
    std::io::Error::other(error)
}

async fn pipe_lines(
    reader: Option<BufReader<tokio::process::ChildStdout>>,
    tx: UnboundedSender<PipelineEvent>,
    tag: &'static str,
) {
    if let Some(reader) = reader {
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let _ = tx.send(PipelineEvent::Log(format!("[{tag}] {line}")));
        }
    }
}

// stderr and stdout have different concrete types, so keep a second small adapter.
async fn pipe_stderr(
    reader: Option<BufReader<tokio::process::ChildStderr>>,
    tx: UnboundedSender<PipelineEvent>,
    tag: &'static str,
) {
    if let Some(reader) = reader {
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let _ = tx.send(PipelineEvent::Log(format!("[{tag}] {line}")));
        }
    }
}

async fn ensure_launcher_profile(root: &Path) -> Result<(), std::io::Error> {
    let path = root.join("launcher_profiles.json");
    if !path.exists() {
        tokio::fs::write(path, br#"{"profiles":{},"settings":{},"version":3}"#).await?;
    }
    Ok(())
}

async fn find_installed_profile(
    root: &Path,
    minecraft: &str,
    loader: &str,
    expected_profile_id: Option<&str>,
) -> Result<Option<String>, std::io::Error> {
    if let Some(expected_profile_id) = expected_profile_id {
        let expected_profile_id = portable_component(expected_profile_id)?;
        let json = root
            .join("versions")
            .join(&expected_profile_id)
            .join(format!("{expected_profile_id}.json"));
        let bytes = match tokio::fs::read(&json).await {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        let profile = serde_json::from_slice::<VersionJson>(&bytes)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
        return Ok((profile.id == expected_profile_id
            && profile.inherits_from.as_deref() == Some(minecraft))
        .then_some(expected_profile_id));
    }

    let mut entries = tokio::fs::read_dir(root.join("versions")).await?;
    while let Some(entry) = entries.next_entry().await? {
        let id = entry.file_name().to_string_lossy().to_string();
        let json = entry.path().join(format!("{id}.json"));
        if id != minecraft
            && json.is_file()
            && let Ok(bytes) = tokio::fs::read(&json).await
            && let Ok(profile) = serde_json::from_slice::<VersionJson>(&bytes)
            && profile.inherits_from.as_deref() == Some(minecraft)
            && profile_contains_loader(&profile, loader)
        {
            return Ok(Some(id));
        }
    }
    Ok(None)
}

fn profile_contains_loader(profile: &VersionJson, loader: &str) -> bool {
    profile.libraries.iter().any(|library| {
        let mut coordinate = library.name.split(':');
        let group = coordinate.next();
        let artifact = coordinate.next();
        let version = coordinate.next();
        matches!(group, Some("net.minecraftforge" | "net.neoforged"))
            && matches!(artifact, Some("forge" | "neoforge"))
            && version == Some(loader)
    })
}

#[derive(Debug, Deserialize)]
struct ForgeMetaItem {
    version: String,
    branch: Option<String>,
}

async fn resolve_forge(
    client: &Client,
    minecraft: &str,
    requested: Option<&str>,
    router: SourceRouter,
) -> Result<String, InstallError> {
    #[derive(Deserialize)]
    struct Promotions {
        promos: HashMap<String, String>,
    }
    let requested = match requested {
        Some(version) => version.to_string(),
        None => {
            let data: Promotions = client
                .get(router.rewrite(
                    "https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json",
                ))
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            data.promos
                .get(&format!("{minecraft}-recommended"))
                .or_else(|| data.promos.get(&format!("{minecraft}-latest")))
                .cloned()
                .ok_or_else(|| InstallError::LoaderVersion(format!("Forge for {minecraft}")))?
        }
    };

    // Legacy Forge builds may carry a branch which is part of both the Maven
    // directory and installer filename (for example 1.7.10 build 1614).
    // The promotions manifest omits that field, so use the same metadata source
    // as SJMCL to recover it before constructing the official Maven URL.
    let metadata_url = format!("https://bmclapi2.bangbang93.com/forge/minecraft/{minecraft}");
    if let Ok(response) = client.get(router.rewrite(&metadata_url)).send().await
        && let Ok(response) = response.error_for_status()
        && let Ok(items) = response.json::<Vec<ForgeMetaItem>>().await
        && let Some(full) = items.iter().find_map(|item| {
            let full = forge_full_version(minecraft, &item.version, item.branch.as_deref());
            (item.version == requested || full == requested).then_some(full)
        })
    {
        return Ok(full);
    }

    Ok(normalize_forge_version(
        LoaderKind::Forge,
        minecraft,
        &requested,
    ))
}

fn forge_full_version(minecraft: &str, version: &str, branch: Option<&str>) -> String {
    [
        Some(minecraft),
        Some(version),
        branch.filter(|value| !value.is_empty()),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join("-")
}

async fn resolve_neoforge(
    client: &Client,
    minecraft: &str,
    router: SourceRouter,
) -> Result<String, InstallError> {
    #[derive(Deserialize)]
    struct Metadata {
        versioning: Versioning,
    }
    #[derive(Deserialize)]
    struct Versioning {
        versions: Versions,
    }
    #[derive(Deserialize)]
    struct Versions {
        version: Vec<String>,
    }
    let url = if minecraft == "1.20.1" {
        "https://maven.neoforged.net/releases/net/neoforged/forge/maven-metadata.xml"
    } else {
        "https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml"
    };
    let xml = client
        .get(router.rewrite(url))
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    let data: Metadata = quick_xml::de::from_str(&xml)
        .map_err(|_| InstallError::LoaderVersion("NeoForge metadata".into()))?;
    data.versioning
        .versions
        .version
        .into_iter()
        .rev()
        .find(|version| loader_catalog::neoforge_matches_minecraft(version, minecraft))
        .ok_or_else(|| InstallError::LoaderVersion(format!("NeoForge for {minecraft}")))
}

fn normalize_forge_version(kind: LoaderKind, minecraft: &str, version: &str) -> String {
    if kind == LoaderKind::Forge && !version.starts_with(&format!("{minecraft}-")) {
        format!("{minecraft}-{version}")
    } else {
        version.into()
    }
}

fn forge_installer_url(kind: LoaderKind, minecraft: &str, version: &str) -> (String, String) {
    if kind == LoaderKind::Forge {
        let filename = format!("forge-{version}-installer.jar");
        (
            format!(
                "https://maven.minecraftforge.net/net/minecraftforge/forge/{version}/{filename}"
            ),
            filename,
        )
    } else if minecraft == "1.20.1" {
        let filename = format!("forge-{version}-installer.jar");
        (
            format!(
                "https://maven.neoforged.net/releases/net/neoforged/forge/{version}/{filename}"
            ),
            filename,
        )
    } else {
        let filename = format!("neoforge-{version}-installer.jar");
        (
            format!(
                "https://maven.neoforged.net/releases/net/neoforged/neoforge/{version}/{filename}"
            ),
            filename,
        )
    }
}

fn progress(tx: &UnboundedSender<PipelineEvent>, stage: InstallStage, detail: &str) {
    let _ = tx.send(PipelineEvent::Progress(InstallProgress {
        stage,
        current: 0,
        total: 0,
        detail: detail.into(),
        files_done: 0,
        files_total: 0,
        bytes_per_second: 0.0,
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};
    use zip::{ZipWriter, write::SimpleFileOptions};

    #[tokio::test]
    async fn instance_directory_receives_client_and_profile_files() {
        let fixture = std::env::temp_dir().join(format!(
            "azulc-instance-version-files-{}",
            uuid::Uuid::new_v4()
        ));
        let minecraft = fixture.join("minecraft");
        let instance = fixture.join("instance");
        let base = minecraft.join("versions/1.20.1");
        let profile = minecraft.join("versions/1.20.1-forge-47.4.20");
        std::fs::create_dir_all(&base).unwrap();
        std::fs::create_dir_all(&profile).unwrap();
        std::fs::create_dir_all(&instance).unwrap();
        std::fs::write(base.join("1.20.1.jar"), b"client").unwrap();
        std::fs::write(base.join("1.20.1.json"), b"base profile").unwrap();
        std::fs::write(profile.join("1.20.1-forge-47.4.20.json"), b"loader profile").unwrap();

        materialize_instance_version_files(&minecraft, &instance, "1.20.1", "1.20.1-forge-47.4.20")
            .await
            .unwrap();

        assert_eq!(
            std::fs::read(instance.join("1.20.1.jar")).unwrap(),
            b"client"
        );
        assert!(instance.join("1.20.1.json").is_file());
        assert!(instance.join("1.20.1-forge-47.4.20.json").is_file());
        std::fs::remove_dir_all(fixture).unwrap();
    }

    #[tokio::test]
    async fn failed_instance_materialization_preserves_existing_files() {
        let fixture = std::env::temp_dir().join(format!(
            "azulc-instance-version-preserve-{}",
            uuid::Uuid::new_v4()
        ));
        let destination = fixture.join("instance/version.json");
        std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
        std::fs::write(&destination, b"existing").unwrap();

        let error = link_or_copy(&fixture.join("missing.json"), &destination)
            .await
            .expect_err("a missing source must fail");

        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
        assert_eq!(std::fs::read(&destination).unwrap(), b"existing");
        std::fs::remove_dir_all(fixture).unwrap();
    }

    #[tokio::test]
    async fn instance_materialization_rejects_traversing_profile_ids() {
        let error = materialize_instance_version_files(
            Path::new("minecraft"),
            Path::new("instance"),
            "1.20.1",
            "../outside",
        )
        .await
        .expect_err("profile ids must stay inside the versions directory");

        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    }

    #[test]
    fn legacy_forge_branch_is_part_of_installer_coordinate() {
        let version = forge_full_version("1.7.10", "10.13.4.1614", Some("1.7.10"));
        assert_eq!(version, "1.7.10-10.13.4.1614-1.7.10");

        let (url, filename) = forge_installer_url(LoaderKind::Forge, "1.7.10", &version);
        assert_eq!(filename, "forge-1.7.10-10.13.4.1614-1.7.10-installer.jar");
        assert!(url.ends_with(&format!("/{version}/{filename}")));
    }

    #[test]
    fn modern_forge_without_branch_keeps_standard_coordinate() {
        assert_eq!(
            forge_full_version("1.20.1", "47.4.10", None),
            "1.20.1-47.4.10"
        );
    }

    #[test]
    fn parses_legacy_forge_install_profile() {
        let profile: LegacyForgeInstallProfile = serde_json::from_str(
            r#"{
                "install": {
                    "path": "net.minecraftforge:forge:1.7.10-10.13.4.1614-1.7.10",
                    "filePath": "forge-1.7.10-10.13.4.1614-1.7.10-universal.jar"
                },
                "versionInfo": {
                    "id": "1.7.10-Forge10.13.4.1614-1.7.10",
                    "inheritsFrom": "1.7.10",
                    "mainClass": "net.minecraft.launchwrapper.Launch",
                    "minecraftArguments": "--tweakClass cpw.mods.fml.common.launcher.FMLTweaker",
                    "libraries": [{"name": "net.minecraft:launchwrapper:1.12"}]
                }
            }"#,
        )
        .expect("legacy install profile should parse");

        assert_eq!(
            profile.version_info.inherits_from.as_deref(),
            Some("1.7.10")
        );
        assert_eq!(
            minecraft::maven_path(&profile.install.path).as_deref(),
            Some(
                "net/minecraftforge/forge/1.7.10-10.13.4.1614-1.7.10/forge-1.7.10-10.13.4.1614-1.7.10.jar"
            )
        );
    }

    #[test]
    fn installed_profile_must_contain_the_exact_loader_coordinate() {
        let profile = VersionJson {
            libraries: vec![minecraft::Library {
                name: "net.minecraftforge:forge:1.20.1-47.4.10".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        assert!(profile_contains_loader(&profile, "1.20.1-47.4.10"));
        assert!(!profile_contains_loader(&profile, "1.20.1-47.4.9"));
    }

    #[tokio::test]
    async fn modern_neoforge_profile_uses_the_installer_declared_id() {
        let fixture =
            std::env::temp_dir().join(format!("azulc-neoforge-profile-{}", uuid::Uuid::new_v4()));
        let profile = VersionJson {
            id: "neoforge-21.1.249".into(),
            inherits_from: Some("1.21.1".into()),
            libraries: vec![minecraft::Library {
                name: "net.neoforged.fancymodloader:loader:4.0.44".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let directory = fixture.join("versions").join(&profile.id);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join(format!("{}.json", profile.id)),
            serde_json::to_vec(&profile).unwrap(),
        )
        .unwrap();

        assert!(!profile_contains_loader(&profile, "21.1.249"));
        assert_eq!(
            find_installed_profile(&fixture, "1.21.1", "21.1.249", Some("neoforge-21.1.249"))
                .await
                .unwrap()
                .as_deref(),
            Some("neoforge-21.1.249")
        );
        assert!(
            find_installed_profile(&fixture, "1.21.1", "21.1.249", Some("neoforge-21.1.248"))
                .await
                .unwrap()
                .is_none()
        );
        std::fs::remove_dir_all(fixture).unwrap();
    }

    #[test]
    fn modern_forge_profile_id_is_read_from_the_installer() {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file("version.json", SimpleFileOptions::default())
            .unwrap();
        writer
            .write_all(br#"{"id":"1.20.1-forge-47.4.20","inheritsFrom":"1.20.1","libraries":[]}"#)
            .unwrap();
        let bytes = writer.finish().unwrap().into_inner();
        let fixture = std::env::temp_dir().join(format!(
            "azulc-forge-profile-id-{}.jar",
            uuid::Uuid::new_v4()
        ));
        std::fs::write(&fixture, bytes).unwrap();

        assert_eq!(
            read_modern_installer_profile_id(&fixture)
                .unwrap()
                .as_deref(),
            Some("1.20.1-forge-47.4.20")
        );

        std::fs::remove_file(fixture).unwrap();
    }

    #[test]
    fn curseforge_file_names_are_confined_to_one_path_component() {
        assert_eq!(safe_file_name("example.jar").unwrap(), "example.jar");
        assert!(safe_file_name("").is_err());
        assert!(safe_file_name("../example.jar").is_err());
        assert!(safe_file_name("mods/example.jar").is_err());
        assert!(safe_file_name(r"mods\example.jar").is_err());
    }

    #[test]
    fn curseforge_manifest_destination_matches_launcher_conventions() {
        assert_eq!(
            curseforge_content_directory(Some(ResourceClass::Mod), 1).unwrap(),
            "mods"
        );
        assert_eq!(
            curseforge_content_directory(Some(ResourceClass::ResourcePack), 1).unwrap(),
            "resourcepacks"
        );
        assert_eq!(
            curseforge_content_directory(Some(ResourceClass::ShaderPack), 1).unwrap(),
            "shaderpacks"
        );
        assert!(curseforge_content_directory(Some(ResourceClass::Modpack), 1).is_err());
    }

    #[test]
    fn neoforge_dependencies_are_planned_from_installer_metadata() {
        let root = Path::new("minecraft-root");
        let library = minecraft::Library {
            name: "com.electronwill.night-config:core:3.8.3".into(),
            downloads: Some(minecraft::LibraryDownloads {
                artifact: Some(minecraft::DownloadRef {
                    path: "com/electronwill/night-config/core/3.8.3/core-3.8.3.jar".into(),
                    url: "https://maven.neoforged.net/releases/com/electronwill/night-config/core/3.8.3/core-3.8.3.jar".into(),
                    sha1: "b442a95f09e349927f5a945ecb594455870fcf4f".into(),
                    size: 382_911,
                }),
                ..Default::default()
            }),
            ..Default::default()
        };
        let item = neoforge_library_download(
            root,
            &library,
            SourceRouter::new(crate::domain::DownloadSource::Bmcl),
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            item.path,
            root.join("libraries/com/electronwill/night-config/core/3.8.3/core-3.8.3.jar")
        );
        assert!(
            item.url
                .starts_with("https://bmclapi2.bangbang93.com/maven/")
        );
        assert_eq!(
            item.sha1.as_deref(),
            Some("b442a95f09e349927f5a945ecb594455870fcf4f")
        );
    }

    #[test]
    fn neoforge_installer_embedded_maven_is_extracted_safely() {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file(
                "maven/net/neoforged/embedded/1.0/embedded-1.0.jar",
                SimpleFileOptions::default(),
            )
            .unwrap();
        writer.write_all(b"embedded library").unwrap();
        writer
            .start_file("install_profile.json", SimpleFileOptions::default())
            .unwrap();
        writer
            .write_all(br#"{"libraries":[{"name":"example:test:1.0"}]}"#)
            .unwrap();
        writer
            .start_file("version.json", SimpleFileOptions::default())
            .unwrap();
        writer
            .write_all(br#"{"id":"neoforge-21.1.249","libraries":[]}"#)
            .unwrap();
        let bytes = writer.finish().unwrap().into_inner();

        let fixture = std::env::temp_dir().join(format!("azulc-neoforge-{}", uuid::Uuid::new_v4()));
        let installer = fixture.join("installer.jar");
        let libraries = fixture.join("libraries");
        std::fs::create_dir_all(&fixture).unwrap();
        std::fs::write(&installer, bytes).unwrap();
        let (parsed, profile_id) =
            extract_embedded_maven_and_read_libraries(&installer, &libraries).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(profile_id, "neoforge-21.1.249");
        assert_eq!(
            std::fs::read(libraries.join("net/neoforged/embedded/1.0/embedded-1.0.jar")).unwrap(),
            b"embedded library"
        );
        std::fs::remove_dir_all(fixture).unwrap();
    }
}
