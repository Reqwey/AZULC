//! Startup catalog loading, repair jobs, and dashboard insight refreshes.

use super::{Launcher, Message};
use crate::{
    domain::{DownloadPolicy, Instance},
    services::{
        download::source::SourceRouter,
        insights::{self, ServicePing, VersionHighlights},
        installer, minecraft,
    },
    storage::Paths,
};
use iced::Task;

impl Launcher {
    pub(super) fn refresh_insights(&mut self) -> Task<Message> {
        self.insights_request_id = self.insights_request_id.wrapping_add(1);
        let request_id = self.insights_request_id;
        let instances = self.persisted.instances.clone();
        Task::perform(
            async move { insights::scan_instances(&instances).await },
            move |summary| Message::InsightsLoaded(request_id, summary),
        )
    }
}

pub(super) async fn load_versions(
    policy: DownloadPolicy,
) -> Result<Vec<minecraft::VersionEntry>, String> {
    let client = reqwest::Client::builder()
        .user_agent("AZULC/0.1.0")
        .build()
        .map_err(|error| error.to_string())?;
    let router = SourceRouter::from_policy(&policy);
    let manifest = minecraft::fetch_manifest_with_router(&client, router)
        .await
        .map_err(|error| format!("Could not retrieve the Minecraft catalog: {error}"))?;
    let mut versions = manifest.versions;
    for highlighted in [manifest.latest.snapshot, manifest.latest.release] {
        if let Some(position) = versions
            .iter()
            .position(|version| version.id == highlighted)
        {
            let version = versions.remove(position);
            versions.insert(0, version);
        }
    }
    Ok(versions)
}

pub(super) async fn repair_instance_version_files(paths: Paths, instances: Vec<Instance>) {
    for instance in instances {
        let base = paths
            .minecraft
            .join("versions")
            .join(&instance.minecraft_version);
        let client = base.join(format!("{}.jar", instance.minecraft_version));
        let metadata = base.join(format!("{}.json", instance.minecraft_version));
        let profile_ready = instance.version_id == instance.minecraft_version
            || paths
                .minecraft
                .join("versions")
                .join(&instance.version_id)
                .join(format!("{}.json", instance.version_id))
                .is_file();
        if client.is_file() && metadata.is_file() && profile_ready {
            let _ = installer::materialize_instance_version_files(
                &paths.minecraft,
                &instance.game_dir,
                &instance.minecraft_version,
                &instance.version_id,
            )
            .await;
        }
    }
}

pub(super) async fn load_highlights() -> Result<VersionHighlights, String> {
    let client = reqwest::Client::builder()
        .user_agent("AZULC/0.1.0")
        .build()
        .map_err(|error| error.to_string())?;
    insights::fetch_version_highlights(&client)
        .await
        .map_err(|error| error.to_string())
}

pub(super) async fn load_pings() -> Vec<ServicePing> {
    match reqwest::Client::builder().user_agent("AZULC/0.1.0").build() {
        Ok(client) => insights::ping_services(&client).await,
        Err(_) => Vec::new(),
    }
}
