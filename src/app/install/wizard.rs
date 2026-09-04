//! New-instance wizard transitions and install request creation.

use super::{InstallAttempt, InstallJob};
use crate::app::{
    Launcher, Message,
    navigation::{InstanceTab, Route, VersionFilter, WizardStep},
};
use crate::{
    domain::{InstallProgress, InstallRequest, InstanceColor, LoaderKind, LoaderSpec},
    services::loader_catalog::{self, LoaderCatalogKey, LoaderVersionEntry},
};
use iced::Task;
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

#[derive(Debug, Clone, Default)]
pub(crate) struct LoaderCatalogState {
    pub(in crate::app) request_id: u64,
    pub(in crate::app) key: Option<LoaderCatalogKey>,
    pub(crate) entries: Vec<LoaderVersionEntry>,
    pub(crate) provider: Option<&'static str>,
    pub(crate) loading: bool,
    pub(crate) error: Option<String>,
}

impl LoaderCatalogState {
    fn begin(&mut self, key: LoaderCatalogKey) -> u64 {
        self.request_id = self.request_id.wrapping_add(1);
        self.key = Some(key);
        self.entries.clear();
        self.provider = None;
        self.loading = true;
        self.error = None;
        self.request_id
    }

    pub(in crate::app) fn clear(&mut self) {
        let request_id = self.request_id.wrapping_add(1);
        *self = Self::default();
        self.request_id = request_id;
    }
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

impl Launcher {
    pub(in crate::app) fn wizard_can_open(&self, step: WizardStep) -> bool {
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

    pub(in crate::app) fn refresh_loader_catalog(&mut self, force: bool) -> Task<Message> {
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

    pub(in crate::app) fn start_install(&mut self) -> Task<Message> {
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
                attempt: InstallAttempt::new(id),
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
}

#[cfg(test)]
mod tests {
    use super::LoaderCatalogState;
    use crate::{
        domain::{DownloadSource, LoaderKind},
        services::loader_catalog::LoaderCatalogKey,
    };

    #[test]
    fn catalog_state_invalidates_same_key_requests_and_clears() {
        let key = LoaderCatalogKey {
            minecraft_version: "1.21.1".into(),
            loader: LoaderKind::Fabric,
            source: DownloadSource::Bmcl,
        };
        let mut state = LoaderCatalogState::default();
        let first = state.begin(key.clone());
        let retry = state.begin(key);
        assert_ne!(first, retry);
        state.clear();
        assert_ne!(state.request_id, retry);
    }
}
