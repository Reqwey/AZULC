//! Application state composition, startup, and long-lived subscriptions.

mod accounts;
mod bootstrap;
mod install;
mod instance;
mod launch;
mod message;
pub(crate) mod navigation;
mod thumbnails;
mod update;

pub(crate) use install::{InstallJob, ModpackBrowserState};
pub(crate) use instance::ResourceBrowserState;
pub(crate) use launch::{LaunchAuthPhase, LaunchAuthState, LaunchSession};
pub use message::Message;

use accounts::MicrosoftLoginState;
use install::{LoaderCatalogState, PipelineKey, WizardDraft};

use self::{
    launch::{LaunchAuthentication, LaunchRegistry},
    navigation::{ModpackTab, NewInstanceTab, Route, SettingsTab, VersionFilter, WizardStep},
};
use crate::{
    domain::{Instance, JavaRuntime, PersistedState},
    services::{
        auth::microsoft,
        content::{ContentEntry, ContentKind},
        insights::{self, InstanceScanSummary, ServicePing, VersionHighlights},
        java, minecraft,
        system_resources::{self, SystemResources},
    },
    storage::Paths,
    theme,
};
use iced::{Subscription, Task, Theme, widget::image, window};
use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};
use uuid::Uuid;

pub struct Launcher {
    pub(crate) paths: Paths,
    pub(crate) persisted: PersistedState,
    pub(crate) route: Route,
    pub(crate) settings_tab: SettingsTab,
    pub(crate) new_instance_tab: NewInstanceTab,
    pub(crate) modpack_tab: ModpackTab,
    pub(crate) wizard_step: WizardStep,
    pub(crate) version_filter: VersionFilter,
    last_instance_id: Option<Uuid>,
    pub(crate) microsoft_login: MicrosoftLoginState,
    pub(crate) wizard: WizardDraft,
    pub(crate) versions: Vec<minecraft::VersionEntry>,
    pub(crate) loader_catalog: LoaderCatalogState,
    pub(crate) java_runtimes: Vec<JavaRuntime>,
    pub(crate) insights: InstanceScanSummary,
    insights_request_id: u64,
    pub(crate) highlights: VersionHighlights,
    pub(crate) pings: Vec<ServicePing>,
    pub(crate) system_resources: SystemResources,
    pub(crate) content_entries: Vec<ContentEntry>,
    pub(crate) content_query: String,
    content_request_id: u64,
    thumbnails: HashMap<String, image::Handle>,
    requested_thumbnails: HashSet<String>,
    pub(crate) content_scope: Option<(Uuid, ContentKind)>,
    pub(crate) content_loading: bool,
    pub(crate) resource_browser: Option<ResourceBrowserState>,
    active_resource_downloads: HashSet<(u64, Uuid)>,
    pub(crate) pending_delete: Option<Uuid>,
    pub(crate) modpacks: ModpackBrowserState,
    pub(crate) jobs: HashMap<Uuid, InstallJob>,
    pub(crate) notice: Option<String>,
    pub(crate) launch_auth: LaunchAuthentication,
    launches: LaunchRegistry,
    deleting_instances: HashSet<Uuid>,
    window_id: Option<window::Id>,
    pub(crate) window_maximized: bool,
    next_modal_id: u64,
}

impl Launcher {
    pub fn new() -> (Self, Task<Message>) {
        let paths = Paths::discover();
        let _ = paths.prepare();
        let mut persisted = paths.load();
        persisted.migrate();
        let _ = paths.save(&persisted);
        let last_instance_id = persisted.instances.first().map(|instance| instance.id);
        let instances = persisted.instances.clone();
        let instance_files = instances.clone();
        let instance_file_paths = paths.clone();
        let policy = persisted.settings.download.clone();
        let accounts_missing_avatars = persisted
            .accounts
            .iter()
            .filter(|account| account.avatar_rgba.is_none())
            .cloned()
            .collect::<Vec<_>>();
        let app = Self {
            paths,
            persisted,
            route: Route::Home,
            settings_tab: SettingsTab::Downloads,
            new_instance_tab: NewInstanceTab::Minecraft,
            modpack_tab: ModpackTab::Browse,
            wizard_step: WizardStep::Version,
            version_filter: VersionFilter::Release,
            last_instance_id,
            microsoft_login: MicrosoftLoginState::default(),
            wizard: WizardDraft::default(),
            versions: Vec::new(),
            loader_catalog: LoaderCatalogState::default(),
            java_runtimes: Vec::new(),
            insights: InstanceScanSummary::default(),
            insights_request_id: 0,
            highlights: VersionHighlights::default(),
            pings: Vec::new(),
            system_resources: SystemResources::default(),
            content_entries: Vec::new(),
            content_query: String::new(),
            content_request_id: 0,
            thumbnails: HashMap::new(),
            requested_thumbnails: HashSet::new(),
            content_scope: None,
            content_loading: false,
            resource_browser: None,
            active_resource_downloads: HashSet::new(),
            pending_delete: None,
            modpacks: ModpackBrowserState::default(),
            jobs: HashMap::new(),
            notice: None,
            launch_auth: LaunchAuthentication::default(),
            launches: LaunchRegistry::default(),
            deleting_instances: HashSet::new(),
            window_id: None,
            window_maximized: false,
            next_modal_id: 0,
        };
        let mut startup_tasks = vec![
            Task::perform(bootstrap::load_versions(policy), Message::VersionsLoaded),
            Task::perform(java::detect(), Message::JavaLoaded),
            Task::perform(
                async move { insights::scan_instances(&instances).await },
                |summary| Message::InsightsLoaded(0, summary),
            ),
            Task::perform(bootstrap::load_highlights(), Message::HighlightsLoaded),
            Task::perform(bootstrap::load_pings(), Message::PingsLoaded),
            Task::perform(system_resources::read(), Message::SystemResourcesLoaded),
            Task::perform(
                bootstrap::repair_instance_version_files(instance_file_paths, instance_files),
                |_| Message::InstanceVersionFilesRepaired,
            ),
            window::oldest().map(Message::WindowLocated),
        ];
        startup_tasks.extend(accounts_missing_avatars.into_iter().map(|account| {
            let account_id = account.uuid;
            Task::perform(
                async move {
                    microsoft::validate_minecraft_token(&account)
                        .await
                        .map_err(|error| error.to_string())
                },
                move |result| Message::MicrosoftAccountAppearanceLoaded(account_id, result),
            )
        }));
        (app, Task::batch(startup_tasks))
    }

    pub fn title(&self) -> String {
        "Azusa Minecraft Launcher".into()
    }

    pub fn theme(&self) -> Theme {
        theme::azulc()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let mut subscriptions: Vec<Subscription<Message>> = vec![
            window::open_events().map(Message::WindowOpened),
            window::resize_events().map(|(id, _)| Message::WindowResized(id)),
            iced::time::every(Duration::from_secs(2)).map(|_| Message::RefreshSystemResources),
        ];
        subscriptions.extend(self.jobs.values().filter(|job| job.active).map(|job| {
            let attempt = job.attempt.clone();
            let key = PipelineKey {
                attempt: attempt.clone(),
                request: job.request.clone(),
                paths: self.paths.clone(),
            };
            Subscription::run_with(key, install::pipeline_stream)
                .with(attempt)
                .map(install::pipeline_message)
        }));
        subscriptions.extend(self.launches.requests().map(|key| {
            let attempt = key.attempt.clone();
            Subscription::run_with(key.clone(), launch::launch_stream)
                .with(attempt)
                .map(launch::launch_message)
        }));
        Subscription::batch(subscriptions)
    }

    pub(crate) fn instance(&self, id: Uuid) -> Option<&Instance> {
        self.persisted
            .instances
            .iter()
            .find(|instance| instance.id == id)
    }

    pub(crate) fn last_instance(&self) -> Option<&Instance> {
        self.last_instance_id.and_then(|id| self.instance(id))
    }

    pub(crate) fn is_instance_launching(&self, id: Uuid) -> bool {
        self.launches.is_active(id)
    }

    pub(crate) fn is_instance_deleting(&self, id: Uuid) -> bool {
        self.deleting_instances.contains(&id)
    }

    pub(crate) fn launch_session(&self, id: Uuid) -> Option<&LaunchSession> {
        self.launches.session(id)
    }

    fn instance_mut(&mut self, id: Uuid) -> Option<&mut Instance> {
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
