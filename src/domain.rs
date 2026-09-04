use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use std::{fmt, path::PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AccountProvider {
    #[default]
    Offline,
    Microsoft,
    ThirdParty,
}

impl fmt::Display for AccountProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Offline => "Offline",
            Self::Microsoft => "Microsoft",
            Self::ThirdParty => "Third-party",
        })
    }
}

#[derive(Debug, Clone, Hash, Serialize, Deserialize)]
pub struct OfflineAccount {
    pub username: String,
    pub uuid: Uuid,
    #[serde(default)]
    pub provider: AccountProvider,
    #[serde(default)]
    pub access_token: Option<String>,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub token_expires_at: Option<u64>,
    #[serde(default)]
    pub xuid: Option<String>,
    /// Pre-rendered 64×64 RGBA player head, including the hat layer.
    #[serde(default)]
    pub avatar_rgba: Option<Vec<u8>>,
}

impl OfflineAccount {
    pub fn new(username: impl Into<String>) -> Self {
        let username = username.into();
        let seed = format!("OfflinePlayer:{username}");
        let mut bytes: [u8; 16] = Md5::digest(seed.as_bytes()).into();
        bytes[6] = (bytes[6] & 0x0f) | 0x30;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        Self {
            username,
            uuid: Uuid::from_bytes(bytes),
            provider: AccountProvider::Offline,
            access_token: None,
            refresh_token: None,
            token_expires_at: None,
            xuid: None,
            avatar_rgba: None,
        }
    }

    pub fn token_needs_refresh(&self, now: u64) -> bool {
        self.provider == AccountProvider::Microsoft
            && (self.access_token.as_deref().is_none_or(str::is_empty)
                || self
                    .token_expires_at
                    .is_none_or(|expires| expires <= now.saturating_add(60)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offline_uuid_matches_java_name_uuid_algorithm() {
        assert_eq!(
            OfflineAccount::new("Steve").uuid.to_string(),
            "5627dd98-e6be-3c21-b8a8-e92344183641"
        );
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LoaderKind {
    #[default]
    Vanilla,
    Fabric,
    Forge,
    NeoForge,
}

impl LoaderKind {
    pub const ALL: [Self; 4] = [Self::Vanilla, Self::Fabric, Self::Forge, Self::NeoForge];

    pub fn label(self) -> &'static str {
        match self {
            Self::Vanilla => "Vanilla",
            Self::Fabric => "Fabric",
            Self::Forge => "Forge",
            Self::NeoForge => "NeoForge",
        }
    }
}

impl fmt::Display for LoaderKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstanceColor {
    #[default]
    Lavender,
    Sky,
    Mint,
    Amber,
    Rose,
}

impl InstanceColor {
    pub const ALL: [Self; 5] = [
        Self::Lavender,
        Self::Sky,
        Self::Mint,
        Self::Amber,
        Self::Rose,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Lavender => "Lavender",
            Self::Sky => "Sky",
            Self::Mint => "Mint",
            Self::Amber => "Amber",
            Self::Rose => "Rose",
        }
    }
}

#[derive(Debug, Clone, Hash, Serialize, Deserialize)]
pub struct LoaderSpec {
    pub kind: LoaderKind,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Hash, Serialize, Deserialize)]
pub struct Instance {
    pub id: Uuid,
    pub name: String,
    pub minecraft_version: String,
    pub version_id: String,
    pub loader: LoaderSpec,
    pub game_dir: PathBuf,
    pub installed: bool,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub color: InstanceColor,
    #[serde(default)]
    pub favorite: bool,
    #[serde(default)]
    pub play_time_seconds: u64,
    #[serde(default)]
    pub last_played_unix: Option<u64>,
    #[serde(default)]
    pub settings: InstanceSettings,
    #[serde(default)]
    pub origin: InstanceOrigin,
}

#[derive(Debug, Clone, Default, Hash, Serialize, Deserialize)]
pub enum InstanceOrigin {
    #[default]
    Custom,
    Modpack {
        provider: String,
        project_id: Option<String>,
        project_name: String,
        version_id: Option<String>,
        version_name: Option<String>,
    },
}

#[derive(Debug, Clone, Hash, Serialize, Deserialize)]
pub struct InstanceSettings {
    pub isolated: bool,
    pub auto_java: bool,
    pub java_path: Option<PathBuf>,
    pub width: u32,
    pub height: u32,
    pub fullscreen: bool,
    pub auto_memory: bool,
    pub max_memory_mb: u32,
    pub custom_window_title: String,
    pub custom_info: String,
}

impl Default for InstanceSettings {
    fn default() -> Self {
        Self {
            isolated: true,
            auto_java: true,
            java_path: None,
            width: 1280,
            height: 720,
            fullscreen: false,
            auto_memory: true,
            max_memory_mb: 4096,
            custom_window_title: String::new(),
            custom_info: String::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DownloadSource {
    #[default]
    Official,
    Bmcl,
}

impl fmt::Display for DownloadSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Official => "Official",
            Self::Bmcl => "BMCLAPI",
        })
    }
}

#[derive(Debug, Clone, Hash, Serialize, Deserialize)]
pub struct DownloadPolicy {
    pub source: DownloadSource,
    pub concurrency: usize,
}

impl Default for DownloadPolicy {
    fn default() -> Self {
        Self {
            source: DownloadSource::Official,
            concurrency: cpu_thread_count(),
        }
    }
}

pub fn cpu_thread_count() -> usize {
    std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1)
        .max(1)
}

#[derive(Debug, Clone, Default, Hash, Serialize, Deserialize)]
pub struct AppSettings {
    pub download: DownloadPolicy,
    pub game: InstanceSettings,
}

#[derive(Debug, Clone, Hash)]
pub struct InstallRequest {
    pub instance_id: Uuid,
    pub name: String,
    pub description: String,
    pub color: InstanceColor,
    pub minecraft_version: String,
    pub loader: LoaderSpec,
    pub settings: InstanceSettings,
    pub download_policy: DownloadPolicy,
    pub modpack: Option<ModpackInstallSpec>,
}

#[derive(Debug, Clone, Hash)]
pub struct ModpackInstallSpec {
    pub source: ModpackSource,
    pub project_name: String,
    pub version_name: Option<String>,
}

#[derive(Debug, Clone, Hash)]
pub enum ModpackSource {
    Local {
        archive: PathBuf,
    },
    CurseForge {
        project_id: u64,
        file_id: u64,
        file_name: String,
    },
    Modrinth {
        project_id: String,
        version_id: String,
        file_name: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallStage {
    Queued,
    ResolvingModpack,
    DownloadingModpack,
    InspectingModpack,
    ResolvingMinecraft,
    PlanningMinecraft,
    DownloadingMinecraft,
    VerifyingMinecraft,
    ResolvingLoader,
    DownloadingLoader,
    InstallingLoader,
    RunningProcessors,
    DownloadingModpackContent,
    ApplyingModpackOverrides,
    Finalizing,
    Complete,
    Failed,
    Cancelled,
}

impl InstallStage {
    pub fn label(self) -> &'static str {
        match self {
            Self::Queued => "Queued",
            Self::ResolvingModpack => "Resolving modpack archive",
            Self::DownloadingModpack => "Downloading modpack archive",
            Self::InspectingModpack => "Inspecting modpack manifest",
            Self::ResolvingMinecraft => "Resolving Minecraft metadata",
            Self::PlanningMinecraft => "Planning game files",
            Self::DownloadingMinecraft => "Downloading Minecraft",
            Self::VerifyingMinecraft => "Verifying Minecraft",
            Self::ResolvingLoader => "Resolving mod loader",
            Self::DownloadingLoader => "Downloading mod loader",
            Self::InstallingLoader => "Installing mod loader",
            Self::RunningProcessors => "Running installer processors",
            Self::DownloadingModpackContent => "Downloading modpack content",
            Self::ApplyingModpackOverrides => "Applying modpack overrides",
            Self::Finalizing => "Finalizing instance",
            Self::Complete => "Install complete",
            Self::Failed => "Install failed",
            Self::Cancelled => "Cancelled",
        }
    }

    pub fn ordinal(self) -> usize {
        match self {
            Self::Queued
            | Self::ResolvingModpack
            | Self::DownloadingModpack
            | Self::InspectingModpack
            | Self::ResolvingMinecraft
            | Self::PlanningMinecraft => 0,
            Self::DownloadingMinecraft | Self::VerifyingMinecraft => 1,
            Self::ResolvingLoader
            | Self::DownloadingLoader
            | Self::InstallingLoader
            | Self::RunningProcessors => 2,
            Self::DownloadingModpackContent | Self::ApplyingModpackOverrides => 3,
            Self::Finalizing => 4,
            Self::Complete => 6,
            Self::Failed | Self::Cancelled => 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct InstallProgress {
    pub stage: InstallStage,
    pub current: u64,
    pub total: u64,
    pub detail: String,
    pub files_done: usize,
    pub files_total: usize,
    pub bytes_per_second: f64,
}

impl Default for InstallProgress {
    fn default() -> Self {
        Self {
            stage: InstallStage::Queued,
            current: 0,
            total: 0,
            detail: "Task entered the queue".into(),
            files_done: 0,
            files_total: 0,
            bytes_per_second: 0.0,
        }
    }
}

impl InstallProgress {
    pub fn fraction(&self) -> f32 {
        if self.stage == InstallStage::Complete {
            1.0
        } else if self.total == 0 {
            0.0
        } else {
            (self.current as f64 / self.total as f64).clamp(0.0, 1.0) as f32
        }
    }
}

#[derive(Debug, Clone)]
pub enum PipelineEvent {
    Progress(InstallProgress),
    ResolvedMetadata {
        minecraft_version: String,
        loader: LoaderSpec,
    },
    Log(String),
    Finished(Box<Instance>),
    Failed {
        stage: InstallStage,
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PersistedState {
    #[serde(default)]
    pub account: Option<OfflineAccount>,
    #[serde(default)]
    pub accounts: Vec<OfflineAccount>,
    #[serde(default)]
    pub selected_account: Option<Uuid>,
    #[serde(default)]
    pub instances: Vec<Instance>,
    #[serde(default)]
    pub settings: AppSettings,
    #[serde(default)]
    pub schema_version: u32,
}

impl PersistedState {
    pub fn migrate(&mut self) {
        if self.accounts.is_empty()
            && let Some(account) = self.account.clone()
        {
            self.accounts.push(account);
        }
        if self.selected_account.is_none()
            || !self
                .accounts
                .iter()
                .any(|account| Some(account.uuid) == self.selected_account)
        {
            self.selected_account = self.accounts.first().map(|account| account.uuid);
        }
        self.account = self
            .selected_account
            .and_then(|id| self.accounts.iter().find(|account| account.uuid == id))
            .cloned();
        self.settings.download.concurrency = self
            .settings
            .download
            .concurrency
            .clamp(1, cpu_thread_count());
        self.schema_version = 4;
    }

    pub fn active_account(&self) -> Option<&OfflineAccount> {
        self.selected_account
            .and_then(|id| self.accounts.iter().find(|account| account.uuid == id))
            .or(self.account.as_ref())
    }
}

#[cfg(test)]
mod persisted_state_tests {
    use super::*;

    #[test]
    fn migrates_the_legacy_single_account_and_clamps_workers() {
        let legacy = OfflineAccount::new("Alex");
        let mut state = PersistedState {
            account: Some(legacy.clone()),
            settings: AppSettings {
                download: DownloadPolicy {
                    source: DownloadSource::Official,
                    concurrency: 1000,
                },
                ..AppSettings::default()
            },
            ..PersistedState::default()
        };

        state.migrate();

        assert_eq!(state.accounts.len(), 1);
        assert_eq!(state.selected_account, Some(legacy.uuid));
        assert_eq!(
            state
                .active_account()
                .map(|account| account.username.as_str()),
            Some("Alex")
        );
        assert_eq!(state.settings.download.concurrency, cpu_thread_count());
        assert_eq!(state.schema_version, 4);
    }

    #[test]
    fn repairs_a_selected_account_that_no_longer_exists() {
        let first = OfflineAccount::new("PlayerOne");
        let mut state = PersistedState {
            account: Some(OfflineAccount::new("Ghost")),
            accounts: vec![first.clone()],
            selected_account: Some(Uuid::new_v4()),
            ..PersistedState::default()
        };

        state.migrate();

        assert_eq!(state.selected_account, Some(first.uuid));
        assert_eq!(
            state.account.as_ref().map(|account| account.uuid),
            Some(first.uuid)
        );
    }
}

#[derive(Debug, Clone)]
pub struct JavaRuntime {
    pub path: PathBuf,
    pub version: String,
    pub major: u32,
    pub vendor: String,
}
