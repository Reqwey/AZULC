use crate::domain::Instance;
use futures::future::join_all;
use reqwest::Client;
use serde::Deserialize;
use std::{path::Path, time::Duration};
use tokio::time::Instant;
use uuid::Uuid;

pub const VERSION_MANIFEST_URL: &str =
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

const PING_TIMEOUT: Duration = Duration::from_secs(8);
const PING_TARGETS: [(&str, &str); 5] = [
    ("Mojang Metadata", VERSION_MANIFEST_URL),
    ("BMCLAPI", "https://bmclapi2.bangbang93.com"),
    ("Forge", "https://files.minecraftforge.net"),
    ("Fabric", "https://meta.fabricmc.net"),
    ("NeoForge", "https://maven.neoforged.net"),
];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InstanceInsights {
    pub instance_id: Uuid,
    pub name: String,
    pub mods: usize,
    pub resource_packs: usize,
    pub screenshots: usize,
    pub saves: usize,
    pub play_time_seconds: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InstanceScanSummary {
    pub instances: Vec<InstanceInsights>,
    pub total_worlds: usize,
    pub total_play_time_seconds: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VersionHighlights {
    pub release: String,
    pub snapshot: String,
    pub april_fools: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServicePing {
    pub name: String,
    pub url: String,
    pub latency_ms: u64,
    pub reachable: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum InsightsError {
    #[error("failed to retrieve Minecraft version metadata: {0}")]
    VersionManifest(#[from] reqwest::Error),
}

#[derive(Debug, Deserialize)]
struct VersionManifest {
    latest: ManifestLatest,
    versions: Vec<ManifestVersion>,
}

#[derive(Debug, Deserialize)]
struct ManifestLatest {
    release: String,
    snapshot: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifestVersion {
    id: String,
    release_time: String,
}

#[derive(Clone, Copy)]
enum EntryKind {
    File,
    Directory,
    FileOrDirectory,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct ContentCounts {
    mods: usize,
    resource_packs: usize,
    screenshots: usize,
    saves: usize,
}

/// Scans lightweight, directly visible instance content without parsing packs or saves.
///
/// Mods and screenshots count files, saves count directories, and resource packs count
/// both archive files and unpacked directories. A missing or unreadable directory counts
/// as zero so one damaged instance does not hide dashboard data for every other instance.
pub async fn scan_instances(instances: &[Instance]) -> InstanceScanSummary {
    let scanned = join_all(instances.iter().map(scan_instance)).await;
    summarize_instances(scanned)
}

fn summarize_instances(scanned: Vec<InstanceInsights>) -> InstanceScanSummary {
    let total_worlds = scanned.iter().map(|instance| instance.saves).sum();
    let total_play_time_seconds = scanned
        .iter()
        .map(|instance| instance.play_time_seconds)
        .sum();

    InstanceScanSummary {
        instances: scanned,
        total_worlds,
        total_play_time_seconds,
    }
}

async fn scan_instance(instance: &Instance) -> InstanceInsights {
    let counts = scan_content(&instance.game_dir).await;

    InstanceInsights {
        instance_id: instance.id,
        name: instance.name.clone(),
        mods: counts.mods,
        resource_packs: counts.resource_packs,
        screenshots: counts.screenshots,
        saves: counts.saves,
        play_time_seconds: instance.play_time_seconds,
    }
}

async fn scan_content(game_dir: &Path) -> ContentCounts {
    let mods_dir = game_dir.join("mods");
    let resource_packs_dir = game_dir.join("resourcepacks");
    let screenshots_dir = game_dir.join("screenshots");
    let saves_dir = game_dir.join("saves");
    let (mods, resource_packs, screenshots, saves) = tokio::join!(
        count_entries(&mods_dir, EntryKind::File),
        count_entries(&resource_packs_dir, EntryKind::FileOrDirectory,),
        count_entries(&screenshots_dir, EntryKind::File),
        count_entries(&saves_dir, EntryKind::Directory),
    );

    ContentCounts {
        mods,
        resource_packs,
        screenshots,
        saves,
    }
}

async fn count_entries(path: &Path, kind: EntryKind) -> usize {
    let Ok(mut entries) = tokio::fs::read_dir(path).await else {
        return 0;
    };
    let mut count = 0;
    while let Ok(Some(entry)) = entries.next_entry().await {
        let Ok(file_type) = entry.file_type().await else {
            continue;
        };
        let matches = match kind {
            EntryKind::File => file_type.is_file(),
            EntryKind::Directory => file_type.is_dir(),
            EntryKind::FileOrDirectory => file_type.is_file() || file_type.is_dir(),
        };
        count += usize::from(matches);
    }
    count
}

pub async fn fetch_version_highlights(client: &Client) -> Result<VersionHighlights, InsightsError> {
    let manifest = client
        .get(VERSION_MANIFEST_URL)
        .send()
        .await?
        .error_for_status()?
        .json::<VersionManifest>()
        .await?;
    Ok(highlights_from_manifest(manifest))
}

fn highlights_from_manifest(manifest: VersionManifest) -> VersionHighlights {
    // SJMCL treats every version released on April 1 as an April Fools build,
    // regardless of Mojang's release/snapshot type. ISO-8601 timestamps sort by
    // recency lexicographically, so max() also works across different years.
    let april_fools = manifest
        .versions
        .into_iter()
        .filter(|version| version.release_time.contains("04-01"))
        .max_by(|left, right| left.release_time.cmp(&right.release_time))
        .map(|version| version.id);

    VersionHighlights {
        release: manifest.latest.release,
        snapshot: manifest.latest.snapshot,
        april_fools,
    }
}

/// Measures HTTP response latency for launcher-critical services concurrently.
///
/// A service is reachable only when it responds with a successful HTTP status. Failed
/// and timed-out requests still report elapsed milliseconds to make the result useful
/// to the settings UI without requiring a second timing field.
pub async fn ping_services(client: &Client) -> Vec<ServicePing> {
    join_all(
        PING_TARGETS
            .into_iter()
            .map(|(name, url)| ping_service(client, name, url)),
    )
    .await
}

async fn ping_service(client: &Client, name: &str, url: &str) -> ServicePing {
    let started = Instant::now();
    let reachable = client
        .get(url)
        .timeout(PING_TIMEOUT)
        .send()
        .await
        .is_ok_and(|response| response.status().is_success());
    let latency_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);

    ServicePing {
        name: name.to_string(),
        url: url.to_string(),
        latency_ms,
        reachable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_latest_april_fools_release() {
        let highlights = highlights_from_manifest(VersionManifest {
            latest: ManifestLatest {
                release: "1.21.5".into(),
                snapshot: "25w15a".into(),
            },
            versions: vec![
                ManifestVersion {
                    id: "20w14infinite".into(),
                    release_time: "2020-04-01T12:00:00+00:00".into(),
                },
                ManifestVersion {
                    id: "25w14craftmine".into(),
                    release_time: "2025-04-01T12:00:00+00:00".into(),
                },
                ManifestVersion {
                    id: "25w15a".into(),
                    release_time: "2025-04-08T12:00:00+00:00".into(),
                },
            ],
        });

        assert_eq!(highlights.release, "1.21.5");
        assert_eq!(highlights.snapshot, "25w15a");
        assert_eq!(highlights.april_fools.as_deref(), Some("25w14craftmine"));
    }

    #[tokio::test]
    async fn scans_expected_instance_entry_types_and_totals() {
        let root = std::env::temp_dir().join(format!("azulc-insights-{}", Uuid::new_v4()));
        std::fs::create_dir_all(root.join("mods/nested")).unwrap();
        std::fs::create_dir_all(root.join("resourcepacks/unpacked-pack")).unwrap();
        std::fs::create_dir_all(root.join("screenshots/nested")).unwrap();
        std::fs::create_dir_all(root.join("saves/world-one")).unwrap();
        std::fs::create_dir_all(root.join("saves/world-two")).unwrap();
        std::fs::write(root.join("mods/example.jar"), []).unwrap();
        std::fs::write(root.join("resourcepacks/archive.zip"), []).unwrap();
        std::fs::write(root.join("screenshots/shot.png"), []).unwrap();
        std::fs::write(root.join("saves/readme.txt"), []).unwrap();

        let counts = scan_content(&root).await;
        assert_eq!(counts.mods, 1);
        assert_eq!(counts.resource_packs, 2);
        assert_eq!(counts.screenshots, 1);
        assert_eq!(counts.saves, 2);

        let summary = summarize_instances(vec![InstanceInsights {
            saves: counts.saves,
            play_time_seconds: 123,
            ..InstanceInsights::default()
        }]);
        assert_eq!(summary.total_worlds, 2);
        assert_eq!(summary.total_play_time_seconds, 123);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ping_target_list_covers_critical_services() {
        let names = PING_TARGETS.map(|(name, _)| name);
        assert_eq!(
            names,
            ["Mojang Metadata", "BMCLAPI", "Forge", "Fabric", "NeoForge"]
        );
    }
}
