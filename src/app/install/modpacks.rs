//! Modpack browser state and install-task orchestration.

use super::{InstallAttempt, InstallJob, prepare_modpack_install_log};
use crate::app::{Launcher, Message, navigation::Route};
use crate::{
    domain::{
        InstallProgress, InstallRequest, InstanceColor, LoaderKind, LoaderSpec, ModpackInstallSpec,
        ModpackSource,
    },
    services::{
        catalog::{self, CatalogProject, CatalogProvider, CatalogRelease},
        modpack::ModpackPlan,
    },
};
use iced::Task;
use std::path::PathBuf;
use uuid::Uuid;

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
    pub(in crate::app) request_id: u64,
}

impl Launcher {
    pub(in crate::app) fn search_modpacks(&mut self) -> Task<Message> {
        self.modpacks.request_id = self.modpacks.request_id.wrapping_add(1);
        self.modpacks.loading = true;
        self.modpacks.error = None;
        self.modpacks.selected_project = None;
        self.modpacks.files.clear();
        let request_id = self.modpacks.request_id;
        let provider = self.modpacks.provider;
        let search_filter = self.modpacks.query.trim().to_owned();

        Task::perform(
            catalog::search_modpacks(provider, search_filter),
            move |result| Message::ModpackSearchLoaded(request_id, result),
        )
    }

    pub(in crate::app) fn select_modpack_project(
        &mut self,
        project: CatalogProject,
    ) -> Task<Message> {
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
        Task::perform(catalog::list_modpack_releases(project_key), move |result| {
            Message::ModpackFilesLoaded(request_id, result)
        })
    }

    pub(in crate::app) fn install_online_modpack(&mut self, file: CatalogRelease) -> Task<Message> {
        let Some(project) = self.modpacks.selected_project.as_ref() else {
            self.modpacks.error = Some("Choose a modpack first.".into());
            return Task::none();
        };
        if !file.belongs_to(project) || !project.available || !file.available {
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
        let source = catalog::modpack_source(&file);
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

    pub(in crate::app) fn install_local_modpack(&mut self) -> Task<Message> {
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
                attempt: InstallAttempt::new(id),
                request,
                progress: InstallProgress::default(),
                logs: vec!["[pipeline] continuous modpack install created".into()],
                log_path: Some(log_path),
                active: true,
            },
        );
        self.route = Route::Installation(id);
        Task::none()
    }
}
