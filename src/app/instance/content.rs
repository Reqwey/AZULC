//! Instance-content browser state and task orchestration.

use crate::{
    app::{
        Launcher, Message,
        navigation::{InstanceTab, Route},
    },
    services::{
        catalog::{
            self, CatalogProject, CatalogProvider, CatalogRelease, ResourceInstallRequest,
            ResourceReleaseRequest, ResourceSearchRequest,
        },
        content::{self as content_service, ContentKind},
    },
};
use iced::Task;
use std::collections::HashSet;
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
    pub(in crate::app) fn is_resource_download_active(&self, instance_id: Uuid) -> bool {
        has_active_resource_download(&self.active_resource_downloads, instance_id)
    }

    pub(in crate::app) fn load_instance_content(
        &mut self,
        id: Uuid,
        tab: InstanceTab,
    ) -> Task<Message> {
        if self.route != (Route::Instance { id, tab }) {
            return Task::none();
        }
        if self.is_instance_deleting(id) {
            self.content_loading = false;
            return Task::none();
        }
        self.content_request_id = self.content_request_id.wrapping_add(1);
        let request_id = self.content_request_id;
        let Some(kind) = tab.content_kind() else {
            self.content_entries.clear();
            self.content_scope = None;
            self.content_loading = false;
            return Task::none();
        };
        let Some(instance) = self.instance(id).cloned() else {
            self.content_entries.clear();
            self.content_scope = None;
            self.content_loading = false;
            return Task::none();
        };
        self.content_loading = true;
        self.content_entries.clear();
        Task::perform(
            async move {
                content_service::scan_content(&instance, kind)
                    .await
                    .map_err(|error| format!("Could not scan {}: {error}", kind_label(kind)))
            },
            move |result| Message::ContentLoaded(request_id, id, kind, result),
        )
    }

    pub(in crate::app) fn open_resource_browser(
        &mut self,
        id: Uuid,
        kind: ContentKind,
    ) -> Task<Message> {
        if !kind.downloadable() {
            self.notice = Some("This content page does not support catalog downloads.".into());
            return Task::none();
        }
        if self.is_instance_deleting(id) {
            self.notice = Some("That instance is currently being deleted.".into());
            return Task::none();
        }
        if !matches!(self.route, Route::Instance { id: route_id, tab } if route_id == id && tab.content_kind() == Some(kind))
        {
            return Task::none();
        }
        let Some(instance) = self.instance(id).cloned() else {
            self.notice = Some("That instance no longer exists.".into());
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
        let Some(instance_id) = self
            .resource_browser
            .as_ref()
            .map(|browser| browser.instance_id)
        else {
            return Task::none();
        };
        if self.is_instance_deleting(instance_id) {
            self.notice = Some("That instance is currently being deleted.".into());
            return Task::none();
        }
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
        self.active_resource_downloads
            .insert((modal_id, instance_id));

        Task::perform(catalog::install_resource(request), move |result| {
            Message::ResourceDownloaded(modal_id, instance_id, kind, result)
        })
    }
}

fn has_active_resource_download(
    active_downloads: &HashSet<(u64, Uuid)>,
    instance_id: Uuid,
) -> bool {
    active_downloads
        .iter()
        .any(|(_, active_instance_id)| *active_instance_id == instance_id)
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

#[cfg(test)]
mod tests {
    use super::has_active_resource_download;
    use std::collections::HashSet;
    use uuid::Uuid;

    #[test]
    fn instance_stays_locked_until_all_of_its_resource_downloads_finish() {
        let instance_id = Uuid::from_u128(1);
        let other_instance_id = Uuid::from_u128(2);
        let mut active = HashSet::from([
            (10, instance_id),
            (11, instance_id),
            (12, other_instance_id),
        ]);

        assert!(has_active_resource_download(&active, instance_id));
        active.remove(&(10, instance_id));
        assert!(has_active_resource_download(&active, instance_id));
        active.remove(&(11, instance_id));
        assert!(!has_active_resource_download(&active, instance_id));
        assert!(has_active_resource_download(&active, other_instance_id));
    }
}
