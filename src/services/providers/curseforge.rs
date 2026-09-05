//! CurseForge API models and client operations.

use crate::{
    environment,
    services::{
        download::{self, DownloadSpec, integrity},
        path_safety,
    },
};
use reqwest::{
    Client, StatusCode, Url,
    header::{ACCEPT, HeaderMap, HeaderValue},
    redirect::Policy,
};
use serde::{Deserialize, Deserializer, de::DeserializeOwned};
use std::{
    collections::{HashMap, VecDeque},
    path::{Path, PathBuf},
    time::Duration,
};

pub use crate::environment::CURSEFORGE_API_KEY_ENV as API_KEY_ENV;
pub const MINECRAFT_GAME_ID: u32 = 432;

const API_BASE: &str = "https://api.curseforge.com/";
const API_HOST: &str = "api.curseforge.com";
const API_KEY_HEADER: &str = "x-api-key";
const MAX_PAGE_SIZE: u32 = 50;
const MAX_RESULT_OFFSET: u32 = 10_000;

/// Top-level Minecraft project classes currently exposed by CurseForge.
///
/// These numeric ids are the wire values accepted by `/v1/mods/search`.
/// Keeping the mapping in one place also makes it possible to replace it with
/// class discovery through `/v1/categories` later without touching the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceClass {
    Modpack,
    Mod,
    ResourcePack,
    World,
    ShaderPack,
    DataPack,
}

impl ResourceClass {
    pub const fn id(self) -> u32 {
        match self {
            Self::Modpack => 4471,
            Self::Mod => 6,
            Self::ResourcePack => 12,
            Self::World => 17,
            Self::ShaderPack => 6552,
            Self::DataPack => 6945,
        }
    }

    pub const fn from_id(id: u32) -> Option<Self> {
        match id {
            4471 => Some(Self::Modpack),
            6 => Some(Self::Mod),
            12 => Some(Self::ResourcePack),
            17 => Some(Self::World),
            6552 => Some(Self::ShaderPack),
            6945 => Some(Self::DataPack),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModLoader {
    Any,
    Forge,
    Cauldron,
    LiteLoader,
    Fabric,
    Quilt,
    NeoForge,
}

impl ModLoader {
    pub const fn id(self) -> u8 {
        match self {
            Self::Any => 0,
            Self::Forge => 1,
            Self::Cauldron => 2,
            Self::LiteLoader => 3,
            Self::Fabric => 4,
            Self::Quilt => 5,
            Self::NeoForge => 6,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SearchSort {
    Popularity,
}

impl SearchSort {
    const fn id(self) -> u8 {
        match self {
            Self::Popularity => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SortOrder {
    Descending,
}

impl SortOrder {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Descending => "desc",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchQuery {
    pub resource_class: ResourceClass,
    pub category_id: Option<u32>,
    pub game_version: Option<String>,
    pub game_version_type_id: Option<u32>,
    pub search_filter: Option<String>,
    pub mod_loader: Option<ModLoader>,
    pub sort: SearchSort,
    pub sort_order: SortOrder,
    pub index: u32,
    pub page_size: u32,
}

impl SearchQuery {
    pub fn new(resource_class: ResourceClass) -> Self {
        Self {
            resource_class,
            category_id: None,
            game_version: None,
            game_version_type_id: None,
            search_filter: None,
            mod_loader: None,
            sort: SearchSort::Popularity,
            sort_order: SortOrder::Descending,
            index: 0,
            page_size: 50,
        }
    }

    fn parameters(&self) -> Result<Vec<(&'static str, String)>, CurseForgeError> {
        validate_pagination(self.index, self.page_size)?;
        let loader = self.mod_loader.filter(|loader| *loader != ModLoader::Any);
        if loader.is_some()
            && self
                .game_version
                .as_deref()
                .is_none_or(|version| version.trim().is_empty())
        {
            return Err(CurseForgeError::InvalidQuery(
                "modLoaderType must be coupled with gameVersion",
            ));
        }

        let mut parameters = vec![
            ("gameId", MINECRAFT_GAME_ID.to_string()),
            ("classId", self.resource_class.id().to_string()),
            ("sortField", self.sort.id().to_string()),
            ("sortOrder", self.sort_order.as_str().to_string()),
            ("index", self.index.to_string()),
            ("pageSize", self.page_size.to_string()),
        ];
        if let Some(category_id) = self.category_id {
            parameters.push(("categoryId", category_id.to_string()));
        }
        if let Some(version) = nonempty(self.game_version.as_deref()) {
            parameters.push(("gameVersion", version.to_string()));
        }
        if let Some(version_type_id) = self.game_version_type_id {
            parameters.push(("gameVersionTypeId", version_type_id.to_string()));
        }
        if let Some(search) = nonempty(self.search_filter.as_deref()) {
            parameters.push(("searchFilter", search.to_string()));
        }
        if let Some(loader) = loader {
            parameters.push(("modLoaderType", loader.id().to_string()));
        }
        Ok(parameters)
    }
}

impl Default for SearchQuery {
    fn default() -> Self {
        Self::new(ResourceClass::Mod)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileQuery {
    pub game_version: Option<String>,
    pub game_version_type_id: Option<u32>,
    pub mod_loader: Option<ModLoader>,
    pub index: u32,
    pub page_size: u32,
}

impl FileQuery {
    fn parameters(&self) -> Result<Vec<(&'static str, String)>, CurseForgeError> {
        validate_pagination(self.index, self.page_size)?;
        let mut parameters = vec![
            ("index", self.index.to_string()),
            ("pageSize", self.page_size.to_string()),
        ];
        if let Some(version) = nonempty(self.game_version.as_deref()) {
            parameters.push(("gameVersion", version.to_string()));
        }
        if let Some(version_type_id) = self.game_version_type_id {
            parameters.push(("gameVersionTypeId", version_type_id.to_string()));
        }
        if let Some(loader) = self.mod_loader.filter(|loader| *loader != ModLoader::Any) {
            parameters.push(("modLoaderType", loader.id().to_string()));
        }
        Ok(parameters)
    }
}

impl Default for FileQuery {
    fn default() -> Self {
        Self {
            game_version: None,
            game_version_type_id: None,
            mod_loader: None,
            index: 0,
            page_size: 50,
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct SearchPage {
    #[serde(rename = "data", default)]
    pub projects: Vec<Project>,
    pub pagination: Pagination,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct FilePage {
    #[serde(rename = "data", default)]
    pub files: Vec<File>,
    pub pagination: Pagination,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Pagination {
    pub index: u32,
    pub page_size: u32,
    pub result_count: u32,
    pub total_count: u64,
}

impl Pagination {
    pub fn next_index(&self) -> Option<u32> {
        if self.result_count == 0 {
            return None;
        }
        let next = self.index.checked_add(self.result_count)?;
        (u64::from(next) < self.total_count && next < MAX_RESULT_OFFSET).then_some(next)
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: u64,
    pub game_id: u64,
    pub name: String,
    pub slug: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub download_count: u64,
    #[serde(default)]
    pub is_featured: bool,
    #[serde(default)]
    pub primary_category_id: Option<u64>,
    #[serde(default)]
    pub categories: Vec<Category>,
    #[serde(default)]
    pub class_id: Option<u64>,
    #[serde(default)]
    pub authors: Vec<Author>,
    #[serde(default)]
    pub logo: Option<ProjectAsset>,
    #[serde(default)]
    pub screenshots: Vec<ProjectAsset>,
    #[serde(default)]
    pub main_file_id: Option<u64>,
    #[serde(default)]
    pub latest_files: Vec<File>,
    #[serde(default)]
    pub latest_files_indexes: Vec<FileIndex>,
    #[serde(default)]
    pub date_created: String,
    #[serde(default)]
    pub date_modified: String,
    #[serde(default)]
    pub date_released: String,
    #[serde(default)]
    pub allow_mod_distribution: Option<bool>,
    pub is_available: bool,
}

impl Project {
    pub fn resource_class(&self) -> Option<ResourceClass> {
        self.class_id
            .and_then(|id| u32::try_from(id).ok())
            .and_then(ResourceClass::from_id)
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    pub id: u64,
    pub game_id: u64,
    pub name: String,
    pub slug: String,
    #[serde(default)]
    pub icon_url: String,
    #[serde(default)]
    pub is_class: Option<bool>,
    #[serde(default)]
    pub class_id: Option<u64>,
    #[serde(default)]
    pub parent_category_id: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Author {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub url: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectAsset {
    pub id: u64,
    pub mod_id: u64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub thumbnail_url: String,
    #[serde(default)]
    pub url: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct File {
    pub id: u64,
    pub game_id: u64,
    pub mod_id: u64,
    pub is_available: bool,
    pub display_name: String,
    pub file_name: String,
    pub release_type: u8,
    pub file_status: u8,
    #[serde(default)]
    pub hashes: Vec<FileHash>,
    #[serde(default)]
    pub file_date: String,
    #[serde(default)]
    pub file_length: u64,
    #[serde(default)]
    pub download_count: u64,
    #[serde(default)]
    pub file_size_on_disk: u64,
    #[serde(default, deserialize_with = "deserialize_optional_nonempty_string")]
    pub download_url: Option<String>,
    #[serde(default)]
    pub game_versions: Vec<String>,
    #[serde(default)]
    pub sortable_game_versions: Vec<SortableGameVersion>,
    #[serde(default)]
    pub dependencies: Vec<FileDependency>,
    #[serde(default)]
    pub is_server_pack: Option<bool>,
    #[serde(default)]
    pub server_pack_file_id: Option<u64>,
    #[serde(default)]
    pub is_early_access_content: Option<bool>,
    #[serde(default)]
    pub early_access_end_date: Option<String>,
    #[serde(default)]
    pub file_fingerprint: u64,
}

impl File {
    pub fn sha1(&self) -> Option<&str> {
        self.hashes
            .iter()
            .find(|hash| hash.algorithm == 1)
            .and_then(|hash| nonempty(Some(&hash.value)))
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct FileHash {
    pub value: String,
    #[serde(rename = "algo")]
    pub algorithm: u8,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SortableGameVersion {
    #[serde(default)]
    pub game_version_name: String,
    #[serde(default)]
    pub game_version_padded: String,
    #[serde(default)]
    pub game_version: String,
    #[serde(default)]
    pub game_version_release_date: String,
    pub game_version_type_id: u32,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileDependency {
    pub mod_id: u64,
    #[serde(default)]
    pub file_id: Option<u64>,
    pub relation_type: u8,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileIndex {
    pub game_version: String,
    pub file_id: u64,
    pub filename: String,
    pub release_type: u8,
    #[serde(default)]
    pub game_version_type_id: Option<u32>,
    #[serde(default)]
    pub mod_loader: u8,
}

#[derive(Debug, Clone)]
pub struct ResourceInstallRequest {
    pub project: Project,
    pub file: File,
    pub expected_resource_class: ResourceClass,
    pub primary_destination: PathBuf,
    pub game_directory: PathBuf,
    pub data_pack_directory: PathBuf,
    pub game_version: String,
    pub mod_loader: Option<ModLoader>,
    pub concurrency: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceInstallResult {
    pub primary_destination: PathBuf,
    pub installed_files: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum CurseForgeError {
    #[error("{API_KEY_ENV} is empty or is not a valid HTTP header value")]
    InvalidApiKey,
    #[error("failed to build an HTTP client: {0}")]
    Client(#[from] reqwest::Error),
    #[error("invalid CurseForge API endpoint")]
    InvalidApiEndpoint,
    #[error("invalid CurseForge query: {0}")]
    InvalidQuery(&'static str),
    #[error("CurseForge API returned HTTP {status} for {endpoint}")]
    ApiStatus {
        endpoint: String,
        status: StatusCode,
    },
    #[error(
        "CurseForge refused access (HTTP 403) for {endpoint}; verify that the key has approved third-party API access and that the network can reach CurseForge"
    )]
    ApiForbidden { endpoint: String },
    #[error("CurseForge project {project_id} is unavailable")]
    ProjectUnavailable { project_id: u64 },
    #[error("CurseForge project {project_id} does not permit third-party distribution")]
    DistributionDisabled { project_id: u64 },
    #[error("CurseForge file {file_id} is unavailable")]
    FileUnavailable { file_id: u64 },
    #[error("CurseForge file {file_id} does not belong to project {project_id}")]
    FileProjectMismatch { project_id: u64, file_id: u64 },
    #[error("CurseForge did not provide a download URL for project {project_id}, file {file_id}")]
    DownloadUrlUnavailable { project_id: u64, file_id: u64 },
    #[error("CurseForge returned an invalid or insecure download URL")]
    InvalidDownloadUrl,
    #[error("CurseForge file {file_id} has no SHA-1 hash")]
    MissingSha1 { file_id: u64 },
    #[error("CurseForge file {file_id} has an invalid SHA-1 hash")]
    InvalidSha1 { file_id: u64 },
    #[error(transparent)]
    BatchDownload(#[from] download::DownloadError),
    #[error(
        "CurseForge file {file_id} for project {project_id} is not compatible with Minecraft {game_version}{loader}"
    )]
    IncompatibleFile {
        project_id: u64,
        file_id: u64,
        game_version: String,
        loader: String,
    },
    #[error(
        "CurseForge project {project_id} has no compatible file for Minecraft {game_version}{loader}"
    )]
    NoCompatibleFile {
        project_id: u64,
        game_version: String,
        loader: String,
    },
    #[error(
        "required dependency project {project_id} requested conflicting files {selected_file_id} and {requested_file_id}"
    )]
    DependencyVersionConflict {
        project_id: u64,
        selected_file_id: u64,
        requested_file_id: u64,
    },
    #[error("CurseForge project {project_id} has an unsupported or unknown resource class")]
    UnsupportedResourceClass { project_id: u64 },
    #[error(
        "CurseForge project {project_id} changed resource class while the download dialog was open"
    )]
    ResourceClassMismatch { project_id: u64 },
    #[error("unsafe or empty CurseForge file name for file {file_id}: {file_name:?}")]
    UnsafeFileName { file_id: u64, file_name: String },
    #[error("the resource install destination has no parent directory")]
    InvalidInstallDestination,
    #[error("a modpack cannot be installed as a resource dependency (project {project_id})")]
    NestedModpackDependency { project_id: u64 },
}

pub struct CurseForgeClient {
    api_client: Client,
    download_client: Client,
    api_key: HeaderValue,
}

impl CurseForgeClient {
    /// Creates a client using the API key embedded from the build-time `.env`.
    pub fn from_env() -> Result<Self, CurseForgeError> {
        let raw_key = environment::curseforge_api_key();
        if raw_key.is_empty() {
            return Err(CurseForgeError::InvalidApiKey);
        }
        let mut api_key =
            HeaderValue::from_str(raw_key).map_err(|_| CurseForgeError::InvalidApiKey)?;
        api_key.set_sensitive(true);

        // API redirects are disabled so the credential can never follow a
        // redirect away from the one fixed CurseForge API host.
        let api_client = Client::builder()
            .user_agent("AZULC/0.1.0")
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(30))
            .redirect(Policy::none())
            .build()?;
        // Since July 2026 CurseForge CDN downloads require x-api-key. Keep a
        // separate client whose credential can only reach the forgecdn.net
        // domain family, including across redirects.
        let mut download_headers = HeaderMap::new();
        download_headers.insert(API_KEY_HEADER, api_key.clone());
        let download_client = Client::builder()
            .user_agent("AZULC/0.1.0")
            .connect_timeout(Duration::from_secs(30))
            .default_headers(download_headers)
            .redirect(Policy::custom(|attempt| {
                if attempt.previous().len() < 5 && trusted_download_url(attempt.url()) {
                    attempt.follow()
                } else {
                    attempt.stop()
                }
            }))
            .build()?;

        Ok(Self {
            api_client,
            download_client,
            api_key,
        })
    }

    pub async fn search(&self, query: &SearchQuery) -> Result<SearchPage, CurseForgeError> {
        let parameters = query.parameters()?;
        self.get_api_json("/v1/mods/search", &parameters).await
    }

    /// Returns the CDN-only client used exclusively with URLs already accepted
    /// by `resolve_download_url`; redirects stay within forgecdn.net.
    pub fn download_client(&self) -> Client {
        self.download_client.clone()
    }

    pub async fn list_files(
        &self,
        project_id: u64,
        query: &FileQuery,
    ) -> Result<FilePage, CurseForgeError> {
        let endpoint = format!("/v1/mods/{project_id}/files");
        let parameters = query.parameters()?;
        self.get_api_json(&endpoint, &parameters).await
    }

    pub async fn get_project(&self, project_id: u64) -> Result<Project, CurseForgeError> {
        let endpoint = format!("/v1/mods/{project_id}");
        let response: ApiResponse<Project> = self.get_api_json(&endpoint, &[]).await?;
        Ok(response.data)
    }

    pub async fn get_file(&self, project_id: u64, file_id: u64) -> Result<File, CurseForgeError> {
        let endpoint = format!("/v1/mods/{project_id}/files/{file_id}");
        let response: ApiResponse<File> = self.get_api_json(&endpoint, &[]).await?;
        Ok(response.data)
    }

    pub async fn resolve_download_url(
        &self,
        project: &Project,
        file: &File,
    ) -> Result<Url, CurseForgeError> {
        validate_download(project, file)?;
        if let Some(value) = file.download_url.as_deref() {
            return parse_download_url(value).ok_or(CurseForgeError::InvalidDownloadUrl);
        }

        let endpoint = format!("/v1/mods/{}/files/{}/download-url", project.id, file.id);
        let response = self.api_request(&endpoint, &[]).await?;
        if response.status() == StatusCode::NOT_FOUND {
            return Err(CurseForgeError::DownloadUrlUnavailable {
                project_id: project.id,
                file_id: file.id,
            });
        }
        let response: DownloadUrlResponse = decode_api_response(response, &endpoint).await?;
        let Some(value) = response.data.as_deref() else {
            return Err(CurseForgeError::DownloadUrlUnavailable {
                project_id: project.id,
                file_id: file.id,
            });
        };
        parse_download_url(value).ok_or(CurseForgeError::InvalidDownloadUrl)
    }

    /// Resolves a file referenced by a CurseForge modpack manifest using the
    /// same compatibility behavior as SJMCL. CurseForge may intentionally
    /// omit `downloadUrl` when `allowModDistribution` is false; SJMCL does not
    /// reject that project and instead derives the documented ForgeCDN path
    /// from the file id and encoded file name.
    ///
    /// Keep this scoped to modpack imports. Interactive resource installs use
    /// `resolve_download_url`, which continues to enforce distribution flags.
    pub fn resolve_modpack_file_url(
        &self,
        project: &Project,
        file: &File,
    ) -> Result<Url, CurseForgeError> {
        if file.mod_id != project.id {
            return Err(CurseForgeError::FileProjectMismatch {
                project_id: project.id,
                file_id: file.id,
            });
        }
        if let Some(value) = file.download_url.as_deref() {
            return parse_download_url(value).ok_or(CurseForgeError::InvalidDownloadUrl);
        }
        forgecdn_fallback_url(file)
    }

    /// Installs one CurseForge resource and every recursively required
    /// dependency (`relationType == 3`). The entire graph is resolved and
    /// validated before the first destination is touched, so a missing or
    /// non-distributable dependency cannot leave a silently incomplete set.
    /// Every individual file is checksum-verified and atomically replaced by
    /// the generic downloader using this client's credential-free CDN client.
    pub async fn install_resource_with_dependencies(
        &self,
        request: ResourceInstallRequest,
    ) -> Result<ResourceInstallResult, CurseForgeError> {
        if request.project.id != request.file.mod_id {
            return Err(CurseForgeError::FileProjectMismatch {
                project_id: request.project.id,
                file_id: request.file.id,
            });
        }

        let root_project_id = request.project.id;
        let root_file_id = request.file.id;
        let mut queue = VecDeque::from([PendingResource {
            project_id: root_project_id,
            file_id: Some(root_file_id),
            root: true,
        }]);
        let mut selected = HashMap::<u64, SelectedResource>::new();
        let mut downloads = Vec::new();
        let mut primary_destination = None;

        while let Some(pending) = queue.pop_front() {
            let requested_file_id = normalized_dependency_file_id(pending.file_id);
            let Some(replace_download) =
                selection_action(&selected, pending.project_id, requested_file_id)?
            else {
                continue;
            };

            // Always fetch current metadata, including for the root selected in
            // the UI. Availability and distribution policy may have changed.
            let project = self.get_project(pending.project_id).await?;
            let class =
                project
                    .resource_class()
                    .ok_or(CurseForgeError::UnsupportedResourceClass {
                        project_id: project.id,
                    })?;
            validate_project_download(&project)?;
            if pending.root && class != request.expected_resource_class {
                return Err(CurseForgeError::ResourceClassMismatch {
                    project_id: project.id,
                });
            }
            if class == ResourceClass::Modpack {
                return Err(CurseForgeError::NestedModpackDependency {
                    project_id: project.id,
                });
            }
            let file = match requested_file_id {
                Some(file_id) => {
                    let file = self.get_file(project.id, file_id).await?;
                    ensure_file_compatible(
                        &project,
                        &file,
                        &request.game_version,
                        request.mod_loader,
                    )?;
                    file
                }
                None => {
                    self.find_compatible_file(&project, &request.game_version, request.mod_loader)
                        .await?
                }
            };

            // Resolve every URL before starting the batch. This is the policy
            // gate for project/file availability, ownership, and the author's
            // allowModDistribution setting. Deliberately do not guess a CDN URL.
            let url = self.resolve_download_url(&project, &file).await?;
            let sha1 = normalized_sha1(&file)?;
            let file_name = safe_file_name(&file)?;
            let destination = if pending.root {
                let parent = request
                    .primary_destination
                    .parent()
                    .filter(|parent| !parent.as_os_str().is_empty())
                    .ok_or(CurseForgeError::InvalidInstallDestination)?;
                parent.join(&file_name)
            } else {
                dependency_destination(
                    class,
                    project.id,
                    &file_name,
                    &request.game_directory,
                    &request.data_pack_directory,
                )?
            };
            if pending.root {
                primary_destination = Some(destination.clone());
            }

            let download = DownloadSpec {
                urls: vec![url.to_string()],
                destination,
                size: file.file_length,
                sha1: Some(sha1),
                sha512: None,
                label: file_name,
            };
            let download_index = if let Some(index) = replace_download {
                downloads[index] = download;
                index
            } else {
                downloads.push(download);
                downloads.len() - 1
            };
            selected.insert(
                project.id,
                SelectedResource {
                    file_id: file.id,
                    pinned: requested_file_id.is_some(),
                    download_index,
                },
            );
            queue.extend(
                required_dependencies(&file).map(|dependency| PendingResource {
                    project_id: dependency.mod_id,
                    file_id: normalized_dependency_file_id(dependency.file_id),
                    root: false,
                }),
            );
        }

        let installed_files = downloads.len();
        // `download_client` carries x-api-key only to validated forgecdn.net
        // URLs and cannot follow a redirect to another host.
        download::download_batch(
            self.download_client.clone(),
            downloads,
            request.concurrency,
            |_| {},
        )
        .await?;
        Ok(ResourceInstallResult {
            primary_destination: primary_destination
                .ok_or(CurseForgeError::InvalidInstallDestination)?,
            installed_files,
        })
    }

    async fn find_compatible_file(
        &self,
        project: &Project,
        game_version: &str,
        mod_loader: Option<ModLoader>,
    ) -> Result<File, CurseForgeError> {
        let mut query = FileQuery {
            game_version: Some(game_version.to_owned()),
            mod_loader: (project.resource_class() == Some(ResourceClass::Mod))
                .then_some(mod_loader)
                .flatten(),
            ..FileQuery::default()
        };

        loop {
            let page = self.list_files(project.id, &query).await?;
            if let Some(file) = page.files.into_iter().find(|file| {
                file.is_available
                    && file.mod_id == project.id
                    && file_is_compatible(project, file, game_version, mod_loader)
            }) {
                return Ok(file);
            }
            let Some(next) = page.pagination.next_index() else {
                break;
            };
            query.index = next;
        }
        Err(no_compatible_file_error(
            project.id,
            game_version,
            mod_loader,
        ))
    }

    async fn get_api_json<T: DeserializeOwned>(
        &self,
        endpoint: &str,
        parameters: &[(&str, String)],
    ) -> Result<T, CurseForgeError> {
        let response = self.api_request(endpoint, parameters).await?;
        decode_api_response(response, endpoint).await
    }

    async fn api_request(
        &self,
        endpoint: &str,
        parameters: &[(&str, String)],
    ) -> Result<reqwest::Response, CurseForgeError> {
        let url = api_request_url(endpoint, parameters)?;
        Ok(self
            .api_client
            .get(url)
            .header(ACCEPT, HeaderValue::from_static("application/json"))
            .header(API_KEY_HEADER, self.api_key.clone())
            .send()
            .await?)
    }
}

fn api_request_url(endpoint: &str, parameters: &[(&str, String)]) -> Result<Url, CurseForgeError> {
    let mut url = api_url(endpoint)?;
    // Calling `query_pairs_mut` by itself marks a URL as having a query and
    // serializes an empty parameter set as a trailing `?`. Match SJMCL's
    // request split: detail endpoints are plain GETs; only actual search/file
    // filters receive a query string.
    if !parameters.is_empty() {
        {
            let mut query = url.query_pairs_mut();
            for (name, value) in parameters {
                query.append_pair(name, value);
            }
        }
    }
    Ok(url)
}

const REQUIRED_DEPENDENCY_RELATION_TYPE: u8 = 3;

#[derive(Debug, Clone, Copy)]
struct PendingResource {
    project_id: u64,
    file_id: Option<u64>,
    root: bool,
}

#[derive(Debug, Clone, Copy)]
struct SelectedResource {
    file_id: u64,
    pinned: bool,
    download_index: usize,
}

fn selection_action(
    selected: &HashMap<u64, SelectedResource>,
    project_id: u64,
    requested_file_id: Option<u64>,
) -> Result<Option<Option<usize>>, CurseForgeError> {
    let Some(selected) = selected.get(&project_id) else {
        return Ok(Some(None));
    };
    if let Some(requested_file_id) = requested_file_id
        && requested_file_id != selected.file_id
    {
        if !selected.pinned {
            return Ok(Some(Some(selected.download_index)));
        }
        return Err(CurseForgeError::DependencyVersionConflict {
            project_id,
            selected_file_id: selected.file_id,
            requested_file_id,
        });
    }
    Ok(None)
}

fn normalized_dependency_file_id(file_id: Option<u64>) -> Option<u64> {
    file_id.filter(|file_id| *file_id != 0)
}

fn required_dependencies(file: &File) -> impl Iterator<Item = &FileDependency> {
    file.dependencies
        .iter()
        .filter(|dependency| dependency.relation_type == REQUIRED_DEPENDENCY_RELATION_TYPE)
}

fn ensure_file_compatible(
    project: &Project,
    file: &File,
    game_version: &str,
    mod_loader: Option<ModLoader>,
) -> Result<(), CurseForgeError> {
    if file_is_compatible(project, file, game_version, mod_loader) {
        Ok(())
    } else {
        Err(CurseForgeError::IncompatibleFile {
            project_id: project.id,
            file_id: file.id,
            game_version: game_version.to_owned(),
            loader: loader_suffix(mod_loader),
        })
    }
}

fn file_is_compatible(
    project: &Project,
    file: &File,
    game_version: &str,
    mod_loader: Option<ModLoader>,
) -> bool {
    let game_matches = file
        .game_versions
        .iter()
        .any(|version| version.eq_ignore_ascii_case(game_version))
        || file.sortable_game_versions.iter().any(|version| {
            version.game_version_name.eq_ignore_ascii_case(game_version)
                || version.game_version.eq_ignore_ascii_case(game_version)
        });
    if !game_matches {
        return false;
    }
    if project.resource_class() != Some(ResourceClass::Mod) {
        return true;
    }

    let declared = file
        .game_versions
        .iter()
        .filter_map(|version| declared_loader(version))
        .collect::<Vec<_>>();
    match mod_loader {
        Some(ModLoader::Any) => true,
        Some(loader) => declared.is_empty() || declared.contains(&loader),
        None => declared.is_empty(),
    }
}

fn declared_loader(value: &str) -> Option<ModLoader> {
    match value.trim().to_ascii_lowercase().as_str() {
        "forge" => Some(ModLoader::Forge),
        "fabric" => Some(ModLoader::Fabric),
        "neoforge" => Some(ModLoader::NeoForge),
        "quilt" => Some(ModLoader::Quilt),
        "cauldron" => Some(ModLoader::Cauldron),
        "liteloader" | "lite loader" => Some(ModLoader::LiteLoader),
        _ => None,
    }
}

fn loader_suffix(loader: Option<ModLoader>) -> String {
    let label = match loader {
        None | Some(ModLoader::Any) => return String::new(),
        Some(ModLoader::Forge) => "Forge",
        Some(ModLoader::Fabric) => "Fabric",
        Some(ModLoader::NeoForge) => "NeoForge",
        Some(ModLoader::Quilt) => "Quilt",
        Some(ModLoader::Cauldron) => "Cauldron",
        Some(ModLoader::LiteLoader) => "LiteLoader",
    };
    format!(" with {label}")
}

fn no_compatible_file_error(
    project_id: u64,
    game_version: &str,
    mod_loader: Option<ModLoader>,
) -> CurseForgeError {
    CurseForgeError::NoCompatibleFile {
        project_id,
        game_version: game_version.to_owned(),
        loader: loader_suffix(mod_loader),
    }
}

fn safe_file_name(file: &File) -> Result<String, CurseForgeError> {
    path_safety::file_name(&file.file_name).ok_or_else(|| CurseForgeError::UnsafeFileName {
        file_id: file.id,
        file_name: file.file_name.clone(),
    })
}

fn dependency_destination(
    class: ResourceClass,
    project_id: u64,
    file_name: &str,
    game_directory: &Path,
    data_pack_directory: &Path,
) -> Result<PathBuf, CurseForgeError> {
    let directory = match class {
        ResourceClass::Mod => game_directory.join("mods"),
        ResourceClass::ResourcePack => game_directory.join("resourcepacks"),
        ResourceClass::World => game_directory.join("saves"),
        ResourceClass::ShaderPack => game_directory.join("shaderpacks"),
        ResourceClass::DataPack => data_pack_directory.to_path_buf(),
        ResourceClass::Modpack => {
            return Err(CurseForgeError::NestedModpackDependency { project_id });
        }
    };
    Ok(directory.join(file_name))
}

#[derive(Deserialize)]
struct ApiResponse<T> {
    data: T,
}

#[derive(Deserialize)]
struct DownloadUrlResponse {
    #[serde(default, deserialize_with = "deserialize_optional_nonempty_string")]
    data: Option<String>,
}

async fn decode_api_response<T: DeserializeOwned>(
    response: reqwest::Response,
    endpoint: &str,
) -> Result<T, CurseForgeError> {
    if response.status() == StatusCode::FORBIDDEN {
        return Err(CurseForgeError::ApiForbidden {
            endpoint: endpoint.to_string(),
        });
    }
    if !response.status().is_success() {
        return Err(CurseForgeError::ApiStatus {
            endpoint: endpoint.to_string(),
            status: response.status(),
        });
    }
    Ok(response.json().await?)
}

fn api_url(endpoint: &str) -> Result<Url, CurseForgeError> {
    let relative = endpoint
        .strip_prefix('/')
        .filter(|value| !value.contains("://"))
        .ok_or(CurseForgeError::InvalidApiEndpoint)?;
    let base = Url::parse(API_BASE).map_err(|_| CurseForgeError::InvalidApiEndpoint)?;
    let url = base
        .join(relative)
        .map_err(|_| CurseForgeError::InvalidApiEndpoint)?;
    if url.scheme() != "https"
        || url.host_str() != Some(API_HOST)
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(CurseForgeError::InvalidApiEndpoint);
    }
    Ok(url)
}

fn validate_pagination(index: u32, page_size: u32) -> Result<(), CurseForgeError> {
    if page_size == 0 || page_size > MAX_PAGE_SIZE {
        return Err(CurseForgeError::InvalidQuery(
            "pageSize must be between 1 and 50",
        ));
    }
    if index
        .checked_add(page_size)
        .is_none_or(|end| end > MAX_RESULT_OFFSET)
    {
        return Err(CurseForgeError::InvalidQuery(
            "index + pageSize must not exceed 10000",
        ));
    }
    Ok(())
}

fn validate_download(project: &Project, file: &File) -> Result<(), CurseForgeError> {
    validate_project_download(project)?;
    if file.mod_id != project.id {
        return Err(CurseForgeError::FileProjectMismatch {
            project_id: project.id,
            file_id: file.id,
        });
    }
    if !file.is_available {
        return Err(CurseForgeError::FileUnavailable { file_id: file.id });
    }
    Ok(())
}

fn validate_project_download(project: &Project) -> Result<(), CurseForgeError> {
    if !project.is_available {
        return Err(CurseForgeError::ProjectUnavailable {
            project_id: project.id,
        });
    }
    if project.allow_mod_distribution == Some(false) {
        return Err(CurseForgeError::DistributionDisabled {
            project_id: project.id,
        });
    }
    Ok(())
}

fn parse_download_url(value: &str) -> Option<Url> {
    let url = Url::parse(value.trim()).ok()?;
    trusted_download_url(&url).then_some(url)
}

fn forgecdn_fallback_url(file: &File) -> Result<Url, CurseForgeError> {
    let file_name = safe_file_name(file)?;
    let high = (file.id / 1000).to_string();
    let low = (file.id % 1000).to_string();
    let mut url = Url::parse("https://edge.forgecdn.net/")
        .map_err(|_| CurseForgeError::InvalidDownloadUrl)?;
    url.path_segments_mut()
        .map_err(|_| CurseForgeError::InvalidDownloadUrl)?
        .extend(["files", &high, &low, &file_name]);
    trusted_download_url(&url)
        .then_some(url)
        .ok_or(CurseForgeError::InvalidDownloadUrl)
}

fn trusted_download_url(url: &Url) -> bool {
    let trusted_host = url.host_str().is_some_and(|host| {
        host.eq_ignore_ascii_case("forgecdn.net")
            || host.to_ascii_lowercase().ends_with(".forgecdn.net")
    });
    url.scheme() == "https"
        && trusted_host
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
}

fn normalized_sha1(file: &File) -> Result<String, CurseForgeError> {
    let value = file
        .sha1()
        .ok_or(CurseForgeError::MissingSha1 { file_id: file.id })?
        .trim();
    integrity::normalized_hex::<20>(value).ok_or(CurseForgeError::InvalidSha1 { file_id: file.id })
}

fn nonempty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn deserialize_optional_nonempty_string<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<String>::deserialize(deserializer)?;
    Ok(value.and_then(|value| {
        let value = value.trim();
        (!value.is_empty()).then(|| value.to_string())
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_client() -> CurseForgeClient {
        CurseForgeClient {
            api_client: Client::new(),
            download_client: Client::new(),
            api_key: HeaderValue::from_static("test-key"),
        }
    }

    const SEARCH_FIXTURE: &str = r#"
    {
      "data": [{
        "id": 1234,
        "gameId": 432,
        "name": "Fixture Pack",
        "slug": "fixture-pack",
        "summary": "A local fixture",
        "downloadCount": 99,
        "classId": 4471,
        "allowModDistribution": true,
        "isAvailable": true,
        "latestFiles": [{
          "id": 5678,
          "gameId": 432,
          "modId": 1234,
          "isAvailable": true,
          "displayName": "Fixture Pack 1.0",
          "fileName": "fixture-pack.zip",
          "releaseType": 1,
          "fileStatus": 4,
          "fileLength": 3,
          "downloadUrl": "",
          "gameVersions": ["1.21.1"],
          "hashes": [{"value": "a9993e364706816aba3e25717850c26c9cd0d89d", "algo": 1}]
        }],
        "latestFilesIndexes": [{
          "gameVersion": "1.7.10",
          "fileId": 5678,
          "filename": "fixture-pack.zip",
          "releaseType": 1,
          "gameVersionTypeId": 4
        }]
      }],
      "pagination": {
        "index": 0,
        "pageSize": 50,
        "resultCount": 1,
        "totalCount": 1
      }
    }
    "#;

    #[test]
    fn deserializes_camel_case_fixture_and_normalizes_empty_url() {
        let page: SearchPage = serde_json::from_str(SEARCH_FIXTURE).expect("valid fixture");
        assert_eq!(page.pagination.page_size, 50);
        assert_eq!(page.projects[0].game_id, MINECRAFT_GAME_ID as u64);
        assert_eq!(
            page.projects[0].resource_class(),
            Some(ResourceClass::Modpack)
        );
        let file = &page.projects[0].latest_files[0];
        assert_eq!(file.file_name, "fixture-pack.zip");
        assert_eq!(file.download_url, None);
        assert_eq!(page.projects[0].latest_files_indexes[0].mod_loader, 0);
    }

    #[test]
    fn serializes_loader_as_the_documented_numeric_id() {
        let mut query = SearchQuery::new(ResourceClass::Mod);
        query.game_version = Some("1.21.1".into());
        query.mod_loader = Some(ModLoader::NeoForge);
        let parameters = query.parameters().expect("valid query");
        assert!(parameters.contains(&("modLoaderType", "6".into())));
        assert!(parameters.contains(&("classId", "6".into())));
    }

    #[test]
    fn rejects_a_loader_without_a_game_version() {
        let mut query = SearchQuery::new(ResourceClass::Mod);
        query.mod_loader = Some(ModLoader::Fabric);
        assert!(matches!(
            query.parameters(),
            Err(CurseForgeError::InvalidQuery(_))
        ));
    }

    #[test]
    fn treats_any_loader_as_no_filter() {
        let mut query = SearchQuery::new(ResourceClass::Mod);
        query.mod_loader = Some(ModLoader::Any);
        let parameters = query.parameters().expect("Any needs no version");
        assert!(!parameters.iter().any(|(name, _)| *name == "modLoaderType"));
    }

    #[test]
    fn rejects_projects_that_disable_distribution() {
        let mut page: SearchPage = serde_json::from_str(SEARCH_FIXTURE).expect("valid fixture");
        let mut project = page.projects.remove(0);
        let file = project.latest_files[0].clone();
        project.allow_mod_distribution = Some(false);
        assert!(matches!(
            validate_download(&project, &file),
            Err(CurseForgeError::DistributionDisabled { project_id: 1234 })
        ));
    }

    #[test]
    fn modpack_compatibility_uses_api_url_even_when_distribution_is_disabled() {
        let mut page: SearchPage = serde_json::from_str(SEARCH_FIXTURE).expect("valid fixture");
        let mut project = page.projects.remove(0);
        project.allow_mod_distribution = Some(false);
        let mut file = project.latest_files.remove(0);
        file.download_url =
            Some("https://media.forgecdn.net/files/5/678/fixture-pack.zip".to_owned());

        assert!(matches!(
            validate_download(&project, &file),
            Err(CurseForgeError::DistributionDisabled { .. })
        ));
        assert_eq!(
            fixture_client()
                .resolve_modpack_file_url(&project, &file)
                .expect("SJMCL-compatible URL")
                .as_str(),
            "https://media.forgecdn.net/files/5/678/fixture-pack.zip"
        );
    }

    #[test]
    fn modpack_compatibility_derives_forgecdn_url_when_api_omits_it() {
        let mut page: SearchPage = serde_json::from_str(SEARCH_FIXTURE).expect("valid fixture");
        let mut project = page.projects.remove(0);
        project.id = 348_025;
        project.allow_mod_distribution = Some(false);
        let mut file = project.latest_files.remove(0);
        file.mod_id = project.id;
        file.id = 5_370_258;
        file.file_name = "SRParasites-1.12.2v1.9.21.jar".into();
        file.download_url = None;

        let url = fixture_client()
            .resolve_modpack_file_url(&project, &file)
            .expect("derived ForgeCDN URL");
        assert_eq!(url.host_str(), Some("edge.forgecdn.net"));
        assert_eq!(
            url.path_segments().unwrap().collect::<Vec<_>>(),
            vec!["files", "5370", "258", "SRParasites-1.12.2v1.9.21.jar"]
        );
        assert!(!url.as_str().contains(' '));
    }

    #[test]
    fn reads_sha1_from_fixture_data() {
        let page: SearchPage = serde_json::from_str(SEARCH_FIXTURE).expect("valid fixture");
        assert_eq!(
            page.projects[0].latest_files[0].sha1(),
            Some("a9993e364706816aba3e25717850c26c9cd0d89d")
        );
    }

    #[test]
    fn builds_only_the_fixed_api_host() {
        let url = api_url("/v1/mods/search").expect("fixed endpoint");
        assert_eq!(url.as_str(), "https://api.curseforge.com/v1/mods/search");
        assert!(api_url("https://example.com/steal").is_err());
    }

    #[test]
    fn detail_request_urls_do_not_gain_an_empty_query() {
        let project = api_request_url("/v1/mods/224770", &[]).expect("project URL");
        assert_eq!(
            project.as_str(),
            "https://api.curseforge.com/v1/mods/224770"
        );
        assert_eq!(project.query(), None);

        let file = api_request_url("/v1/mods/224770/files/123", &[]).expect("file URL");
        assert_eq!(
            file.as_str(),
            "https://api.curseforge.com/v1/mods/224770/files/123"
        );
        assert_eq!(file.query(), None);
    }

    #[test]
    fn filtered_request_urls_encode_only_real_query_pairs() {
        let parameters = vec![
            ("gameVersion", "1.20.1".to_owned()),
            ("searchFilter", "JEI tools".to_owned()),
        ];
        let url = api_request_url("/v1/mods/search", &parameters).expect("search URL");
        assert_eq!(url.path(), "/v1/mods/search");
        assert_eq!(
            url.query_pairs().collect::<Vec<_>>(),
            vec![
                ("gameVersion".into(), "1.20.1".into()),
                ("searchFilter".into(), "JEI tools".into())
            ]
        );
        assert!(!url.as_str().ends_with('?'));
    }

    #[test]
    fn download_credentials_are_scoped_to_curseforge_cdn_hosts() {
        assert!(parse_download_url("https://edge.forgecdn.net/files/1/2/mod.jar").is_some());
        assert!(parse_download_url("https://media.forgecdn.net/files/1/2/mod.jar").is_some());
        assert!(parse_download_url("https://forgecdn.net/files/1/2/mod.jar").is_some());
        assert!(parse_download_url("https://forgecdn.net.example.com/mod.jar").is_none());
        assert!(parse_download_url("https://example.com/mod.jar").is_none());
    }

    #[test]
    fn selects_only_required_dependency_edges() {
        let page: SearchPage = serde_json::from_str(SEARCH_FIXTURE).expect("valid fixture");
        let mut file = page.projects[0].latest_files[0].clone();
        file.dependencies = vec![
            FileDependency {
                mod_id: 10,
                file_id: Some(100),
                relation_type: 3,
            },
            FileDependency {
                mod_id: 20,
                file_id: Some(200),
                relation_type: 2,
            },
            FileDependency {
                mod_id: 30,
                file_id: None,
                relation_type: 3,
            },
        ];
        let required = required_dependencies(&file)
            .map(|dependency| (dependency.mod_id, dependency.file_id))
            .collect::<Vec<_>>();
        assert_eq!(required, vec![(10, Some(100)), (30, None)]);
    }

    #[test]
    fn dependency_cycles_deduplicate_and_version_conflicts_fail() {
        let pinned = SelectedResource {
            file_id: 100,
            pinned: true,
            download_index: 0,
        };
        let selected = HashMap::from([(42, pinned)]);
        assert_eq!(selection_action(&selected, 42, None).unwrap(), None);
        assert_eq!(selection_action(&selected, 42, Some(100)).unwrap(), None);
        assert!(matches!(
            selection_action(&selected, 42, Some(101)),
            Err(CurseForgeError::DependencyVersionConflict {
                project_id: 42,
                selected_file_id: 100,
                requested_file_id: 101,
            })
        ));
        assert_eq!(selection_action(&selected, 7, None).unwrap(), Some(None));

        let automatically_selected = HashMap::from([(
            42,
            SelectedResource {
                pinned: false,
                ..pinned
            },
        )]);
        assert_eq!(
            selection_action(&automatically_selected, 42, Some(101)).unwrap(),
            Some(Some(0))
        );
        assert_eq!(normalized_dependency_file_id(Some(0)), None);
    }

    #[test]
    fn mod_compatibility_requires_game_and_declared_loader() {
        let mut page: SearchPage = serde_json::from_str(SEARCH_FIXTURE).expect("valid fixture");
        let mut project = page.projects.remove(0);
        project.class_id = Some(u64::from(ResourceClass::Mod.id()));
        let mut file = project.latest_files.remove(0);
        file.game_versions = vec!["1.21.1".into(), "Fabric".into()];

        assert!(file_is_compatible(
            &project,
            &file,
            "1.21.1",
            Some(ModLoader::Fabric)
        ));
        assert!(!file_is_compatible(
            &project,
            &file,
            "1.21.1",
            Some(ModLoader::Forge)
        ));
        assert!(!file_is_compatible(
            &project,
            &file,
            "1.20.1",
            Some(ModLoader::Fabric)
        ));
        assert!(!file_is_compatible(&project, &file, "1.21.1", None));
    }

    #[test]
    fn dependency_destinations_stay_in_the_expected_instance_tree() {
        let root = Path::new("instance");
        let data_packs = root.join("datapacks");
        assert_eq!(
            dependency_destination(ResourceClass::Mod, 1, "a.jar", root, &data_packs).unwrap(),
            root.join("mods/a.jar")
        );
        assert_eq!(
            dependency_destination(ResourceClass::ResourcePack, 1, "a.zip", root, &data_packs)
                .unwrap(),
            root.join("resourcepacks/a.zip")
        );
        assert_eq!(
            dependency_destination(ResourceClass::DataPack, 1, "a.zip", root, &data_packs).unwrap(),
            root.join("datapacks/a.zip")
        );
        assert!(matches!(
            dependency_destination(ResourceClass::Modpack, 1, "a.zip", root, &data_packs),
            Err(CurseForgeError::NestedModpackDependency { project_id: 1 })
        ));
    }

    #[test]
    fn resource_file_name_must_be_a_single_component() {
        let page: SearchPage = serde_json::from_str(SEARCH_FIXTURE).expect("valid fixture");
        let mut file = page.projects[0].latest_files[0].clone();
        file.file_name = "mod.jar".into();
        assert_eq!(safe_file_name(&file).unwrap(), "mod.jar");
        file.file_name = "../mod.jar".into();
        assert!(safe_file_name(&file).is_err());
        file.file_name = r"mods\mod.jar".into();
        assert!(safe_file_name(&file).is_err());
    }
}
