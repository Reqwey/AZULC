//! Provider-neutral catalog models and operations.

mod model;
mod modpacks;
mod resources;

pub(crate) use model::{
    CatalogProject, CatalogProjectKey, CatalogProvider, CatalogRelease, CatalogReleaseKey,
    thumbnail_urls,
};
pub(crate) use modpacks::{list_modpack_releases, modpack_source, search_modpacks};
pub(crate) use resources::{
    ResourceInstallRequest, ResourceReleaseRequest, ResourceSearchRequest, install_resource,
    list_resource_releases, search_resources,
};

pub(crate) fn missing_credential(provider: CatalogProvider) -> Option<&'static str> {
    match provider {
        CatalogProvider::CurseForge
            if std::env::var_os(crate::services::providers::curseforge::API_KEY_ENV).is_none() =>
        {
            Some(crate::services::providers::curseforge::API_KEY_ENV)
        }
        CatalogProvider::CurseForge | CatalogProvider::Modrinth => None,
    }
}
