use crate::domain::JavaRuntime;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    process::Command,
};

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
    if let Ok(output) = Command::new("where.exe").arg("java").output() {
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
    #[cfg(windows)]
    use std::os::windows::process::CommandExt;
    let mut command = Command::new(&path);
    command.args(["-XshowSettings:properties", "-version"]);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
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
    if major > 1 || minor >= 20 {
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
