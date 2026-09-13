use super::{Launcher, Message};
use crate::{
    domain::{PersistedState, ReleaseNotes},
    services::updates::{self, CURRENT_VERSION, Release},
};
use iced::Task;

#[derive(Debug)]
pub(crate) struct UpdateState {
    pub checking: bool,
    pub preview: Option<ReleaseNotes>,
    pub latest: Option<Release>,
    pub error: Option<String>,
}

impl Default for UpdateState {
    fn default() -> Self {
        Self {
            checking: true,
            preview: None,
            latest: None,
            error: None,
        }
    }
}

impl UpdateState {
    pub fn available(&self) -> Option<&Release> {
        self.latest
            .as_ref()
            .filter(|release| release.is_newer_than(CURRENT_VERSION))
    }
}

pub(super) fn cached_notes(state: &PersistedState) -> Option<ReleaseNotes> {
    state
        .cached_release_notes
        .as_ref()
        .filter(|notes| {
            state.seen_release_notes.as_deref() != Some(CURRENT_VERSION)
                && updates::version(&notes.version).ok() == updates::version(CURRENT_VERSION).ok()
        })
        .cloned()
}

pub(super) fn check_task() -> Task<Message> {
    Task::perform(updates::fetch(None), Message::ReleaseChecked)
}

impl Launcher {
    pub(super) fn show_update_details(&mut self) {
        self.release_update.preview = self.release_update.available().map(Release::notes);
    }

    pub(super) fn load_release_notes(&self) -> Task<Message> {
        if self.persisted.seen_release_notes.as_deref() == Some(CURRENT_VERSION)
            || self.release_notes.is_some()
        {
            return Task::none();
        }
        Task::perform(
            updates::fetch(Some(CURRENT_VERSION)),
            Message::ReleaseNotesLoaded,
        )
    }

    pub(super) fn release_checked(&mut self, result: Result<Release, String>) {
        self.release_update.checking = false;
        match result {
            Ok(release) => {
                if updates::version(&release.tag_name).is_err() {
                    self.release_update.error =
                        Some("The release has an invalid version number.".into());
                    return;
                }
                if release.is_newer_than(CURRENT_VERSION) {
                    self.persisted.cached_release_notes = Some(release.notes());
                    self.save();
                }
                self.release_update.latest = Some(release);
                self.release_update.error = None;
            }
            Err(error) => self.release_update.error = Some(error),
        }
    }

    pub(super) fn download_update(&self) -> Task<Message> {
        let Some(url) = self
            .release_update
            .available()
            .and_then(|release| release.download_url(std::env::consts::OS, std::env::consts::ARCH))
            .map(str::to_owned)
        else {
            return Task::none();
        };
        Task::perform(
            async move { open::that(url).map_err(|error| error.to_string()) },
            Message::ExternalUrlOpened,
        )
    }

    pub(super) fn release_notes_loaded(&mut self, result: Result<Release, String>) {
        if self.persisted.seen_release_notes.as_deref() == Some(CURRENT_VERSION) {
            return;
        }
        if let Ok(release) = result
            && updates::version(&release.tag_name).ok() == updates::version(CURRENT_VERSION).ok()
        {
            self.release_notes = Some(release.notes());
        }
    }

    pub(super) fn dismiss_release_notes(&mut self) {
        if self.release_update.preview.take().is_some() {
            return;
        }
        self.release_notes = None;
        self.persisted.seen_release_notes = Some(CURRENT_VERSION.into());
        self.save();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_notes_only_show_for_running_version_until_acknowledged() {
        let mut state = PersistedState {
            cached_release_notes: Some(ReleaseNotes {
                version: format!("v{CURRENT_VERSION}"),
                body: "Changes".into(),
            }),
            ..PersistedState::default()
        };
        assert!(cached_notes(&state).is_some());
        state.seen_release_notes = Some(CURRENT_VERSION.into());
        assert!(cached_notes(&state).is_none());
        state.seen_release_notes = None;
        state.cached_release_notes.as_mut().unwrap().version = "99.0.0".into();
        assert!(cached_notes(&state).is_none());
    }
}
