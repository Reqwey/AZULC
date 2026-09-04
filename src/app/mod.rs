pub(crate) mod catalog;
pub(crate) mod navigation;

use self::{
    catalog::{
        CatalogProject, CatalogProjectKey, CatalogProvider, CatalogRelease, CatalogReleaseKey,
        thumbnail_urls as project_thumbnail_urls,
    },
    navigation::{
        InstanceTab, ModpackTab, NewInstanceTab, Route, SettingsTab, VersionFilter, WizardStep,
    },
};
use crate::{
    domain::{
        AccountProvider, DownloadPolicy, DownloadSource, InstallProgress, InstallRequest,
        InstallStage, Instance, InstanceColor, JavaRuntime, LoaderKind, LoaderSpec,
        ModpackInstallSpec, ModpackSource, OfflineAccount, PersistedState, PipelineEvent,
    },
    services::{
        auth::microsoft,
        content::{self, ContentEntry, ContentKind},
        download::{self, source::SourceRouter},
        insights::{self, InstanceScanSummary, ServicePing, VersionHighlights},
        installer, java, launcher,
        loader_catalog::{self, LoaderCatalogKey, LoaderCatalogState},
        minecraft,
        modpack::{self, ModpackPlan},
        providers::{
            curseforge::{
                CurseForgeClient, FileQuery, ModLoader as CurseForgeLoader, ResourceClass,
                ResourceInstallRequest, SearchQuery,
            },
            modrinth::{
                self, ContentType as ModrinthContentType, Loader as ModrinthLoader, ModrinthClient,
                SearchQuery as ModrinthSearchQuery, VersionQuery as ModrinthVersionQuery,
            },
        },
        shell,
        system_resources::{self, SystemResources},
        thumbnail,
    },
    storage::Paths,
    theme,
};
use futures::SinkExt;
use iced::{Subscription, Task, Theme, widget::image, window};
use std::{
    collections::{HashMap, HashSet},
    fs,
    hash::Hash,
    io::Write,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub(crate) struct WizardDraft {
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) color: InstanceColor,
    pub(crate) selected_version: Option<String>,
    pub(crate) loader: LoaderKind,
    pub(crate) loader_version: String,
    pub(crate) search: String,
}

impl Default for WizardDraft {
    fn default() -> Self {
        Self {
            name: "New Instance".into(),
            description: String::new(),
            color: InstanceColor::default(),
            selected_version: None,
            loader: LoaderKind::Vanilla,
            loader_version: String::new(),
            search: String::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct InstallJob {
    pub(crate) request: InstallRequest,
    pub(crate) progress: InstallProgress,
    pub(crate) logs: Vec<String>,
    pub(crate) log_path: Option<PathBuf>,
    pub(crate) active: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct ResourceBrowserState {
    pub(crate) id: u64,
    pub(crate) instance_id: Uuid,
    pub(crate) kind: ContentKind,
    pub(crate) provider: CatalogProvider,
    pub(crate) query: String,
    pub(crate) projects: Vec<CatalogProject>,
    pub(crate) selected_project: Option<CatalogProject>,
    pub(crate) files: Vec<CatalogRelease>,
    pub(crate) loading: bool,
    pub(crate) downloading: bool,
    pub(crate) error: Option<String>,
    pub(crate) status: Option<String>,
    request_id: u64,
}

impl ResourceBrowserState {
    fn new(id: u64, instance_id: Uuid, kind: ContentKind) -> Self {
        Self {
            id,
            instance_id,
            kind,
            provider: CatalogProvider::CurseForge,
            query: String::new(),
            projects: Vec::new(),
            selected_project: None,
            files: Vec::new(),
            loading: true,
            downloading: false,
            error: None,
            status: None,
            request_id: 0,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ModpackBrowserState {
    pub(crate) provider: CatalogProvider,
    pub(crate) query: String,
    pub(crate) projects: Vec<CatalogProject>,
    pub(crate) selected_project: Option<CatalogProject>,
    pub(crate) files: Vec<CatalogRelease>,
    pub(crate) loading: bool,
    pub(crate) error: Option<String>,
    pub(crate) local_path: Option<PathBuf>,
    pub(crate) local_plan: Option<ModpackPlan>,
    pub(crate) local_loading: bool,
    request_id: u64,
}

#[derive(Debug, Clone, Hash)]
struct PipelineKey {
    request: InstallRequest,
    paths: Paths,
}

#[derive(Debug, Clone, Hash)]
struct LaunchKey {
    attempt: Uuid,
    instance: Instance,
    account: OfflineAccount,
    paths: Paths,
}

#[derive(Debug, Clone)]
pub(crate) struct LaunchSession {
    pub(crate) instance_id: Uuid,
    pub(crate) status: String,
    pub(crate) logs: Vec<String>,
    pub(crate) log_path: Option<PathBuf>,
    pub(crate) pid: Option<u32>,
    pub(crate) ready: bool,
    pub(crate) active: bool,
    pub(crate) failed: bool,
    ready_at: Option<Instant>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct MicrosoftLoginState {
    pub(crate) active: bool,
    pub(crate) user_code: String,
    pub(crate) verification_url: String,
    pub(crate) status: String,
    pub(crate) error: Option<String>,
    request_id: u64,
    cancelled: Option<Arc<AtomicBool>>,
}

pub struct Launcher {
    pub(crate) paths: Paths,
    pub(crate) persisted: PersistedState,
    pub(crate) route: Route,
    pub(crate) instance_tab: InstanceTab,
    pub(crate) settings_tab: SettingsTab,
    pub(crate) new_instance_tab: NewInstanceTab,
    pub(crate) modpack_tab: ModpackTab,
    pub(crate) wizard_step: WizardStep,
    pub(crate) version_filter: VersionFilter,
    pub(crate) selected: Option<Uuid>,
    pub(crate) account_input: String,
    pub(crate) microsoft_login: MicrosoftLoginState,
    pub(crate) wizard: WizardDraft,
    pub(crate) versions: Vec<minecraft::VersionEntry>,
    pub(crate) loader_catalog: LoaderCatalogState,
    pub(crate) java_runtimes: Vec<JavaRuntime>,
    pub(crate) insights: InstanceScanSummary,
    pub(crate) highlights: VersionHighlights,
    pub(crate) pings: Vec<ServicePing>,
    pub(crate) system_resources: SystemResources,
    pub(crate) content_entries: Vec<ContentEntry>,
    pub(crate) content_query: String,
    thumbnails: HashMap<String, image::Handle>,
    requested_thumbnails: HashSet<String>,
    pub(crate) content_scope: Option<(Uuid, ContentKind)>,
    pub(crate) content_loading: bool,
    pub(crate) resource_browser: Option<ResourceBrowserState>,
    pub(crate) pending_delete: Option<Uuid>,
    pub(crate) modpacks: ModpackBrowserState,
    pub(crate) jobs: HashMap<Uuid, InstallJob>,
    pub(crate) notice: Option<String>,
    pub(crate) launching: bool,
    launch_request: Option<LaunchKey>,
    pub(crate) launch_session: Option<LaunchSession>,
    window_id: Option<window::Id>,
    next_modal_id: u64,
}

#[derive(Debug, Clone)]
pub enum Message {
    Navigate(Route),
    WindowOpened(window::Id),
    DragWindow,
    ResizeWindow(window::Direction),
    ToggleMaximize,
    MinimizeWindow,
    CloseWindow,
    AccountInputChanged(String),
    AddOfflineAccount,
    BeginMicrosoftLogin,
    MicrosoftDeviceAuthorizationLoaded(u64, Result<microsoft::DeviceAuthorization, String>),
    CopyMicrosoftLoginCode,
    MicrosoftLoginFinished(u64, Result<OfflineAccount, String>),
    CancelMicrosoftLogin,
    MicrosoftAccountRefreshedForLaunch(Uuid, Result<OfflineAccount, String>),
    SelectAccount(Uuid),
    DeleteAccount(Uuid),
    WizardStepSelected(WizardStep),
    WizardNext,
    WizardBack,
    VersionFilterSelected(VersionFilter),
    OpenHighlightedVersion(VersionFilter, String),
    VersionSearchChanged(String),
    VersionPicked(String),
    LoaderPicked(LoaderKind),
    LoaderVersionPicked(String),
    WizardNameChanged(String),
    WizardDescriptionChanged(String),
    WizardColorPicked(InstanceColor),
    VersionsLoaded(Result<Vec<minecraft::VersionEntry>, String>),
    LoaderCatalogLoaded(
        u64,
        LoaderCatalogKey,
        Result<loader_catalog::LoaderCatalog, String>,
    ),
    RetryLoaderCatalog,
    JavaLoaded(Vec<JavaRuntime>),
    InsightsLoaded(InstanceScanSummary),
    HighlightsLoaded(Result<VersionHighlights, String>),
    PingsLoaded(Vec<ServicePing>),
    ContentLoaded(Uuid, ContentKind, Result<Vec<ContentEntry>, String>),
    ContentQueryChanged(String),
    ContentThumbnailsLoaded(
        Uuid,
        ContentKind,
        Vec<(PathBuf, Option<thumbnail::Thumbnail>)>,
    ),
    ThumbnailsLoaded(Vec<(String, Option<thumbnail::Thumbnail>)>),
    OpenResourceBrowser(ContentKind),
    CloseResourceBrowser,
    ResourceQueryChanged(String),
    ResourceProviderPicked(CatalogProvider),
    SearchResources,
    ResourceSearchLoaded(u64, u64, Result<Vec<CatalogProject>, String>),
    ResourceProjectPicked(CatalogProject),
    ResourceBackToProjects,
    ResourceFilesLoaded(u64, u64, Result<Vec<CatalogRelease>, String>),
    ResourceFilePicked(CatalogRelease),
    ResourceDownloaded(u64, Uuid, ContentKind, Result<PathBuf, String>),
    ModpackQueryChanged(String),
    ModpackProviderPicked(CatalogProvider),
    SearchModpacks,
    ModpackSearchLoaded(u64, Result<Vec<CatalogProject>, String>),
    ModpackProjectPicked(CatalogProject),
    ModpackBackToProjects,
    ModpackFilesLoaded(u64, Result<Vec<CatalogRelease>, String>),
    ModpackFilePicked(CatalogRelease),
    ChooseLocalModpack,
    LocalModpackPicked(Option<PathBuf>),
    LocalModpackInspected(PathBuf, Result<ModpackPlan, String>),
    InstallLocalModpack,
    RefreshJava,
    RefreshPings,
    RefreshSystemResources,
    SystemResourcesLoaded(SystemResources),
    InstanceVersionFilesRepaired,
    SelectInstance(Uuid),
    SelectInstanceTab(InstanceTab),
    EditInstanceName(String),
    EditInstanceDescription(String),
    EditInstanceColor(InstanceColor),
    ToggleInstanceFavorite(bool),
    SetInstanceIsolation(bool),
    SetInstanceAutoJava(bool),
    SetInstanceJava(PathBuf),
    SetInstanceWidth(u32),
    SetInstanceHeight(u32),
    SetInstanceFullscreen(bool),
    SetInstanceAutoMemory(bool),
    SetInstanceMemory(u32),
    SetInstanceWindowTitle(String),
    SetInstanceCustomInfo(String),
    OpenPath(PathBuf),
    PathOpened(Result<(), String>),
    OpenFolder(PathBuf),
    FolderOpened(Result<(), String>),
    RevealPath(PathBuf),
    PathRevealed(Result<(), String>),
    OpenExternalUrl(&'static str),
    ExternalUrlOpened(Result<(), String>),
    DeleteInstance(Uuid),
    CancelDeleteInstance,
    ConfirmDeleteInstance,
    Deleted(Uuid, Result<(), String>),
    SettingsTabSelected(SettingsTab),
    NewInstanceTabSelected(NewInstanceTab),
    ModpackTabSelected(ModpackTab),
    DownloadSourcePicked(DownloadSource),
    DownloadConcurrencyChanged(u16),
    CancelInstall(Uuid),
    RetryInstall(Uuid),
    Pipeline(Uuid, Box<PipelineEvent>),
    LaunchSelected,
    LaunchEvent(launcher::LaunchEvent),
    DismissNotice,
}

impl Launcher {
    pub fn new() -> (Self, Task<Message>) {
        let paths = Paths::discover();
        let _ = paths.prepare();
        let mut persisted = paths.load();
        persisted.migrate();
        let _ = paths.save(&persisted);
        let selected = persisted.instances.first().map(|instance| instance.id);
        let instances = persisted.instances.clone();
        let instance_files = instances.clone();
        let instance_file_paths = paths.clone();
        let policy = persisted.settings.download.clone();
        let app = Self {
            paths,
            persisted,
            route: Route::Home,
            instance_tab: InstanceTab::Overview,
            settings_tab: SettingsTab::Downloads,
            new_instance_tab: NewInstanceTab::Minecraft,
            modpack_tab: ModpackTab::Browse,
            wizard_step: WizardStep::Version,
            version_filter: VersionFilter::Release,
            selected,
            account_input: String::new(),
            microsoft_login: MicrosoftLoginState::default(),
            wizard: WizardDraft::default(),
            versions: Vec::new(),
            loader_catalog: LoaderCatalogState::default(),
            java_runtimes: Vec::new(),
            insights: InstanceScanSummary::default(),
            highlights: VersionHighlights::default(),
            pings: Vec::new(),
            system_resources: SystemResources::default(),
            content_entries: Vec::new(),
            content_query: String::new(),
            thumbnails: HashMap::new(),
            requested_thumbnails: HashSet::new(),
            content_scope: None,
            content_loading: false,
            resource_browser: None,
            pending_delete: None,
            modpacks: ModpackBrowserState::default(),
            jobs: HashMap::new(),
            notice: None,
            launching: false,
            launch_request: None,
            launch_session: None,
            window_id: None,
            next_modal_id: 0,
        };

        (
            app,
            Task::batch([
                Task::perform(load_versions(policy), Message::VersionsLoaded),
                Task::perform(java::detect(), Message::JavaLoaded),
                Task::perform(
                    async move { insights::scan_instances(&instances).await },
                    Message::InsightsLoaded,
                ),
                Task::perform(load_highlights(), Message::HighlightsLoaded),
                Task::perform(load_pings(), Message::PingsLoaded),
                Task::perform(system_resources::read(), Message::SystemResourcesLoaded),
                Task::perform(
                    repair_instance_version_files(instance_file_paths, instance_files),
                    |_| Message::InstanceVersionFilesRepaired,
                ),
            ]),
        )
    }

    pub fn title(&self) -> String {
        "Azusa Minecraft Launcher".into()
    }

    pub fn theme(&self) -> Theme {
        theme::azul()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let mut subscriptions: Vec<Subscription<Message>> = vec![
            window::open_events().map(Message::WindowOpened),
            iced::time::every(Duration::from_secs(2)).map(|_| Message::RefreshSystemResources),
        ];
        subscriptions.extend(self.jobs.values().filter(|job| job.active).map(|job| {
            let key = PipelineKey {
                request: job.request.clone(),
                paths: self.paths.clone(),
            };
            let id = key.request.instance_id;
            Subscription::run_with(key, pipeline_stream)
                .with(id)
                .map(pipeline_message)
        }));
        if let Some(key) = &self.launch_request {
            subscriptions
                .push(Subscription::run_with(key.clone(), launch_stream).map(Message::LaunchEvent));
        }
        Subscription::batch(subscriptions)
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Navigate(route) => {
                self.route = route;
                if route == Route::Instances {
                    return self.load_selected_content();
                }
            }
            Message::WindowOpened(id) => self.window_id = Some(id),
            Message::DragWindow => {
                if let Some(id) = self.window_id {
                    return window::drag(id);
                }
            }
            Message::ResizeWindow(direction) => {
                if let Some(id) = self.window_id {
                    return window::drag_resize(id, direction);
                }
            }
            Message::ToggleMaximize => {
                if let Some(id) = self.window_id {
                    return window::toggle_maximize(id);
                }
            }
            Message::MinimizeWindow => {
                if let Some(id) = self.window_id {
                    return window::minimize(id, true);
                }
            }
            Message::CloseWindow => {
                if let Some(id) = self.window_id {
                    return window::close(id);
                }
            }
            Message::AccountInputChanged(value) => self.account_input = value,
            Message::AddOfflineAccount => self.add_account(),
            Message::BeginMicrosoftLogin => return self.begin_microsoft_login(),
            Message::MicrosoftDeviceAuthorizationLoaded(request_id, result) => {
                if self.microsoft_login.request_id != request_id {
                    return Task::none();
                }
                match result {
                    Ok(authorization) => {
                        self.microsoft_login.user_code = authorization.user_code.clone();
                        self.microsoft_login.verification_url =
                            authorization.verification_url().to_owned();
                        self.microsoft_login.status =
                            "Code copied. Complete sign-in in your browser; AZULC is waiting…"
                                .into();
                        let _ = open::that(authorization.verification_url());
                        let cancelled = Arc::new(AtomicBool::new(false));
                        self.microsoft_login.cancelled = Some(cancelled.clone());
                        return Task::batch([
                            iced::clipboard::write(self.microsoft_login.user_code.clone()),
                            Task::perform(
                                async move {
                                    microsoft::complete_device_authorization(
                                        authorization,
                                        cancelled,
                                    )
                                    .await
                                    .map_err(|error| error.to_string())
                                },
                                move |result| Message::MicrosoftLoginFinished(request_id, result),
                            ),
                        ]);
                    }
                    Err(error) => {
                        self.microsoft_login.active = false;
                        self.microsoft_login.error = Some(error);
                    }
                }
            }
            Message::CopyMicrosoftLoginCode => {
                if !self.microsoft_login.user_code.is_empty() {
                    self.microsoft_login.status =
                        "Code copied. Complete sign-in in your browser; AZULC is waiting…".into();
                    return iced::clipboard::write(self.microsoft_login.user_code.clone());
                }
            }
            Message::MicrosoftLoginFinished(request_id, result) => {
                if self.microsoft_login.request_id != request_id {
                    return Task::none();
                }
                self.microsoft_login.active = false;
                self.microsoft_login.cancelled = None;
                self.microsoft_login.user_code.clear();
                self.microsoft_login.verification_url.clear();
                match result {
                    Ok(account) => {
                        self.upsert_microsoft_account(account);
                        self.microsoft_login.status = "Microsoft account connected.".into();
                        self.microsoft_login.error = None;
                    }
                    Err(error) => self.microsoft_login.error = Some(error),
                }
            }
            Message::CancelMicrosoftLogin => {
                if let Some(cancelled) = self.microsoft_login.cancelled.take() {
                    cancelled.store(true, Ordering::Relaxed);
                }
                self.microsoft_login.request_id = self.microsoft_login.request_id.wrapping_add(1);
                self.microsoft_login.active = false;
                self.microsoft_login.user_code.clear();
                self.microsoft_login.verification_url.clear();
                self.microsoft_login.status = "Microsoft sign-in cancelled.".into();
            }
            Message::MicrosoftAccountRefreshedForLaunch(id, result) => {
                self.launching = false;
                match result {
                    Ok(account) => {
                        if account.uuid != id {
                            self.notice = Some("Microsoft returned a different profile.".into());
                            return Task::none();
                        }
                        self.upsert_microsoft_account(account);
                        return self.launch_selected();
                    }
                    Err(error) => {
                        self.notice = Some(format!("Microsoft sign-in expired: {error}"));
                    }
                }
            }
            Message::SelectAccount(id) => {
                if self
                    .persisted
                    .accounts
                    .iter()
                    .any(|account| account.uuid == id)
                {
                    self.persisted.selected_account = Some(id);
                    self.persisted.account = self.persisted.active_account().cloned();
                    self.save();
                }
            }
            Message::DeleteAccount(id) => {
                self.persisted.accounts.retain(|account| account.uuid != id);
                if self.persisted.selected_account == Some(id) {
                    self.persisted.selected_account =
                        self.persisted.accounts.first().map(|account| account.uuid);
                }
                self.persisted.account = self
                    .persisted
                    .selected_account
                    .and_then(|selected| {
                        self.persisted
                            .accounts
                            .iter()
                            .find(|account| account.uuid == selected)
                    })
                    .cloned();
                self.save();
            }
            Message::WizardStepSelected(step) => {
                if self.wizard_can_open(step) {
                    self.wizard_step = step;
                    if step == WizardStep::Loader {
                        return self.refresh_loader_catalog(false);
                    }
                }
            }
            Message::WizardNext => match self.wizard_step {
                WizardStep::Version if self.wizard.selected_version.is_none() => {
                    self.notice = Some("Choose a Minecraft version first.".into());
                }
                WizardStep::Version => {
                    self.wizard_step = WizardStep::Loader;
                    return self.refresh_loader_catalog(false);
                }
                WizardStep::Loader
                    if self.wizard.loader != LoaderKind::Vanilla
                        && self.wizard.loader_version.trim().is_empty() =>
                {
                    self.notice = Some("Choose a loader build first.".into());
                }
                WizardStep::Loader => self.wizard_step = WizardStep::Details,
                WizardStep::Details => return self.start_install(),
            },
            Message::WizardBack => {
                self.wizard_step = match self.wizard_step {
                    WizardStep::Version => WizardStep::Version,
                    WizardStep::Loader => WizardStep::Version,
                    WizardStep::Details => WizardStep::Loader,
                };
            }
            Message::VersionFilterSelected(filter) => self.version_filter = filter,
            Message::OpenHighlightedVersion(filter, version) => {
                if !version.trim().is_empty() {
                    self.new_instance_tab = NewInstanceTab::Minecraft;
                    self.route = Route::NewInstance;
                    self.wizard_step = WizardStep::Version;
                    self.version_filter = filter;
                    self.wizard = WizardDraft::default();
                    self.wizard.color = filter.color();
                    self.wizard.name = version.clone();
                    self.wizard.selected_version = Some(version);
                    self.loader_catalog.clear();
                }
            }
            Message::VersionSearchChanged(value) => self.wizard.search = value,
            Message::VersionPicked(version) => {
                if self.wizard.selected_version.as_deref() != Some(version.as_str()) {
                    if let Some(entry) = self.versions.iter().find(|entry| entry.id == version) {
                        self.wizard.color = VersionFilter::for_version(entry).color();
                    }
                    self.wizard.selected_version = Some(version.clone());
                    self.wizard.loader_version.clear();
                    if self.wizard.name == "New Instance" || self.wizard.name.trim().is_empty() {
                        self.wizard.name = version;
                    }
                    if self.wizard.loader != LoaderKind::Vanilla {
                        return self.refresh_loader_catalog(true);
                    }
                }
            }
            Message::LoaderPicked(loader) => {
                if self.wizard.loader != loader {
                    self.wizard.loader = loader;
                    self.wizard.loader_version.clear();
                    return self.refresh_loader_catalog(true);
                }
            }
            Message::LoaderVersionPicked(value) => self.wizard.loader_version = value,
            Message::WizardNameChanged(value) => self.wizard.name = value,
            Message::WizardDescriptionChanged(value) => self.wizard.description = value,
            Message::WizardColorPicked(value) => self.wizard.color = value,
            Message::VersionsLoaded(result) => match result {
                Ok(versions) => {
                    if self.wizard.selected_version.is_none() {
                        let selected = versions
                            .iter()
                            .find(|version| version.kind == "release")
                            .or_else(|| versions.first());
                        if let Some(version) = selected {
                            self.wizard.color = VersionFilter::for_version(version).color();
                            self.wizard.selected_version = Some(version.id.clone());
                        }
                    }
                    self.versions = versions;
                }
                Err(error) => self.notice = Some(error),
            },
            Message::LoaderCatalogLoaded(request_id, key, result) => {
                if self.loader_catalog.request_id != request_id
                    || self.loader_catalog.key.as_ref() != Some(&key)
                {
                    return Task::none();
                }
                self.loader_catalog.loading = false;
                match result {
                    Ok(catalog) => {
                        if self.wizard.loader_version.trim().is_empty() {
                            self.wizard.loader_version = catalog
                                .latest_stable_install_version()
                                .unwrap_or_default()
                                .to_string();
                        }
                        self.loader_catalog.entries = catalog.entries;
                        self.loader_catalog.provider = Some(catalog.provider);
                        self.loader_catalog.error = None;
                    }
                    Err(error) => {
                        self.loader_catalog.entries.clear();
                        self.loader_catalog.provider = None;
                        self.loader_catalog.error = Some(error);
                    }
                }
            }
            Message::RetryLoaderCatalog => return self.refresh_loader_catalog(true),
            Message::JavaLoaded(runtimes) => self.java_runtimes = runtimes,
            Message::InsightsLoaded(summary) => self.insights = summary,
            Message::HighlightsLoaded(result) => match result {
                Ok(highlights) => self.highlights = highlights,
                Err(error) => self.notice = Some(error),
            },
            Message::PingsLoaded(pings) => self.pings = pings,
            Message::ContentLoaded(id, kind, result) => {
                if self.selected == Some(id) && self.instance_tab.content_kind() == Some(kind) {
                    self.content_loading = false;
                    self.content_scope = Some((id, kind));
                    match result {
                        Ok(entries) => {
                            let jobs = entries
                                .iter()
                                .map(|entry| (entry.path.clone(), entry.is_directory))
                                .collect::<Vec<_>>();
                            self.content_entries = entries;
                            if !jobs.is_empty() {
                                return Task::perform(
                                    content::load_thumbnails(kind, jobs),
                                    move |results| {
                                        Message::ContentThumbnailsLoaded(id, kind, results)
                                    },
                                );
                            }
                        }
                        Err(error) => {
                            self.content_entries.clear();
                            self.notice = Some(error);
                        }
                    }
                }
            }
            Message::ContentQueryChanged(query) => self.content_query = query,
            Message::ContentThumbnailsLoaded(id, kind, results) => {
                if self.selected == Some(id) && self.content_scope == Some((id, kind)) {
                    for (path, thumbnail) in results {
                        if let Some(thumbnail) = thumbnail {
                            self.thumbnails.insert(
                                local_thumbnail_key(&path),
                                image::Handle::from_rgba(
                                    thumbnail::SIZE,
                                    thumbnail::SIZE,
                                    thumbnail.rgba,
                                ),
                            );
                        }
                    }
                }
            }
            Message::ThumbnailsLoaded(results) => {
                for (url, thumbnail) in results {
                    if let Some(thumbnail) = thumbnail {
                        self.thumbnails.insert(
                            url,
                            image::Handle::from_rgba(
                                thumbnail::SIZE,
                                thumbnail::SIZE,
                                thumbnail.rgba,
                            ),
                        );
                    }
                }
            }
            Message::OpenResourceBrowser(kind) => return self.open_resource_browser(kind),
            Message::CloseResourceBrowser => self.resource_browser = None,
            Message::ResourceQueryChanged(value) => {
                if let Some(browser) = self.resource_browser.as_mut() {
                    browser.query = value;
                }
            }
            Message::ResourceProviderPicked(provider) => {
                if let Some(browser) = self.resource_browser.as_mut()
                    && browser.provider != provider
                {
                    browser.provider = provider;
                    browser.selected_project = None;
                    browser.projects.clear();
                    browser.files.clear();
                    browser.error = None;
                    browser.status = None;
                    return self.search_resources();
                }
            }
            Message::SearchResources => return self.search_resources(),
            Message::ResourceSearchLoaded(modal_id, request_id, result) => {
                let mut thumbnail_urls = Vec::new();
                if let Some(browser) = self.resource_browser.as_mut()
                    && browser.id == modal_id
                    && browser.request_id == request_id
                {
                    browser.loading = false;
                    match result {
                        Ok(projects) => {
                            thumbnail_urls = project_thumbnail_urls(&projects);
                            browser.projects = projects;
                            browser.error = None;
                        }
                        Err(error) => {
                            browser.projects.clear();
                            browser.error = Some(error);
                        }
                    }
                }
                return self.load_thumbnails(thumbnail_urls);
            }
            Message::ResourceProjectPicked(project) => {
                return self.select_resource_project(project);
            }
            Message::ResourceBackToProjects => {
                if let Some(browser) = self.resource_browser.as_mut() {
                    browser.selected_project = None;
                    browser.files.clear();
                    browser.error = None;
                    browser.status = None;
                }
            }
            Message::ResourceFilesLoaded(modal_id, request_id, result) => {
                if let Some(browser) = self.resource_browser.as_mut()
                    && browser.id == modal_id
                    && browser.request_id == request_id
                {
                    browser.loading = false;
                    match result {
                        Ok(files) => {
                            browser.files = files;
                            browser.error = None;
                        }
                        Err(error) => {
                            browser.files.clear();
                            browser.error = Some(error);
                        }
                    }
                }
            }
            Message::ResourceFilePicked(file) => return self.download_resource(file),
            Message::ResourceDownloaded(modal_id, instance_id, kind, result) => {
                if let Some(browser) = self.resource_browser.as_mut()
                    && browser.id == modal_id
                {
                    browser.downloading = false;
                    match &result {
                        Ok(path) => {
                            browser.error = None;
                            browser.status = Some(format!(
                                "Installed {}",
                                path.file_name()
                                    .and_then(|name| name.to_str())
                                    .unwrap_or("resource")
                            ));
                        }
                        Err(error) => {
                            browser.status = None;
                            browser.error = Some(error.clone());
                        }
                    }
                } else if let Err(error) = &result {
                    self.notice = Some(error.clone());
                }
                if result.is_ok()
                    && self.selected == Some(instance_id)
                    && self.instance_tab.content_kind() == Some(kind)
                {
                    return Task::batch([self.load_selected_content(), self.refresh_insights()]);
                }
            }
            Message::ModpackQueryChanged(value) => self.modpacks.query = value,
            Message::ModpackProviderPicked(provider) => {
                if self.modpacks.provider != provider {
                    self.modpacks.provider = provider;
                    self.modpacks.selected_project = None;
                    self.modpacks.projects.clear();
                    self.modpacks.files.clear();
                    self.modpacks.error = None;
                    return self.search_modpacks();
                }
            }
            Message::SearchModpacks => return self.search_modpacks(),
            Message::ModpackSearchLoaded(request_id, result) => {
                let mut thumbnail_urls = Vec::new();
                if self.modpacks.request_id == request_id {
                    self.modpacks.loading = false;
                    match result {
                        Ok(projects) => {
                            thumbnail_urls = project_thumbnail_urls(&projects);
                            self.modpacks.projects = projects;
                            self.modpacks.error = None;
                        }
                        Err(error) => {
                            self.modpacks.projects.clear();
                            self.modpacks.error = Some(error);
                        }
                    }
                }
                return self.load_thumbnails(thumbnail_urls);
            }
            Message::ModpackProjectPicked(project) => {
                return self.select_modpack_project(project);
            }
            Message::ModpackBackToProjects => {
                self.modpacks.selected_project = None;
                self.modpacks.files.clear();
                self.modpacks.error = None;
            }
            Message::ModpackFilesLoaded(request_id, result) => {
                if self.modpacks.request_id == request_id {
                    self.modpacks.loading = false;
                    match result {
                        Ok(files) => {
                            self.modpacks.files = files;
                            self.modpacks.error = None;
                        }
                        Err(error) => {
                            self.modpacks.files.clear();
                            self.modpacks.error = Some(error);
                        }
                    }
                }
            }
            Message::ModpackFilePicked(file) => return self.install_online_modpack(file),
            Message::ChooseLocalModpack => {
                return Task::perform(
                    async {
                        rfd::AsyncFileDialog::new()
                            .add_filter("Supported modpacks", &["zip", "mrpack"])
                            .pick_file()
                            .await
                            .map(|file| file.path().to_owned())
                    },
                    Message::LocalModpackPicked,
                );
            }
            Message::LocalModpackPicked(path) => {
                if let Some(path) = path {
                    self.modpacks.local_path = Some(path.clone());
                    self.modpacks.local_plan = None;
                    self.modpacks.local_loading = true;
                    self.modpacks.error = None;
                    let result_path = path.clone();
                    return Task::perform(modpack::inspect_archive(path), move |result| {
                        Message::LocalModpackInspected(result_path.clone(), result)
                    });
                }
            }
            Message::LocalModpackInspected(path, result) => {
                if self.modpacks.local_path.as_ref() == Some(&path) {
                    self.modpacks.local_loading = false;
                    match result {
                        Ok(plan) => {
                            self.modpacks.local_plan = Some(plan);
                            self.modpacks.error = None;
                        }
                        Err(error) => {
                            self.modpacks.local_plan = None;
                            self.modpacks.error = Some(error);
                        }
                    }
                }
            }
            Message::InstallLocalModpack => return self.install_local_modpack(),
            Message::RefreshJava => return Task::perform(java::detect(), Message::JavaLoaded),
            Message::RefreshPings => return Task::perform(load_pings(), Message::PingsLoaded),
            Message::RefreshSystemResources => {
                return Task::perform(system_resources::read(), Message::SystemResourcesLoaded);
            }
            Message::SystemResourcesLoaded(resources) => {
                self.system_resources = resources;
                let worker_limit = resources.cpu_threads.max(1);
                if self.persisted.settings.download.concurrency > worker_limit {
                    self.persisted.settings.download.concurrency = worker_limit;
                    self.save();
                }
            }
            Message::InstanceVersionFilesRepaired => {}
            Message::SelectInstance(id) => {
                self.selected = Some(id);
                self.route = Route::Instances;
                self.content_query.clear();
                return self.load_selected_content();
            }
            Message::SelectInstanceTab(tab) => {
                self.instance_tab = tab;
                return self.load_selected_content();
            }
            Message::EditInstanceName(value) => {
                self.edit_instance(|instance| instance.name = value)
            }
            Message::EditInstanceDescription(value) => {
                self.edit_instance(|instance| instance.description = value)
            }
            Message::EditInstanceColor(value) => {
                self.edit_instance(|instance| instance.color = value)
            }
            Message::ToggleInstanceFavorite(value) => {
                self.edit_instance(|instance| instance.favorite = value)
            }
            Message::SetInstanceIsolation(value) => {
                self.edit_instance(|instance| instance.settings.isolated = value)
            }
            Message::SetInstanceAutoJava(value) => {
                self.edit_instance(|instance| instance.settings.auto_java = value)
            }
            Message::SetInstanceJava(path) => self.edit_instance(|instance| {
                instance.settings.auto_java = false;
                instance.settings.java_path = Some(path);
            }),
            Message::SetInstanceWidth(value) => {
                self.edit_instance(|instance| instance.settings.width = value)
            }
            Message::SetInstanceHeight(value) => {
                self.edit_instance(|instance| instance.settings.height = value)
            }
            Message::SetInstanceFullscreen(value) => {
                self.edit_instance(|instance| instance.settings.fullscreen = value)
            }
            Message::SetInstanceAutoMemory(value) => {
                self.edit_instance(|instance| instance.settings.auto_memory = value)
            }
            Message::SetInstanceMemory(value) => {
                let limit = self.system_resources.memory_limit_mb();
                self.edit_instance(|instance| {
                    instance.settings.max_memory_mb = value.clamp(512, limit)
                })
            }
            Message::SetInstanceWindowTitle(value) => {
                self.edit_instance(|instance| instance.settings.custom_window_title = value)
            }
            Message::SetInstanceCustomInfo(value) => {
                self.edit_instance(|instance| instance.settings.custom_info = value)
            }
            Message::OpenPath(path) => {
                return Task::perform(
                    async move { open::that(path).map_err(|error| error.to_string()) },
                    Message::PathOpened,
                );
            }
            Message::PathOpened(result) => {
                if let Err(error) = result {
                    self.notice = Some(format!("Could not open that path: {error}"));
                }
            }
            Message::OpenFolder(path) => {
                return Task::perform(
                    async move {
                        tokio::fs::create_dir_all(&path)
                            .await
                            .map_err(|error| error.to_string())?;
                        open::that(path).map_err(|error| error.to_string())
                    },
                    Message::FolderOpened,
                );
            }
            Message::FolderOpened(result) => {
                if let Err(error) = result {
                    self.notice = Some(format!("Could not open that folder: {error}"));
                }
            }
            Message::RevealPath(path) => {
                return Task::perform(
                    async move { shell::reveal(path).await.map_err(|error| error.to_string()) },
                    Message::PathRevealed,
                );
            }
            Message::PathRevealed(result) => {
                if let Err(error) = result {
                    self.notice = Some(format!("Could not reveal that file: {error}"));
                }
            }
            Message::OpenExternalUrl(url) => {
                return Task::perform(
                    async move { open::that(url).map_err(|error| error.to_string()) },
                    Message::ExternalUrlOpened,
                );
            }
            Message::ExternalUrlOpened(result) => {
                if let Err(error) = result {
                    self.notice = Some(format!("Could not open that link: {error}"));
                }
            }
            Message::DeleteInstance(id) => {
                if self
                    .persisted
                    .instances
                    .iter()
                    .any(|instance| instance.id == id)
                {
                    self.pending_delete = Some(id);
                }
            }
            Message::CancelDeleteInstance => self.pending_delete = None,
            Message::ConfirmDeleteInstance => {
                if let Some(id) = self.pending_delete.take() {
                    return self.delete_instance(id);
                }
            }
            Message::Deleted(id, result) => match result {
                Ok(()) => {
                    self.persisted
                        .instances
                        .retain(|instance| instance.id != id);
                    self.jobs.remove(&id);
                    if self.selected == Some(id) {
                        self.selected =
                            self.persisted.instances.first().map(|instance| instance.id);
                    }
                    self.save();
                    return self.refresh_insights();
                }
                Err(error) => self.notice = Some(format!("Could not delete instance: {error}")),
            },
            Message::SettingsTabSelected(tab) => self.settings_tab = tab,
            Message::NewInstanceTabSelected(tab) => {
                self.new_instance_tab = tab;
                if tab == NewInstanceTab::Modpacks
                    && self.modpack_tab == ModpackTab::Browse
                    && self.modpacks.projects.is_empty()
                    && !self.modpacks.loading
                {
                    return self.search_modpacks();
                }
            }
            Message::ModpackTabSelected(tab) => {
                self.modpack_tab = tab;
                self.modpacks.error = None;
                if tab == ModpackTab::Browse
                    && self.modpacks.projects.is_empty()
                    && !self.modpacks.loading
                {
                    return self.search_modpacks();
                }
            }
            Message::DownloadSourcePicked(source) => {
                self.persisted.settings.download.source = source;
                self.save();
                let policy = self.persisted.settings.download.clone();
                let loader_catalog = self.refresh_loader_catalog(true);
                return Task::batch([
                    Task::perform(load_versions(policy), Message::VersionsLoaded),
                    loader_catalog,
                ]);
            }
            Message::DownloadConcurrencyChanged(value) => {
                self.persisted.settings.download.concurrency =
                    usize::from(value).clamp(1, self.system_resources.cpu_threads.max(1));
                self.save();
            }
            Message::CancelInstall(id) => {
                if let Some(job) = self.jobs.get_mut(&id) {
                    job.active = false;
                    job.progress.stage = InstallStage::Cancelled;
                    job.progress.detail = "Task cancelled; verified files were kept.".into();
                    job.logs.push("[pipeline] task cancelled by user".into());
                    if let Some(path) = job.log_path.as_deref() {
                        let _ = append_install_log(path, "[cancelled] Task cancelled by user");
                    }
                }
            }
            Message::RetryInstall(id) => self.retry_install(id),
            Message::Pipeline(id, event) => return self.handle_pipeline(id, *event),
            Message::LaunchSelected => return self.launch_selected(),
            Message::LaunchEvent(event) => return self.handle_launch_event(event),
            Message::DismissNotice => self.notice = None,
        }
        Task::none()
    }

    fn add_account(&mut self) {
        let username = self.account_input.trim().to_string();
        if !valid_username(&username) {
            self.notice = Some(
                "Offline usernames must contain 3–16 letters, numbers, or underscores.".into(),
            );
        } else if self
            .persisted
            .accounts
            .iter()
            .any(|account| account.username.eq_ignore_ascii_case(&username))
        {
            self.notice = Some(format!("{username} is already in your account list."));
        } else {
            let account = OfflineAccount::new(&username);
            self.persisted.selected_account = Some(account.uuid);
            self.persisted.accounts.push(account.clone());
            self.persisted.account = Some(account);
            self.account_input.clear();
            self.save();
            self.notice = Some(format!("Offline account {username} added."));
        }
    }

    fn begin_microsoft_login(&mut self) -> Task<Message> {
        if let Some(cancelled) = self.microsoft_login.cancelled.take() {
            cancelled.store(true, Ordering::Relaxed);
        }
        self.microsoft_login.request_id = self.microsoft_login.request_id.wrapping_add(1);
        let request_id = self.microsoft_login.request_id;
        self.microsoft_login.active = true;
        self.microsoft_login.user_code.clear();
        self.microsoft_login.verification_url.clear();
        self.microsoft_login.status = "Requesting a Microsoft device code…".into();
        self.microsoft_login.error = None;
        Task::perform(
            async {
                microsoft::begin_device_authorization()
                    .await
                    .map_err(|error| error.to_string())
            },
            move |result| Message::MicrosoftDeviceAuthorizationLoaded(request_id, result),
        )
    }

    fn upsert_microsoft_account(&mut self, account: OfflineAccount) {
        self.persisted
            .accounts
            .retain(|existing| existing.uuid != account.uuid);
        self.persisted.accounts.push(account.clone());
        self.persisted.selected_account = Some(account.uuid);
        self.persisted.account = Some(account);
        self.save();
    }

    fn edit_instance(&mut self, edit: impl FnOnce(&mut Instance)) {
        if let Some(instance) = self.selected_instance_mut() {
            edit(instance);
            self.save();
        }
    }

    fn delete_instance(&self, id: Uuid) -> Task<Message> {
        let path = self.paths.instance_dir(id);
        Task::perform(
            async move {
                tokio::fs::remove_dir_all(path)
                    .await
                    .or_else(|error| {
                        if error.kind() == std::io::ErrorKind::NotFound {
                            Ok(())
                        } else {
                            Err(error)
                        }
                    })
                    .map_err(|error| error.to_string())
            },
            move |result| Message::Deleted(id, result),
        )
    }

    fn retry_install(&mut self, id: Uuid) {
        if self.jobs.values().any(|job| job.active) {
            self.notice = Some("Another install pipeline is already active.".into());
        } else if let Some(job) = self.jobs.get_mut(&id) {
            if job.request.modpack.is_some() {
                match prepare_modpack_install_log(&self.paths, &job.request) {
                    Ok(path) => job.log_path = Some(path),
                    Err(error) => {
                        self.notice = Some(error);
                        return;
                    }
                }
            }
            job.progress = InstallProgress::default();
            job.logs
                .push("[pipeline] retrying; verified files will be reused".into());
            job.active = true;
        }
    }

    fn handle_pipeline(&mut self, id: Uuid, event: PipelineEvent) -> Task<Message> {
        let mut finished = None;
        if let Some(job) = self.jobs.get_mut(&id) {
            match event {
                PipelineEvent::Progress(progress) => job.progress = progress,
                PipelineEvent::ResolvedMetadata {
                    minecraft_version,
                    loader,
                } => {
                    job.request.minecraft_version = minecraft_version;
                    job.request.loader = loader;
                }
                PipelineEvent::Log(line) => {
                    job.logs.push(line);
                    if job.logs.len() > 400 {
                        job.logs.drain(..100);
                    }
                }
                PipelineEvent::Finished(instance) => {
                    job.active = false;
                    job.progress.stage = InstallStage::Complete;
                    job.log_path = None;
                    finished = Some(*instance);
                }
                PipelineEvent::Failed { stage, message } => {
                    job.active = false;
                    job.progress.stage = InstallStage::Failed;
                    job.progress.detail = format!("{}: {message}", stage.label());
                    job.logs.push(format!("[failed] {message}"));
                }
            }
        }
        if let Some(instance) = finished {
            self.persisted.instances.retain(|old| old.id != instance.id);
            self.persisted.instances.push(instance);
            self.save();
            self.notice = Some("Instance installed. It is ready to launch.".into());
            return self.refresh_insights();
        }
        Task::none()
    }

    fn launch_selected(&mut self) -> Task<Message> {
        let Some(account) = self.persisted.active_account().cloned() else {
            self.notice = Some("Sign in or select a player profile first.".into());
            return Task::none();
        };
        if self.launching {
            self.notice = Some("A Minecraft process is already active.".into());
        } else if account.provider == AccountProvider::Microsoft && !microsoft::is_configured() {
            self.notice = Some(format!(
                "Microsoft launch requires {} in .env.",
                microsoft::CLIENT_ID_ENV
            ));
        } else if account.token_needs_refresh(now_unix().unwrap_or_default()) {
            self.launching = true;
            self.notice = Some("Refreshing Microsoft sign-in…".into());
            let id = account.uuid;
            return Task::perform(
                async move {
                    microsoft::refresh_account(&account)
                        .await
                        .map_err(|error| error.to_string())
                },
                move |result| Message::MicrosoftAccountRefreshedForLaunch(id, result),
            );
        } else if let Some(instance) = self.selected_instance().cloned() {
            self.launching = true;
            self.launch_session = Some(LaunchSession {
                instance_id: instance.id,
                status: "Preparing Java, libraries, natives, and launch arguments…".into(),
                logs: Vec::new(),
                log_path: None,
                pid: None,
                ready: false,
                active: true,
                failed: false,
                ready_at: None,
            });
            self.launch_request = Some(LaunchKey {
                attempt: Uuid::new_v4(),
                instance,
                account,
                paths: self.paths.clone(),
            });
        }
        Task::none()
    }

    fn handle_launch_event(&mut self, event: launcher::LaunchEvent) -> Task<Message> {
        let mut play_time = None;
        if let Some(session) = self.launch_session.as_mut() {
            match event {
                launcher::LaunchEvent::Started(result) => {
                    session.pid = Some(result.pid);
                    session.log_path = Some(result.log_path);
                    session.status = format!(
                        "Process created · PID {} · Java {} · waiting for game readiness",
                        result.pid, result.java.major
                    );
                }
                launcher::LaunchEvent::Log(line) => {
                    session.logs.push(line);
                    if session.logs.len() > 700 {
                        session.logs.drain(..150);
                    }
                }
                launcher::LaunchEvent::Ready => {
                    session.ready = true;
                    session.ready_at = Some(Instant::now());
                    session.status =
                        "Render thread detected · Minecraft started successfully".into();
                    self.notice = Some("Minecraft started successfully.".into());
                }
                launcher::LaunchEvent::Exited {
                    code,
                    ready,
                    log_path,
                } => {
                    session.active = false;
                    session.log_path = Some(log_path);
                    self.launching = false;
                    self.launch_request = None;
                    if let Some(started) = session.ready_at {
                        play_time = Some((session.instance_id, started.elapsed().as_secs()));
                    }
                    if ready && code == Some(0) {
                        session.status = "Minecraft exited normally.".into();
                    } else {
                        session.failed = true;
                        let code = code.map_or_else(|| "unknown".into(), |code| code.to_string());
                        session.status = if ready {
                            format!("Minecraft exited unexpectedly · exit code {code}")
                        } else {
                            format!("Minecraft exited before startup completed · exit code {code}")
                        };
                        self.notice = Some(
                            "Minecraft exited with an error. The launch log is shown here.".into(),
                        );
                    }
                }
                launcher::LaunchEvent::Failed { message, log_path } => {
                    session.active = false;
                    session.failed = true;
                    session.status = format!("Launch failed: {message}");
                    session.log_path = log_path;
                    session.logs.push(format!("[AZULC] {message}"));
                    self.launching = false;
                    self.launch_request = None;
                    self.notice =
                        Some("Minecraft launch failed. The detailed log is shown here.".into());
                }
            }
        }
        if let Some((id, seconds)) = play_time
            && let Some(instance) = self
                .persisted
                .instances
                .iter_mut()
                .find(|instance| instance.id == id)
        {
            instance.play_time_seconds = instance.play_time_seconds.saturating_add(seconds);
            instance.last_played_unix = now_unix();
            self.save();
            return self.refresh_insights();
        }
        Task::none()
    }

    fn wizard_can_open(&self, step: WizardStep) -> bool {
        match step {
            WizardStep::Version => true,
            WizardStep::Loader => self.wizard.selected_version.is_some(),
            WizardStep::Details => {
                self.wizard.selected_version.is_some()
                    && (self.wizard.loader == LoaderKind::Vanilla
                        || !self.wizard.loader_version.trim().is_empty())
            }
        }
    }

    fn refresh_loader_catalog(&mut self, force: bool) -> Task<Message> {
        let Some(minecraft_version) = self.wizard.selected_version.clone() else {
            self.loader_catalog.clear();
            return Task::none();
        };
        if self.wizard.loader == LoaderKind::Vanilla {
            self.loader_catalog.clear();
            return Task::none();
        }

        let key = LoaderCatalogKey {
            minecraft_version,
            loader: self.wizard.loader,
            source: self.persisted.settings.download.source,
        };
        let is_current = self.loader_catalog.key.as_ref() == Some(&key);
        if !force
            && is_current
            && (self.loader_catalog.loading
                || (!self.loader_catalog.entries.is_empty() && self.loader_catalog.error.is_none()))
        {
            return Task::none();
        }

        let request_id = self.loader_catalog.begin(key.clone());
        let result_key = key.clone();
        Task::perform(loader_catalog::fetch(key), move |result| {
            Message::LoaderCatalogLoaded(request_id, result_key.clone(), result)
        })
    }

    fn start_install(&mut self) -> Task<Message> {
        if self.jobs.values().any(|job| job.active) {
            self.notice = Some(
                "Another install pipeline is active. Shared files are installed one at a time."
                    .into(),
            );
            return Task::none();
        }
        let Some(version) = self.wizard.selected_version.clone() else {
            self.notice = Some("Choose a Minecraft version first.".into());
            return Task::none();
        };
        if self.wizard.loader != LoaderKind::Vanilla && self.wizard.loader_version.trim().is_empty()
        {
            self.notice = Some("Choose a loader build first.".into());
            return Task::none();
        }
        let name = self.wizard.name.trim().to_string();
        if name.is_empty() {
            self.notice = Some("Give the instance a name.".into());
            return Task::none();
        }
        let id = Uuid::new_v4();
        let request = InstallRequest {
            instance_id: id,
            name,
            description: self.wizard.description.trim().to_string(),
            color: self.wizard.color,
            minecraft_version: version,
            loader: LoaderSpec {
                kind: self.wizard.loader,
                version: (!self.wizard.loader_version.trim().is_empty())
                    .then(|| self.wizard.loader_version.trim().to_string()),
            },
            settings: self.persisted.settings.game.clone(),
            download_policy: self.persisted.settings.download.clone(),
            modpack: None,
        };
        self.jobs.insert(
            id,
            InstallJob {
                request,
                progress: InstallProgress::default(),
                logs: vec!["[pipeline] install task created".into()],
                log_path: None,
                active: true,
            },
        );
        self.reset_wizard();
        self.selected = Some(id);
        self.route = Route::Instances;
        self.instance_tab = InstanceTab::Overview;
        Task::none()
    }

    fn reset_wizard(&mut self) {
        self.wizard = WizardDraft::default();
        self.wizard_step = WizardStep::Version;
        self.version_filter = VersionFilter::Release;
        self.loader_catalog.clear();
        if let Some(version) = self
            .versions
            .iter()
            .find(|version| version.kind == "release")
            .or_else(|| self.versions.first())
        {
            self.wizard.color = VersionFilter::for_version(version).color();
            self.wizard.selected_version = Some(version.id.clone());
        }
    }

    fn load_selected_content(&mut self) -> Task<Message> {
        let Some(kind) = self.instance_tab.content_kind() else {
            self.content_loading = false;
            return Task::none();
        };
        let Some(instance) = self.selected_instance().cloned() else {
            self.content_entries.clear();
            self.content_scope = None;
            self.content_loading = false;
            return Task::none();
        };
        self.content_loading = true;
        self.content_entries.clear();
        let id = instance.id;
        Task::perform(
            async move {
                content::scan_content(&instance, kind)
                    .await
                    .map_err(|error| format!("Could not scan {}: {error}", kind_label(kind)))
            },
            move |result| Message::ContentLoaded(id, kind, result),
        )
    }

    fn open_resource_browser(&mut self, kind: ContentKind) -> Task<Message> {
        if !kind.downloadable() {
            self.notice = Some("This content page does not support catalog downloads.".into());
            return Task::none();
        }
        let Some(instance) = self.selected_instance().cloned() else {
            self.notice = Some("Select an installed instance first.".into());
            return Task::none();
        };
        self.next_modal_id = self.next_modal_id.wrapping_add(1);
        self.resource_browser = Some(ResourceBrowserState::new(
            self.next_modal_id,
            instance.id,
            kind,
        ));
        self.search_resources()
    }

    fn search_resources(&mut self) -> Task<Message> {
        let Some(browser) = self.resource_browser.as_mut() else {
            return Task::none();
        };
        let Some(resource_class) = resource_class(browser.kind) else {
            browser.loading = false;
            browser.error = Some("This content type is not available from online catalogs.".into());
            return Task::none();
        };
        let Some(instance) = self
            .persisted
            .instances
            .iter()
            .find(|instance| instance.id == browser.instance_id)
            .cloned()
        else {
            browser.loading = false;
            browser.error = Some("The selected instance no longer exists.".into());
            return Task::none();
        };

        browser.request_id = browser.request_id.wrapping_add(1);
        browser.loading = true;
        browser.error = None;
        browser.status = None;
        browser.selected_project = None;
        browser.files.clear();
        let modal_id = browser.id;
        let request_id = browser.request_id;
        let provider = browser.provider;
        let kind = browser.kind;
        let game_version = instance.minecraft_version;
        let loader_kind = instance.loader.kind;
        let search_filter = browser.query.trim().to_owned();
        let mut query = SearchQuery::new(resource_class);
        query.game_version = Some(game_version.clone());
        query.search_filter = (!search_filter.is_empty()).then(|| search_filter.clone());
        if kind == ContentKind::Mods {
            query.mod_loader = curseforge_loader(loader_kind);
        }

        Task::perform(
            async move {
                match provider {
                    CatalogProvider::CurseForge => {
                        let client =
                            CurseForgeClient::from_env().map_err(|error| error.to_string())?;
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
                        let content_type = modrinth_content_type(kind).ok_or_else(|| {
                            "This content type is not available from Modrinth.".to_string()
                        })?;
                        let mut query = ModrinthSearchQuery::new(content_type);
                        query.query = search_filter;
                        query.game_version = Some(game_version);
                        query.loader = modrinth_loader(kind, loader_kind);
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
            },
            move |result| Message::ResourceSearchLoaded(modal_id, request_id, result),
        )
    }

    fn select_resource_project(&mut self, project: CatalogProject) -> Task<Message> {
        let Some(browser) = self.resource_browser.as_mut() else {
            return Task::none();
        };
        if !browser
            .projects
            .iter()
            .any(|candidate| candidate.key == project.key)
            || project.key.provider() != browser.provider
        {
            return Task::none();
        }
        let Some(instance) = self
            .persisted
            .instances
            .iter()
            .find(|instance| instance.id == browser.instance_id)
            .cloned()
        else {
            browser.error = Some("The selected instance no longer exists.".into());
            return Task::none();
        };

        browser.request_id = browser.request_id.wrapping_add(1);
        browser.selected_project = Some(project.clone());
        browser.files.clear();
        browser.loading = true;
        browser.error = None;
        browser.status = None;
        let modal_id = browser.id;
        let request_id = browser.request_id;
        let provider = browser.provider;
        let kind = browser.kind;
        let project_key = project.key.clone();
        let game_version = instance.minecraft_version;
        let loader_kind = instance.loader.kind;
        let mut query = FileQuery {
            game_version: Some(game_version.clone()),
            ..FileQuery::default()
        };
        if kind == ContentKind::Mods {
            query.mod_loader = curseforge_loader(loader_kind);
        }

        Task::perform(
            async move {
                match (provider, project_key) {
                    (CatalogProvider::CurseForge, CatalogProjectKey::CurseForge(project_id)) => {
                        let client =
                            CurseForgeClient::from_env().map_err(|error| error.to_string())?;
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
                            game_version,
                            modrinth_loader(kind, loader_kind),
                        );
                        let versions = client
                            .list_versions(&project_id, &query)
                            .await
                            .map_err(|error| error.to_string())?;
                        Ok(CatalogRelease::from_modrinth_versions(versions))
                    }
                    _ => Err("The selected project belongs to a different provider.".into()),
                }
            },
            move |result| Message::ResourceFilesLoaded(modal_id, request_id, result),
        )
    }

    fn download_resource(&mut self, file: CatalogRelease) -> Task<Message> {
        let Some(browser) = self.resource_browser.as_mut() else {
            return Task::none();
        };
        let Some(project) = browser.selected_project.clone() else {
            browser.error = Some("Choose a project first.".into());
            return Task::none();
        };
        let matching_project = match (&project.key, &file.key) {
            (
                CatalogProjectKey::CurseForge(project_id),
                CatalogReleaseKey::CurseForge {
                    project_id: file_project_id,
                    ..
                },
            ) => project_id == file_project_id,
            (
                CatalogProjectKey::Modrinth(project_id),
                CatalogReleaseKey::Modrinth {
                    project_id: file_project_id,
                    ..
                },
            ) => project_id == file_project_id,
            _ => false,
        } && project.key.provider() == file.key.provider();
        if !matching_project || !project.available || !file.available || browser.downloading {
            return Task::none();
        }
        let Some(instance) = self
            .persisted
            .instances
            .iter()
            .find(|instance| instance.id == browser.instance_id)
            .cloned()
        else {
            browser.error = Some("The selected instance no longer exists.".into());
            return Task::none();
        };
        let directory = instance.game_dir.join(
            browser
                .kind
                .directory()
                .expect("downloadable instance content has a directory"),
        );
        let data_pack_directory = instance.game_dir.join("datapacks");
        let modal_id = browser.id;
        let instance_id = browser.instance_id;
        let kind = browser.kind;
        let game_directory = instance.game_dir.clone();
        let game_version = instance.minecraft_version.clone();
        let loader_kind = instance.loader.kind;
        let mod_loader = curseforge_loader(instance.loader.kind);
        let concurrency = self.persisted.settings.download.concurrency;
        let expected_resource_class = resource_class(kind)
            .expect("downloadable instance content has a CurseForge resource class");
        let release_key = file.key.clone();
        browser.downloading = true;
        browser.error = None;
        browser.status = Some(format!(
            "Resolving required dependencies for {}…",
            file.display_name
        ));

        Task::perform(
            async move {
                match release_key {
                    CatalogReleaseKey::CurseForge {
                        project_id,
                        file_id,
                    } => {
                        let client =
                            CurseForgeClient::from_env().map_err(|error| error.to_string())?;
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
                            .install_resource_with_dependencies(ResourceInstallRequest {
                                project,
                                file,
                                expected_resource_class,
                                primary_destination: directory.join(file_name),
                                game_directory,
                                data_pack_directory: data_pack_directory.clone(),
                                game_version,
                                mod_loader,
                                concurrency,
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
                            game_version,
                            modrinth_loader(kind, loader_kind),
                        );
                        let resolved = client
                            .resolve_installation(&project_id, &version_id, &dependency_query)
                            .await
                            .map_err(|error| error.to_string())?;
                        let expected_type = modrinth_content_type(kind)
                            .ok_or_else(|| "Unsupported Modrinth content type.".to_string())?;
                        if resolved
                            .first()
                            .is_none_or(|root| root.project_type != expected_type)
                        {
                            return Err(format!(
                                "The selected Modrinth release is not a {} project.",
                                expected_type
                            ));
                        }

                        let mut primary_destination = None;
                        let mut specs = Vec::with_capacity(resolved.len());
                        for (index, item) in resolved.into_iter().enumerate() {
                            let destination_directory = if index == 0 {
                                directory.clone()
                            } else {
                                match item.project_type {
                                    ModrinthContentType::Mod => game_directory.join("mods"),
                                    ModrinthContentType::ResourcePack => {
                                        game_directory.join("resourcepacks")
                                    }
                                    ModrinthContentType::Shader => {
                                        game_directory.join("shaderpacks")
                                    }
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
                        download::download_batch(
                            client.download_client(),
                            specs,
                            concurrency,
                            |_| {},
                        )
                        .await
                        .map_err(|error| error.to_string())?;
                        primary_destination.ok_or_else(|| {
                            "Modrinth returned an empty installation plan.".to_string()
                        })
                    }
                }
            },
            move |result| Message::ResourceDownloaded(modal_id, instance_id, kind, result),
        )
    }

    fn search_modpacks(&mut self) -> Task<Message> {
        self.modpacks.request_id = self.modpacks.request_id.wrapping_add(1);
        self.modpacks.loading = true;
        self.modpacks.error = None;
        self.modpacks.selected_project = None;
        self.modpacks.files.clear();
        let request_id = self.modpacks.request_id;
        let provider = self.modpacks.provider;
        let search_filter = self.modpacks.query.trim().to_owned();
        let mut query = SearchQuery::new(ResourceClass::Modpack);
        query.search_filter = (!search_filter.is_empty()).then(|| search_filter.clone());

        Task::perform(
            async move {
                match provider {
                    CatalogProvider::CurseForge => {
                        let client =
                            CurseForgeClient::from_env().map_err(|error| error.to_string())?;
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
            },
            move |result| Message::ModpackSearchLoaded(request_id, result),
        )
    }

    fn select_modpack_project(&mut self, project: CatalogProject) -> Task<Message> {
        if !self
            .modpacks
            .projects
            .iter()
            .any(|candidate| candidate.key == project.key)
            || project.key.provider() != self.modpacks.provider
        {
            return Task::none();
        }
        self.modpacks.request_id = self.modpacks.request_id.wrapping_add(1);
        self.modpacks.loading = true;
        self.modpacks.error = None;
        self.modpacks.selected_project = Some(project.clone());
        self.modpacks.files.clear();
        let request_id = self.modpacks.request_id;
        let project_key = project.key;
        Task::perform(
            async move {
                match project_key {
                    CatalogProjectKey::CurseForge(project_id) => {
                        let client =
                            CurseForgeClient::from_env().map_err(|error| error.to_string())?;
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
            },
            move |result| Message::ModpackFilesLoaded(request_id, result),
        )
    }

    fn install_online_modpack(&mut self, file: CatalogRelease) -> Task<Message> {
        let Some(project) = self.modpacks.selected_project.as_ref() else {
            self.modpacks.error = Some("Choose a modpack first.".into());
            return Task::none();
        };
        let matches_project = match (&project.key, &file.key) {
            (
                CatalogProjectKey::CurseForge(project_id),
                CatalogReleaseKey::CurseForge {
                    project_id: release_project_id,
                    ..
                },
            ) => project_id == release_project_id,
            (
                CatalogProjectKey::Modrinth(project_id),
                CatalogReleaseKey::Modrinth {
                    project_id: release_project_id,
                    ..
                },
            ) => project_id == release_project_id,
            _ => false,
        };
        if !matches_project || !project.available || !file.available {
            self.modpacks.error = Some("That modpack file is no longer available.".into());
            return Task::none();
        }
        let minecraft_version = file
            .game_versions
            .iter()
            .find(|candidate| {
                self.versions
                    .iter()
                    .any(|version| version.id == candidate.as_str())
            })
            .cloned()
            .unwrap_or_else(|| "resolving".into());
        let source = match &file.key {
            CatalogReleaseKey::CurseForge {
                project_id,
                file_id,
            } => ModpackSource::CurseForge {
                project_id: *project_id,
                file_id: *file_id,
                file_name: file.file_name.clone(),
            },
            CatalogReleaseKey::Modrinth {
                project_id,
                version_id,
            } => ModpackSource::Modrinth {
                project_id: project_id.clone(),
                version_id: version_id.clone(),
                file_name: file.file_name.clone(),
            },
        };
        let request = ModpackInstallSpec {
            source,
            project_name: project.name.clone(),
            version_name: Some(file.display_name.clone()),
        };
        self.start_modpack_install(
            request,
            minecraft_version,
            LoaderSpec {
                kind: LoaderKind::Vanilla,
                version: None,
            },
        )
    }

    fn install_local_modpack(&mut self) -> Task<Message> {
        let Some(path) = self.modpacks.local_path.clone() else {
            self.modpacks.error = Some("Choose a modpack archive first.".into());
            return Task::none();
        };
        let Some(plan) = self.modpacks.local_plan.as_ref() else {
            self.modpacks.error = Some("Wait for the archive inspection to finish.".into());
            return Task::none();
        };
        let request = ModpackInstallSpec {
            source: ModpackSource::Local { archive: path },
            project_name: plan.metadata.name.clone(),
            version_name: plan.metadata.version.clone(),
        };
        self.start_modpack_install(
            request,
            plan.metadata.minecraft_version.clone(),
            plan.metadata.loader.clone(),
        )
    }

    fn start_modpack_install(
        &mut self,
        modpack: ModpackInstallSpec,
        minecraft_version: String,
        loader: LoaderSpec,
    ) -> Task<Message> {
        if self.jobs.values().any(|job| job.active) {
            self.notice = Some(
                "Another install pipeline is active. Shared files are installed one at a time."
                    .into(),
            );
            return Task::none();
        }
        let id = Uuid::new_v4();
        let version = modpack
            .version_name
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("selected release");
        let request = InstallRequest {
            instance_id: id,
            name: modpack.project_name.clone(),
            description: format!("Modpack // {version}"),
            color: InstanceColor::default(),
            minecraft_version,
            loader,
            settings: self.persisted.settings.game.clone(),
            download_policy: self.persisted.settings.download.clone(),
            modpack: Some(modpack),
        };
        let log_path = match prepare_modpack_install_log(&self.paths, &request) {
            Ok(path) => path,
            Err(error) => {
                self.notice = Some(error);
                return Task::none();
            }
        };
        self.jobs.insert(
            id,
            InstallJob {
                request,
                progress: InstallProgress::default(),
                logs: vec!["[pipeline] continuous modpack install created".into()],
                log_path: Some(log_path),
                active: true,
            },
        );
        self.selected = Some(id);
        self.route = Route::Instances;
        self.instance_tab = InstanceTab::Overview;
        Task::none()
    }

    fn refresh_insights(&self) -> Task<Message> {
        let instances = self.persisted.instances.clone();
        Task::perform(
            async move { insights::scan_instances(&instances).await },
            Message::InsightsLoaded,
        )
    }

    fn load_thumbnails(&mut self, urls: Vec<String>) -> Task<Message> {
        let missing = urls
            .into_iter()
            .filter(|url| !url.trim().is_empty())
            .filter(|url| !self.thumbnails.contains_key(url))
            .filter(|url| self.requested_thumbnails.insert(url.clone()))
            .collect::<Vec<_>>();
        if missing.is_empty() {
            Task::none()
        } else {
            Task::perform(
                thumbnail::fetch_remote_batch(missing),
                Message::ThumbnailsLoaded,
            )
        }
    }

    pub(crate) fn catalog_thumbnail(&self, project: &CatalogProject) -> Option<&image::Handle> {
        project
            .icon_url
            .as_ref()
            .and_then(|url| self.thumbnails.get(url))
    }

    pub(crate) fn content_thumbnail(&self, entry: &ContentEntry) -> Option<&image::Handle> {
        self.thumbnails.get(&local_thumbnail_key(&entry.path))
    }

    pub(crate) fn selected_instance(&self) -> Option<&Instance> {
        let id = self.selected?;
        self.persisted
            .instances
            .iter()
            .find(|instance| instance.id == id)
    }

    fn selected_instance_mut(&mut self) -> Option<&mut Instance> {
        let id = self.selected?;
        self.persisted
            .instances
            .iter_mut()
            .find(|instance| instance.id == id)
    }

    fn save(&mut self) {
        if let Err(error) = self.paths.save(&self.persisted) {
            self.notice = Some(format!("Could not save launcher data: {error}"));
        }
    }
}

fn pipeline_stream(key: &PipelineKey) -> impl futures::Stream<Item = PipelineEvent> + use<> {
    let key = key.clone();
    iced::stream::channel(64, async move |mut output| {
        let log_path = key
            .request
            .modpack
            .as_ref()
            .map(|_| modpack_install_log_path(&key.paths, key.request.instance_id));
        let mut install_log = if let Some(path) = log_path.as_deref() {
            tokio::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .await
                .ok()
        } else {
            None
        };
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let pipeline = installer::run(key.request, key.paths, tx);
        tokio::pin!(pipeline);
        loop {
            tokio::select! {
                _ = &mut pipeline => {
                    while let Some(event) = rx.recv().await {
                        let terminal = matches!(event, PipelineEvent::Finished(_) | PipelineEvent::Failed { .. });
                        record_install_event(&mut install_log, log_path.as_deref(), &event).await;
                        if output.send(event).await.is_err() || terminal { break; }
                    }
                    break;
                }
                event = rx.recv() => match event {
                    Some(event) => {
                        let terminal = matches!(event, PipelineEvent::Finished(_) | PipelineEvent::Failed { .. });
                        record_install_event(&mut install_log, log_path.as_deref(), &event).await;
                        if output.send(event).await.is_err() || terminal { break; }
                    }
                    None => break,
                }
            }
        }
    })
}

fn modpack_install_log_path(paths: &Paths, id: Uuid) -> PathBuf {
    paths
        .instance_dir(id)
        .join(".azulc")
        .join("latest-install.log")
}

fn prepare_modpack_install_log(paths: &Paths, request: &InstallRequest) -> Result<PathBuf, String> {
    let path = modpack_install_log_path(paths, request.instance_id);
    let parent = path
        .parent()
        .ok_or_else(|| "Could not determine the modpack install log directory.".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Could not create the modpack install log directory: {error}"))?;
    let started_at = chrono::Local::now().to_rfc3339();
    let loader_version = request.loader.version.as_deref().unwrap_or("none");
    let header = format!(
        "AZULC modpack installation log\nStarted: {started_at}\nInstance: {}\nMinecraft: {}\nLoader: {} {}\n\n",
        request.name, request.minecraft_version, request.loader.kind, loader_version
    );
    fs::write(&path, header)
        .map_err(|error| format!("Could not create the modpack install log: {error}"))?;
    Ok(path)
}

fn append_install_log(path: &Path, line: &str) -> std::io::Result<()> {
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(
        file,
        "{} {line}",
        chrono::Local::now().format("[%Y-%m-%d %H:%M:%S]")
    )
}

async fn record_install_event(
    log: &mut Option<tokio::fs::File>,
    log_path: Option<&Path>,
    event: &PipelineEvent,
) {
    if let Some(file) = log.as_mut() {
        let line = format_install_event(event);
        let timestamp = chrono::Local::now().format("[%Y-%m-%d %H:%M:%S]");
        let _ = file
            .write_all(format!("{timestamp} {line}\n").as_bytes())
            .await;
        let _ = file.flush().await;
    }

    if matches!(event, PipelineEvent::Finished(_)) {
        if let Some(mut file) = log.take() {
            let _ = file.flush().await;
            drop(file);
        }
        if let Some(path) = log_path {
            let _ = tokio::fs::remove_file(path).await;
        }
    }
}

fn format_install_event(event: &PipelineEvent) -> String {
    match event {
        PipelineEvent::Progress(progress) => format!(
            "[progress] {} | {} | files {}/{} | bytes {}/{} | {:.0} B/s",
            progress.stage.label(),
            progress.detail,
            progress.files_done,
            progress.files_total,
            progress.current,
            progress.total,
            progress.bytes_per_second
        ),
        PipelineEvent::ResolvedMetadata {
            minecraft_version,
            loader,
        } => format!(
            "[metadata] Minecraft {} | {} {}",
            minecraft_version,
            loader.kind,
            loader.version.as_deref().unwrap_or("none")
        ),
        PipelineEvent::Log(line) => line.clone(),
        PipelineEvent::Finished(instance) => {
            format!(
                "[complete] Instance {} installed successfully",
                instance.name
            )
        }
        PipelineEvent::Failed { stage, message } => {
            format!("[failed] {}: {message}", stage.label())
        }
    }
}

fn pipeline_message((id, event): (Uuid, PipelineEvent)) -> Message {
    Message::Pipeline(id, Box::new(event))
}

fn launch_stream(key: &LaunchKey) -> impl futures::Stream<Item = launcher::LaunchEvent> + use<> {
    let key = key.clone();
    iced::stream::channel(256, async move |mut output| {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let monitor = launcher::monitor(key.instance, key.account, key.paths, tx);
        tokio::pin!(monitor);
        loop {
            tokio::select! {
                _ = &mut monitor => {
                    while let Some(event) = rx.recv().await {
                        let terminal = matches!(event, launcher::LaunchEvent::Exited { .. } | launcher::LaunchEvent::Failed { .. });
                        if output.send(event).await.is_err() || terminal { break; }
                    }
                    break;
                }
                event = rx.recv() => match event {
                    Some(event) => {
                        let terminal = matches!(event, launcher::LaunchEvent::Exited { .. } | launcher::LaunchEvent::Failed { .. });
                        if output.send(event).await.is_err() || terminal { break; }
                    }
                    None => break,
                }
            }
        }
    })
}

async fn load_versions(policy: DownloadPolicy) -> Result<Vec<minecraft::VersionEntry>, String> {
    let client = reqwest::Client::builder()
        .user_agent("AZULC/0.1.0")
        .build()
        .map_err(|error| error.to_string())?;
    let router = SourceRouter::from_policy(&policy);
    let manifest = minecraft::fetch_manifest_with_router(&client, router)
        .await
        .map_err(|error| format!("Could not retrieve the Minecraft catalog: {error}"))?;
    let mut versions = manifest.versions;
    for highlighted in [manifest.latest.snapshot, manifest.latest.release] {
        if let Some(position) = versions
            .iter()
            .position(|version| version.id == highlighted)
        {
            let version = versions.remove(position);
            versions.insert(0, version);
        }
    }
    Ok(versions)
}

async fn repair_instance_version_files(paths: Paths, instances: Vec<Instance>) {
    for instance in instances {
        let base = paths
            .minecraft
            .join("versions")
            .join(&instance.minecraft_version);
        let client = base.join(format!("{}.jar", instance.minecraft_version));
        let metadata = base.join(format!("{}.json", instance.minecraft_version));
        let profile_ready = instance.version_id == instance.minecraft_version
            || paths
                .minecraft
                .join("versions")
                .join(&instance.version_id)
                .join(format!("{}.json", instance.version_id))
                .is_file();
        if client.is_file() && metadata.is_file() && profile_ready {
            let _ = installer::materialize_instance_version_files(
                &paths.minecraft,
                &instance.game_dir,
                &instance.minecraft_version,
                &instance.version_id,
            )
            .await;
        }
    }
}

async fn load_highlights() -> Result<VersionHighlights, String> {
    let client = reqwest::Client::builder()
        .user_agent("AZULC/0.1.0")
        .build()
        .map_err(|error| error.to_string())?;
    insights::fetch_version_highlights(&client)
        .await
        .map_err(|error| error.to_string())
}

async fn load_pings() -> Vec<ServicePing> {
    match reqwest::Client::builder().user_agent("AZULC/0.1.0").build() {
        Ok(client) => insights::ping_services(&client).await,
        Err(_) => Vec::new(),
    }
}

fn valid_username(name: &str) -> bool {
    (3..=16).contains(&name.len())
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
}

fn now_unix() -> Option<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs())
}

fn local_thumbnail_key(path: &std::path::Path) -> String {
    format!("local:{}", path.to_string_lossy())
}

fn kind_label(kind: ContentKind) -> &'static str {
    match kind {
        ContentKind::Worlds => "worlds",
        ContentKind::Mods => "mods",
        ContentKind::ResourcePacks => "resource packs",
        ContentKind::ShaderPacks => "shader packs",
        ContentKind::DataPacks => "data packs",
        ContentKind::Screenshots => "screenshots",
    }
}

fn safe_catalog_file_name(value: &str) -> Result<String, String> {
    crate::services::path_safety::file_name(value)
        .ok_or_else(|| "The provider returned an unsafe file name.".into())
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
mod version_filter_tests {
    use super::*;

    fn version(kind: &str, release_time: &str) -> minecraft::VersionEntry {
        minecraft::VersionEntry {
            id: "test".into(),
            kind: kind.into(),
            release_time: release_time.into(),
            url: "https://example.invalid/version.json".into(),
            sha1: String::new(),
        }
    }

    #[test]
    fn version_channels_have_stable_instance_colors() {
        assert_eq!(
            VersionFilter::for_version(&version("release", "2026-09-04T00:00:00Z")).color(),
            InstanceColor::Lavender
        );
        assert_eq!(
            VersionFilter::for_version(&version("snapshot", "2026-09-04T00:00:00Z")).color(),
            InstanceColor::Sky
        );
        assert_eq!(
            VersionFilter::for_version(&version("old_beta", "2011-01-01T00:00:00Z")).color(),
            InstanceColor::Amber
        );
        assert_eq!(
            VersionFilter::for_version(&version("snapshot", "2026-04-01T00:00:00Z")).color(),
            InstanceColor::Rose
        );
    }

    #[tokio::test]
    async fn modpack_install_log_is_kept_on_failure_and_removed_on_success() {
        let fixture = std::env::temp_dir().join(format!("azulc-install-log-{}", Uuid::new_v4()));
        fs::create_dir_all(&fixture).unwrap();
        let path = fixture.join("latest-install.log");
        fs::write(&path, "header\n").unwrap();
        let mut log = Some(
            tokio::fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .await
                .unwrap(),
        );

        record_install_event(
            &mut log,
            Some(&path),
            &PipelineEvent::Failed {
                stage: InstallStage::DownloadingModpackContent,
                message: "fixture failure".into(),
            },
        )
        .await;
        assert!(path.exists());
        assert!(
            fs::read_to_string(&path)
                .unwrap()
                .contains("fixture failure")
        );

        let instance = Instance {
            id: Uuid::new_v4(),
            name: "Fixture Pack".into(),
            minecraft_version: "1.21.1".into(),
            version_id: "1.21.1".into(),
            loader: LoaderSpec {
                kind: LoaderKind::Fabric,
                version: Some("0.16.0".into()),
            },
            game_dir: fixture.clone(),
            installed: true,
            description: String::new(),
            color: InstanceColor::default(),
            favorite: false,
            play_time_seconds: 0,
            last_played_unix: None,
            settings: Default::default(),
            origin: Default::default(),
        };
        record_install_event(
            &mut log,
            Some(&path),
            &PipelineEvent::Finished(Box::new(instance)),
        )
        .await;
        assert!(!path.exists());

        fs::remove_dir_all(fixture).unwrap();
    }
}
