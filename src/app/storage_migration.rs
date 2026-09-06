//! Storage-root selection and migration orchestration.

use super::{Launcher, Message, StorageMigrationState};
use crate::storage::{self, MigrationOutcome};
use iced::Task;
use std::path::PathBuf;

impl Launcher {
    pub(super) fn choose_storage_root(&mut self) -> Task<Message> {
        let current = self.paths.data.clone();
        self.storage_migration_error = None;
        Task::perform(
            async move {
                rfd::AsyncFileDialog::new()
                    .set_title("Choose a new AZULC storage root")
                    .set_directory(current)
                    .pick_folder()
                    .await
                    .map(|folder| folder.path().to_owned())
            },
            Message::StorageRootPicked,
        )
    }

    pub(super) fn select_storage_root(&mut self, path: Option<PathBuf>) {
        if matches!(self.storage_migration, StorageMigrationState::Migrating(_)) {
            return;
        }
        if let Some(path) = path {
            self.storage_migration = StorageMigrationState::Pending(path);
            self.storage_migration_error = None;
        }
    }

    pub(super) fn cancel_storage_migration(&mut self) {
        if matches!(self.storage_migration, StorageMigrationState::Pending(_)) {
            self.storage_migration = StorageMigrationState::Idle;
            self.storage_migration_error = None;
        }
    }

    pub(super) fn confirm_storage_migration(&mut self) -> Task<Message> {
        let StorageMigrationState::Pending(destination) = &self.storage_migration else {
            return Task::none();
        };
        if self.storage_migration_blocked() {
            self.notice = Some(
                "Finish running games, installs, downloads, and deletions before moving storage."
                    .into(),
            );
            return Task::none();
        }

        let destination = destination.clone();
        let source = self.paths.clone();
        let state = self.persisted.clone();
        self.storage_migration = StorageMigrationState::Migrating(destination.clone());
        self.storage_migration_error = None;
        Task::perform(
            storage::migrate(source, destination, state),
            Message::StorageMigrated,
        )
    }

    pub(super) fn finish_storage_migration(
        &mut self,
        result: Result<MigrationOutcome, String>,
    ) -> Task<Message> {
        self.storage_migration = StorageMigrationState::Idle;
        match result {
            Ok(outcome) => {
                self.paths = outcome.paths;
                self.persisted = outcome.state;
                self.content_entries.clear();
                self.content_scope = None;
                self.resource_browser = None;
                self.thumbnails.clear();
                self.requested_thumbnails.clear();
                self.storage_migration_error = None;
                self.notice = Some(
                    outcome
                        .cleanup_warning
                        .unwrap_or_else(|| "Launcher storage moved successfully.".into()),
                );
                self.refresh_insights()
            }
            Err(error) => {
                self.storage_migration_error = Some(error);
                Task::none()
            }
        }
    }

    fn storage_migration_blocked(&self) -> bool {
        self.launches.has_active()
            || self.jobs.values().any(|job| job.active)
            || !self.active_resource_downloads.is_empty()
            || !self.deleting_instances.is_empty()
            || self.instance_files_repairing
    }
}
