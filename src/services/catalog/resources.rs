//! Provider-neutral resource discovery and installation.

use std::path::PathBuf;

use crate::{
    domain::LoaderKind,
    services::{
        content::ContentKind,
        download, path_safety,
        providers::{
            curseforge::{
                CurseForgeClient, FileQuery, ModLoader as CurseForgeLoader, ResourceClass,
                ResourceInstallRequest as CurseForgeInstallRequest, SearchQuery,
            },
            modrinth::{
                self, ContentType as ModrinthContentType, Loader as ModrinthLoader, ModrinthClient,
                SearchQuery as ModrinthSearchQuery, VersionQuery as ModrinthVersionQuery,
            },
        },
    },
};

use super::{
    CatalogProject, CatalogProjectKey, CatalogProvider, CatalogRelease, CatalogReleaseKey,
};

#[derive(Debug, Clone)]
pub(crate) struct ResourceSearchRequest {
    pub(crate) provider: CatalogProvider,
    pub(crate) kind: ContentKind,
    pub(crate) game_version: String,
    pub(crate) loader: LoaderKind,
    pub(crate) query: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ResourceReleaseRequest {
    pub(crate) provider: CatalogProvider,
    pub(crate) project: CatalogProjectKey,
    pub(crate) kind: ContentKind,
    pub(crate) game_version: String,
    pub(crate) loader: LoaderKind,
}

#[derive(Debug, Clone)]
pub(crate) struct ResourceInstallRequest {
    pub(crate) release: CatalogReleaseKey,
    pub(crate) kind: ContentKind,
    pub(crate) game_directory: PathBuf,
    pub(crate) game_version: String,
    pub(crate) loader: LoaderKind,
    pub(crate) concurrency: usize,
}

pub(crate) async fn search_resources(
    request: ResourceSearchRequest,
) -> Result<Vec<CatalogProject>, String> {
    let Some(resource_class) = resource_class(request.kind) else {
        return Err("This content type is not available from online catalogs.".into());
    };

    match request.provider {
        CatalogProvider::CurseForge => {
            let mut query = SearchQuery::new(resource_class);
            query.game_version = Some(request.game_version);
            query.search_filter = (!request.query.is_empty()).then_some(request.query);
            if request.kind == ContentKind::Mods {
                query.mod_loader = curseforge_loader(request.loader);
            }
            let client = CurseForgeClient::from_env().map_err(|error| error.to_string())?;
            let page = client
                .search(&query)
                .await
                .map_err(|error| error.to_string())?;
            Ok(page
                .projects
                .into_iter()
                .map(CatalogProject::from_curseforge)
                .collect())
        }
        CatalogProvider::Modrinth => {
            let content_type = modrinth_content_type(request.kind)
                .ok_or_else(|| "This content type is not available from Modrinth.".to_string())?;
            let mut query = ModrinthSearchQuery::new(content_type);
            query.query = request.query;
            query.game_version = Some(request.game_version);
            query.loader = modrinth_loader(request.kind, request.loader);
            query.sort = modrinth::SearchSort::Downloads;
            query.limit = 50;
            let client = ModrinthClient::new().map_err(|error| error.to_string())?;
            let page = client
                .search(&query)
                .await
                .map_err(|error| error.to_string())?;
            Ok(page
                .hits
                .into_iter()
                .map(CatalogProject::from_modrinth)
                .collect())
        }
    }
}

pub(crate) async fn list_resource_releases(
    request: ResourceReleaseRequest,
) -> Result<Vec<CatalogRelease>, String> {
    match (request.provider, request.project) {
        (CatalogProvider::CurseForge, CatalogProjectKey::CurseForge(project_id)) => {
            let mut query = FileQuery {
                game_version: Some(request.game_version),
                ..FileQuery::default()
            };
            if request.kind == ContentKind::Mods {
                query.mod_loader = curseforge_loader(request.loader);
            }
            let client = CurseForgeClient::from_env().map_err(|error| error.to_string())?;
            let page = client
                .list_files(project_id, &query)
                .await
                .map_err(|error| error.to_string())?;
            Ok(page
                .files
                .into_iter()
                .map(CatalogRelease::from_curseforge)
                .collect())
        }
        (CatalogProvider::Modrinth, CatalogProjectKey::Modrinth(project_id)) => {
            let client = ModrinthClient::new().map_err(|error| error.to_string())?;
            let query = ModrinthVersionQuery::compatible(
                request.game_version,
                modrinth_loader(request.kind, request.loader),
            );
            let versions = client
                .list_versions(&project_id, &query)
                .await
                .map_err(|error| error.to_string())?;
            Ok(CatalogRelease::from_modrinth_versions(versions))
        }
        _ => Err("The selected project belongs to a different provider.".into()),
    }
}

pub(crate) async fn install_resource(request: ResourceInstallRequest) -> Result<PathBuf, String> {
    let directory_name = request
        .kind
        .directory()
        .ok_or_else(|| "The selected content type has no instance directory.".to_string())?;
    let directory = request.game_directory.join(directory_name);
    let data_pack_directory = request.game_directory.join("datapacks");

    match request.release {
        CatalogReleaseKey::CurseForge {
            project_id,
            file_id,
        } => {
            let expected_resource_class = resource_class(request.kind).ok_or_else(|| {
                "This content type is not available from online catalogs.".to_string()
            })?;
            let client = CurseForgeClient::from_env().map_err(|error| error.to_string())?;
            let project = client
                .get_project(project_id)
                .await
                .map_err(|error| error.to_string())?;
            let file = client
                .get_file(project_id, file_id)
                .await
                .map_err(|error| error.to_string())?;
            let file_name = safe_catalog_file_name(&file.file_name)?;
            let installed = client
                .install_resource_with_dependencies(CurseForgeInstallRequest {
                    project,
                    file,
                    expected_resource_class,
                    primary_destination: directory.join(file_name),
                    game_directory: request.game_directory,
                    data_pack_directory,
                    game_version: request.game_version,
                    mod_loader: curseforge_loader(request.loader),
                    concurrency: request.concurrency,
                })
                .await
                .map_err(|error| error.to_string())?;
            Ok(installed.primary_destination)
        }
        CatalogReleaseKey::Modrinth {
            project_id,
            version_id,
        } => {
            let client = ModrinthClient::new().map_err(|error| error.to_string())?;
            let dependency_query = ModrinthVersionQuery::compatible(
                request.game_version,
                modrinth_loader(request.kind, request.loader),
            );
            let resolved = client
                .resolve_installation(&project_id, &version_id, &dependency_query)
                .await
                .map_err(|error| error.to_string())?;
            let expected_type = modrinth_content_type(request.kind)
                .ok_or_else(|| "Unsupported Modrinth content type.".to_string())?;
            if resolved
                .first()
                .is_none_or(|root| root.project_type != expected_type)
            {
                return Err(format!(
                    "The selected Modrinth release is not a {expected_type} project."
                ));
            }

            let mut primary_destination = None;
            let mut specs = Vec::with_capacity(resolved.len());
            for (index, item) in resolved.into_iter().enumerate() {
                let destination_directory = if index == 0 {
                    directory.clone()
                } else {
                    match item.project_type {
                        ModrinthContentType::Mod => request.game_directory.join("mods"),
                        ModrinthContentType::ResourcePack => {
                            request.game_directory.join("resourcepacks")
                        }
                        ModrinthContentType::Shader => request.game_directory.join("shaderpacks"),
                        ModrinthContentType::DataPack => data_pack_directory.clone(),
                        ModrinthContentType::Modpack
                        | ModrinthContentType::Plugin
                        | ModrinthContentType::Unknown => {
                            return Err(format!(
                                "Unsupported required dependency type {} for {}.",
                                item.project_type, item.project.title
                            ));
                        }
                    }
                };
                let spec = item.install.download_spec(destination_directory);
                if index == 0 {
                    primary_destination = Some(spec.destination.clone());
                }
                specs.push(spec);
            }
            download::download_batch(client.download_client(), specs, request.concurrency, |_| {})
                .await
                .map_err(|error| error.to_string())?;
            primary_destination
                .ok_or_else(|| "Modrinth returned an empty installation plan.".to_string())
        }
    }
}

fn safe_catalog_file_name(value: &str) -> Result<String, String> {
    path_safety::file_name(value).ok_or_else(|| "The provider returned an unsafe file name.".into())
}

fn resource_class(kind: ContentKind) -> Option<ResourceClass> {
    match kind {
        ContentKind::Mods => Some(ResourceClass::Mod),
        ContentKind::ResourcePacks => Some(ResourceClass::ResourcePack),
        ContentKind::ShaderPacks => Some(ResourceClass::ShaderPack),
        ContentKind::DataPacks => Some(ResourceClass::DataPack),
        ContentKind::Worlds | ContentKind::Screenshots => None,
    }
}

fn modrinth_content_type(kind: ContentKind) -> Option<ModrinthContentType> {
    match kind {
        ContentKind::Mods => Some(ModrinthContentType::Mod),
        ContentKind::ResourcePacks => Some(ModrinthContentType::ResourcePack),
        ContentKind::ShaderPacks => Some(ModrinthContentType::Shader),
        ContentKind::DataPacks => Some(ModrinthContentType::DataPack),
        ContentKind::Worlds | ContentKind::Screenshots => None,
    }
}

fn modrinth_loader(kind: ContentKind, loader: LoaderKind) -> Option<ModrinthLoader> {
    match kind {
        ContentKind::Mods if loader != LoaderKind::Vanilla => Some(loader.into()),
        ContentKind::ResourcePacks => Some(ModrinthLoader::Minecraft),
        ContentKind::DataPacks => Some(ModrinthLoader::DataPack),
        ContentKind::Mods
        | ContentKind::ShaderPacks
        | ContentKind::Worlds
        | ContentKind::Screenshots => None,
    }
}

fn curseforge_loader(loader: LoaderKind) -> Option<CurseForgeLoader> {
    match loader {
        LoaderKind::Vanilla => None,
        LoaderKind::Fabric => Some(CurseForgeLoader::Fabric),
        LoaderKind::Forge => Some(CurseForgeLoader::Forge),
        LoaderKind::NeoForge => Some(CurseForgeLoader::NeoForge),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_loaders_follow_content_kind() {
        assert!(matches!(
            curseforge_loader(LoaderKind::Fabric),
            Some(CurseForgeLoader::Fabric)
        ));
        assert!(matches!(
            modrinth_loader(ContentKind::ResourcePacks, LoaderKind::Vanilla),
            Some(ModrinthLoader::Minecraft)
        ));
        assert!(matches!(
            modrinth_loader(ContentKind::DataPacks, LoaderKind::Vanilla),
            Some(ModrinthLoader::DataPack)
        ));
        assert!(modrinth_loader(ContentKind::ShaderPacks, LoaderKind::Fabric).is_none());
    }

    #[tokio::test]
    async fn release_query_rejects_a_provider_key_mismatch_before_network_access() {
        let result = list_resource_releases(ResourceReleaseRequest {
            provider: CatalogProvider::CurseForge,
            project: CatalogProjectKey::Modrinth("project".into()),
            kind: ContentKind::Mods,
            game_version: "1.21.1".into(),
            loader: LoaderKind::Fabric,
        })
        .await;

        assert_eq!(
            result.unwrap_err(),
            "The selected project belongs to a different provider."
        );
    }

    #[tokio::test]
    async fn search_rejects_local_only_content_before_network_access() {
        let result = search_resources(ResourceSearchRequest {
            provider: CatalogProvider::CurseForge,
            kind: ContentKind::Worlds,
            game_version: "1.21.1".into(),
            loader: LoaderKind::Vanilla,
            query: String::new(),
        })
        .await;

        assert_eq!(
            result.unwrap_err(),
            "This content type is not available from online catalogs."
        );
    }
}
