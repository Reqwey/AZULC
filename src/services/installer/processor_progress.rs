//! Counts client processors using Forge's console task boundaries.
//!
//! Protocol: MinecraftForge/Installer, branch 2.0,
//! actions/PostProcessors.java and json/Install.java. Console callbacks omit
//! numeric progress, but emit a building stage followed by one separator per
//! processor (including cached processors).

use super::{InstallError, read_installer_json, zip_io_error};
use crate::domain::{InstallProgress, InstallStage};
use serde::Deserialize;
use std::path::Path;

const PROCESSOR_BOUNDARY: &str =
    "===============================================================================";

#[derive(Default)]
pub(super) struct ProcessorProgress {
    names: Vec<String>,
    building: bool,
    started: usize,
    unsupported: bool,
}

#[derive(Deserialize)]
struct Profile {
    #[serde(default)]
    processors: Option<Vec<Processor>>,
}

#[derive(Deserialize)]
struct Processor {
    jar: String,
    sides: Option<Vec<String>>,
}

impl ProcessorProgress {
    pub(super) fn from_installer(path: &Path) -> Result<Self, InstallError> {
        let file = std::fs::File::open(path)?;
        let mut archive = zip::ZipArchive::new(file).map_err(zip_io_error)?;
        let profile: Profile = read_installer_json(&mut archive, "install_profile.json")?;
        Ok(Self::from_profile(profile))
    }

    fn from_profile(profile: Profile) -> Self {
        Self {
            names: profile
                .processors
                .unwrap_or_default()
                .into_iter()
                .filter(|processor| {
                    processor
                        .sides
                        .as_ref()
                        .is_none_or(|sides| sides.iter().any(|side| side == "client"))
                })
                .map(|processor| processor.jar)
                .collect(),
            ..Self::default()
        }
    }

    pub(super) fn observe(&mut self, line: &str) -> Option<InstallProgress> {
        if self.names.is_empty() || self.unsupported {
            return None;
        }
        match line {
            "Building Processors" | "Building Processor" if !self.building => {
                self.building = true;
                Some(self.snapshot(0, "Preparing processors"))
            }
            PROCESSOR_BOUNDARY if self.building => {
                if self.started == self.names.len() {
                    // A different log protocol must not manufacture completion.
                    self.unsupported = true;
                    return Some(InstallProgress {
                        stage: InstallStage::RunningProcessors,
                        detail: "Running installer; processor progress unavailable. See log."
                            .into(),
                        ..InstallProgress::default()
                    });
                }
                self.started += 1;
                // The next task proves the previous one returned successfully.
                // The last task is only completed after installer success and
                // verification that the installed version profile exists.
                Some(self.snapshot(
                    self.started - 1,
                    &format!(
                        "Running processor {}/{}: {}",
                        self.started,
                        self.names.len(),
                        self.names[self.started - 1]
                    ),
                ))
            }
            _ => None,
        }
    }

    pub(super) fn finished(&self) -> Option<InstallProgress> {
        (self.building && !self.unsupported && self.started == self.names.len())
            .then(|| self.snapshot(self.names.len(), "Processors complete"))
    }

    fn snapshot(&self, completed: usize, detail: &str) -> InstallProgress {
        InstallProgress {
            stage: InstallStage::RunningProcessors,
            current: completed as u64,
            total: self.names.len() as u64,
            // Keep file counters empty: these units are tasks, not bytes/files.
            detail: format!(
                "{completed}/{} processors complete ({}%) // {detail}",
                self.names.len(),
                completed * 100 / self.names.len()
            ),
            ..InstallProgress::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tracker(json: &str) -> ProcessorProgress {
        ProcessorProgress::from_profile(serde_json::from_str(json).unwrap())
    }

    #[test]
    fn client_tasks_follow_installer_side_filter() {
        let progress = tracker(
            r#"{"processors":[
            {"jar":"common"}, {"jar":"null","sides":null},
            {"jar":"client","sides":["client","server"]},
            {"jar":"server","sides":["server"]}, {"jar":"empty","sides":[]}
        ]}"#,
        );
        assert_eq!(progress.names, ["common", "null", "client"]);
    }

    #[test]
    fn cached_and_running_tasks_advance_only_at_next_boundary() {
        let mut progress = tracker(r#"{"processors":[{"jar":"a"},{"jar":"b"},{"jar":"c"}]}"#);
        assert!(progress.observe(PROCESSOR_BOUNDARY).is_none());
        assert_eq!(progress.observe("Building Processors").unwrap().total, 3);
        assert_eq!(progress.observe(PROCESSOR_BOUNDARY).unwrap().current, 0);
        assert!(progress.observe("  MainClass: example.Main").is_none());
        assert!(
            progress
                .observe("  Output: one Checksum Validated: hash")
                .is_none()
        );
        assert_eq!(progress.observe(PROCESSOR_BOUNDARY).unwrap().current, 1);
        assert!(progress.observe("  Cache Hit!").is_none());
        let last = progress.observe(PROCESSOR_BOUNDARY).unwrap();
        assert_eq!((last.current, last.total, last.files_total), (2, 3, 0));
        assert!(last.detail.contains("Running processor 3/3: c"));
        assert_eq!(progress.finished().unwrap().fraction(), 1.0);
    }

    #[test]
    fn one_processor_does_not_complete_when_it_starts() {
        let mut progress = tracker(r#"{"processors":[{"jar":"patcher"}]}"#);
        progress.observe("Building Processor");
        assert_eq!(
            progress.observe(PROCESSOR_BOUNDARY).unwrap().fraction(),
            0.0
        );
        // Failure never calls finished(); the last emitted progress stays at 0.
        assert!(
            progress
                .observe("  Processor failed, invalid outputs:")
                .is_none()
        );
    }

    #[test]
    fn incomplete_log_cannot_report_all_processors_completed() {
        let mut progress = tracker(r#"{"processors":[{"jar":"a"},{"jar":"b"}]}"#);
        progress.observe("Building Processors");
        progress.observe(PROCESSOR_BOUNDARY);
        assert!(progress.finished().is_none());
    }

    #[test]
    fn absent_or_unrecognized_protocol_has_no_percentage() {
        for json in ["{}", r#"{"processors":null}"#, r#"{"processors":[]}"#] {
            let mut progress = tracker(json);
            assert!(progress.observe("Building Processors").is_none());
            assert!(progress.finished().is_none());
        }
        let mut progress = tracker(r#"{"processors":[{"jar":"a"}]}"#);
        assert!(progress.observe("Unknown processor log format").is_none());
        assert!(progress.finished().is_none());
        progress.observe("Building Processor");
        progress.observe(PROCESSOR_BOUNDARY);
        assert_eq!(progress.observe(PROCESSOR_BOUNDARY).unwrap().total, 0);
        assert!(progress.finished().is_none());
    }
}
