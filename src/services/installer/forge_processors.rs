//! Executes one Java process per Forge processor; stdout is only a diagnostic log.

use super::{
    InstallError,
    forge_profile::{Processor, expand, invalid, library_path, output_path},
};
use crate::{
    domain::{InstallProgress, InstallStage, PipelineEvent},
    services::{
        download::integrity,
        process::{self, OutputStream},
    },
};
use sha1::{Digest, Sha1};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
use tokio::{io::AsyncReadExt, process::Command, sync::mpsc::UnboundedSender};

pub(super) struct PlannedProcessor {
    pub label: String,
    pub jar: PathBuf,
    pub classpath: Vec<PathBuf>,
    pub args: Vec<String>,
    pub outputs: Vec<(PathBuf, String)>,
    pub mappings_prepared: bool,
}

impl PlannedProcessor {
    pub fn new(
        processor: &Processor,
        variables: &HashMap<String, String>,
        root: &Path,
    ) -> Result<Self, InstallError> {
        let args = processor
            .args
            .iter()
            .map(|arg| expand(arg, variables, root))
            .collect::<Result<Vec<_>, _>>()?;
        let label = args
            .windows(2)
            .find(|pair| pair[0] == "--task")
            .map(|pair| format!("{} ({})", pair[1], processor.jar))
            .unwrap_or_else(|| processor.jar.clone());
        let outputs = processor
            .outputs
            .iter()
            .flat_map(|outputs| outputs.iter())
            .map(|(path, hash)| {
                let path = output_path(root, &expand(path, variables, root)?)?;
                let hash = expand(hash, variables, root)?;
                let hash = hash
                    .strip_prefix('\'')
                    .and_then(|s| s.strip_suffix('\''))
                    .unwrap_or(&hash);
                let hash = integrity::normalized_hex::<20>(hash)
                    .ok_or_else(|| invalid("invalid processor output SHA-1"))?;
                Ok((path, hash))
            })
            .collect::<Result<_, InstallError>>()?;
        Ok(Self {
            label,
            jar: library_path(root, &processor.jar)?,
            classpath: processor
                .classpath
                .iter()
                .map(|coord| library_path(root, coord))
                .collect::<Result<_, _>>()?,
            args,
            outputs,
            mappings_prepared: false,
        })
    }
}

pub(super) async fn outputs_valid(outputs: &[(PathBuf, String)]) -> Result<bool, InstallError> {
    if outputs.is_empty() {
        return Ok(false);
    }
    for (path, expected) in outputs {
        let mut file = match tokio::fs::File::open(path).await {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error.into()),
        };
        let mut digest = Sha1::new();
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let read = file.read(&mut buffer).await?;
            if read == 0 {
                break;
            }
            digest.update(&buffer[..read]);
        }
        if !integrity::hex_matches::<20>(expected, &hex::encode(digest.finalize())) {
            return Ok(false);
        }
    }
    Ok(true)
}

pub(super) async fn execute(
    java: &Path,
    root: &Path,
    processors: &[PlannedProcessor],
    tx: &UnboundedSender<PipelineEvent>,
) -> Result<(), InstallError> {
    if processors.is_empty() {
        return Ok(());
    }
    report(tx, 0, processors.len(), "Preparing processors");
    for (index, processor) in processors.iter().enumerate() {
        if tx.is_closed() {
            return Err(InstallError::Cancelled);
        }
        report(
            tx,
            index,
            processors.len(),
            &format!(
                "Processor {}/{}: {}",
                index + 1,
                processors.len(),
                processor.label
            ),
        );
        let result = execute_one(java, root, processor, tx).await;
        let cached = result.map_err(|error| {
            InstallError::Processor(format!(
                "{}/{} {}: {error}",
                index + 1,
                processors.len(),
                processor.label
            ))
        })?;
        let detail = format!(
            "{}: {}",
            if cached {
                "Reused verified outputs"
            } else {
                "Completed"
            },
            processor.label
        );
        let _ = tx.send(PipelineEvent::Log(format!(
            "[processor {}/{}] {detail}",
            index + 1,
            processors.len()
        )));
        report(tx, index + 1, processors.len(), &detail);
    }
    Ok(())
}

async fn execute_one(
    java: &Path,
    root: &Path,
    processor: &PlannedProcessor,
    tx: &UnboundedSender<PipelineEvent>,
) -> Result<bool, InstallError> {
    if outputs_valid(&processor.outputs).await? {
        return Ok(true);
    }
    if processor.mappings_prepared {
        return Err(invalid("prepared mappings failed output validation"));
    }
    let classpath: Vec<_> = std::iter::once(processor.jar.clone())
        .chain(processor.classpath.iter().cloned())
        .collect();
    for path in &classpath {
        if !path.is_file() {
            return Err(invalid(format!(
                "missing processor dependency {}",
                path.display()
            )));
        }
    }
    let jar = processor.jar.clone();
    let main = tokio::task::spawn_blocking(move || read_main_class(&jar))
        .await
        .map_err(|error| invalid(error.to_string()))??;
    let classpath = std::env::join_paths(classpath).map_err(|error| invalid(error.to_string()))?;
    for (path, _) in &processor.outputs {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
    }
    // Some tools reuse an existing output without overwriting it. Match the
    // installer by removing only invalid declared outputs before rerunning.
    for output in &processor.outputs {
        if !outputs_valid(std::slice::from_ref(output)).await? {
            match tokio::fs::remove_file(&output.0).await {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
    }
    let _ = tx.send(PipelineEvent::Log(format!(
        "[processor] Running {}",
        processor.label
    )));
    let mut command = Command::new(java);
    command
        .arg("-cp")
        .arg(classpath)
        .arg(main)
        .args(&processor.args)
        .current_dir(root);
    let (status, cancelled) = process::run_controlled(
        &mut command,
        process::OnDrop::Terminate,
        |_| {},
        |stream, line| {
            let tag = match stream {
                OutputStream::Stdout => "processor",
                OutputStream::Stderr => "processor!",
            };
            let _ = tx.send(PipelineEvent::Log(format!("[{tag}] {line}")));
        },
        tx.closed(),
    )
    .await?;
    if cancelled {
        return Err(InstallError::Cancelled);
    }
    if !status.success() {
        return Err(InstallError::InstallerExit(status.code().unwrap_or(-1)));
    }
    if !processor.outputs.is_empty() && !outputs_valid(&processor.outputs).await? {
        return Err(invalid(
            "processor exited successfully but its outputs are missing or have incorrect SHA-1",
        ));
    }
    Ok(false)
}

fn report(tx: &UnboundedSender<PipelineEvent>, completed: usize, total: usize, detail: &str) {
    let _ = tx.send(PipelineEvent::Progress(InstallProgress {
        stage: InstallStage::RunningProcessors,
        current: completed as u64,
        total: total as u64,
        detail: format!(
            "{completed}/{total} processors complete ({}%) // {detail}",
            completed * 100 / total
        ),
        ..InstallProgress::default()
    }));
}

fn read_main_class(path: &Path) -> Result<String, InstallError> {
    use std::io::Read;
    let mut archive =
        zip::ZipArchive::new(std::fs::File::open(path)?).map_err(super::zip_io_error)?;
    let mut manifest = String::new();
    archive
        .by_name("META-INF/MANIFEST.MF")
        .map_err(super::zip_io_error)?
        .read_to_string(&mut manifest)?;
    main_class(&manifest)
        .ok_or_else(|| invalid(format!("Main-Class missing from {}", path.display())))
}

fn main_class(manifest: &str) -> Option<String> {
    let mut unfolded: Vec<String> = Vec::new();
    for line in manifest.lines() {
        if line.is_empty() {
            break;
        } // Only main attributes; ignore named sections.
        if let Some(continuation) = line.strip_prefix(' ') {
            unfolded.last_mut()?.push_str(continuation);
        } else {
            unfolded.push(line.to_owned());
        }
    }
    unfolded.iter().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        let value = value.trim();
        (key.eq_ignore_ascii_case("Main-Class") && !value.is_empty()).then(|| value.to_owned())
    })
}

#[cfg(test)]
#[path = "forge_processor_tests.rs"]
mod tests;
