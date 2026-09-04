//! Instance-content browser state and task orchestration.

use crate::{
    app::{Launcher, Message},
    services::{
        catalog::{
            self, CatalogProject, CatalogProvider, CatalogRelease, ResourceInstallRequest,
            ResourceReleaseRequest, ResourceSearchRequest,
        },
        content::{self as content_service, ContentKind},
    },
};
use iced::Task;
use uuid::Uuid;

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
    pub(in crate::app) request_id: u64,
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

impl Launcher {
    pub(in crate::app) fn load_selected_content(&mut self) -> Task<Message> {
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
                content_service::scan_content(&instance, kind)
                    .await
                    .map_err(|error| format!("Could not scan {}: {error}", kind_label(kind)))
            },
            move |result| Message::ContentLoaded(id, kind, result),
        )
    }

    pub(in crate::app) fn open_resource_browser(&mut self, kind: ContentKind) -> Task<Message> {
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

    pub(in crate::app) fn search_resources(&mut self) -> Task<Message> {
        let Some(browser) = self.resource_browser.as_mut() else {
            return Task::none();
        };
        if !browser.kind.downloadable() {
            browser.loading = false;
            browser.error = Some("This content type is not available from online catalogs.".into());
            return Task::none();
        }
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
        let search = ResourceSearchRequest {
            provider: browser.provider,
            kind: browser.kind,
            game_version: instance.minecraft_version,
            loader: instance.loader.kind,
            query: browser.query.trim().to_owned(),
        };

        Task::perform(catalog::search_resources(search), move |result| {
            Message::ResourceSearchLoaded(modal_id, request_id, result)
        })
    }

    pub(in crate::app) fn select_resource_project(
        &mut self,
        project: CatalogProject,
    ) -> Task<Message> {
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
        let request = ResourceReleaseRequest {
            provider: browser.provider,
            project: project.key.clone(),
            kind: browser.kind,
            game_version: instance.minecraft_version,
            loader: instance.loader.kind,
        };

        Task::perform(catalog::list_resource_releases(request), move |result| {
            Message::ResourceFilesLoaded(modal_id, request_id, result)
        })
    }

    pub(in crate::app) fn download_resource(&mut self, file: CatalogRelease) -> Task<Message> {
        let Some(browser) = self.resource_browser.as_mut() else {
            return Task::none();
        };
        let Some(project) = browser.selected_project.clone() else {
            browser.error = Some("Choose a project first.".into());
            return Task::none();
        };
        if !file.belongs_to(&project)
            || !project.available
            || !file.available
            || browser.downloading
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
        let modal_id = browser.id;
        let instance_id = browser.instance_id;
        let kind = browser.kind;
        let request = ResourceInstallRequest {
            release: file.key.clone(),
            kind,
            game_directory: instance.game_dir,
            game_version: instance.minecraft_version,
            loader: instance.loader.kind,
            concurrency: self.persisted.settings.download.concurrency,
        };
        browser.downloading = true;
        browser.error = None;
        browser.status = Some(format!(
            "Resolving required dependencies for {}…",
            file.display_name
        ));

        Task::perform(catalog::install_resource(request), move |result| {
            Message::ResourceDownloaded(modal_id, instance_id, kind, result)
        })
    }
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
