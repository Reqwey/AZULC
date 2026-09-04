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
pub(crate) use launch::{LaunchAuthState, LaunchSession};
pub use message::Message;

use accounts::MicrosoftLoginState;
use install::{LoaderCatalogState, PipelineKey, WizardDraft};

use self::{
    launch::{LaunchAuthentication, LaunchRegistry},
    navigation::{
        InstanceTab, ModpackTab, NewInstanceTab, Route, SettingsTab, VersionFilter, WizardStep,
    },
};
use crate::{
    domain::{Instance, JavaRuntime, PersistedState},
    services::{
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
    insights_request_id: u64,
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
            insights_request_id: 0,
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
            launch_auth: LaunchAuthentication::default(),
            launches: LaunchRegistry::default(),
            deleting_instances: HashSet::new(),
            window_id: None,
            window_maximized: false,
            next_modal_id: 0,
        };
        (
            app,
            Task::batch([
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

    pub(crate) fn selected_instance(&self) -> Option<&Instance> {
        let id = self.selected?;
        self.persisted
            .instances
            .iter()
            .find(|instance| instance.id == id)
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
