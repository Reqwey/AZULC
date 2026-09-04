//! Provider-neutral modpack discovery.

use super::{CatalogProject, CatalogProjectKey, CatalogProvider, CatalogRelease};
use crate::{
    domain::ModpackSource,
    services::providers::{
        curseforge::{CurseForgeClient, FileQuery, ResourceClass, SearchQuery},
        modrinth::{
            self, ContentType as ModrinthContentType, ModrinthClient,
            SearchQuery as ModrinthSearchQuery, VersionQuery as ModrinthVersionQuery,
        },
    },
};

pub(crate) async fn search_modpacks(
    provider: CatalogProvider,
    search_filter: String,
) -> Result<Vec<CatalogProject>, String> {
    match provider {
        CatalogProvider::CurseForge => {
            let mut query = SearchQuery::new(ResourceClass::Modpack);
            query.search_filter = (!search_filter.is_empty()).then_some(search_filter);
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
            let mut query = ModrinthSearchQuery::new(ModrinthContentType::Modpack);
            query.query = search_filter;
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

pub(crate) async fn list_modpack_releases(
    project: CatalogProjectKey,
) -> Result<Vec<CatalogRelease>, String> {
    match project {
        CatalogProjectKey::CurseForge(project_id) => {
            let client = CurseForgeClient::from_env().map_err(|error| error.to_string())?;
            let page = client
                .list_files(project_id, &FileQuery::default())
                .await
                .map_err(|error| error.to_string())?;
            Ok(page
                .files
                .into_iter()
                .map(CatalogRelease::from_curseforge)
                .collect())
        }
        CatalogProjectKey::Modrinth(project_id) => {
            let client = ModrinthClient::new().map_err(|error| error.to_string())?;
            let versions = client
                .list_versions(&project_id, &ModrinthVersionQuery::default())
                .await
                .map_err(|error| error.to_string())?;
            Ok(CatalogRelease::from_modrinth_versions(versions))
        }
    }
}

pub(crate) fn modpack_source(release: &CatalogRelease) -> ModpackSource {
    match &release.key {
        super::CatalogReleaseKey::CurseForge {
            project_id,
            file_id,
        } => ModpackSource::CurseForge {
            project_id: *project_id,
            file_id: *file_id,
            file_name: release.file_name.clone(),
        },
        super::CatalogReleaseKey::Modrinth {
            project_id,
            version_id,
        } => ModpackSource::Modrinth {
            project_id: project_id.clone(),
            version_id: version_id.clone(),
            file_name: release.file_name.clone(),
        },
    }
}
