//! Microsoft verification for the account attached to a pending game launch.

use crate::domain::{AccountProvider, Instance, OfflineAccount};
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LaunchAuthCheck {
    pub(crate) id: Uuid,
    pub(crate) account_id: Uuid,
}

#[derive(Debug)]
pub(crate) struct PendingLaunch {
    pub(crate) check: LaunchAuthCheck,
    pub(crate) instance: Instance,
    pub(crate) username: String,
}

#[derive(Debug, Default)]
pub(crate) enum LaunchAuthState {
    #[default]
    Idle,
    Checking(PendingLaunch),
    Failed {
        launch: PendingLaunch,
        message: String,
    },
}

#[derive(Debug, Default)]
pub(crate) struct LaunchAuthentication {
    verified_accounts: HashSet<Uuid>,
    state: LaunchAuthState,
}

impl LaunchAuthentication {
    pub(crate) fn state(&self) -> &LaunchAuthState {
        &self.state
    }

    pub(crate) fn is_blocking(&self) -> bool {
        !matches!(self.state, LaunchAuthState::Idle)
    }

    pub(crate) fn needs_verification(&self, account: &OfflineAccount) -> bool {
        account.provider == AccountProvider::Microsoft
            && !self.verified_accounts.contains(&account.uuid)
    }

    pub(crate) fn begin(
        &mut self,
        instance: Instance,
        account: &OfflineAccount,
    ) -> Option<LaunchAuthCheck> {
        if self.is_blocking() || !self.needs_verification(account) {
            return None;
        }

        let check = LaunchAuthCheck {
            id: Uuid::new_v4(),
            account_id: account.uuid,
        };
        self.state = LaunchAuthState::Checking(PendingLaunch {
            check,
            instance,
            username: account.username.clone(),
        });
        Some(check)
    }

    pub(crate) fn is_checking(&self, check: LaunchAuthCheck) -> bool {
        matches!(
            self.state,
            LaunchAuthState::Checking(ref launch) if launch.check == check
        )
    }

    pub(crate) fn fail(&mut self, check: LaunchAuthCheck, message: String) -> bool {
        let state = std::mem::take(&mut self.state);
        match state {
            LaunchAuthState::Checking(launch) if launch.check == check => {
                self.state = LaunchAuthState::Failed { launch, message };
                true
            }
            state => {
                self.state = state;
                false
            }
        }
    }

    pub(crate) fn retry(&mut self) -> Option<LaunchAuthCheck> {
        let state = std::mem::take(&mut self.state);
        match state {
            LaunchAuthState::Failed { mut launch, .. } => {
                launch.check.id = Uuid::new_v4();
                let check = launch.check;
                self.state = LaunchAuthState::Checking(launch);
                Some(check)
            }
            state => {
                self.state = state;
                None
            }
        }
    }

    pub(crate) fn complete(&mut self, check: LaunchAuthCheck) -> Option<Instance> {
        let state = std::mem::take(&mut self.state);
        match state {
            LaunchAuthState::Checking(launch) if launch.check == check => {
                self.verified_accounts.insert(check.account_id);
                Some(launch.instance)
            }
            state => {
                self.state = state;
                None
            }
        }
    }

    pub(crate) fn cancel_failed_launch(&mut self) -> bool {
        if matches!(self.state, LaunchAuthState::Failed { .. }) {
            self.state = LaunchAuthState::Idle;
            true
        } else {
            false
        }
    }

    pub(crate) fn invalidate(&mut self, account_id: Uuid) {
        self.verified_accounts.remove(&account_id);
    }
}

pub(crate) fn validate_refreshed_account(
    account_id: Uuid,
    account: OfflineAccount,
) -> Result<OfflineAccount, String> {
    if account.uuid == account_id {
        Ok(account)
    } else {
        Err("Microsoft returned a different Minecraft profile.".into())
    }
}

pub(crate) fn apply_refreshed_account(
    accounts: &mut [OfflineAccount],
    refreshed: OfflineAccount,
) -> bool {
    let Some(stored) = accounts.iter_mut().find(|account| {
        account.provider == AccountProvider::Microsoft && account.uuid == refreshed.uuid
    }) else {
        return false;
    };
    *stored = refreshed;
    true
}

pub(crate) fn apply_replacement_refresh_token(
    accounts: &mut [OfflineAccount],
    account_id: Uuid,
    refresh_token: String,
) -> bool {
    let Some(stored) = accounts.iter_mut().find(|account| {
        account.provider == AccountProvider::Microsoft && account.uuid == account_id
    }) else {
        return false;
    };
    stored.refresh_token = Some(refresh_token);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{InstanceColor, InstanceOrigin, InstanceSettings, LoaderKind, LoaderSpec};
    use std::path::PathBuf;

    #[test]
    fn offline_accounts_do_not_need_launch_verification() {
        let authentication = LaunchAuthentication::default();

        assert!(!authentication.needs_verification(&OfflineAccount::new("Offline")));
    }

    #[test]
    fn beginning_a_check_blocks_for_only_the_selected_account() {
        let mut authentication = LaunchAuthentication::default();
        let account = microsoft_account("Selected");

        let check = authentication
            .begin(instance("Selected instance"), &account)
            .unwrap();

        assert!(
            authentication.is_blocking()
                && check.account_id == account.uuid
                && matches!(
                    authentication.state(),
                    LaunchAuthState::Checking(launch)
                        if launch.username == "Selected"
                            && launch.instance.name == "Selected instance"
                )
        );
    }

    #[test]
    fn completing_a_check_returns_the_original_instance_and_caches_the_account() {
        let mut authentication = LaunchAuthentication::default();
        let account = microsoft_account("Player");
        let check = authentication
            .begin(instance("Original instance"), &account)
            .unwrap();

        let pending_instance = authentication.complete(check).unwrap();

        assert!(
            pending_instance.name == "Original instance"
                && !authentication.is_blocking()
                && !authentication.needs_verification(&account)
        );
    }

    #[test]
    fn a_second_begin_cannot_replace_the_pending_launch() {
        let mut authentication = LaunchAuthentication::default();
        let first = microsoft_account("First");
        let second = microsoft_account("Second");
        let first_check = authentication
            .begin(instance("First game"), &first)
            .unwrap();

        let second_check = authentication.begin(instance("Second game"), &second);

        assert!(
            second_check.is_none()
                && authentication.is_checking(first_check)
                && matches!(
                    authentication.state(),
                    LaunchAuthState::Checking(launch) if launch.instance.name == "First game"
                )
        );
    }

    #[test]
    fn verifying_one_account_does_not_cache_another_account() {
        let mut authentication = LaunchAuthentication::default();
        let first = microsoft_account("First");
        let second = microsoft_account("Second");
        let check = authentication.begin(instance("Game"), &first).unwrap();

        authentication.complete(check);

        assert!(authentication.needs_verification(&second));
    }

    #[test]
    fn a_failed_check_is_not_cached_and_can_be_retried_for_the_same_launch() {
        let mut authentication = LaunchAuthentication::default();
        let account = microsoft_account("Player");
        let first_check = authentication
            .begin(instance("Original instance"), &account)
            .unwrap();
        authentication.fail(first_check, "expired".into());

        let retry = authentication.retry().unwrap();

        assert!(
            retry.account_id == account.uuid
                && retry.id != first_check.id
                && authentication.needs_verification(&account)
                && matches!(
                    authentication.state(),
                    LaunchAuthState::Checking(launch)
                        if launch.instance.name == "Original instance"
                )
        );
    }

    #[test]
    fn a_stale_result_cannot_complete_a_retried_check() {
        let mut authentication = LaunchAuthentication::default();
        let account = microsoft_account("Player");
        let stale = authentication.begin(instance("Game"), &account).unwrap();
        authentication.fail(stale, "expired".into());
        let current = authentication.retry().unwrap();

        let completed = authentication.complete(stale);

        assert!(
            completed.is_none()
                && authentication.is_checking(current)
                && authentication.needs_verification(&account)
        );
    }

    #[test]
    fn cancelling_a_failed_check_discards_the_pending_launch() {
        let mut authentication = LaunchAuthentication::default();
        let account = microsoft_account("Player");
        let check = authentication.begin(instance("Game"), &account).unwrap();
        authentication.fail(check, "expired".into());

        authentication.cancel_failed_launch();

        assert!(!authentication.is_blocking() && authentication.needs_verification(&account));
    }

    #[test]
    fn an_in_flight_check_cannot_be_cancelled_before_token_rotation_is_recorded() {
        let mut authentication = LaunchAuthentication::default();
        let account = microsoft_account("Player");
        let check = authentication.begin(instance("Game"), &account).unwrap();

        let cancelled = authentication.cancel_failed_launch();

        assert!(!cancelled && authentication.is_checking(check));
    }

    #[test]
    fn invalidating_a_verified_account_requires_a_new_check() {
        let mut authentication = LaunchAuthentication::default();
        let account = microsoft_account("Player");
        let check = authentication.begin(instance("Game"), &account).unwrap();
        authentication.complete(check);

        authentication.invalidate(account.uuid);

        assert!(authentication.needs_verification(&account));
    }

    #[test]
    fn applying_a_refreshed_account_preserves_account_order() {
        let offline = OfflineAccount::new("Offline");
        let original = microsoft_account("Before");
        let mut refreshed = original.clone();
        refreshed.username = "After".into();
        refreshed.access_token = Some("new-access-token".into());
        let mut accounts = vec![offline.clone(), original];

        apply_refreshed_account(&mut accounts, refreshed.clone());

        assert_eq!(
            accounts
                .iter()
                .map(|account| (
                    account.uuid,
                    account.username.as_str(),
                    account.access_token.as_deref()
                ))
                .collect::<Vec<_>>(),
            vec![
                (offline.uuid, "Offline", None),
                (refreshed.uuid, "After", refreshed.access_token.as_deref())
            ]
        );
    }

    #[test]
    fn replacement_refresh_tokens_are_updated_without_moving_the_profile() {
        let offline = OfflineAccount::new("Offline");
        let microsoft = microsoft_account("Microsoft");
        let mut accounts = vec![offline.clone(), microsoft.clone()];

        apply_replacement_refresh_token(&mut accounts, microsoft.uuid, "rotated".into());

        assert_eq!(
            accounts
                .iter()
                .map(|account| (account.uuid, account.refresh_token.as_deref()))
                .collect::<Vec<_>>(),
            vec![(offline.uuid, None), (microsoft.uuid, Some("rotated"))]
        );
    }

    #[test]
    fn a_mismatched_profile_is_rejected() {
        let expected = microsoft_account("Player");
        let unexpected = microsoft_account("SomeoneElse");

        let refreshed = validate_refreshed_account(expected.uuid, unexpected);

        assert!(refreshed.is_err());
    }

    fn microsoft_account(username: &str) -> OfflineAccount {
        OfflineAccount {
            username: username.into(),
            uuid: Uuid::new_v4(),
            provider: AccountProvider::Microsoft,
            access_token: Some("access-token".into()),
            refresh_token: Some("refresh-token".into()),
            token_expires_at: Some(u64::MAX),
            xuid: Some("xuid".into()),
            avatar_rgba: None,
        }
    }

    fn instance(name: &str) -> Instance {
        Instance {
            id: Uuid::new_v4(),
            name: name.into(),
            minecraft_version: "1.20.1".into(),
            version_id: "1.20.1".into(),
            loader: LoaderSpec {
                kind: LoaderKind::Vanilla,
                version: None,
            },
            game_dir: PathBuf::from("game"),
            installed: true,
            description: String::new(),
            color: InstanceColor::default(),
            favorite: false,
            play_time_seconds: 0,
            last_played_unix: None,
            settings: InstanceSettings::default(),
            origin: InstanceOrigin::default(),
        }
    }
}
