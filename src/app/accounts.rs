//! Offline and Microsoft account lifecycle operations.

use super::{Launcher, Message};
use crate::{domain::OfflineAccount, services::auth::microsoft};
use iced::Task;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[derive(Debug, Clone, Default)]
pub(crate) struct MicrosoftLoginState {
    pub(crate) active: bool,
    pub(crate) user_code: String,
    pub(crate) verification_url: String,
    pub(crate) status: String,
    pub(crate) error: Option<String>,
    pub(super) request_id: u64,
    pub(super) cancelled: Option<Arc<AtomicBool>>,
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

    pub(super) fn upsert_microsoft_account(&mut self, account: OfflineAccount) {
        let account_id = account.uuid;
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
