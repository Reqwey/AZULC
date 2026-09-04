//! Top-level message dispatcher for application state transitions.

use super::{
    Launcher, Message,
    install::WizardDraft,
    navigation::{ModpackTab, NewInstanceTab, Route, VersionFilter, WizardStep},
};
use crate::{
    domain::{InstallStage, LoaderKind},
    services::{
        auth::microsoft, catalog::thumbnail_urls as project_thumbnail_urls, content, java, modpack,
        shell, system_resources, thumbnail,
    },
};
use iced::{Task, widget::image, window};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

impl Launcher {
    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Navigate(route) => {
                self.route = route;
                if route == Route::Instances {
                    return self.load_selected_content();
                }
            }
            Message::WindowOpened(id) => {
                self.window_id = Some(id);
                return window::is_maximized(id).map(Message::WindowMaximizedChanged);
            }
            Message::WindowLocated(id) => {
                self.window_id = id;
                if let Some(id) = id {
                    return window::is_maximized(id).map(Message::WindowMaximizedChanged);
                }
            }
            Message::WindowResized(id) => {
                if self.window_id == Some(id) {
                    return window::is_maximized(id).map(Message::WindowMaximizedChanged);
                }
            }
            Message::WindowMaximizedChanged(maximized) => self.window_maximized = maximized,
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
                    self.window_maximized = !self.window_maximized;
                    return window::maximize(id, self.window_maximized);
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
            Message::LaunchMicrosoftAccountChecked(check, result) => {
                self.finish_launch_authentication(check, result);
            }
            Message::RetryLaunchAuthentication => {
                return self.retry_launch_authentication();
            }
            Message::CancelLaunchAuthentication => {
                if self.launch_auth.cancel_failed_launch() {
                    self.notice = Some("Minecraft launch cancelled.".into());
                }
            }
            Message::LaunchAuthenticationBackdropPressed => {}
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
                self.launch_auth.invalidate(id);
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
            Message::InsightsLoaded(request_id, summary) => {
                if request_id == self.insights_request_id {
                    self.insights = summary;
                }
            }
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
                                super::thumbnails::local_thumbnail_key(&path),
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
            Message::RefreshPings => {
                return Task::perform(super::bootstrap::load_pings(), Message::PingsLoaded);
            }
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
                if self.is_instance_launching(id) {
                    let name = self
                        .persisted
                        .instances
                        .iter()
                        .find(|instance| instance.id == id)
                        .map_or("This instance", |instance| instance.name.as_str());
                    self.notice = Some(format!(
                        "{name} is still running. Exit Minecraft before deleting its files."
                    ));
                } else if self.deleting_instances.contains(&id) {
                    self.notice = Some("This instance is already being deleted.".into());
                } else if self
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
                    if self.is_instance_launching(id) {
                        self.notice = Some(
                            "This instance is still running. Exit Minecraft before deleting its files."
                                .into(),
                        );
                    } else {
                        self.deleting_instances.insert(id);
                        return self.delete_instance(id);
                    }
                }
            }
            Message::Deleted(id, result) => {
                self.deleting_instances.remove(&id);
                match result {
                    Ok(()) => {
                        self.persisted
                            .instances
                            .retain(|instance| instance.id != id);
                        self.jobs.remove(&id);
                        self.launches.remove_instance(id);
                        if self.selected == Some(id) {
                            self.selected =
                                self.persisted.instances.first().map(|instance| instance.id);
                        }
                        self.save();
                        return self.refresh_insights();
                    }
                    Err(error) => self.notice = Some(format!("Could not delete instance: {error}")),
                }
            }
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
                    Task::perform(
                        super::bootstrap::load_versions(policy),
                        Message::VersionsLoaded,
                    ),
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
                        let _ = super::install::append_install_log(
                            path,
                            "[cancelled] Task cancelled by user",
                        );
                    }
                }
            }
            Message::RetryInstall(id) => self.retry_install(id),
            Message::Pipeline(attempt, event) => {
                return self.handle_pipeline(&attempt, *event);
            }
            Message::LaunchSelected => return self.launch_selected(),
            Message::LaunchEvent(attempt, event) => {
                return self.handle_launch_event(&attempt, event);
            }
            Message::DismissNotice => self.notice = None,
        }
        Task::none()
    }
}
