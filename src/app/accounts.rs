//! Offline and Microsoft account lifecycle operations.

use super::{Launcher, Message};
use crate::{
    domain::{AccountProvider, OfflineAccount},
    services::auth::microsoft,
};
use iced::Task;
use std::{
    collections::HashSet,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use uuid::Uuid;

#[derive(Debug, Clone, Default)]
pub(crate) struct MicrosoftLoginState {
    pub(crate) active: bool,
    pub(crate) user_code: String,
    pub(crate) verification_url: String,
    pub(crate) status: String,
    pub(crate) error: Option<String>,
    pub(crate) refreshing_accounts: HashSet<Uuid>,
    pub(super) request_id: u64,
    pub(super) cancelled: Option<Arc<AtomicBool>>,
}

pub(super) fn apply_microsoft_account_appearance(
    accounts: &mut [OfflineAccount],
    account_id: uuid::Uuid,
    profile: OfflineAccount,
) -> bool {
    let Some(account) = accounts.iter_mut().find(|account| {
        account.uuid == account_id && account.provider == AccountProvider::Microsoft
    }) else {
        return false;
    };
    if profile.uuid != account_id || profile.provider != AccountProvider::Microsoft {
        return false;
    }

    let changed = account.username != profile.username
        || (account.avatar_rgba.is_none() && profile.avatar_rgba.is_some());
    account.username = profile.username;
    if profile.avatar_rgba.is_some() {
        account.avatar_rgba = profile.avatar_rgba;
    }
    changed
}

pub(crate) fn replace_microsoft_account(
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

impl Launcher {
    pub(super) fn add_account(&mut self) {
        let username = self.account_input.trim().to_string();
        if !valid_username(&username) {
            self.notice = Some(
                "Offline usernames must contain 3–16 letters, numbers, or underscores.".into(),
            );
        } else if self
            .persisted
            .accounts
            .iter()
            .any(|account| account.username.eq_ignore_ascii_case(&username))
        {
            self.notice = Some(format!("{username} is already in your account list."));
        } else {
            let account = OfflineAccount::new(&username);
            let account_id = account.uuid;
            self.persisted.selected_account = Some(account_id);
            self.persisted.accounts.push(account.clone());
            self.persisted.account = Some(account);
            self.account_input.clear();
            self.save();
            self.notice = Some(format!("Offline account {username} added."));
        }
    }

    pub(super) fn begin_microsoft_login(&mut self) -> Task<Message> {
        if let Some(cancelled) = self.microsoft_login.cancelled.take() {
            cancelled.store(true, Ordering::Relaxed);
        }
        self.microsoft_login.request_id = self.microsoft_login.request_id.wrapping_add(1);
        let request_id = self.microsoft_login.request_id;
        self.microsoft_login.active = true;
        self.microsoft_login.user_code.clear();
        self.microsoft_login.verification_url.clear();
        self.microsoft_login.status = "Requesting a Microsoft device code…".into();
        self.microsoft_login.error = None;
        Task::perform(
            async {
                microsoft::begin_device_authorization()
                    .await
                    .map_err(|error| error.to_string())
            },
            move |result| Message::MicrosoftDeviceAuthorizationLoaded(request_id, result),
        )
    }

    pub(super) fn refresh_microsoft_account(&mut self, account_id: Uuid) -> Task<Message> {
        if self
            .microsoft_login
            .refreshing_accounts
            .contains(&account_id)
        {
            return Task::none();
        }
        let Some(account) = self
            .persisted
            .accounts
            .iter()
            .find(|account| {
                account.uuid == account_id && account.provider == AccountProvider::Microsoft
            })
            .cloned()
        else {
            return Task::none();
        };

        self.microsoft_login.refreshing_accounts.insert(account_id);
        Task::perform(
            async move { microsoft::refresh_account(&account).await },
            move |result| Message::MicrosoftAccountRefreshed(account_id, result),
        )
    }

    pub(super) fn finish_microsoft_account_refresh(
        &mut self,
        account_id: Uuid,
        result: Result<OfflineAccount, microsoft::AccountRefreshError>,
    ) {
        if !self.microsoft_login.refreshing_accounts.remove(&account_id) {
            return;
        }

        match result {
            Ok(account) => {
                if account.uuid != account_id
                    || account.provider != AccountProvider::Microsoft
                    || !replace_microsoft_account(&mut self.persisted.accounts, account)
                {
                    self.notice = Some("Microsoft returned a different Minecraft profile.".into());
                    return;
                }
                self.launch_auth.invalidate(account_id);
                self.persisted.account = self.persisted.active_account().cloned();
                let has_skin = self
                    .persisted
                    .accounts
                    .iter()
                    .find(|account| account.uuid == account_id)
                    .is_some_and(|account| account.avatar_rgba.is_some());
                self.notice = Some(match self.paths.save(&self.persisted) {
                    Ok(()) if has_skin => "Microsoft account refreshed.".into(),
                    Ok(()) => {
                        "Microsoft account refreshed, but no skin image was available.".into()
                    }
                    Err(error) => {
                        format!("Microsoft account refreshed but could not be saved: {error}")
                    }
                });
            }
            Err(error) => {
                let (mut message, replacement_refresh_token) = error.into_parts();
                if let Some(refresh_token) = replacement_refresh_token
                    && apply_replacement_refresh_token(
                        &mut self.persisted.accounts,
                        account_id,
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
                self.notice = Some(format!("Could not refresh Microsoft account: {message}"));
            }
        }
    }

    pub(super) fn upsert_microsoft_account(&mut self, account: OfflineAccount) {
        let account_id = account.uuid;
        self.microsoft_login.refreshing_accounts.remove(&account_id);
        self.launch_auth.invalidate(account_id);
        self.persisted
            .accounts
            .retain(|existing| existing.uuid != account_id);
        self.persisted.accounts.push(account.clone());
        self.persisted.selected_account = Some(account_id);
        self.persisted.account = Some(account);
        self.save();
    }
}

fn valid_username(name: &str) -> bool {
    (3..=16).contains(&name.len())
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn appearance_update_does_not_replace_current_credentials() {
        let account_id = Uuid::new_v4();
        let mut stored = microsoft_account(account_id, "Before", "current-token");
        stored.avatar_rgba = None;
        let mut profile = microsoft_account(account_id, "After", "stale-token");
        profile.avatar_rgba = Some(vec![9; 64 * 64 * 4]);
        let mut accounts = vec![stored];

        assert!(apply_microsoft_account_appearance(
            &mut accounts,
            account_id,
            profile
        ));
        assert_eq!(accounts[0].username, "After");
        assert_eq!(accounts[0].access_token.as_deref(), Some("current-token"));
        assert!(accounts[0].avatar_rgba.is_some());
    }

    #[test]
    fn full_refresh_replaces_every_saved_account_field_in_place() {
        let account_id = Uuid::new_v4();
        let offline = OfflineAccount::new("Offline");
        let stored = microsoft_account(account_id, "Before", "old-token");
        let mut refreshed = microsoft_account(account_id, "After", "new-token");
        refreshed.xuid = Some("new-xuid".into());
        refreshed.avatar_rgba = Some(vec![5; 64 * 64 * 4]);
        let mut accounts = vec![offline.clone(), stored];

        assert!(replace_microsoft_account(&mut accounts, refreshed.clone()));
        assert_eq!(accounts[0].uuid, offline.uuid);
        assert_eq!(accounts[1].username, "After");
        assert_eq!(accounts[1].access_token.as_deref(), Some("new-token"));
        assert_eq!(accounts[1].xuid.as_deref(), Some("new-xuid"));
        assert!(accounts[1].avatar_rgba.is_some());
    }

    #[test]
    fn rotated_refresh_token_is_saved_without_replacing_other_fields() {
        let account_id = Uuid::new_v4();
        let stored = microsoft_account(account_id, "Player", "current-access-token");
        let mut accounts = vec![stored];

        assert!(apply_replacement_refresh_token(
            &mut accounts,
            account_id,
            "rotated-refresh-token".into(),
        ));
        assert_eq!(
            (
                accounts[0].access_token.as_deref(),
                accounts[0].refresh_token.as_deref(),
            ),
            (Some("current-access-token"), Some("rotated-refresh-token"))
        );
    }

    fn microsoft_account(id: Uuid, username: &str, token: &str) -> OfflineAccount {
        OfflineAccount {
            username: username.into(),
            uuid: id,
            provider: AccountProvider::Microsoft,
            access_token: Some(token.into()),
            refresh_token: Some("refresh-token".into()),
            token_expires_at: Some(u64::MAX),
            xuid: None,
            avatar_rgba: None,
        }
    }
}
