use crate::domain::JavaRuntime;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    process::Command,
};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
pub(super) const CREATE_NO_WINDOW: u32 = 0x08000000;

pub async fn detect() -> Vec<JavaRuntime> {
    tokio::task::spawn_blocking(detect_blocking)
        .await
        .unwrap_or_default()
}

fn detect_blocking() -> Vec<JavaRuntime> {
    let executable = if cfg!(windows) { "java.exe" } else { "java" };
    let mut candidates = HashSet::new();

    if let Some(home) = std::env::var_os("JAVA_HOME") {
        candidates.insert(PathBuf::from(home).join("bin").join(executable));
    }

    #[cfg(windows)]
    let where_output = {
        let mut command = Command::new("where.exe");
        command.arg("java").creation_flags(CREATE_NO_WINDOW);
        command.output()
    };
    #[cfg(windows)]
    if let Ok(output) = where_output {
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            candidates.insert(PathBuf::from(line.trim()));
        }
    }

    #[cfg(not(windows))]
    if let Ok(output) = Command::new("sh")
        .args(["-c", "command -v -a java"])
        .output()
    {
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            candidates.insert(PathBuf::from(line.trim()));
        }
    }

    for root in common_java_roots() {
        scan_java_root(&root, executable, &mut candidates);
    }

    let mut runtimes: Vec<_> = candidates
        .into_iter()
        .filter(|path| path.is_file())
        .filter_map(inspect)
        .collect();
    runtimes.sort_by_key(|java| java.major);
    runtimes.dedup_by(|a, b| a.path == b.path);
    runtimes
}

fn common_java_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    #[cfg(windows)]
    {
        for var in ["ProgramFiles", "ProgramFiles(x86)"] {
            if let Some(root) = std::env::var_os(var) {
                let root = PathBuf::from(root);
                for vendor in [
                    "Java",
                    "Eclipse Adoptium",
                    "Microsoft",
                    "Zulu",
                    "Amazon Corretto",
                    "BellSoft",
                ] {
                    roots.push(root.join(vendor));
                }
            }
        }
    }
    #[cfg(target_os = "linux")]
    roots.extend([PathBuf::from("/usr/lib/jvm"), PathBuf::from("/usr/java")]);
    #[cfg(target_os = "macos")]
    roots.push(PathBuf::from("/Library/Java/JavaVirtualMachines"));
    roots
}

fn scan_java_root(root: &Path, executable: &str, out: &mut HashSet<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let dir = entry.path();
        for home in [dir.clone(), dir.join("Contents/Home")] {
            let candidate = home.join("bin").join(executable);
            if candidate.is_file() {
                out.insert(candidate);
            }
        }
    }
}

fn inspect(path: PathBuf) -> Option<JavaRuntime> {
    let mut command = Command::new(&path);
    command.args(["-XshowSettings:properties", "-version"]);
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    let output = command.output().ok()?;
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let value = |key: &str| {
        text.lines()
            .find_map(|line| line.trim().strip_prefix(key).map(|v| v.trim().to_string()))
    };
    let version = value("java.version =")?;
    let vendor = value("java.vendor =").unwrap_or_else(|| "Unknown".into());
    let head = version.split('.').next().unwrap_or("0");
    let major = if head == "1" {
        version.split('.').nth(1)
    } else {
        Some(head)
    }?
    .parse()
    .ok()?;
    Some(JavaRuntime {
        path,
        version,
        major,
        vendor,
    })
}

pub fn required_major(minecraft_version: &str, manifest_requirement: Option<u32>) -> u32 {
    if let Some(required) = manifest_requirement {
        return required;
    }
    let mut parts = minecraft_version
        .split('.')
        .filter_map(|part| part.parse::<u32>().ok());
    let major = parts.next().unwrap_or(1);
    let minor = parts.next().unwrap_or(0);
    let patch = parts.next().unwrap_or(0);
    if major > 1 || minor > 20 || (minor == 20 && patch >= 5) {
        21
    } else if minor >= 18 {
        17
    } else if minor >= 17 {
        16
    } else {
        8
    }
}

pub fn select(runtimes: &[JavaRuntime], required: u32) -> Option<JavaRuntime> {
    runtimes
        .iter()
        .find(|j| j.major == required)
        .cloned()
        .or_else(|| {
            runtimes
                .iter()
                .filter(|j| j.major > required)
                .min_by_key(|j| j.major)
                .cloned()
        })
}

pub(crate) fn compatibility_warning(
    runtimes: &[JavaRuntime],
    settings: &crate::domain::InstanceSettings,
    required: u32,
) -> Option<String> {
    if !settings.auto_java {
        let selected = runtimes
            .iter()
            .find(|runtime| Some(&runtime.path) == settings.java_path.as_ref());
        return match selected {
            Some(runtime) if runtime.major == required => None,
            Some(runtime) => Some(format!(
                "Java {required} is expected by this instance's launch configuration, but Java {} is selected. Choose Java {required} in Settings; other versions may not be compatible.",
                runtime.major
            )),
            None => Some(format!(
                "The selected Java runtime was not detected. Install or select Java {required}, then rescan Java."
            )),
        };
    }
    if runtimes.iter().any(|runtime| runtime.major == required) {
        return None;
    }
    let mut versions: Vec<_> = runtimes.iter().map(|runtime| runtime.major).collect();
    versions.sort_unstable();
    versions.dedup();
    let detected = if versions.is_empty() {
        "No Java runtime was detected.".to_owned()
    } else {
        format!(
            "Detected Java: {}.",
            versions
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    Some(format!(
        "Java {required} is expected by this instance's launch configuration. {detected} Install Java {required}, then rescan Java. A newer Java version is not necessarily compatible."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::InstanceSettings;

    fn runtime(major: u32) -> JavaRuntime {
        JavaRuntime {
            path: PathBuf::from(format!("java-{major}")),
            version: major.to_string(),
            major,
            vendor: "Test".into(),
        }
    }

    #[test]
    fn java_25_does_not_satisfy_a_legacy_instance_warning_check() {
        let warning =
            compatibility_warning(&[runtime(25)], &InstanceSettings::default(), 8).unwrap();
        assert!(warning.contains("Java 8"));
        assert!(warning.contains("Detected Java: 25"));
    }

    #[test]
    fn matching_runtime_clears_warning_even_with_newer_java_installed() {
        assert!(
            compatibility_warning(&[runtime(25), runtime(8)], &InstanceSettings::default(), 8)
                .is_none()
        );
    }

    #[test]
    fn manual_selection_is_checked_even_when_a_matching_runtime_exists() {
        let settings = InstanceSettings {
            auto_java: false,
            java_path: Some(runtime(25).path),
            ..InstanceSettings::default()
        };
        assert!(
            compatibility_warning(&[runtime(8), runtime(25)], &settings, 8)
                .unwrap()
                .contains("Java 25 is selected")
        );
    }

    #[test]
    fn missing_manual_runtime_is_reported() {
        let settings = InstanceSettings {
            auto_java: false,
            java_path: Some(runtime(8).path),
            ..InstanceSettings::default()
        };
        assert!(
            compatibility_warning(&[runtime(25)], &settings, 8)
                .unwrap()
                .contains("was not detected")
        );
    }

    #[test]
    fn no_installed_java_is_reported() {
        assert!(
            compatibility_warning(&[], &InstanceSettings::default(), 17)
                .unwrap()
                .contains("No Java runtime")
        );
    }

    #[test]
    fn explicit_configuration_takes_precedence_over_minecraft_fallback() {
        assert_eq!(required_major("1.7.10", Some(25)), 25);
        assert_eq!(required_major("1.7.10", None), 8);
        assert_eq!(required_major("1.20.1", None), 17);
        assert_eq!(required_major("1.20.4", None), 17);
        assert_eq!(required_major("1.20.5", None), 21);
    }
}
