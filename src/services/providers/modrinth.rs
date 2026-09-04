//! Modrinth API models and client operations.

use crate::{
    domain::LoaderKind,
    services::{
        download::{DownloadSpec, integrity},
        path_safety,
    },
};
use chrono::{DateTime, FixedOffset};
use reqwest::{
    Client, StatusCode, Url,
    header::{ACCEPT, CONTENT_TYPE},
    redirect::Policy,
};
use serde::{Deserialize, de::DeserializeOwned};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    fmt,
    path::Path,
    time::Duration,
};

pub const DEFAULT_USER_AGENT: &str = concat!("AZULC/", env!("CARGO_PKG_VERSION"));

const API_BASE: &str = "https://api.modrinth.com/v2/";
const API_HOST: &str = "api.modrinth.com";
const CDN_HOST: &str = "cdn.modrinth.com";
const MAX_SEARCH_LIMIT: u32 = 100;

/// A Modrinth project/content type.
///
/// `DataPack` projects are searched through Modrinth's `all_project_types`
/// facet because a project may publish both mod and data-pack versions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContentType {
    Modpack,
    Mod,
    #[serde(rename = "resourcepack")]
    ResourcePack,
    Shader,
    #[serde(rename = "datapack")]
    DataPack,
    Plugin,
    #[serde(other)]
    Unknown,
}

impl ContentType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Modpack => "modpack",
            Self::Mod => "mod",
            Self::ResourcePack => "resourcepack",
            Self::Shader => "shader",
            Self::DataPack => "datapack",
            Self::Plugin => "plugin",
            Self::Unknown => "unknown",
        }
    }

    fn search_facet(self) -> Result<&'static str, ModrinthError> {
        match self {
            Self::Modpack | Self::Mod | Self::ResourcePack | Self::Shader => Ok("project_type"),
            Self::DataPack => Ok("all_project_types"),
            Self::Plugin | Self::Unknown => Err(ModrinthError::InvalidQuery(
                "only modpacks, mods, resource packs, shaders, and data packs are searchable",
            )),
        }
    }
}

impl fmt::Display for ContentType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Loader {
    Minecraft,
    Fabric,
    Forge,
    NeoForge,
    DataPack,
}

impl Loader {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Minecraft => "minecraft",
            Self::Fabric => "fabric",
            Self::Forge => "forge",
            Self::NeoForge => "neoforge",
            Self::DataPack => "datapack",
        }
    }
}

impl From<LoaderKind> for Loader {
    fn from(loader: LoaderKind) -> Self {
        match loader {
            LoaderKind::Vanilla => Self::Minecraft,
            LoaderKind::Fabric => Self::Fabric,
            LoaderKind::Forge => Self::Forge,
            LoaderKind::NeoForge => Self::NeoForge,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum SearchSort {
    #[default]
    Relevance,
    Downloads,
}

impl SearchSort {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Relevance => "relevance",
            Self::Downloads => "downloads",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchQuery {
    pub content_type: ContentType,
    pub query: String,
    pub game_version: Option<String>,
    pub loader: Option<Loader>,
    pub sort: SearchSort,
    pub offset: u32,
    pub limit: u32,
}

impl SearchQuery {
    pub fn new(content_type: ContentType) -> Self {
        Self {
            content_type,
            query: String::new(),
            game_version: None,
            loader: None,
            sort: SearchSort::Relevance,
            offset: 0,
            limit: 20,
        }
    }

    fn parameters(&self) -> Result<Vec<(&'static str, String)>, ModrinthError> {
        validate_search_pagination(self.offset, self.limit)?;
        let mut facets = vec![vec![format!(
            "{}:{}",
            self.content_type.search_facet()?,
            self.content_type.as_str()
        )]];
        if let Some(version) = normalized_filter(self.game_version.as_deref(), "game version")? {
            facets.push(vec![format!("versions:{version}")]);
        }
        if let Some(loader) = self.loader {
            facets.push(vec![format!("categories:{}", loader.as_str())]);
        }
        let facets = serde_json::to_string(&facets)
            .map_err(|_| ModrinthError::InvalidQuery("could not encode search facets"))?;

        let mut parameters = vec![
            ("facets", facets),
            ("index", self.sort.as_str().to_owned()),
            ("offset", self.offset.to_string()),
            ("limit", self.limit.to_string()),
        ];
        if !self.query.trim().is_empty() {
            parameters.push(("query", self.query.trim().to_owned()));
        }
        Ok(parameters)
    }
}

impl Default for SearchQuery {
    fn default() -> Self {
        Self::new(ContentType::Mod)
    }
}

/// The public project shape shared by search hits and `/project/{id}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    pub id: String,
    pub slug: Option<String>,
    pub title: String,
    pub description: String,
    pub author: Option<String>,
    pub project_type: ContentType,
    pub project_types: Vec<ContentType>,
    pub categories: Vec<String>,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    pub downloads: u64,
    pub followers: u64,
    pub icon_url: Option<String>,
    pub updated: String,
    pub version_ids: Vec<String>,
    pub latest_version: Option<String>,
}

impl Project {
    /// Hybrid projects can publish data-pack versions while their primary
    /// project type remains `mod`. The version loader is authoritative there.
    pub fn content_type_for(&self, version: &Version) -> ContentType {
        if version
            .loaders
            .iter()
            .any(|loader| loader.eq_ignore_ascii_case("datapack"))
        {
            ContentType::DataPack
        } else {
            self.project_type
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchPage {
    pub hits: Vec<Project>,
    pub offset: u32,
    pub limit: u32,
    pub total_hits: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VersionQuery {
    pub game_versions: Vec<String>,
    pub loaders: Vec<Loader>,
    pub featured: Option<bool>,
}

impl VersionQuery {
    pub fn compatible(game_version: impl Into<String>, loader: Option<Loader>) -> Self {
        Self {
            game_versions: vec![game_version.into()],
            loaders: loader.into_iter().collect(),
            featured: None,
        }
    }

    fn parameters(&self) -> Result<Vec<(&'static str, String)>, ModrinthError> {
        let game_versions = normalized_filters(&self.game_versions, "game version")?;
        let mut parameters = vec![("include_changelog", "false".to_owned())];
        if !game_versions.is_empty() {
            parameters.push((
                "game_versions",
                serde_json::to_string(&game_versions).map_err(|_| {
                    ModrinthError::InvalidQuery("could not encode game version filters")
                })?,
            ));
        }
        if !self.loaders.is_empty() {
            let loaders = self
                .loaders
                .iter()
                .map(|loader| loader.as_str())
                .collect::<Vec<_>>();
            parameters.push((
                "loaders",
                serde_json::to_string(&loaders)
                    .map_err(|_| ModrinthError::InvalidQuery("could not encode loader filters"))?,
            ));
        }
        if let Some(featured) = self.featured {
            parameters.push(("featured", featured.to_string()));
        }
        Ok(parameters)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VersionType {
    Release,
    Beta,
    Alpha,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VersionStatus {
    Listed,
    Archived,
    Draft,
    Unlisted,
    Scheduled,
    #[serde(other)]
    Unknown,
}

impl VersionStatus {
    fn installable(self) -> bool {
        matches!(self, Self::Listed | Self::Archived | Self::Unlisted)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DependencyType {
    Required,
    Optional,
    Incompatible,
    Embedded,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Dependency {
    #[serde(default)]
    pub version_id: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub file_name: Option<String>,
    pub dependency_type: DependencyType,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct FileHashes {
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub sha512: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct VersionFile {
    #[serde(default)]
    pub id: Option<String>,
    pub hashes: FileHashes,
    pub url: String,
    pub filename: String,
    pub primary: bool,
    pub size: u64,
    #[serde(default)]
    pub file_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Version {
    pub id: String,
    pub project_id: String,
    pub author_id: String,
    pub name: String,
    pub version_number: String,
    #[serde(default)]
    pub changelog: Option<String>,
    #[serde(default)]
    pub dependencies: Vec<Dependency>,
    #[serde(default)]
    pub game_versions: Vec<String>,
    pub version_type: VersionType,
    #[serde(default)]
    pub loaders: Vec<String>,
    #[serde(default)]
    pub featured: bool,
    pub status: VersionStatus,
    pub date_published: String,
    pub downloads: u64,
    #[serde(default)]
    pub environment: Option<String>,
    #[serde(default)]
    pub files: Vec<VersionFile>,
}

impl Version {
    pub fn required_dependencies(&self) -> impl Iterator<Item = &Dependency> {
        self.dependencies
            .iter()
            .filter(|dependency| dependency.dependency_type == DependencyType::Required)
    }

    pub fn primary_file(&self) -> Option<&VersionFile> {
        self.files
            .iter()
            .find(|file| file.primary)
            .or_else(|| self.files.first())
    }

    pub fn install_plan(&self) -> Result<InstallPlan, ModrinthError> {
        let file = self
            .primary_file()
            .ok_or_else(|| ModrinthError::NoVersionFiles {
                version_id: self.id.clone(),
            })?;
        install_plan(self, file)
    }
}

/// Validated metadata for one Modrinth CDN file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallPlan {
    pub project_id: String,
    pub version_id: String,
    pub url: String,
    pub file_name: String,
    pub sha1: Option<String>,
    pub sha512: Option<String>,
    pub size: u64,
}

impl InstallPlan {
    /// Converts the provider plan into the launcher's checksum-verifying
    /// downloader format. `file_name` is validated before this type is built.
    pub fn download_spec(&self, destination_directory: impl AsRef<Path>) -> DownloadSpec {
        DownloadSpec {
            urls: vec![self.url.clone()],
            destination: destination_directory.as_ref().join(&self.file_name),
            size: self.size,
            sha1: self.sha1.clone(),
            sha512: self.sha512.clone(),
            label: self.file_name.clone(),
        }
    }

    pub fn is_mrpack(&self) -> bool {
        Path::new(&self.file_name)
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("mrpack"))
    }
}

/// A project, its selected version, and its primary file. Dependency graph
/// results always place the requested root project at index zero.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedProject {
    pub project: Project,
    pub version: Version,
    pub project_type: ContentType,
    pub install: InstallPlan,
}

#[derive(Debug, thiserror::Error)]
pub enum ModrinthError {
    #[error("could not build or send Modrinth HTTP request: {0}")]
    Http(#[from] reqwest::Error),
    #[error("invalid Modrinth query: {0}")]
    InvalidQuery(&'static str),
    #[error("a non-empty, uniquely identifying Modrinth User-Agent is required")]
    InvalidUserAgent,
    #[error("invalid Modrinth project or version identifier: {0:?}")]
    InvalidIdentifier(String),
    #[error("invalid fixed Modrinth API endpoint")]
    InvalidApiEndpoint,
    #[error("Modrinth returned HTTP {status} for {endpoint}: {detail}")]
    ApiStatus {
        endpoint: String,
        status: StatusCode,
        detail: String,
    },
    #[error(
        "Modrinth returned invalid JSON for {endpoint} (content type {content_type:?}): {source}; body starts with {body:?}"
    )]
    InvalidJson {
        endpoint: String,
        content_type: Option<String>,
        body: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("Modrinth version {version_id} has invalid publication date {value:?}")]
    InvalidPublicationDate { version_id: String, value: String },
    #[error("Modrinth project {project_id} has no compatible stable version")]
    NoStableVersion { project_id: String },
    #[error(
        "Modrinth version {version_id} belongs to {actual_project_id}, not {expected_project_id}"
    )]
    VersionProjectMismatch {
        version_id: String,
        expected_project_id: String,
        actual_project_id: String,
    },
    #[error("Modrinth version {version_id} has no downloadable files")]
    NoVersionFiles { version_id: String },
    #[error("unsafe Modrinth file name in version {version_id}: {file_name:?}")]
    UnsafeFileName {
        version_id: String,
        file_name: String,
    },
    #[error("untrusted Modrinth file URL in version {version_id}: {url:?}")]
    UntrustedDownloadUrl { version_id: String, url: String },
    #[error("Modrinth file {file_name:?} in version {version_id} has invalid {algorithm}")]
    InvalidHash {
        version_id: String,
        file_name: String,
        algorithm: &'static str,
    },
    #[error("Modrinth file {file_name:?} in version {version_id} has no SHA-1 or SHA-512 hash")]
    MissingHash {
        version_id: String,
        file_name: String,
    },
    #[error("Modrinth file {file_name:?} in version {version_id} has zero size")]
    InvalidFileSize {
        version_id: String,
        file_name: String,
    },
    #[error("required dependency in version {version_id} has neither a project nor a version id")]
    UnresolvableDependency { version_id: String },
    #[error(
        "required dependency project {project_id} requested conflicting versions {selected_version_id} and {requested_version_id}"
    )]
    DependencyVersionConflict {
        project_id: String,
        selected_version_id: String,
        requested_version_id: String,
    },
}

#[derive(Debug, Clone)]
pub struct ModrinthClient {
    api_client: Client,
    download_client: Client,
}

impl ModrinthClient {
    pub fn new() -> Result<Self, ModrinthError> {
        Self::with_user_agent(DEFAULT_USER_AGENT)
    }

    pub fn with_user_agent(user_agent: &str) -> Result<Self, ModrinthError> {
        let user_agent = user_agent.trim();
        if user_agent.is_empty() {
            return Err(ModrinthError::InvalidUserAgent);
        }
        let api_client = Client::builder()
            .user_agent(user_agent)
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(30))
            .redirect(Policy::none())
            .build()?;
        let download_client = Client::builder()
            .user_agent(user_agent)
            .connect_timeout(Duration::from_secs(30))
            .redirect(Policy::custom(|attempt| {
                if attempt.previous().len() < 5 && trusted_cdn_url(attempt.url()) {
                    attempt.follow()
                } else {
                    attempt.stop()
                }
            }))
            .build()?;
        Ok(Self {
            api_client,
            download_client,
        })
    }

    /// A credential-free client whose redirects are confined to Modrinth's
    /// HTTPS CDN. It can be passed directly to `download::download_batch`.
    pub fn download_client(&self) -> Client {
        self.download_client.clone()
    }

    pub async fn search(&self, query: &SearchQuery) -> Result<SearchPage, ModrinthError> {
        let mut url = api_url(&["search"])?;
        url.query_pairs_mut().extend_pairs(query.parameters()?);
        let wire: SearchResponse = self.get_json(url, "/v2/search").await?;
        Ok(wire.into())
    }

    pub async fn get_project(&self, id_or_slug: &str) -> Result<Project, ModrinthError> {
        let identifier = identifier(id_or_slug)?;
        let url = api_url(&["project", identifier])?;
        let wire: ProjectResponse = self.get_json(url, "/v2/project/{id}").await?;
        Ok(wire.into())
    }

    pub async fn list_versions(
        &self,
        project_id_or_slug: &str,
        query: &VersionQuery,
    ) -> Result<Vec<Version>, ModrinthError> {
        let identifier = identifier(project_id_or_slug)?;
        let mut url = api_url(&["project", identifier, "version"])?;
        url.query_pairs_mut().extend_pairs(query.parameters()?);
        self.get_json(url, "/v2/project/{id}/version").await
    }

    pub async fn get_version(&self, version_id: &str) -> Result<Version, ModrinthError> {
        let identifier = identifier(version_id)?;
        let url = api_url(&["version", identifier])?;
        self.get_json(url, "/v2/version/{id}").await
    }

    pub async fn latest_stable_version(
        &self,
        project_id_or_slug: &str,
        query: &VersionQuery,
    ) -> Result<Option<Version>, ModrinthError> {
        let versions = self.list_versions(project_id_or_slug, query).await?;
        Ok(latest_stable(&versions)?.cloned())
    }

    /// Resolves and validates one explicitly selected project version.
    pub async fn get_install_plan(
        &self,
        project_id_or_slug: &str,
        version_id: &str,
    ) -> Result<ResolvedProject, ModrinthError> {
        let project = self.get_project(project_id_or_slug).await?;
        let version = self.get_version(version_id).await?;
        resolved_project(project, version)
    }

    /// Resolves the root and all recursively required dependencies before any
    /// file is downloaded. The root is always result index zero. Pinned
    /// dependency version ids are honored; project-only dependencies select
    /// the latest release matching `dependency_query`.
    pub async fn resolve_installation(
        &self,
        project_id_or_slug: &str,
        version_id: &str,
        dependency_query: &VersionQuery,
    ) -> Result<Vec<ResolvedProject>, ModrinthError> {
        let root = self
            .get_install_plan(project_id_or_slug, version_id)
            .await?;
        let mut resolved = vec![root];
        let mut selected = HashMap::from([(
            resolved[0].project.id.clone(),
            resolved[0].version.id.clone(),
        )]);
        let mut queue = VecDeque::new();
        enqueue_required(&mut queue, &resolved[0].version);

        while let Some(request) = queue.pop_front() {
            // A project-only edge accepts whichever compatible version is
            // already selected. This is what makes A -> B -> A cycles a
            // no-op instead of an artificial version conflict.
            if request.version_id.is_none()
                && request
                    .project_id
                    .as_ref()
                    .is_some_and(|project_id| selected.contains_key(project_id))
            {
                continue;
            }
            // When both ids are present, an existing matching selection also
            // avoids an unnecessary request. A different pinned id is a real
            // conflict and must not be silently replaced.
            if let (Some(project_id), Some(version_id)) =
                (request.project_id.as_ref(), request.version_id.as_ref())
                && let Some(selected_version_id) = selected.get(project_id)
            {
                if selected_version_id == version_id {
                    continue;
                }
                return Err(ModrinthError::DependencyVersionConflict {
                    project_id: project_id.clone(),
                    selected_version_id: selected_version_id.clone(),
                    requested_version_id: version_id.clone(),
                });
            }

            let version = if let Some(version_id) = request.version_id.as_deref() {
                self.get_version(version_id).await?
            } else if let Some(project_id) = request.project_id.as_deref() {
                self.latest_stable_version(project_id, dependency_query)
                    .await?
                    .ok_or_else(|| ModrinthError::NoStableVersion {
                        project_id: project_id.to_owned(),
                    })?
            } else {
                return Err(ModrinthError::UnresolvableDependency {
                    version_id: request.parent_version_id,
                });
            };

            if let Some(expected) = request.project_id.as_deref()
                && expected != version.project_id
            {
                return Err(ModrinthError::VersionProjectMismatch {
                    version_id: version.id,
                    expected_project_id: expected.to_owned(),
                    actual_project_id: version.project_id,
                });
            }
            if let Some(selected_version) = selected.get(&version.project_id) {
                if selected_version != &version.id {
                    return Err(ModrinthError::DependencyVersionConflict {
                        project_id: version.project_id,
                        selected_version_id: selected_version.clone(),
                        requested_version_id: version.id,
                    });
                }
                continue;
            }

            let project = self.get_project(&version.project_id).await?;
            let item = resolved_project(project, version)?;
            selected.insert(item.project.id.clone(), item.version.id.clone());
            enqueue_required(&mut queue, &item.version);
            resolved.push(item);
        }
        Ok(resolved)
    }

    async fn get_json<T: DeserializeOwned>(
        &self,
        url: Url,
        endpoint: &str,
    ) -> Result<T, ModrinthError> {
        let response = self
            .api_client
            .get(url)
            .header(ACCEPT, "application/json")
            .send()
            .await?;
        decode_response(response, endpoint).await
    }
}

#[derive(Debug, Clone)]
struct DependencyRequest {
    parent_version_id: String,
    project_id: Option<String>,
    version_id: Option<String>,
}

fn enqueue_required(queue: &mut VecDeque<DependencyRequest>, version: &Version) {
    queue.extend(
        version
            .required_dependencies()
            .map(|dependency| DependencyRequest {
                parent_version_id: version.id.clone(),
                project_id: dependency.project_id.clone(),
                version_id: dependency.version_id.clone(),
            }),
    );
}

fn resolved_project(project: Project, version: Version) -> Result<ResolvedProject, ModrinthError> {
    if version.project_id != project.id {
        return Err(ModrinthError::VersionProjectMismatch {
            version_id: version.id,
            expected_project_id: project.id,
            actual_project_id: version.project_id,
        });
    }
    let project_type = project.content_type_for(&version);
    let install = version.install_plan()?;
    Ok(ResolvedProject {
        project,
        version,
        project_type,
        install,
    })
}

pub fn latest_stable(versions: &[Version]) -> Result<Option<&Version>, ModrinthError> {
    let mut latest: Option<(&Version, DateTime<FixedOffset>)> = None;
    for version in versions.iter().filter(|version| {
        version.version_type == VersionType::Release && version.status.installable()
    }) {
        let published = DateTime::parse_from_rfc3339(&version.date_published).map_err(|_| {
            ModrinthError::InvalidPublicationDate {
                version_id: version.id.clone(),
                value: version.date_published.clone(),
            }
        })?;
        let replace = latest.as_ref().is_none_or(|(current, current_date)| {
            published > *current_date || (published == *current_date && version.id > current.id)
        });
        if replace {
            latest = Some((version, published));
        }
    }
    Ok(latest.map(|(version, _)| version))
}

fn install_plan(version: &Version, file: &VersionFile) -> Result<InstallPlan, ModrinthError> {
    let file_name =
        path_safety::file_name(&file.filename).ok_or_else(|| ModrinthError::UnsafeFileName {
            version_id: version.id.clone(),
            file_name: file.filename.clone(),
        })?;
    let legacy_mrpack_version_number =
        is_mrpack_file_name(&file_name).then_some(version.version_number.as_str());
    let url = trusted_version_file_url(
        &file.url,
        &version.project_id,
        &version.id,
        legacy_mrpack_version_number,
    )
    .ok_or_else(|| ModrinthError::UntrustedDownloadUrl {
        version_id: version.id.clone(),
        url: file.url.clone(),
    })?;
    let sha1 = normalized_hash::<20>(file.hashes.sha1.as_deref(), "SHA-1", version, &file_name)?;
    let sha512 = normalized_hash::<64>(
        file.hashes.sha512.as_deref(),
        "SHA-512",
        version,
        &file_name,
    )?;
    if sha1.is_none() && sha512.is_none() {
        return Err(ModrinthError::MissingHash {
            version_id: version.id.clone(),
            file_name,
        });
    }
    if file.size == 0 {
        return Err(ModrinthError::InvalidFileSize {
            version_id: version.id.clone(),
            file_name,
        });
    }
    Ok(InstallPlan {
        project_id: version.project_id.clone(),
        version_id: version.id.clone(),
        url,
        file_name,
        sha1,
        sha512,
        size: file.size,
    })
}

fn is_mrpack_file_name(file_name: &str) -> bool {
    Path::new(file_name)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("mrpack"))
}

fn normalized_hash<const BYTES: usize>(
    value: Option<&str>,
    algorithm: &'static str,
    version: &Version,
    file_name: &str,
) -> Result<Option<String>, ModrinthError> {
    let Some(value) = value else {
        return Ok(None);
    };
    integrity::normalized_hex::<BYTES>(value)
        .map(Some)
        .ok_or_else(|| ModrinthError::InvalidHash {
            version_id: version.id.clone(),
            file_name: file_name.to_owned(),
            algorithm,
        })
}

fn trusted_version_file_url(
    value: &str,
    project_id: &str,
    version_id: &str,
    legacy_mrpack_version_number: Option<&str>,
) -> Option<String> {
    let url = Url::parse(value.trim()).ok()?;
    if !trusted_cdn_url(&url) || url.query().is_some() || url.fragment().is_some() {
        return None;
    }
    let segments = url.path_segments()?.collect::<Vec<_>>();
    // Most Modrinth files use the opaque version id in this path segment.
    // A small number of old modpack releases instead use their human-readable
    // version number (while the API still returns an unrelated opaque id).
    // Keep that compatibility exception confined to `.mrpack` metadata; mods
    // and every other resource continue to require the opaque version id.
    let trusted_version_segment = segments.get(3).is_some_and(|segment| {
        *segment == version_id
            || legacy_mrpack_version_number.is_some_and(|version_number| {
                !version_number.is_empty() && *segment == version_number
            })
    });
    if segments.len() != 5
        || segments[0] != "data"
        || segments[1] != project_id
        || segments[2] != "versions"
        || !trusted_version_segment
        || segments[4].is_empty()
    {
        return None;
    }
    Some(url.into())
}

fn trusted_cdn_url(url: &Url) -> bool {
    url.scheme() == "https"
        && url
            .host_str()
            .is_some_and(|host| host.eq_ignore_ascii_case(CDN_HOST))
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
}

fn validate_search_pagination(offset: u32, limit: u32) -> Result<(), ModrinthError> {
    if limit == 0 || limit > MAX_SEARCH_LIMIT {
        return Err(ModrinthError::InvalidQuery(
            "search limit must be between 1 and 100",
        ));
    }
    offset
        .checked_add(limit)
        .ok_or(ModrinthError::InvalidQuery(
            "search offset plus limit overflowed",
        ))?;
    Ok(())
}

fn normalized_filter<'a>(
    value: Option<&'a str>,
    label: &'static str,
) -> Result<Option<&'a str>, ModrinthError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim();
    if value.is_empty() {
        return Err(ModrinthError::InvalidQuery(match label {
            "game version" => "game version filters cannot be blank",
            _ => "filters cannot be blank",
        }));
    }
    Ok(Some(value))
}

fn normalized_filters<'a>(
    values: &'a [String],
    label: &'static str,
) -> Result<Vec<&'a str>, ModrinthError> {
    let mut seen = HashSet::new();
    let mut normalized = Vec::with_capacity(values.len());
    for value in values {
        let value = normalized_filter(Some(value), label)?.expect("Some remains Some");
        if seen.insert(value.to_ascii_lowercase()) {
            normalized.push(value);
        }
    }
    Ok(normalized)
}

fn identifier(value: &str) -> Result<&str, ModrinthError> {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || trimmed.len() > 128
        || trimmed != value
        || trimmed
            .chars()
            .any(|character| character.is_control() || matches!(character, '/' | '\\' | '?' | '#'))
    {
        return Err(ModrinthError::InvalidIdentifier(value.to_owned()));
    }
    Ok(trimmed)
}

fn api_url(segments: &[&str]) -> Result<Url, ModrinthError> {
    let mut url = Url::parse(API_BASE).map_err(|_| ModrinthError::InvalidApiEndpoint)?;
    {
        let mut path = url
            .path_segments_mut()
            .map_err(|_| ModrinthError::InvalidApiEndpoint)?;
        path.pop_if_empty();
        for segment in segments {
            path.push(segment);
        }
    }
    if url.scheme() != "https"
        || url.host_str() != Some(API_HOST)
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(ModrinthError::InvalidApiEndpoint);
    }
    Ok(url)
}

#[derive(Debug, Deserialize)]
struct ApiErrorBody {
    #[serde(default)]
    error: String,
    #[serde(default)]
    description: String,
}

async fn decode_response<T: DeserializeOwned>(
    response: reqwest::Response,
    endpoint: &str,
) -> Result<T, ModrinthError> {
    let status = response.status();
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let bytes = response.bytes().await?;
    if !status.is_success() {
        let detail = serde_json::from_slice::<ApiErrorBody>(&bytes)
            .ok()
            .map(
                |body| match (body.error.is_empty(), body.description.is_empty()) {
                    (false, false) => format!("{}: {}", body.error, body.description),
                    (false, true) => body.error,
                    _ => body.description,
                },
            )
            .filter(|detail| !detail.is_empty())
            .unwrap_or_else(|| body_preview(&bytes));
        return Err(ModrinthError::ApiStatus {
            endpoint: endpoint.to_owned(),
            status,
            detail,
        });
    }
    serde_json::from_slice(&bytes).map_err(|source| ModrinthError::InvalidJson {
        endpoint: endpoint.to_owned(),
        content_type,
        body: body_preview(&bytes),
        source,
    })
}

fn body_preview(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).chars().take(300).collect()
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    hits: Vec<SearchHit>,
    offset: u32,
    limit: u32,
    total_hits: u64,
}

#[derive(Debug, Deserialize)]
struct SearchHit {
    project_id: String,
    project_type: ContentType,
    #[serde(default)]
    all_project_types: Vec<ContentType>,
    #[serde(default)]
    slug: Option<String>,
    title: String,
    description: String,
    author: String,
    #[serde(default)]
    categories: Vec<String>,
    #[serde(default)]
    display_categories: Vec<String>,
    #[serde(default)]
    versions: Vec<String>,
    #[serde(default)]
    downloads: u64,
    #[serde(default)]
    follows: u64,
    #[serde(default)]
    icon_url: Option<String>,
    #[serde(default)]
    date_modified: String,
    #[serde(default)]
    latest_version: Option<String>,
}

impl From<SearchResponse> for SearchPage {
    fn from(response: SearchResponse) -> Self {
        Self {
            hits: response.hits.into_iter().map(Project::from).collect(),
            offset: response.offset,
            limit: response.limit,
            total_hits: response.total_hits,
        }
    }
}

impl From<SearchHit> for Project {
    fn from(hit: SearchHit) -> Self {
        let mut project_types = hit.all_project_types;
        if !project_types.contains(&hit.project_type) {
            project_types.insert(0, hit.project_type);
        }
        let categories = if hit.display_categories.is_empty() {
            hit.categories
        } else {
            hit.display_categories
        };
        Self {
            id: hit.project_id,
            slug: hit.slug,
            title: hit.title,
            description: hit.description,
            author: Some(hit.author),
            project_type: hit.project_type,
            project_types,
            categories,
            game_versions: hit.versions,
            loaders: Vec::new(),
            downloads: hit.downloads,
            followers: hit.follows,
            icon_url: hit.icon_url,
            updated: hit.date_modified,
            version_ids: Vec::new(),
            latest_version: hit.latest_version,
        }
    }
}

#[derive(Debug, Deserialize)]
struct ProjectResponse {
    id: String,
    #[serde(default)]
    slug: Option<String>,
    title: String,
    description: String,
    project_type: ContentType,
    #[serde(default)]
    categories: Vec<String>,
    #[serde(default)]
    additional_categories: Vec<String>,
    #[serde(default)]
    game_versions: Vec<String>,
    #[serde(default)]
    loaders: Vec<String>,
    #[serde(default)]
    downloads: u64,
    #[serde(default)]
    followers: u64,
    #[serde(default)]
    icon_url: Option<String>,
    #[serde(default)]
    updated: String,
    #[serde(default)]
    versions: Vec<String>,
}

impl From<ProjectResponse> for Project {
    fn from(response: ProjectResponse) -> Self {
        let mut categories = response.categories;
        for category in response.additional_categories {
            if !categories.contains(&category) {
                categories.push(category);
            }
        }
        let mut project_types = vec![response.project_type];
        if response
            .loaders
            .iter()
            .any(|loader| loader.eq_ignore_ascii_case("datapack"))
            && !project_types.contains(&ContentType::DataPack)
        {
            project_types.push(ContentType::DataPack);
        }
        Self {
            id: response.id,
            slug: response.slug,
            title: response.title,
            description: response.description,
            author: None,
            project_type: response.project_type,
            project_types,
            categories,
            game_versions: response.game_versions,
            loaders: response.loaders,
            downloads: response.downloads,
            followers: response.followers,
            icon_url: response.icon_url,
            updated: response.updated,
            version_ids: response.versions,
            latest_version: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const SEARCH_FIXTURE: &str = r#"
    {
      "hits": [{
        "project_id": "OhduvhIc",
        "project_type": "mod",
        "all_project_types": ["datapack", "mod", "plugin"],
        "slug": "veinminer",
        "author": "Miraculixx",
        "title": "VeinMiner",
        "description": "Mine a whole vein.",
        "categories": ["fabric", "datapack"],
        "display_categories": ["fabric", "datapack"],
        "versions": ["1.21.1"],
        "downloads": 42,
        "follows": 7,
        "icon_url": null,
        "date_modified": "2026-08-22T16:47:18Z",
        "latest_version": "syKekkIm"
      }],
      "offset": 0,
      "limit": 20,
      "total_hits": 21
    }
    "#;

    const VERSION_FIXTURE: &str = r#"
    {
      "id": "XJrQXP5u",
      "project_id": "OhduvhIc",
      "author_id": "BTUi4ZVT",
      "name": "Veinminer Fabric - 2.11.2",
      "version_number": "2.11.2",
      "dependencies": [
        {"version_id": null, "project_id": "Ha28R6CL", "file_name": null, "dependency_type": "required"},
        {"version_id": null, "project_id": "dxa0Bm8m", "file_name": null, "dependency_type": "optional"}
      ],
      "game_versions": ["1.21.1"],
      "version_type": "release",
      "loaders": ["fabric", "quilt"],
      "featured": false,
      "status": "listed",
      "date_published": "2026-07-26T12:19:34.100789Z",
      "downloads": 12,
      "environment": "unknown",
      "files": [{
        "id": "yCrvfz7V",
        "hashes": {
          "sha1": "67D3589A4CE4F3DD436AF47C9DC4A1F439B9B12D",
          "sha512": "9bdfc452920c93676a8de58ca8a314d9896c33f107da4ef8eabc990ef48c6f3adfd905497f7fa239b0deed7bfd42e5fbf93fa2b06f694880115f2d826f26542a"
        },
        "url": "https://cdn.modrinth.com/data/OhduvhIc/versions/XJrQXP5u/veinminer-fabric.jar",
        "filename": "veinminer-fabric.jar",
        "primary": true,
        "size": 643645,
        "file_type": null
      }]
    }
    "#;

    fn fixture_version() -> Version {
        serde_json::from_str(VERSION_FIXTURE).expect("valid fixture")
    }

    #[test]
    fn search_uses_documented_facets_for_every_supported_type() {
        for (content_type, expected) in [
            (ContentType::Modpack, "project_type:modpack"),
            (ContentType::Mod, "project_type:mod"),
            (ContentType::ResourcePack, "project_type:resourcepack"),
            (ContentType::Shader, "project_type:shader"),
            (ContentType::DataPack, "all_project_types:datapack"),
        ] {
            let parameters = SearchQuery::new(content_type).parameters().unwrap();
            let facets = parameters
                .iter()
                .find(|(name, _)| *name == "facets")
                .unwrap()
                .1
                .clone();
            let facets: Vec<Vec<String>> = serde_json::from_str(&facets).unwrap();
            assert_eq!(facets[0], [expected]);
        }
    }

    #[test]
    fn search_facets_are_json_and_pagination_is_validated() {
        let mut query = SearchQuery::new(ContentType::Mod);
        query.query = " sodium & lithium ".into();
        query.game_version = Some("1.21.1".into());
        query.loader = Some(Loader::Fabric);
        query.offset = 20;
        query.limit = 20;
        let parameters = query.parameters().unwrap();
        assert!(parameters.contains(&("query", "sodium & lithium".into())));
        let facets = parameters.iter().find(|(key, _)| *key == "facets").unwrap();
        let decoded: Vec<Vec<String>> = serde_json::from_str(&facets.1).unwrap();
        assert_eq!(
            decoded,
            vec![
                vec!["project_type:mod"],
                vec!["versions:1.21.1"],
                vec!["categories:fabric"]
            ]
        );

        query.limit = 101;
        assert!(matches!(
            query.parameters(),
            Err(ModrinthError::InvalidQuery(_))
        ));
    }

    #[test]
    fn search_page_normalizes_hybrid_project_hits() {
        let wire: SearchResponse = serde_json::from_str(SEARCH_FIXTURE).unwrap();
        let page = SearchPage::from(wire);
        assert_eq!(page.hits.len(), 1);
        assert_eq!(page.hits[0].id, "OhduvhIc");
        assert_eq!(page.hits[0].project_type, ContentType::Mod);
        assert!(page.hits[0].project_types.contains(&ContentType::DataPack));
        assert_eq!(page.hits[0].latest_version.as_deref(), Some("syKekkIm"));
        assert_eq!(page.offset, 0);
        assert_eq!(page.total_hits, 21);
    }

    #[test]
    fn version_filters_are_json_arrays_not_ad_hoc_strings() {
        let query = VersionQuery {
            game_versions: vec![" 1.21.1 ".into(), "1.21.1".into()],
            loaders: vec![Loader::Fabric, Loader::Forge],
            featured: Some(false),
        };
        let parameters = query.parameters().unwrap();
        assert!(parameters.contains(&("game_versions", r#"["1.21.1"]"#.into())));
        assert!(parameters.contains(&("loaders", r#"["fabric","forge"]"#.into())));
        assert!(parameters.contains(&("include_changelog", "false".into())));
    }

    #[test]
    fn version_fixture_exposes_only_required_dependency_edges() {
        let version = fixture_version();
        let required = version.required_dependencies().collect::<Vec<_>>();
        assert_eq!(required.len(), 1);
        assert_eq!(required[0].project_id.as_deref(), Some("Ha28R6CL"));
    }

    #[test]
    fn latest_stable_uses_release_channel_and_timestamp() {
        let mut old_release = fixture_version();
        old_release.id = "old".into();
        old_release.date_published = "2024-01-01T00:00:00Z".into();
        let mut beta = fixture_version();
        beta.id = "beta".into();
        beta.version_type = VersionType::Beta;
        beta.date_published = "2027-01-01T00:00:00Z".into();
        let new_release = fixture_version();
        let versions = vec![old_release, beta, new_release];
        assert_eq!(latest_stable(&versions).unwrap().unwrap().id, "XJrQXP5u");
    }

    #[test]
    fn install_plan_selects_primary_and_normalizes_hashes() {
        let version = fixture_version();
        let plan = version.install_plan().unwrap();
        assert_eq!(plan.file_name, "veinminer-fabric.jar");
        assert_eq!(plan.size, 643645);
        assert_eq!(
            plan.sha1.as_deref(),
            Some("67d3589a4ce4f3dd436af47c9dc4a1f439b9b12d")
        );
        let spec = plan.download_spec(Path::new("instance/mods"));
        assert_eq!(
            spec.destination,
            PathBuf::from("instance/mods/veinminer-fabric.jar")
        );
        assert_eq!(spec.urls, [plan.url]);
    }

    #[test]
    fn install_plan_falls_back_to_first_file_when_primary_is_absent() {
        let mut version = fixture_version();
        version.files[0].primary = false;
        assert!(version.install_plan().is_ok());
        version.files.clear();
        assert!(matches!(
            version.install_plan(),
            Err(ModrinthError::NoVersionFiles { .. })
        ));
    }

    #[test]
    fn legacy_mrpack_url_may_use_version_number_instead_of_version_id() {
        let mut version = fixture_version();
        version.id = "t70hZh87".into();
        version.project_id = "1KVo5zza".into();
        version.version_number = "4.2.0-beta.3".into();
        version.files[0].filename = "MR_Fabulously Optimized_4.2.0-beta.3.mrpack".into();
        version.files[0].url = "https://cdn.modrinth.com/data/1KVo5zza/versions/4.2.0-beta.3/MR_Fabulously%20Optimized_4.2.0-beta.3.mrpack".into();

        let plan = version.install_plan().expect("legacy mrpack URL is valid");
        assert!(plan.is_mrpack());
        assert_eq!(plan.project_id, "1KVo5zza");
        assert_eq!(plan.version_id, "t70hZh87");
        assert_eq!(plan.url, version.files[0].url);
    }

    #[test]
    fn version_number_url_exception_is_restricted_to_mrpack_files() {
        let mut version = fixture_version();
        version.id = "opaque-version-id".into();
        version.version_number = "human-readable-version".into();
        version.files[0].url = format!(
            "https://cdn.modrinth.com/data/{}/versions/human-readable-version/file.jar",
            version.project_id
        );
        assert!(matches!(
            version.install_plan(),
            Err(ModrinthError::UntrustedDownloadUrl { .. })
        ));

        version.files[0].filename = "pack.mrpack".into();
        version.files[0].url = format!(
            "https://cdn.modrinth.com/data/{}/versions/unrelated-version/pack.mrpack",
            version.project_id
        );
        assert!(matches!(
            version.install_plan(),
            Err(ModrinthError::UntrustedDownloadUrl { .. })
        ));
    }

    #[test]
    fn install_plan_rejects_traversal_untrusted_hosts_and_bad_hashes() {
        let mut version = fixture_version();
        version.files[0].filename = r"..\evil.jar".into();
        assert!(matches!(
            version.install_plan(),
            Err(ModrinthError::UnsafeFileName { .. })
        ));

        let mut version = fixture_version();
        version.files[0].url = "https://cdn.modrinth.com.example/data/file.jar".into();
        assert!(matches!(
            version.install_plan(),
            Err(ModrinthError::UntrustedDownloadUrl { .. })
        ));

        let mut version = fixture_version();
        version.files[0].hashes.sha1 = Some("not-a-hash".into());
        assert!(matches!(
            version.install_plan(),
            Err(ModrinthError::InvalidHash {
                algorithm: "SHA-1",
                ..
            })
        ));
    }

    #[test]
    fn hybrid_data_pack_versions_override_the_primary_project_type() {
        let mut version = fixture_version();
        version.loaders = vec!["datapack".into()];
        let project = Project {
            id: version.project_id.clone(),
            slug: None,
            title: "Hybrid".into(),
            description: String::new(),
            author: None,
            project_type: ContentType::Mod,
            project_types: vec![ContentType::Mod, ContentType::DataPack],
            categories: Vec::new(),
            game_versions: Vec::new(),
            loaders: Vec::new(),
            downloads: 0,
            followers: 0,
            icon_url: None,
            updated: String::new(),
            version_ids: Vec::new(),
            latest_version: None,
        };
        assert_eq!(project.content_type_for(&version), ContentType::DataPack);
        assert_eq!(
            resolved_project(project, version).unwrap().project_type,
            ContentType::DataPack
        );
    }

    #[test]
    fn fixed_urls_and_identifiers_cannot_escape_the_api_host() {
        assert_eq!(
            api_url(&["project", "OhduvhIc", "version"])
                .unwrap()
                .as_str(),
            "https://api.modrinth.com/v2/project/OhduvhIc/version"
        );
        assert!(identifier("../search").is_err());
        assert!(identifier(" project ").is_err());
        assert!(ModrinthClient::with_user_agent(" ").is_err());
    }

    #[test]
    fn windows_reserved_file_names_are_rejected_on_every_platform() {
        assert!(path_safety::file_name("CON.jar").is_none());
        assert!(path_safety::file_name("lpt1.zip").is_none());
        assert!(path_safety::file_name("valid pack.mrpack").is_some());
    }
}
