use crate::{domain::PersistedState, services::download::file_ops};
use directories::ProjectDirs;
use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Hash)]
pub struct Paths {
    pub data: PathBuf,
    pub minecraft: PathBuf,
    pub instances: PathBuf,
    pub state_file: PathBuf,
}

impl Paths {
    pub fn discover() -> Self {
        let default = Self::default_data_root();
        let data = fs::read_to_string(Self::locator_file())
            .ok()
            .and_then(|text| serde_json::from_str::<PathBuf>(&text).ok())
            .filter(|path| path.is_absolute())
            .unwrap_or(default);
        Self::from_data_root(data)
    }

    pub fn from_data_root(data: PathBuf) -> Self {
        Self {
            minecraft: data.join("minecraft"),
            instances: data.join("instances"),
            state_file: data.join("state.json"),
            data,
        }
    }

    fn default_data_root() -> PathBuf {
        ProjectDirs::from("dev", "AZULC", "AZULC")
            .map(|p| p.data_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from(".azulc"))
    }

    fn locator_file() -> PathBuf {
        Self::default_data_root().join("storage-root.json")
    }

    pub fn prepare(&self) -> io::Result<()> {
        fs::create_dir_all(&self.minecraft)?;
        fs::create_dir_all(&self.instances)?;
        Ok(())
    }

    pub fn instance_dir(&self, id: uuid::Uuid) -> PathBuf {
        self.instances.join(id.to_string())
    }

    fn bind_instance_dirs(&self, state: &mut PersistedState) {
        // Instance directories are launcher-owned and derive from the active storage root.
        for instance in &mut state.instances {
            instance.game_dir = self.instance_dir(instance.id);
        }
    }

    pub fn load(&self) -> PersistedState {
        let mut state = fs::read_to_string(&self.state_file)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        self.bind_instance_dirs(&mut state);
        state
    }

    pub fn save(&self, state: &PersistedState) -> io::Result<()> {
        self.prepare()?;
        fs::write(&self.state_file, serde_json::to_vec_pretty(state)?)
    }
}

#[derive(Debug, Clone)]
pub struct MigrationOutcome {
    pub paths: Paths,
    pub state: PersistedState,
    pub cleanup_warning: Option<String>,
}

pub async fn migrate(
    source: Paths,
    destination: PathBuf,
    state: PersistedState,
) -> Result<MigrationOutcome, String> {
    tokio::task::spawn_blocking(move || migrate_blocking(source, destination, state))
        .await
        .map_err(|error| format!("Storage migration task failed: {error}"))?
}

fn migrate_blocking(
    source: Paths,
    destination: PathBuf,
    state: PersistedState,
) -> Result<MigrationOutcome, String> {
    let source_comparison = fs::canonicalize(&source.data)
        .map_err(|error| format!("Could not resolve the current storage folder: {error}"))?;
    let destination_comparison = fs::canonicalize(&destination)
        .map_err(|error| format!("Could not resolve the selected folder: {error}"))?;
    let default_root = Paths::default_data_root();
    migrate_blocking_with_default(
        source.data,
        destination,
        state,
        default_root,
        source_comparison,
        destination_comparison,
    )
}

fn migrate_blocking_with_default(
    source_root: PathBuf,
    destination_root: PathBuf,
    mut state: PersistedState,
    default_root: PathBuf,
    source_comparison: PathBuf,
    destination_comparison: PathBuf,
) -> Result<MigrationOutcome, String> {
    let locator = default_root.join("storage-root.json");
    let default_comparison = canonicalize_or_original(default_root.clone());
    let source_is_default = source_comparison == default_comparison;
    let destination_is_default = destination_comparison == default_comparison;

    if source_comparison == destination_comparison {
        return Err("The selected folder is already the current storage root.".into());
    }
    if destination_comparison.starts_with(&source_comparison)
        || source_comparison.starts_with(&destination_comparison)
    {
        return Err(
            "The new storage root cannot contain, or be contained by, the current root.".into(),
        );
    }
    ensure_destination_is_empty(&destination_root, destination_is_default, &locator)?;

    if let Err(error) = copy_directory_contents(&source_root, &destination_root, &locator) {
        let _ = remove_destination_payload(&destination_root, destination_is_default, &locator);
        return Err(error);
    }

    let new_paths = Paths::from_data_root(destination_root.clone());
    new_paths.bind_instance_dirs(&mut state);
    if let Err(error) = new_paths.save(&state) {
        let _ = remove_destination_payload(&destination_root, destination_is_default, &locator);
        return Err(format!(
            "Could not write launcher state at the new location: {error}"
        ));
    }

    if let Err(error) = switch_locator(&destination_root, destination_is_default, &locator) {
        let _ = remove_destination_payload(&destination_root, destination_is_default, &locator);
        return Err(error);
    }
    let cleanup_warning = cleanup_source(&source_root, source_is_default, &locator)
        .err()
        .map(|error| {
            format!("The new location is active, but the old files could not be removed: {error}")
        });

    Ok(MigrationOutcome {
        paths: new_paths,
        state,
        cleanup_warning,
    })
}

fn canonicalize_or_original(path: PathBuf) -> PathBuf {
    fs::canonicalize(&path).unwrap_or(path)
}

fn ensure_destination_is_empty(
    destination: &Path,
    destination_is_default: bool,
    locator: &Path,
) -> Result<(), String> {
    let entries = fs::read_dir(destination)
        .map_err(|error| format!("Could not inspect the selected folder: {error}"))?;
    for entry in entries {
        let entry =
            entry.map_err(|error| format!("Could not inspect the selected folder: {error}"))?;
        if destination_is_default && entry.file_name() == locator.file_name().unwrap_or_default() {
            continue;
        }
        return Err("Choose an empty folder for the new storage root.".into());
    }
    Ok(())
}

fn copy_directory_contents(
    source: &Path,
    destination: &Path,
    locator: &Path,
) -> Result<(), String> {
    for entry in fs::read_dir(source)
        .map_err(|error| format!("Could not read {}: {error}", source.display()))?
    {
        let entry =
            entry.map_err(|error| format!("Could not read {}: {error}", source.display()))?;
        if entry.path() == locator {
            continue;
        }
        copy_entry(&entry.path(), &destination.join(entry.file_name()), locator)?;
    }
    Ok(())
}

fn copy_entry(source: &Path, destination: &Path, locator: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(source)
        .map_err(|error| format!("Could not inspect {}: {error}", source.display()))?;
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "Storage contains a symbolic link that cannot be migrated safely: {}",
            source.display()
        ));
    }
    if metadata.is_dir() {
        fs::create_dir(destination)
            .map_err(|error| format!("Could not create {}: {error}", destination.display()))?;
        copy_directory_contents(source, destination, locator)
    } else if metadata.is_file() {
        fs::copy(source, destination)
            .map(|_| ())
            .map_err(|error| format!("Could not copy {}: {error}", source.display()))
    } else {
        Err(format!(
            "Storage contains an unsupported file type: {}",
            source.display()
        ))
    }
}

fn switch_locator(
    destination: &Path,
    destination_is_default: bool,
    locator: &Path,
) -> Result<(), String> {
    if destination_is_default {
        if locator.exists() {
            fs::remove_file(locator).map_err(|error| {
                format!("Could not activate the default storage folder: {error}")
            })?;
        }
        return Ok(());
    }
    let parent = locator
        .parent()
        .ok_or_else(|| "Could not determine where to save the storage preference.".to_owned())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Could not prepare the storage preference folder: {error}"))?;
    let encoded = serde_json::to_vec_pretty(&destination)
        .map_err(|error| format!("Could not encode the storage preference: {error}"))?;
    file_ops::write_atomic_sync(locator, &encoded)
        .map_err(|error| format!("Could not save the storage preference: {error}"))
}

fn cleanup_source(source: &Path, source_is_default: bool, locator: &Path) -> io::Result<()> {
    if !source_is_default {
        return fs::remove_dir_all(source);
    }
    for entry in fs::read_dir(source)? {
        let path = entry?.path();
        if path.file_name() == locator.file_name() {
            continue;
        }
        if path.is_dir() {
            fs::remove_dir_all(path)?;
        } else {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

fn remove_destination_payload(
    destination: &Path,
    destination_is_default: bool,
    locator: &Path,
) -> io::Result<()> {
    if !destination_is_default {
        for entry in fs::read_dir(destination)? {
            let path = entry?.path();
            if path.is_dir() {
                fs::remove_dir_all(path)?;
            } else {
                fs::remove_file(path)?;
            }
        }
        return Ok(());
    }
    cleanup_source(destination, true, locator)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        Instance, InstanceColor, InstanceOrigin, InstanceSettings, LoaderKind, LoaderSpec,
    };
    use uuid::Uuid;

    #[test]
    fn migration_moves_files_persists_the_root_and_rebases_instances() {
        let fixture = fixture();
        let source = fixture.join("old");
        let destination = fixture.join("new");
        let default = fixture.join("default");
        fs::create_dir_all(&destination).unwrap();
        fs::create_dir_all(&default).unwrap();
        fs::write(
            default.join("storage-root.json"),
            serde_json::to_vec(&source).unwrap(),
        )
        .unwrap();

        let paths = Paths::from_data_root(source.clone());
        paths.prepare().unwrap();
        fs::write(paths.minecraft.join("marker.txt"), b"minecraft").unwrap();
        let id = Uuid::new_v4();
        let mut state = PersistedState::default();
        state.instances.push(instance(
            id,
            PathBuf::from("a differently spelled old root").join(id.to_string()),
        ));
        paths.save(&state).unwrap();

        let outcome = migrate_blocking_with_default(
            source.clone(),
            destination.clone(),
            state,
            default.clone(),
            fs::canonicalize(&source).unwrap(),
            fs::canonicalize(&destination).unwrap(),
        )
        .unwrap();

        assert_eq!(outcome.paths.data, destination);
        assert_eq!(
            outcome.state.instances[0].game_dir,
            outcome.paths.instance_dir(id)
        );
        assert_eq!(
            fs::read(outcome.paths.minecraft.join("marker.txt")).unwrap(),
            b"minecraft"
        );
        assert!(!source.exists());
        let selected: PathBuf =
            serde_json::from_slice(&fs::read(default.join("storage-root.json")).unwrap()).unwrap();
        assert_eq!(selected, outcome.paths.data);

        fs::remove_dir_all(fixture).unwrap();
    }

    #[test]
    fn load_repairs_instance_directories_using_the_active_root() {
        let fixture = fixture();
        let paths = Paths::from_data_root(fixture.clone());
        paths.prepare().unwrap();
        let id = Uuid::new_v4();
        let mut state = PersistedState::default();
        state.instances.push(instance(
            id,
            PathBuf::from("stale-root").join(id.to_string()),
        ));
        paths.save(&state).unwrap();

        let loaded = paths.load();

        assert_eq!(loaded.instances[0].game_dir, paths.instance_dir(id));

        fs::remove_dir_all(fixture).unwrap();
    }

    #[test]
    fn migration_can_return_to_a_default_folder_that_contains_only_the_locator() {
        let fixture = fixture();
        let source = fixture.join("custom");
        let default = fixture.join("default");
        fs::create_dir_all(&default).unwrap();
        let paths = Paths::from_data_root(source.clone());
        paths.prepare().unwrap();
        paths.save(&PersistedState::default()).unwrap();
        fs::write(
            default.join("storage-root.json"),
            serde_json::to_vec(&source).unwrap(),
        )
        .unwrap();

        let outcome = migrate_blocking_with_default(
            source.clone(),
            default.clone(),
            PersistedState::default(),
            default.clone(),
            fs::canonicalize(&source).unwrap(),
            fs::canonicalize(&default).unwrap(),
        )
        .unwrap();

        assert_eq!(outcome.paths.data, default);
        assert!(outcome.paths.state_file.is_file());
        assert!(!outcome.paths.data.join("storage-root.json").exists());
        assert!(!source.exists());

        fs::remove_dir_all(fixture).unwrap();
    }

    #[test]
    fn migration_from_default_keeps_only_the_locator_in_the_old_root() {
        let fixture = fixture();
        let default = fixture.join("default");
        let destination = fixture.join("custom");
        fs::create_dir_all(&destination).unwrap();
        let paths = Paths::from_data_root(default.clone());
        paths.prepare().unwrap();
        paths.save(&PersistedState::default()).unwrap();
        fs::write(paths.minecraft.join("marker.txt"), b"minecraft").unwrap();

        let outcome = migrate_blocking_with_default(
            default.clone(),
            destination.clone(),
            PersistedState::default(),
            default.clone(),
            fs::canonicalize(&default).unwrap(),
            fs::canonicalize(&destination).unwrap(),
        )
        .unwrap();

        assert_eq!(outcome.paths.data, destination);
        assert!(outcome.paths.minecraft.join("marker.txt").is_file());
        let remaining = fs::read_dir(&default)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        assert_eq!(remaining, [std::ffi::OsString::from("storage-root.json")]);

        fs::remove_dir_all(fixture).unwrap();
    }

    #[test]
    fn migration_rejects_a_nonempty_destination_without_touching_the_source() {
        let fixture = fixture();
        let source = fixture.join("old");
        let destination = fixture.join("new");
        let default = fixture.join("default");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&destination).unwrap();
        fs::create_dir_all(&default).unwrap();
        fs::write(source.join("keep.txt"), b"source").unwrap();
        fs::write(destination.join("keep.txt"), b"destination").unwrap();

        let result = migrate_blocking_with_default(
            source.clone(),
            destination.clone(),
            PersistedState::default(),
            default,
            fs::canonicalize(&source).unwrap(),
            fs::canonicalize(&destination).unwrap(),
        );

        assert!(result.unwrap_err().contains("empty folder"));
        assert_eq!(fs::read(source.join("keep.txt")).unwrap(), b"source");
        assert_eq!(
            fs::read(destination.join("keep.txt")).unwrap(),
            b"destination"
        );

        fs::remove_dir_all(fixture).unwrap();
    }

    fn fixture() -> PathBuf {
        let path = std::env::temp_dir().join(format!("azulc-storage-test-{}", Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        path
    }

    fn instance(id: Uuid, game_dir: PathBuf) -> Instance {
        Instance {
            id,
            name: "Test".into(),
            minecraft_version: "1.21.8".into(),
            version_id: "1.21.8".into(),
            loader: LoaderSpec {
                kind: LoaderKind::Vanilla,
                version: None,
            },
            game_dir,
            installed: true,
            description: String::new(),
            color: InstanceColor::default(),
            favorite: false,
            play_time_seconds: 0,
            last_played_unix: None,
            settings: InstanceSettings::default(),
            origin: InstanceOrigin::default(),
        }
    }
}
