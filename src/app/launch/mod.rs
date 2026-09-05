//! Minecraft launch orchestration, authentication, and process monitoring.

mod auth;
mod sessions;

pub(super) use auth::LaunchAuthentication;
pub(crate) use auth::{LaunchAuthCheck, LaunchAuthPhase, LaunchAuthState};
pub(crate) use sessions::{LaunchAttempt, LaunchSession};
pub(super) use sessions::{LaunchKey, LaunchRegistry};

use self::sessions::LaunchConflict;
use super::{Launcher, Message};
use crate::{
    domain::{Instance, OfflineAccount},
    services::launcher,
};
use futures::SinkExt;
use iced::Task;
use std::time::{SystemTime, UNIX_EPOCH};

impl Launcher {
    pub(super) fn launch_instance(&mut self, id: uuid::Uuid) -> Task<Message> {
        if self.launch_auth.is_blocking() {
            return Task::none();
        }
        let Some(instance) = self.instance(id).cloned() else {
            self.notice = Some("That instance no longer exists.".into());
            return Task::none();
        };
        if self.deleting_instances.contains(&instance.id) {
            self.notice = Some(format!("{} is currently being deleted.", instance.name));
            return Task::none();
        }
        let Some(account) = self.persisted.active_account().cloned() else {
            self.notice = Some("Sign in or select a player profile first.".into());
            return Task::none();
        };

        if self.launch_auth.needs_verification(&account) {
            let Some(check) = self.launch_auth.begin(instance, &account) else {
                return Task::none();
            };
            return Self::validate_microsoft_account_for_launch(check, account);
        }

        self.start_instance_launch(instance, account);
        Task::none()
    }

    pub(super) fn start_instance_launch(&mut self, instance: Instance, account: OfflineAccount) {
        if self.deleting_instances.contains(&instance.id)
            || !self
                .persisted
                .instances
                .iter()
                .any(|stored| stored.id == instance.id)
        {
            self.notice = Some(format!(
                "{} is no longer available to launch.",
                instance.name
            ));
            return;
        }
        let attempt = match self.launches.begin(
            &instance,
            "Preparing Java, libraries, natives, and launch arguments…",
        ) {
            Ok(attempt) => attempt,
            Err(LaunchConflict::InstanceAlreadyActive) => {
                self.notice = Some(format!("{} is already running.", instance.name));
                return;
            }
            Err(LaunchConflict::SharedDirectoryActive) => {
                self.notice = Some(
                    "Another non-isolated instance is already using the shared Minecraft directory."
                        .into(),
                );
                return;
            }
        };

        self.activate_launch(attempt, account);
    }

    pub(super) fn activate_launch(&mut self, attempt: LaunchAttempt, account: OfflineAccount) {
        if let Some(session) = self.launches.session_mut(&attempt) {
            session.status = "Preparing Java, libraries, natives, and launch arguments…".into();
        }
        if !self
            .launches
            .activate(&attempt, account, self.paths.clone())
        {
            self.fail_launch_attempt(
                &attempt,
                "Launch failed: the request is no longer current.".into(),
                "The launch request was superseded before it could start.".into(),
            );
        }
    }

    pub(super) fn fail_launch_attempt(
        &mut self,
        attempt: &LaunchAttempt,
        status: String,
        notice: String,
    ) {
        let failed = if let Some(session) = self.launches.session_mut(attempt) {
            session.active = false;
            session.failed = true;
            session.logs.push(format!("[AZULC] {status}"));
            session.status = status;
            true
        } else {
            false
        };
        if failed {
            self.launches.finish(attempt);
            self.notice = Some(notice);
        }
    }

    pub(super) fn handle_launch_event(
        &mut self,
        attempt: &LaunchAttempt,
        event: launcher::LaunchEvent,
    ) -> Task<Message> {
        let instance_name = self
            .persisted
            .instances
            .iter()
            .find(|instance| instance.id == attempt.instance_id)
            .map_or_else(|| "Minecraft".into(), |instance| instance.name.clone());
        let mut play_time = None;
        let mut terminal = false;
        let mut notice = None;
        if let Some(session) = self.launches.session_mut(attempt) {
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
                    session.mark_ready();
                    session.status =
                        "Render thread detected · Minecraft started successfully".into();
                    notice = Some(format!("{instance_name} started successfully."));
                }
                launcher::LaunchEvent::Exited {
                    code,
                    ready,
                    log_path,
                } => {
                    session.active = false;
                    session.log_path = Some(log_path);
                    terminal = true;
                    if let Some(seconds) = session.ready_elapsed_seconds() {
                        play_time = Some((session.instance_id, seconds));
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
                        notice = Some(format!(
                            "{instance_name} exited with an error. Its launch log is shown up."
                        ));
                    }
                }
                launcher::LaunchEvent::Failed { message, log_path } => {
                    session.active = false;
                    session.failed = true;
                    terminal = true;
                    session.status = format!("Launch failed: {message}");
                    session.log_path = log_path;
                    session.logs.push(format!("[AZULC] {message}"));
                    notice = Some(format!(
                        "{instance_name} failed to launch. Its detailed log is shown up."
                    ));
                }
            }
        } else {
            return Task::none();
        }
        if terminal {
            self.launches.finish(attempt);
        }
        if let Some(notice) = notice {
            self.notice = Some(notice);
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
}

pub(super) fn launch_stream(
    key: &LaunchKey,
) -> impl futures::Stream<Item = launcher::LaunchEvent> + use<> {
    let key = key.clone();
    iced::stream::channel(256, async move |mut output| {
        let (tx, mut rx) = tokio::sync::mpsc::channel(256);
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

pub(super) fn launch_message((attempt, event): (LaunchAttempt, launcher::LaunchEvent)) -> Message {
    Message::LaunchEvent(attempt, event)
}

fn now_unix() -> Option<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs())
}
