//! Per-instance launch attempts, monitor requests, and visible session state.

use crate::{
    domain::{Instance, OfflineAccount},
    storage::Paths,
};
use std::{collections::HashMap, path::PathBuf, time::Instant};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct LaunchAttempt {
    pub(super) instance_id: Uuid,
    pub(super) id: Uuid,
}

#[derive(Debug, Clone, Hash)]
pub(super) struct LaunchKey {
    pub(super) attempt: LaunchAttempt,
    pub(super) instance: Instance,
    pub(super) account: OfflineAccount,
    pub(super) paths: Paths,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LaunchTarget {
    Isolated(Uuid),
    SharedMinecraft,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LaunchConflict {
    InstanceAlreadyActive,
    SharedDirectoryActive,
}

#[derive(Debug, Clone)]
pub(crate) struct LaunchSession {
    pub(crate) instance_id: Uuid,
    pub(crate) status: String,
    pub(crate) logs: Vec<String>,
    pub(crate) log_path: Option<PathBuf>,
    pub(crate) pid: Option<u32>,
    pub(crate) ready: bool,
    pub(crate) active: bool,
    pub(crate) failed: bool,
    ready_at: Option<Instant>,
    attempt_id: Uuid,
    target: LaunchTarget,
}

impl LaunchSession {
    pub(super) fn mark_ready(&mut self) {
        self.ready = true;
        self.ready_at = Some(Instant::now());
    }

    pub(super) fn ready_elapsed_seconds(&self) -> Option<u64> {
        self.ready_at.map(|started| started.elapsed().as_secs())
    }
}

#[derive(Default)]
pub(super) struct LaunchRegistry {
    requests: HashMap<Uuid, LaunchKey>,
    reserved_instances: HashMap<Uuid, Instance>,
    sessions: HashMap<Uuid, LaunchSession>,
}

impl LaunchRegistry {
    pub(super) fn begin(
        &mut self,
        instance: &Instance,
        status: impl Into<String>,
    ) -> Result<LaunchAttempt, LaunchConflict> {
        if self.is_active(instance.id) {
            return Err(LaunchConflict::InstanceAlreadyActive);
        }

        let target = if instance.settings.isolated {
            LaunchTarget::Isolated(instance.id)
        } else {
            LaunchTarget::SharedMinecraft
        };
        if self
            .sessions
            .values()
            .any(|session| session.active && session.target == target)
        {
            return Err(LaunchConflict::SharedDirectoryActive);
        }

        let attempt = LaunchAttempt {
            instance_id: instance.id,
            id: Uuid::new_v4(),
        };
        self.reserved_instances
            .insert(instance.id, instance.clone());
        self.sessions.insert(
            instance.id,
            LaunchSession {
                instance_id: instance.id,
                status: status.into(),
                logs: Vec::new(),
                log_path: None,
                pid: None,
                ready: false,
                active: true,
                failed: false,
                ready_at: None,
                attempt_id: attempt.id,
                target,
            },
        );
        Ok(attempt)
    }

    pub(super) fn activate(
        &mut self,
        attempt: &LaunchAttempt,
        account: OfflineAccount,
        paths: Paths,
    ) -> bool {
        if self.requests.contains_key(&attempt.instance_id) || !self.is_current(attempt) {
            return false;
        }
        let Some(instance) = self.reserved_instances.get(&attempt.instance_id).cloned() else {
            return false;
        };

        self.requests.insert(
            attempt.instance_id,
            LaunchKey {
                attempt: attempt.clone(),
                instance,
                account,
                paths,
            },
        );
        true
    }

    pub(super) fn requests(&self) -> impl Iterator<Item = &LaunchKey> {
        self.requests.values()
    }

    pub(super) fn session_mut(&mut self, attempt: &LaunchAttempt) -> Option<&mut LaunchSession> {
        self.sessions
            .get_mut(&attempt.instance_id)
            .filter(|session| session.active && session.attempt_id == attempt.id)
    }

    pub(super) fn session(&self, instance_id: Uuid) -> Option<&LaunchSession> {
        self.sessions.get(&instance_id)
    }

    pub(super) fn is_active(&self, instance_id: Uuid) -> bool {
        self.sessions
            .get(&instance_id)
            .is_some_and(|session| session.active)
    }

    pub(super) fn finish(&mut self, attempt: &LaunchAttempt) {
        let is_current_attempt = self
            .sessions
            .get(&attempt.instance_id)
            .is_some_and(|session| session.attempt_id == attempt.id);
        if is_current_attempt {
            self.requests.remove(&attempt.instance_id);
            self.reserved_instances.remove(&attempt.instance_id);
        }
    }

    pub(super) fn remove_instance(&mut self, instance_id: Uuid) {
        self.requests.remove(&instance_id);
        self.reserved_instances.remove(&instance_id);
        self.sessions.remove(&instance_id);
    }

    fn is_current(&self, attempt: &LaunchAttempt) -> bool {
        self.sessions
            .get(&attempt.instance_id)
            .is_some_and(|session| session.active && session.attempt_id == attempt.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{InstanceColor, InstanceOrigin, InstanceSettings, LoaderKind, LoaderSpec};

    #[test]
    fn begin_allows_two_isolated_instances_to_be_active_together() {
        let mut launches = LaunchRegistry::default();
        let first = instance(Uuid::new_v4(), true);
        let second = instance(Uuid::new_v4(), true);

        let first_result = launches.begin(&first, "Preparing");
        let second_result = launches.begin(&second, "Preparing");

        assert!(
            first_result.is_ok() && second_result.is_ok(),
            "different isolated instances should launch in parallel"
        );
    }

    #[test]
    fn begin_rejects_a_second_attempt_for_the_same_instance() {
        let mut launches = LaunchRegistry::default();
        let instance = instance(Uuid::new_v4(), true);
        launches.begin(&instance, "Preparing").unwrap();

        let result = launches.begin(&instance, "Preparing");

        assert_eq!(result, Err(LaunchConflict::InstanceAlreadyActive));
    }

    #[test]
    fn begin_rejects_two_instances_that_share_the_global_game_directory() {
        let mut launches = LaunchRegistry::default();
        launches
            .begin(&instance(Uuid::new_v4(), false), "Preparing")
            .unwrap();

        let result = launches.begin(&instance(Uuid::new_v4(), false), "Preparing");

        assert_eq!(result, Err(LaunchConflict::SharedDirectoryActive));
    }

    #[test]
    fn begin_allows_an_isolated_instance_alongside_a_shared_instance() {
        let mut launches = LaunchRegistry::default();

        let shared = launches.begin(&instance(Uuid::new_v4(), false), "Preparing");
        let isolated = launches.begin(&instance(Uuid::new_v4(), true), "Preparing");

        assert!(shared.is_ok() && isolated.is_ok());
    }

    #[test]
    fn one_verified_account_can_activate_two_isolated_instances_in_parallel() {
        let mut launches = LaunchRegistry::default();
        let account = OfflineAccount::new("Player");
        let first = launches
            .begin(&instance(Uuid::new_v4(), true), "Preparing")
            .unwrap();
        let second = launches
            .begin(&instance(Uuid::new_v4(), true), "Preparing")
            .unwrap();

        launches.activate(&first, account.clone(), paths());
        launches.activate(&second, account, paths());

        assert_eq!(launches.requests().count(), 2);
    }

    #[test]
    fn finishing_one_monitor_request_keeps_the_other_request_active() {
        let mut launches = LaunchRegistry::default();
        let first = instance(Uuid::new_v4(), true);
        let second = instance(Uuid::new_v4(), true);
        let first_attempt = launches.begin(&first, "Preparing").unwrap();
        let second_attempt = launches.begin(&second, "Preparing").unwrap();
        assert!(launches.activate(&first_attempt, OfflineAccount::new("First"), paths()));
        assert!(launches.activate(&second_attempt, OfflineAccount::new("Second"), paths()));

        launches.session_mut(&first_attempt).unwrap().active = false;
        launches.finish(&first_attempt);

        assert_eq!(launches.requests().count(), 1);
        assert!(!launches.is_active(first_attempt.instance_id));
        assert!(launches.is_active(second_attempt.instance_id));
    }

    #[test]
    fn stale_attempt_cannot_mutate_a_new_session() {
        let mut launches = LaunchRegistry::default();
        let instance = instance(Uuid::new_v4(), true);
        let stale = launches.begin(&instance, "First").unwrap();
        launches.session_mut(&stale).unwrap().active = false;
        let current = launches.begin(&instance, "Second").unwrap();

        let stale_session = launches.session_mut(&stale);

        assert!(
            stale_session.is_none() && launches.session_mut(&current).is_some(),
            "only the current attempt may mutate the instance session"
        );
    }

    #[test]
    fn activation_uses_the_instance_snapshot_reserved_at_begin() {
        let mut launches = LaunchRegistry::default();
        let mut instance = instance(Uuid::new_v4(), true);
        instance.name = "Before".into();
        let attempt = launches.begin(&instance, "Preparing").unwrap();
        instance.name = "After".into();

        assert!(launches.activate(&attempt, OfflineAccount::new("Player"), paths()));

        let launched = launches.requests().next().unwrap();
        assert_eq!(launched.instance.name, "Before");
        assert!(launched.instance.settings.isolated);
    }

    #[test]
    fn stale_finish_does_not_remove_the_current_attempt_request() {
        let mut launches = LaunchRegistry::default();
        let instance = instance(Uuid::new_v4(), true);
        let stale = launches.begin(&instance, "First").unwrap();
        launches.session_mut(&stale).unwrap().active = false;
        let current = launches.begin(&instance, "Second").unwrap();
        assert!(launches.activate(&current, OfflineAccount::new("Player"), paths()));

        launches.finish(&stale);

        assert_eq!(launches.requests().count(), 1);
        assert!(launches.session_mut(&current).is_some());
    }

    fn instance(id: Uuid, isolated: bool) -> Instance {
        Instance {
            id,
            name: id.to_string(),
            minecraft_version: "1.20.1".into(),
            version_id: "1.20.1".into(),
            loader: LoaderSpec {
                kind: LoaderKind::Vanilla,
                version: None,
            },
            game_dir: PathBuf::from(id.to_string()),
            installed: true,
            description: String::new(),
            color: InstanceColor::default(),
            favorite: false,
            play_time_seconds: 0,
            last_played_unix: None,
            settings: InstanceSettings {
                isolated,
                ..InstanceSettings::default()
            },
            origin: InstanceOrigin::default(),
        }
    }

    fn paths() -> Paths {
        let data = PathBuf::from("test-data");
        Paths {
            minecraft: data.join("minecraft"),
            instances: data.join("instances"),
            state_file: data.join("state.json"),
            data,
        }
    }
}
