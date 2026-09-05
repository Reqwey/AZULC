//! Microsoft verification for the account attached to a pending game launch.

use crate::{
    app::{
        Launcher, Message,
        accounts::{apply_replacement_refresh_token, replace_microsoft_account},
    },
    domain::{AccountProvider, Instance, OfflineAccount},
    services::auth::microsoft,
};
use iced::Task;
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LaunchAuthCheck {
    pub(crate) id: Uuid,
    pub(crate) account_id: Uuid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LaunchAuthPhase {
    Validating,
    Refreshing,
}

#[derive(Debug)]
pub(crate) struct PendingLaunch {
    pub(crate) check: LaunchAuthCheck,
    pub(crate) instance: Instance,
    pub(crate) username: String,
    pub(crate) phase: LaunchAuthPhase,
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
            phase: LaunchAuthPhase::Validating,
        });
        Some(check)
    }

    pub(crate) fn mark_refreshing(&mut self, check: LaunchAuthCheck) -> bool {
        match &mut self.state {
            LaunchAuthState::Checking(launch) if launch.check == check => {
                launch.phase = LaunchAuthPhase::Refreshing;
                true
            }
            _ => false,
        }
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
                launch.phase = LaunchAuthPhase::Validating;
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

impl Launcher {
    pub(in crate::app) fn retry_launch_authentication(&mut self) -> Task<Message> {
        let Some(check) = self.launch_auth.retry() else {
            return Task::none();
        };
        let Some(account) = self.saved_microsoft_account(check) else {
            self.launch_auth.fail(
                check,
                "The Microsoft account selected for this launch no longer exists.".into(),
            );
            return Task::none();
        };
        Self::validate_microsoft_account_for_launch(check, account)
    }

    pub(super) fn validate_microsoft_account_for_launch(
        check: LaunchAuthCheck,
        account: OfflineAccount,
    ) -> Task<Message> {
        Task::perform(
            async move {
                microsoft::validate_minecraft_token(&account)
                    .await
                    .map_err(|error| error.to_string())
            },
            move |result| Message::LaunchMicrosoftTokenValidated(check, result),
        )
    }

    pub(in crate::app) fn finish_launch_token_validation(
        &mut self,
        check: LaunchAuthCheck,
        result: Result<microsoft::MinecraftTokenValidation, String>,
    ) -> Task<Message> {
        if !self.launch_auth.is_checking(check) {
            return Task::none();
        }
        match result {
            Ok(microsoft::MinecraftTokenValidation::Valid(account)) => {
                self.complete_launch_authentication(check, account);
                Task::none()
            }
            Ok(microsoft::MinecraftTokenValidation::Invalid) => {
                self.refresh_microsoft_account_for_launch(check)
            }
            Err(message) => {
                self.launch_auth.fail(check, message);
                Task::none()
            }
        }
    }

    fn refresh_microsoft_account_for_launch(&mut self, check: LaunchAuthCheck) -> Task<Message> {
        if !self.launch_auth.mark_refreshing(check) {
            return Task::none();
        }
        let Some(account) = self.saved_microsoft_account(check) else {
            self.launch_auth.fail(
                check,
                "The Microsoft account selected for this launch no longer exists.".into(),
            );
            return Task::none();
        };
        Task::perform(
            async move { microsoft::refresh_account(&account).await },
            move |result| Message::LaunchMicrosoftAccountRefreshed(check, result),
        )
    }

    pub(in crate::app) fn finish_launch_account_refresh(
        &mut self,
        check: LaunchAuthCheck,
        result: Result<OfflineAccount, microsoft::AccountRefreshError>,
    ) {
        if !self.launch_auth.is_checking(check) {
            return;
        }
        let refreshed = match result {
            Ok(account) => account,
            Err(error) => {
                let (mut message, replacement_refresh_token) = error.into_parts();
                if let Some(refresh_token) = replacement_refresh_token
                    && apply_replacement_refresh_token(
                        &mut self.persisted.accounts,
                        check.account_id,
                        refresh_token,
                    )
                {
                    self.persisted.account = self.persisted.active_account().cloned();
                    if let Err(error) = self.paths.save(&self.persisted) {
                        message.push_str(&format!(
                            " The rotated refresh token could not be saved: {error}"
                        ));
                    }
                }
                self.launch_auth.fail(check, message);
                return;
            }
        };
        self.complete_launch_authentication(check, refreshed);
    }

    fn complete_launch_authentication(&mut self, check: LaunchAuthCheck, account: OfflineAccount) {
        let verified = match validate_refreshed_account(check.account_id, account) {
            Ok(account) => account,
            Err(message) => {
                self.launch_auth.fail(check, message);
                return;
            }
        };
        let launch_account = verified.clone();
        if !replace_microsoft_account(&mut self.persisted.accounts, verified) {
            self.launch_auth.fail(
                check,
                "The saved Microsoft profile no longer exists.".into(),
            );
            return;
        }
        self.persisted.account = self.persisted.active_account().cloned();
        if let Err(error) = self.paths.save(&self.persisted) {
            self.launch_auth.fail(
                check,
                format!("Could not save refreshed Microsoft credentials: {error}"),
            );
            return;
        }
        let Some(instance) = self.launch_auth.complete(check) else {
            return;
        };
        self.start_instance_launch(instance, launch_account);
    }

    fn saved_microsoft_account(&self, check: LaunchAuthCheck) -> Option<OfflineAccount> {
        self.persisted
            .accounts
            .iter()
            .find(|account| {
                account.uuid == check.account_id && account.provider == AccountProvider::Microsoft
            })
            .cloned()
    }
}

fn validate_refreshed_account(
    account_id: Uuid,
    account: OfflineAccount,
) -> Result<OfflineAccount, String> {
    if account.uuid == account_id {
        Ok(account)
    } else {
        Err("Microsoft returned a different Minecraft profile.".into())
    }
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
                            && launch.phase == LaunchAuthPhase::Validating
                )
        );
    }

    #[test]
    fn invalid_token_moves_the_current_check_to_refreshing() {
        let mut authentication = LaunchAuthentication::default();
        let account = microsoft_account("Player");
        let check = authentication.begin(instance("Game"), &account).unwrap();

        assert!(authentication.mark_refreshing(check));
        assert!(matches!(
            authentication.state(),
            LaunchAuthState::Checking(launch)
                if launch.check == check && launch.phase == LaunchAuthPhase::Refreshing
        ));
    }

    #[test]
    fn stale_check_cannot_change_the_current_phase() {
        let mut authentication = LaunchAuthentication::default();
        let account = microsoft_account("Player");
        let stale = authentication.begin(instance("Game"), &account).unwrap();
        authentication.fail(stale, "expired".into());
        let current = authentication.retry().unwrap();

        assert!(!authentication.mark_refreshing(stale));
        assert!(matches!(
            authentication.state(),
            LaunchAuthState::Checking(launch)
                if launch.check == current && launch.phase == LaunchAuthPhase::Validating
        ));
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
                            && launch.phase == LaunchAuthPhase::Validating
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
