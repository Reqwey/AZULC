use crate::{
    domain::{AccountProvider, Instance, JavaRuntime, LoaderKind, OfflineAccount},
    services::{
        auth::microsoft,
        java,
        minecraft::{self, Arguments, Library, Rule, VersionJson},
        path_safety,
    },
    storage::Paths,
};
use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::sync::mpsc::Sender;

const READY_FLAGS: [&str; 3] = ["render thread", "lwjgl version", "lwjgl openal"];

#[derive(Debug, Clone)]
pub struct LaunchResult {
    pub pid: u32,
    pub java: JavaRuntime,
    pub log_path: PathBuf,
}

#[derive(Debug, Clone)]
pub enum LaunchEvent {
    Started(LaunchResult),
    Log(String),
    Ready,
    Exited {
        code: Option<i32>,
        ready: bool,
        log_path: PathBuf,
    },
    Failed {
        message: String,
        log_path: Option<PathBuf>,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum LaunchError {
    #[error("file operation failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid version profile: {0}")]
    Json(#[from] serde_json::Error),
    #[error("version {0} is not completely installed")]
    MissingVersion(String),
    #[error("unsafe Minecraft version id: {0:?}")]
    UnsafeVersionId(String),
    #[error("Minecraft version inheritance contains a cycle at {0}")]
    VersionCycle(String),
    #[error("Minecraft version inheritance exceeds {max_depth} profiles from {version}")]
    VersionChainTooDeep { version: String, max_depth: usize },
    #[error("version profile is missing its main class")]
    MissingMainClass,
    #[error("required library is missing: {0}")]
    MissingLibrary(PathBuf),
    #[error("unsafe library path in version metadata: {0:?}")]
    UnsafeLibraryPath(String),
    #[error("the computed classpath is invalid: {0}")]
    InvalidClasspath(#[source] std::env::JoinPathsError),
    #[error("Java {0} was not found; install it and scan again")]
    MissingJava(u32),
    #[error("could not start Java: {0}")]
    Spawn(String),
}

pub async fn monitor(
    instance: Instance,
    account: OfflineAccount,
    paths: Paths,
    tx: Sender<LaunchEvent>,
) {
    let fallback_log = instance.game_dir.join(".azulc/latest-launch.log");
    let error_tx = tx.clone();
    let result = tokio::task::spawn_blocking(move || {
        launch_and_monitor_blocking(instance, account, paths, tx)
    })
    .await;
    let error = match result {
        Ok(Ok(())) => return,
        Ok(Err(error)) => error.to_string(),
        Err(error) => format!("launch monitor task failed: {error}"),
    };
    let _ = error_tx
        .send(LaunchEvent::Failed {
            message: error,
            log_path: fallback_log.is_file().then_some(fallback_log),
        })
        .await;
}

fn launch_and_monitor_blocking(
    instance: Instance,
    account: OfflineAccount,
    paths: Paths,
    tx: Sender<LaunchEvent>,
) -> Result<(), LaunchError> {
    let chain = load_chain(&paths.minecraft, &instance.version_id)?;
    let merged = merge_chain(&chain);
    let requirement = merged.java_version.as_ref().map(|v| v.major_version);
    let runtimes = futures::executor::block_on(java::detect());
    let required = java::required_major(&instance.minecraft_version, requirement);
    let configured_runtime = (!instance.settings.auto_java)
        .then_some(instance.settings.java_path.as_ref())
        .flatten()
        .and_then(|configured| runtimes.iter().find(|runtime| runtime.path == *configured))
        .cloned();
    let runtime = configured_runtime
        .or_else(|| java::select(&runtimes, required))
        .ok_or(LaunchError::MissingJava(required))?;
    std::fs::create_dir_all(&instance.game_dir)?;
    let game_directory = if instance.settings.isolated {
        instance.game_dir.clone()
    } else {
        paths.minecraft.clone()
    };
    std::fs::create_dir_all(&game_directory)?;

    let natives = instance
        .game_dir
        .join(".azulc/natives")
        .join(&instance.version_id);
    if natives.exists() {
        std::fs::remove_dir_all(&natives)?;
    }
    std::fs::create_dir_all(&natives)?;
    extract_natives(&paths.minecraft, &merged.libraries, &natives)?;

    let client_jar = instance_client_jar(&instance, &paths.minecraft);
    let classpath = build_classpath(&paths.minecraft, &client_jar, &merged.libraries)?;
    let asset_index = merged
        .asset_index
        .as_ref()
        .map(|v| v.id.as_str())
        .or(merged.assets.as_deref())
        .unwrap_or(&instance.minecraft_version);
    let mut vars = HashMap::<&str, String>::new();
    vars.insert("${auth_player_name}", account.username.clone());
    vars.insert("${version_name}", instance.version_id.clone());
    vars.insert(
        "${game_directory}",
        game_directory.to_string_lossy().into_owned(),
    );
    vars.insert(
        "${assets_root}",
        paths
            .minecraft
            .join("assets")
            .to_string_lossy()
            .into_owned(),
    );
    vars.insert(
        "${game_assets}",
        paths
            .minecraft
            .join("assets/virtual/legacy")
            .to_string_lossy()
            .into_owned(),
    );
    vars.insert("${assets_index_name}", asset_index.to_string());
    vars.insert("${auth_uuid}", account.uuid.simple().to_string());
    let access_token = account
        .access_token
        .as_deref()
        .filter(|token| !token.is_empty())
        .unwrap_or("0")
        .to_owned();
    vars.insert("${auth_access_token}", access_token.clone());
    vars.insert(
        "${auth_session}",
        format!("token:{access_token}:{}", account.uuid.simple()),
    );
    vars.insert(
        "${clientid}",
        std::env::var(microsoft::CLIENT_ID_ENV).unwrap_or_default(),
    );
    vars.insert("${auth_xuid}", account.xuid.clone().unwrap_or_default());
    vars.insert(
        "${user_type}",
        if account.provider == AccountProvider::Microsoft {
            "msa"
        } else {
            "legacy"
        }
        .into(),
    );
    vars.insert("${version_type}", "release".into());
    vars.insert(
        "${natives_directory}",
        natives.to_string_lossy().into_owned(),
    );
    vars.insert("${launcher_name}", "AZULC".into());
    vars.insert("${launcher_version}", env!("CARGO_PKG_VERSION").into());
    vars.insert("${classpath}", classpath.clone());
    vars.insert(
        "${classpath_separator}",
        if cfg!(windows) { ";" } else { ":" }.into(),
    );
    vars.insert(
        "${library_directory}",
        paths
            .minecraft
            .join("libraries")
            .to_string_lossy()
            .into_owned(),
    );
    vars.insert("${user_properties}", "{}".into());
    vars.insert("${resolution_width}", instance.settings.width.to_string());
    vars.insert("${resolution_height}", instance.settings.height.to_string());

    // Forge's generated JVM arguments use `${version_name}.jar` in `ignoreList`.
    // AZULC keeps the vanilla client under versions/<minecraft>/<minecraft>.jar,
    // rather than renaming it to the loader profile id. Make the JVM-only value
    // match that real filename so SecureJarHandler does not discover both the
    // vanilla client and Forge's transformed Minecraft module.
    let mut jvm_vars = vars.clone();
    jvm_vars.insert(
        "${version_name}",
        jvm_version_name(
            instance.loader.kind,
            &instance.version_id,
            &instance.minecraft_version,
        ),
    );

    let available_memory = crate::services::system_resources::read_blocking().memory_limit_mb();
    let maximum_memory = if instance.settings.auto_memory {
        (if required >= 17 { 4096 } else { 2048 }).min(available_memory)
    } else {
        instance.settings.max_memory_mb.clamp(512, available_memory)
    };

    let mut args = vec![
        "-Xms512M".into(),
        format!("-Xmx{maximum_memory}M"),
        format!("-Djava.library.path={}", natives.display()),
        "-Dfile.encoding=UTF-8".into(),
    ];
    if runtime.major < 19 {
        args.extend([
            "-Dsun.stdout.encoding=UTF-8".into(),
            "-Dsun.stderr.encoding=UTF-8".into(),
        ]);
    } else {
        args.extend([
            "-Dstdout.encoding=UTF-8".into(),
            "-Dstderr.encoding=UTF-8".into(),
        ]);
    }
    args.extend(forge_compatibility_jvm_args(
        instance.loader.kind,
        &client_jar,
    ));
    if let Some(arguments) = &merged.arguments {
        args.extend(without_classpath_switch(expand_arguments(
            &arguments.jvm,
            &jvm_vars,
        )));
    }
    args.push(
        merged
            .main_class
            .clone()
            .ok_or(LaunchError::MissingMainClass)?,
    );
    if let Some(arguments) = &merged.arguments {
        args.extend(expand_arguments(&arguments.game, &vars));
    } else if let Some(legacy) = &merged.minecraft_arguments {
        args.extend(
            shlex::split(legacy)
                .unwrap_or_default()
                .into_iter()
                .map(|arg| substitute(&arg, &vars)),
        );
    }
    if instance.settings.fullscreen && !args.iter().any(|argument| argument == "--fullscreen") {
        args.push("--fullscreen".into());
    }

    let log_path = instance.game_dir.join(".azulc/latest-launch.log");
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut launch_log = File::create(&log_path)?;
    writeln!(launch_log, "[AZULC] version={}", instance.version_id)?;
    writeln!(launch_log, "[AZULC] java={}", runtime.path.display())?;
    writeln!(
        launch_log,
        "[AZULC] game_directory={}",
        game_directory.display()
    )?;
    writeln!(launch_log, "[AZULC] memory={}MiB", maximum_memory)?;
    writeln!(launch_log, "[AZULC] client_jar={}", client_jar.display())?;
    writeln!(
        launch_log,
        "[AZULC] jvm_version_name={}",
        jvm_vars["${version_name}"]
    )?;
    writeln!(
        launch_log,
        "[AZULC] resolution={}x{} fullscreen={}",
        instance.settings.width, instance.settings.height, instance.settings.fullscreen
    )?;
    writeln!(
        launch_log,
        "[AZULC] classpath_entries={}",
        std::env::split_paths(&classpath).count()
    )?;
    launch_log.flush()?;
    let launch_log = Arc::new(Mutex::new(launch_log));
    let mut command = Command::new(&runtime.path);
    command
        .args(&args)
        .current_dir(&game_directory)
        .env("CLASSPATH", &classpath)
        .env("AZULC_WINDOW_TITLE", &instance.settings.custom_window_title)
        .env("AZULC_CUSTOM_INFO", &instance.settings.custom_info)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command
        .spawn()
        .map_err(|e| LaunchError::Spawn(e.to_string()))?;
    let result = LaunchResult {
        pid: child.id(),
        java: runtime,
        log_path: log_path.clone(),
    };
    let _ = tx.blocking_send(LaunchEvent::Started(result));

    let ready = Arc::new(AtomicBool::new(false));
    let stdout = child
        .stdout
        .take()
        .map(|output| pipe_game_output(output, launch_log.clone(), tx.clone(), ready.clone()));
    let stderr = child
        .stderr
        .take()
        .map(|output| pipe_game_output(output, launch_log.clone(), tx.clone(), ready.clone()));
    let status = child.wait()?;
    if let Some(thread) = stdout {
        let _ = thread.join();
    }
    if let Some(thread) = stderr {
        let _ = thread.join();
    }
    let was_ready = ready.load(Ordering::SeqCst);
    if let Ok(mut log) = launch_log.lock() {
        let _ = log.flush();
    }
    let _ = tx.blocking_send(LaunchEvent::Exited {
        code: status.code(),
        ready: was_ready,
        log_path,
    });
    Ok(())
}

fn forge_compatibility_jvm_args(loader: LoaderKind, client: &Path) -> Vec<String> {
    if !matches!(loader, LoaderKind::Forge | LoaderKind::NeoForge) {
        return Vec::new();
    }
    vec![
        format!("-Dminecraft.client.jar={}", client.display()),
        "-Dfml.ignoreInvalidMinecraftCertificates=true".into(),
        "-Dfml.ignorePatchDiscrepancies=true".into(),
    ]
}

fn instance_client_jar(instance: &Instance, minecraft_root: &Path) -> PathBuf {
    let local = instance
        .game_dir
        .join(format!("{}.jar", instance.minecraft_version));
    if local.is_file() {
        local
    } else {
        minecraft_root
            .join("versions")
            .join(&instance.minecraft_version)
            .join(format!("{}.jar", instance.minecraft_version))
    }
}

fn jvm_version_name(loader: LoaderKind, version_id: &str, minecraft_version: &str) -> String {
    if matches!(loader, LoaderKind::Forge | LoaderKind::NeoForge) {
        minecraft_version.to_owned()
    } else {
        version_id.to_owned()
    }
}

fn pipe_game_output<T: Read + Send + 'static>(
    output: T,
    log: Arc<Mutex<File>>,
    tx: Sender<LaunchEvent>,
    ready: Arc<AtomicBool>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut reader = BufReader::new(output);
        let mut buffer = Vec::new();
        loop {
            buffer.clear();
            let Ok(read) = reader.read_until(b'\n', &mut buffer) else {
                break;
            };
            if read == 0 {
                break;
            }
            while buffer
                .last()
                .is_some_and(|byte| matches!(byte, b'\r' | b'\n'))
            {
                buffer.pop();
            }
            let line = String::from_utf8_lossy(&buffer).into_owned();
            if let Ok(mut file) = log.lock() {
                let _ = writeln!(file, "{line}");
            }
            let is_ready = launch_line_is_ready(&line);
            // The complete line is already persisted above. Sampling live output when
            // the bounded UI bridge is full keeps noisy games from blocking their own
            // stdout/stderr pipes while preventing unbounded memory growth.
            let _ = tx.try_send(LaunchEvent::Log(line));
            if is_ready
                && ready
                    .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
            {
                let _ = tx.blocking_send(LaunchEvent::Ready);
            }
        }
    })
}

fn launch_line_is_ready(line: &str) -> bool {
    let line = line.to_ascii_lowercase();
    READY_FLAGS.iter().any(|flag| line.contains(flag))
}

fn load_chain(root: &Path, version_id: &str) -> Result<Vec<VersionJson>, LaunchError> {
    const MAX_DEPTH: usize = 8;

    let mut chain = Vec::new();
    let mut current = version_id.to_string();
    let mut visited = HashSet::new();
    for _ in 0..MAX_DEPTH {
        if path_safety::file_name(&current).as_deref() != Some(current.as_str()) {
            return Err(LaunchError::UnsafeVersionId(current));
        }
        if !visited.insert(current.clone()) {
            return Err(LaunchError::VersionCycle(current));
        }
        let path = root
            .join("versions")
            .join(&current)
            .join(format!("{current}.json"));
        if !path.is_file() {
            return Err(LaunchError::MissingVersion(current));
        }
        let value: VersionJson = serde_json::from_slice(&std::fs::read(path)?)?;
        let parent = value.inherits_from.clone();
        chain.push(value);
        match parent {
            Some(id) => current = id,
            None => {
                chain.reverse();
                return Ok(chain);
            }
        }
    }
    Err(LaunchError::VersionChainTooDeep {
        version: version_id.to_owned(),
        max_depth: MAX_DEPTH,
    })
}

fn merge_chain(chain: &[VersionJson]) -> VersionJson {
    let mut merged = VersionJson::default();
    for part in chain {
        if !part.id.is_empty() {
            merged.id = part.id.clone();
        }
        if part.main_class.is_some() {
            merged.main_class = part.main_class.clone();
        }
        if part.assets.is_some() {
            merged.assets = part.assets.clone();
        }
        if part.asset_index.is_some() {
            merged.asset_index = part.asset_index.clone();
        }
        if part.java_version.is_some() {
            merged.java_version = part.java_version.clone();
        }
        if part.minecraft_arguments.is_some() {
            merged.minecraft_arguments = part.minecraft_arguments.clone();
        }
        for (key, value) in &part.downloads {
            merged.downloads.insert(key.clone(), value.clone());
        }
        for library in &part.libraries {
            let identity = library_identity(library);
            if let Some(position) = merged
                .libraries
                .iter()
                .position(|old| library_identity(old) == identity)
            {
                let current_allowed = minecraft::rules_allow(&merged.libraries[position].rules);
                let candidate_allowed = minecraft::rules_allow(&library.rules);
                if (candidate_allowed && !current_allowed)
                    || (candidate_allowed == current_allowed
                        && library_version(&library.name)
                            > library_version(&merged.libraries[position].name))
                {
                    merged.libraries[position] = library.clone();
                }
            } else {
                merged.libraries.push(library.clone());
            }
        }
        if let Some(arguments) = &part.arguments {
            let target = merged.arguments.get_or_insert_with(Arguments::default);
            target.jvm.extend(arguments.jvm.clone());
            target.game.extend(arguments.game.clone());
        }
    }
    merged
}

fn library_identity(library: &Library) -> String {
    let name = &library.name;
    let (coordinate, extension) = name.split_once('@').unwrap_or((name, "jar"));
    let parts: Vec<_> = coordinate.split(':').collect();
    let group = parts.first().copied().unwrap_or_default();
    let artifact = parts.get(1).copied().unwrap_or_default();
    let classifier = parts.get(3).copied().unwrap_or_default();
    let role = if library.natives.is_empty() {
        "classpath"
    } else {
        "natives"
    };
    format!("{group}:{artifact}:{classifier}@{extension}:{role}")
}

fn library_version(name: &str) -> semver::Version {
    let coordinate = name.split_once('@').map_or(name, |(value, _)| value);
    let version = coordinate.split(':').nth(2).unwrap_or_default();
    semver::Version::parse(version).unwrap_or_else(|_| {
        let mut parts = version.split('.').collect::<Vec<_>>();
        while parts.len() < 3 {
            parts.push("0");
        }
        semver::Version::parse(&parts[..3].join("."))
            .unwrap_or_else(|_| semver::Version::new(0, 1, 0))
    })
}

fn build_classpath(
    root: &Path,
    client: &Path,
    libraries: &[Library],
) -> Result<String, LaunchError> {
    let mut paths = Vec::new();
    let mut seen = HashSet::new();
    for library in libraries {
        if !minecraft::rules_allow(&library.rules) || minecraft::is_legacy_native_container(library)
        {
            continue;
        }
        let relative = library
            .downloads
            .as_ref()
            .and_then(|v| v.artifact.as_ref())
            .map(|v| v.path.clone())
            .filter(|v| !v.is_empty())
            .or_else(|| minecraft::maven_path(&library.name));
        if let Some(relative) = relative {
            let relative = path_safety::relative_path(&relative)
                .ok_or_else(|| LaunchError::UnsafeLibraryPath(relative.clone()))?;
            let path = root.join("libraries").join(relative);
            if !path.is_file() {
                return Err(LaunchError::MissingLibrary(path));
            }
            if seen.insert(path.clone()) {
                paths.push(path);
            }
        }
    }
    if !client.is_file() {
        return Err(LaunchError::MissingLibrary(client.to_path_buf()));
    }
    paths.push(client.to_path_buf());
    Ok(std::env::join_paths(paths)
        .map_err(LaunchError::InvalidClasspath)?
        .to_string_lossy()
        .into_owned())
}

fn extract_natives(root: &Path, libraries: &[Library], output: &Path) -> Result<(), LaunchError> {
    for library in libraries {
        if !minecraft::rules_allow(&library.rules) {
            continue;
        }
        let Some(classifier) = minecraft::native_classifier(library) else {
            continue;
        };
        let Some(download) = library
            .downloads
            .as_ref()
            .and_then(|d| d.classifiers.get(&classifier))
        else {
            continue;
        };
        let relative = path_safety::relative_path(&download.path)
            .ok_or_else(|| LaunchError::UnsafeLibraryPath(download.path.clone()))?;
        let path = root.join("libraries").join(relative);
        let file = File::open(path)?;
        let mut archive =
            zip::ZipArchive::new(file).map_err(|e| LaunchError::Io(std::io::Error::other(e)))?;
        let excludes = library
            .extract
            .as_ref()
            .map(|v| v.exclude.as_slice())
            .unwrap_or(&[]);
        for index in 0..archive.len() {
            let mut entry = archive
                .by_index(index)
                .map_err(|e| LaunchError::Io(std::io::Error::other(e)))?;
            let name = entry.name().replace('\\', "/");
            if entry.is_dir()
                || name.starts_with("META-INF/")
                || excludes.iter().any(|prefix| name.starts_with(prefix))
            {
                continue;
            }
            let Some(safe) = entry.enclosed_name() else {
                continue;
            };
            let target = output.join(safe);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut out = File::create(target)?;
            std::io::copy(&mut entry, &mut out)?;
        }
    }
    Ok(())
}

fn expand_arguments(values: &[serde_json::Value], vars: &HashMap<&str, String>) -> Vec<String> {
    let mut out = Vec::new();
    for item in values {
        if let Some(value) = item.as_str() {
            out.push(substitute(value, vars));
            continue;
        }
        let Some(object) = item.as_object() else {
            continue;
        };
        let rules: Vec<Rule> = object
            .get("rules")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        if !minecraft::rules_allow(&rules) {
            continue;
        }
        match object.get("value") {
            Some(serde_json::Value::String(value)) => out.push(substitute(value, vars)),
            Some(serde_json::Value::Array(values)) => out.extend(
                values
                    .iter()
                    .filter_map(|v| v.as_str())
                    .map(|v| substitute(v, vars)),
            ),
            _ => {}
        }
    }
    out
}

fn substitute(value: &str, vars: &HashMap<&str, String>) -> String {
    vars.iter()
        .fold(value.to_string(), |result, (key, replacement)| {
            result.replace(key, replacement)
        })
}

fn without_classpath_switch(arguments: Vec<String>) -> Vec<String> {
    let mut output = Vec::with_capacity(arguments.len());
    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        if argument == "-cp" || argument == "-classpath" || argument == "--class-path" {
            let _ = arguments.next();
        } else {
            output.push(argument);
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn library(name: &str) -> Library {
        Library {
            name: name.into(),
            ..Library::default()
        }
    }

    #[test]
    fn load_chain_rejects_traversing_version_id_before_file_access() {
        assert!(matches!(
            load_chain(Path::new("unused"), "../outside"),
            Err(LaunchError::UnsafeVersionId(_))
        ));
    }

    #[test]
    fn load_chain_reports_inheritance_cycles() {
        let root = std::env::temp_dir().join(format!("azulc-launch-chain-{}", Uuid::new_v4()));
        for (version, parent) in [("a", "b"), ("b", "a")] {
            let directory = root.join("versions").join(version);
            std::fs::create_dir_all(&directory).expect("create version fixture");
            let profile = serde_json::json!({ "id": version, "inheritsFrom": parent });
            std::fs::write(
                directory.join(format!("{version}.json")),
                serde_json::to_vec(&profile).expect("serialize version fixture"),
            )
            .expect("write version fixture");
        }

        let result = load_chain(&root, "a");
        std::fs::remove_dir_all(root).expect("remove version fixture");

        assert!(matches!(result, Err(LaunchError::VersionCycle(id)) if id == "a"));
    }

    #[test]
    fn merge_keeps_lwjgl_main_and_each_native_classifier() {
        let base = VersionJson {
            id: "1.21.10".into(),
            libraries: vec![
                library("org.lwjgl:lwjgl:3.3.3"),
                library("org.lwjgl:lwjgl:3.3.3:natives-windows"),
                library("org.lwjgl:lwjgl:3.3.3:natives-windows-x86"),
            ],
            ..VersionJson::default()
        };
        let forge = VersionJson {
            libraries: vec![library("org.lwjgl:lwjgl:3.3.4")],
            ..VersionJson::default()
        };
        let merged = merge_chain(&[base, forge]);
        let names: Vec<_> = merged
            .libraries
            .iter()
            .map(|lib| lib.name.as_str())
            .collect();
        assert!(names.contains(&"org.lwjgl:lwjgl:3.3.4"));
        assert!(names.contains(&"org.lwjgl:lwjgl:3.3.3:natives-windows"));
        assert!(names.contains(&"org.lwjgl:lwjgl:3.3.3:natives-windows-x86"));
        assert_eq!(names.len(), 3);
    }

    #[test]
    fn classpath_switch_is_moved_to_environment() {
        assert_eq!(
            without_classpath_switch(vec![
                "-Dfoo=bar".into(),
                "-cp".into(),
                "a.jar;b.jar".into(),
                "Main".into(),
            ]),
            vec!["-Dfoo=bar", "Main"]
        );
    }

    #[test]
    fn forge_family_identifies_the_primary_minecraft_jar() {
        let root = Path::new("minecraft-root");
        let client = root.join("instance").join("1.20.1.jar");
        for loader in [LoaderKind::Forge, LoaderKind::NeoForge] {
            let arguments = forge_compatibility_jvm_args(loader, &client);
            assert_eq!(arguments.len(), 3);
            assert_eq!(
                arguments[0],
                format!("-Dminecraft.client.jar={}", client.display())
            );
            assert!(
                arguments
                    .iter()
                    .any(|argument| argument == "-Dfml.ignorePatchDiscrepancies=true")
            );
        }
        assert!(forge_compatibility_jvm_args(LoaderKind::Vanilla, &client).is_empty());
        assert!(forge_compatibility_jvm_args(LoaderKind::Fabric, &client).is_empty());
    }

    #[test]
    fn forge_jvm_version_name_matches_the_vanilla_client_jar() {
        assert_eq!(
            jvm_version_name(LoaderKind::Forge, "1.20.1-forge-47.4.20", "1.20.1"),
            "1.20.1"
        );
        assert_eq!(
            jvm_version_name(LoaderKind::NeoForge, "neoforge-21.1.249", "1.21.1"),
            "1.21.1"
        );
        assert_eq!(
            jvm_version_name(LoaderKind::Fabric, "fabric-loader-0.16.14-1.21.8", "1.21.8"),
            "fabric-loader-0.16.14-1.21.8"
        );
    }

    #[test]
    fn merge_keeps_newer_platform_specific_lwjgl_variant() {
        let windows = Library {
            name: "org.lwjgl.lwjgl:lwjgl:2.9.4-nightly-20150209".into(),
            rules: vec![Rule {
                action: "allow".into(),
                os: None,
                features: HashMap::new(),
            }],
            ..Library::default()
        };
        let macos = Library {
            name: "org.lwjgl.lwjgl:lwjgl:2.9.2-nightly-20140822".into(),
            rules: vec![Rule {
                action: "allow".into(),
                os: Some(minecraft::OsRule {
                    name: Some("osx".into()),
                    arch: None,
                    version: None,
                }),
                features: HashMap::new(),
            }],
            ..Library::default()
        };
        let merged = merge_chain(&[VersionJson {
            libraries: vec![windows, macos],
            ..VersionJson::default()
        }]);

        assert_eq!(merged.libraries.len(), 1);
        assert_eq!(
            merged.libraries[0].name,
            "org.lwjgl.lwjgl:lwjgl:2.9.4-nightly-20150209"
        );
        assert!(minecraft::rules_allow(&merged.libraries[0].rules));
    }

    #[test]
    fn merge_does_not_replace_current_platform_lwjgl_with_foreign_nightly() {
        let current_os = if cfg!(windows) {
            "windows"
        } else if cfg!(target_os = "macos") {
            "osx"
        } else {
            "linux"
        };
        let foreign_os = if current_os == "osx" {
            "windows"
        } else {
            "osx"
        };
        let platform = |version: &str, os: &str| Library {
            name: format!("org.lwjgl.lwjgl:lwjgl-platform:{version}"),
            rules: vec![Rule {
                action: "allow".into(),
                os: Some(minecraft::OsRule {
                    name: Some(os.into()),
                    arch: None,
                    version: None,
                }),
                features: HashMap::new(),
            }],
            natives: HashMap::from([(current_os.into(), "natives-windows".into())]),
            ..Library::default()
        };
        let merged = merge_chain(&[VersionJson {
            libraries: vec![
                platform("2.9.0", current_os),
                platform("2.9.1-nightly-20130708-debug3", foreign_os),
            ],
            ..VersionJson::default()
        }]);

        assert_eq!(merged.libraries.len(), 1);
        assert_eq!(
            merged.libraries[0].name,
            "org.lwjgl.lwjgl:lwjgl-platform:2.9.0"
        );
        assert!(minecraft::rules_allow(&merged.libraries[0].rules));
    }

    #[test]
    fn merge_keeps_same_coordinate_as_classpath_and_native_entries() {
        let main = library("org.lwjgl:lwjgl:3.2.2");
        let native = Library {
            name: "org.lwjgl:lwjgl:3.2.2".into(),
            natives: HashMap::from([("windows".into(), "natives-windows".into())]),
            ..Library::default()
        };
        let merged = merge_chain(&[VersionJson {
            libraries: vec![main, native],
            ..VersionJson::default()
        }]);

        assert_eq!(merged.libraries.len(), 2);
        assert_eq!(
            merged
                .libraries
                .iter()
                .filter(|library| library.natives.is_empty())
                .count(),
            1
        );
        assert_eq!(
            merged
                .libraries
                .iter()
                .filter(|library| !library.natives.is_empty())
                .count(),
            1
        );
    }

    #[test]
    fn recognizes_sjmcl_game_ready_markers_case_insensitively() {
        assert!(launch_line_is_ready(
            "[Render thread/INFO] Setting user: Steve"
        ));
        assert!(launch_line_is_ready("Backend library: LWJGL version 3.3.3"));
        assert!(launch_line_is_ready(
            "Starting up SoundSystem... LWJGL OpenAL"
        ));
        assert!(!launch_line_is_ready("Loading Minecraft 1.20.1"));
    }
}
