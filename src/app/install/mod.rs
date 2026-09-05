//! Install pipelines, new-instance setup, and modpack installation.

mod modpacks;
mod wizard;

pub(crate) use modpacks::ModpackBrowserState;
pub(crate) use wizard::{LoaderCatalogState, WizardDraft};

use super::{
    Launcher, Message,
    navigation::{Route, WizardStep},
};
use crate::{
    domain::{InstallProgress, InstallRequest, InstallStage, PipelineEvent},
    services::installer,
    storage::Paths,
};
use futures::SinkExt;
use iced::Task;
use std::{
    fs,
    hash::{Hash, Hasher},
    io::Write,
    path::{Path, PathBuf},
};
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub(crate) struct InstallJob {
    pub(super) attempt: InstallAttempt,
    pub(crate) request: InstallRequest,
    pub(crate) progress: InstallProgress,
    pub(crate) logs: Vec<String>,
    pub(crate) log_path: Option<PathBuf>,
    pub(crate) active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct InstallAttempt {
    pub(super) instance_id: Uuid,
    id: Uuid,
}

impl InstallAttempt {
    fn new(instance_id: Uuid) -> Self {
        Self {
            instance_id,
            id: Uuid::new_v4(),
        }
    }
}

impl InstallJob {
    pub(crate) fn attempt(&self) -> InstallAttempt {
        self.attempt.clone()
    }

    fn accepts(&self, attempt: &InstallAttempt) -> bool {
        self.active && self.attempt == *attempt
    }

    fn can_retry(&self, attempt: &InstallAttempt) -> bool {
        !self.active
            && self.attempt == *attempt
            && matches!(
                self.progress.stage,
                InstallStage::Failed | InstallStage::Cancelled
            )
    }
}

#[derive(Debug, Clone)]
pub(super) struct PipelineKey {
    pub(super) attempt: InstallAttempt,
    pub(super) request: InstallRequest,
    pub(super) paths: Paths,
}

impl Hash for PipelineKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Iced hashes `run_with` data as subscription identity. The resolved metadata may update
        // the request shown by the UI, but it must not restart the in-flight installer.
        self.attempt.hash(state);
    }
}

impl Launcher {
    pub(super) fn cancel_install(&mut self, attempt: &InstallAttempt) {
        let Some(job) = self.jobs.get_mut(&attempt.instance_id) else {
            return;
        };
        if !job.accepts(attempt) {
            return;
        }

        job.active = false;
        job.progress.stage = InstallStage::Cancelled;
        job.progress.detail = "Task cancelled; verified files were kept.".into();
        job.logs.push("[pipeline] task cancelled by user".into());
        if let Some(path) = job.log_path.as_deref() {
            let _ = append_install_log(path, "[cancelled] Task cancelled by user");
        }
    }

    pub(super) fn retry_install(&mut self, attempt: InstallAttempt) {
        let id = attempt.instance_id;
        let Some(job) = self.jobs.get(&id) else {
            return;
        };
        if !job.can_retry(&attempt) {
            return;
        }
        if self.jobs.values().any(|job| job.active) {
            self.notice = Some("Another install pipeline is already active.".into());
            return;
        }

        let job = self
            .jobs
            .get_mut(&id)
            .expect("retry candidate was checked above");
        if job.request.modpack.is_some() {
            match prepare_modpack_install_log(&self.paths, &job.request) {
                Ok(path) => job.log_path = Some(path),
                Err(error) => {
                    self.notice = Some(error);
                    return;
                }
            }
        }
        job.progress = InstallProgress::default();
        job.logs
            .push("[pipeline] retrying; verified files will be reused".into());
        job.attempt = InstallAttempt::new(id);
        job.active = true;
    }

    pub(super) fn handle_pipeline(
        &mut self,
        attempt: &InstallAttempt,
        event: PipelineEvent,
    ) -> Task<Message> {
        let mut finished = None;
        if let Some(job) = self.jobs.get_mut(&attempt.instance_id)
            && job.accepts(attempt)
        {
            match event {
                PipelineEvent::Progress(progress) => job.progress = progress,
                PipelineEvent::ResolvedMetadata {
                    minecraft_version,
                    loader,
                } => {
                    job.request.minecraft_version = minecraft_version;
                    job.request.loader = loader;
                }
                PipelineEvent::Log(line) => {
                    job.logs.push(line);
                    if job.logs.len() > 400 {
                        job.logs.drain(..100);
                    }
                }
                PipelineEvent::Finished(instance) => {
                    job.active = false;
                    job.progress.stage = InstallStage::Complete;
                    job.log_path = None;
                    finished = Some(*instance);
                }
                PipelineEvent::Failed { stage, message } => {
                    job.active = false;
                    job.progress.stage = InstallStage::Failed;
                    job.progress.detail = format!("{}: {message}", stage.label());
                    job.logs.push(format!("[failed] {message}"));
                }
            }
        }
        if let Some(instance) = finished {
            let id = instance.id;
            self.persisted.instances.retain(|old| old.id != instance.id);
            self.persisted.instances.push(instance);
            self.last_instance_id = Some(id);
            if self.route == Route::Installation(id) {
                self.route = Route::instance(id);
            }
            self.wizard_step = WizardStep::Version;
            self.save();
            self.notice = Some("Instance installed. It is ready to launch.".into());
            return self.refresh_insights();
        }
        Task::none()
    }
}

pub(super) fn pipeline_stream(
    key: &PipelineKey,
) -> impl futures::Stream<Item = PipelineEvent> + use<> {
    let key = key.clone();
    iced::stream::channel(64, async move |mut output| {
        let log_path = key
            .request
            .modpack
            .as_ref()
            .map(|_| modpack_install_log_path(&key.paths, key.request.instance_id));
        let mut install_log = if let Some(path) = log_path.as_deref() {
            tokio::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .await
                .ok()
        } else {
            None
        };
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let pipeline = installer::run(key.request, key.paths, tx);
        tokio::pin!(pipeline);
        loop {
            tokio::select! {
                _ = &mut pipeline => {
                    while let Some(event) = rx.recv().await {
                        let terminal = matches!(event, PipelineEvent::Finished(_) | PipelineEvent::Failed { .. });
                        record_install_event(&mut install_log, log_path.as_deref(), &event).await;
                        if output.send(event).await.is_err() || terminal { break; }
                    }
                    break;
                }
                event = rx.recv() => match event {
                    Some(event) => {
                        let terminal = matches!(event, PipelineEvent::Finished(_) | PipelineEvent::Failed { .. });
                        record_install_event(&mut install_log, log_path.as_deref(), &event).await;
                        if output.send(event).await.is_err() || terminal { break; }
                    }
                    None => break,
                }
            }
        }
    })
}

fn modpack_install_log_path(paths: &Paths, id: Uuid) -> PathBuf {
    paths
        .instance_dir(id)
        .join(".azulc")
        .join("latest-install.log")
}

pub(super) fn prepare_modpack_install_log(
    paths: &Paths,
    request: &InstallRequest,
) -> Result<PathBuf, String> {
    let path = modpack_install_log_path(paths, request.instance_id);
    let parent = path
        .parent()
        .ok_or_else(|| "Could not determine the modpack install log directory.".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Could not create the modpack install log directory: {error}"))?;
    let started_at = chrono::Local::now().to_rfc3339();
    let loader_version = request.loader.version.as_deref().unwrap_or("none");
    let header = format!(
        "AZULC modpack installation log\nStarted: {started_at}\nInstance: {}\nMinecraft: {}\nLoader: {} {}\n\n",
        request.name, request.minecraft_version, request.loader.kind, loader_version
    );
    fs::write(&path, header)
        .map_err(|error| format!("Could not create the modpack install log: {error}"))?;
    Ok(path)
}

pub(super) fn append_install_log(path: &Path, line: &str) -> std::io::Result<()> {
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(
        file,
        "{} {line}",
        chrono::Local::now().format("[%Y-%m-%d %H:%M:%S]")
    )
}

pub(super) async fn record_install_event(
    log: &mut Option<tokio::fs::File>,
    log_path: Option<&Path>,
    event: &PipelineEvent,
) {
    if let Some(file) = log.as_mut() {
        let line = format_install_event(event);
        let timestamp = chrono::Local::now().format("[%Y-%m-%d %H:%M:%S]");
        let _ = file
            .write_all(format!("{timestamp} {line}\n").as_bytes())
            .await;
        let _ = file.flush().await;
    }

    if matches!(event, PipelineEvent::Finished(_)) {
        if let Some(mut file) = log.take() {
            let _ = file.flush().await;
            drop(file);
        }
        if let Some(path) = log_path {
            let _ = tokio::fs::remove_file(path).await;
        }
    }
}

fn format_install_event(event: &PipelineEvent) -> String {
    match event {
        PipelineEvent::Progress(progress) => format!(
            "[progress] {} | {} | files {}/{} | bytes {}/{} | {:.0} B/s",
            progress.stage.label(),
            progress.detail,
            progress.files_done,
            progress.files_total,
            progress.current,
            progress.total,
            progress.bytes_per_second
        ),
        PipelineEvent::ResolvedMetadata {
            minecraft_version,
            loader,
        } => format!(
            "[metadata] Minecraft {} | {} {}",
            minecraft_version,
            loader.kind,
            loader.version.as_deref().unwrap_or("none")
        ),
        PipelineEvent::Log(line) => line.clone(),
        PipelineEvent::Finished(instance) => {
            format!(
                "[complete] Instance {} installed successfully",
                instance.name
            )
        }
        PipelineEvent::Failed { stage, message } => {
            format!("[failed] {}: {message}", stage.label())
        }
    }
}

pub(super) fn pipeline_message((attempt, event): (InstallAttempt, PipelineEvent)) -> Message {
    Message::Pipeline(attempt, Box::new(event))
}

#[cfg(test)]
mod tests {
    use super::{InstallAttempt, InstallJob, PipelineKey, record_install_event};
    use crate::domain::{
        InstallProgress, InstallRequest, InstallStage, Instance, InstanceColor, LoaderKind,
        LoaderSpec, PipelineEvent,
    };
    use crate::storage::Paths;
    use std::{
        collections::hash_map::DefaultHasher,
        fs,
        hash::{Hash, Hasher},
        path::PathBuf,
    };
    use uuid::Uuid;

    #[test]
    fn pipeline_identity_stays_stable_when_resolved_metadata_updates_the_request() {
        let instance_id = Uuid::new_v4();
        let attempt = InstallAttempt::new(instance_id);
        let original = PipelineKey {
            attempt,
            request: install_request(instance_id),
            paths: paths(),
        };
        let mut resolved = original.clone();
        resolved.request.minecraft_version = "1.21.1".into();
        resolved.request.loader = LoaderSpec {
            kind: LoaderKind::NeoForge,
            version: Some("21.1.200".into()),
        };

        assert_eq!(hash(&original), hash(&resolved));
    }

    #[test]
    fn inactive_job_rejects_an_event_from_its_current_attempt() {
        let attempt = InstallAttempt::new(Uuid::new_v4());
        let mut job = install_job(attempt.clone());
        job.active = false;

        assert!(!job.accepts(&attempt));
    }

    #[test]
    fn retried_job_rejects_an_event_from_the_previous_attempt() {
        let instance_id = Uuid::new_v4();
        let previous = InstallAttempt::new(instance_id);
        let mut job = install_job(previous.clone());
        job.attempt = InstallAttempt::new(instance_id);

        assert!(!job.accepts(&previous));
    }

    #[test]
    fn retried_job_accepts_an_event_from_the_replacement_attempt() {
        let instance_id = Uuid::new_v4();
        let mut job = install_job(InstallAttempt::new(instance_id));
        job.attempt = InstallAttempt::new(instance_id);
        let replacement = job.attempt.clone();

        assert!(job.accepts(&replacement));
    }

    #[test]
    fn completed_job_rejects_cancel_and_retry_from_its_finished_attempt() {
        let attempt = InstallAttempt::new(Uuid::new_v4());
        let mut job = install_job(attempt.clone());
        job.active = false;
        job.progress.stage = InstallStage::Complete;

        assert!(!job.accepts(&attempt));
        assert!(!job.can_retry(&attempt));
    }

    #[test]
    fn replacement_attempt_rejects_controls_from_the_previous_generation() {
        let instance_id = Uuid::new_v4();
        let previous = InstallAttempt::new(instance_id);
        let mut job = install_job(previous.clone());
        job.active = false;
        job.progress.stage = InstallStage::Failed;
        job.attempt = InstallAttempt::new(instance_id);

        assert!(!job.accepts(&previous));
        assert!(!job.can_retry(&previous));
    }

    #[test]
    fn failed_current_attempt_can_retry_but_cannot_cancel() {
        let attempt = InstallAttempt::new(Uuid::new_v4());
        let mut job = install_job(attempt.clone());
        job.active = false;
        job.progress.stage = InstallStage::Failed;

        assert!(!job.accepts(&attempt));
        assert!(job.can_retry(&attempt));
    }

    #[tokio::test]
    async fn modpack_install_log_is_kept_on_failure_and_removed_on_success() {
        let fixture = std::env::temp_dir().join(format!("azulc-install-log-{}", Uuid::new_v4()));
        fs::create_dir_all(&fixture).unwrap();
        let path = fixture.join("latest-install.log");
        fs::write(&path, "header\n").unwrap();
        let mut log = Some(
            tokio::fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .await
                .unwrap(),
        );

        record_install_event(
            &mut log,
            Some(&path),
            &PipelineEvent::Failed {
                stage: InstallStage::DownloadingModpackContent,
                message: "fixture failure".into(),
            },
        )
        .await;
        assert!(path.exists());
        assert!(
            fs::read_to_string(&path)
                .unwrap()
                .contains("fixture failure")
        );

        let instance = Instance {
            id: Uuid::new_v4(),
            name: "Fixture Pack".into(),
            minecraft_version: "1.21.1".into(),
            version_id: "1.21.1".into(),
            loader: LoaderSpec {
                kind: LoaderKind::Fabric,
                version: Some("0.16.0".into()),
            },
            game_dir: fixture.clone(),
            installed: true,
            description: String::new(),
            color: InstanceColor::default(),
            favorite: false,
            play_time_seconds: 0,
            last_played_unix: None,
            settings: Default::default(),
            origin: Default::default(),
        };
        record_install_event(
            &mut log,
            Some(&path),
            &PipelineEvent::Finished(Box::new(instance)),
        )
        .await;
        assert!(!path.exists());

        fs::remove_dir_all(fixture).unwrap();
    }

    fn install_job(attempt: InstallAttempt) -> InstallJob {
        InstallJob {
            request: install_request(attempt.instance_id),
            attempt,
            progress: InstallProgress::default(),
            logs: Vec::new(),
            log_path: None,
            active: true,
        }
    }

    fn install_request(instance_id: Uuid) -> InstallRequest {
        InstallRequest {
            instance_id,
            name: "Fixture".into(),
            description: String::new(),
            color: InstanceColor::default(),
            minecraft_version: "resolving".into(),
            loader: LoaderSpec {
                kind: LoaderKind::Vanilla,
                version: None,
            },
            settings: Default::default(),
            download_policy: Default::default(),
            modpack: None,
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

    fn hash(key: &PipelineKey) -> u64 {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        hasher.finish()
    }
}
