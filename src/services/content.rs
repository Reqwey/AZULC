use crate::{domain::Instance, services::thumbnail};
use futures::{StreamExt, stream};
use std::{
    io,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

const MAX_LOCAL_SCAN_WORKERS: usize = 16;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum ContentKind {
    #[default]
    Worlds,
    Mods,
    ResourcePacks,
    ShaderPacks,
    DataPacks,
    Screenshots,
}

impl ContentKind {
    pub fn directory(self) -> Option<&'static str> {
        match self {
            Self::Worlds => Some("saves"),
            Self::Mods => Some("mods"),
            Self::ResourcePacks => Some("resourcepacks"),
            Self::ShaderPacks => Some("shaderpacks"),
            Self::DataPacks => Some("datapacks"),
            Self::Screenshots => Some("screenshots"),
        }
    }

    pub fn downloadable(self) -> bool {
        matches!(
            self,
            Self::Mods | Self::ResourcePacks | Self::ShaderPacks | Self::DataPacks
        )
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContentEntry {
    pub name: String,
    pub path: PathBuf,
    pub modified_unix: u64,
    pub size: u64,
    pub is_directory: bool,
}

/// Scans one instance content directory and returns supported direct children.
///
/// A missing content directory is treated as an empty list because Minecraft only
/// creates several of these directories after first use. Other filesystem failures
/// remain visible to the caller.
pub async fn scan_content(instance: &Instance, kind: ContentKind) -> io::Result<Vec<ContentEntry>> {
    let directory = instance.game_dir.join(
        kind.directory()
            .expect("every content kind has an instance directory"),
    );
    let mut content = scan_directory(directory, kind).await?;

    content.sort_by(|left, right| {
        right
            .modified_unix
            .cmp(&left.modified_unix)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
            .then_with(|| left.name.cmp(&right.name))
    });
    Ok(content)
}

pub async fn load_thumbnails(
    kind: ContentKind,
    jobs: Vec<(PathBuf, bool)>,
) -> Vec<(PathBuf, Option<thumbnail::Thumbnail>)> {
    stream::iter(jobs.into_iter().map(|(path, is_directory)| async move {
        let thumbnail = thumbnail::load_local(kind, path.clone(), is_directory).await;
        (path, thumbnail)
    }))
    .buffer_unordered(local_scan_workers())
    .collect::<Vec<_>>()
    .await
}

pub fn name_matches_query(name: &str, query: &str) -> bool {
    let query = query.trim();
    query.is_empty() || name.to_lowercase().contains(&query.to_lowercase())
}

async fn scan_directory(directory: PathBuf, kind: ContentKind) -> io::Result<Vec<ContentEntry>> {
    let mut reader = match tokio::fs::read_dir(directory).await {
        Ok(reader) => reader,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut paths = Vec::new();
    while let Some(entry) = reader.next_entry().await? {
        paths.push(entry.path());
    }

    let entries = stream::iter(paths.into_iter().map(|path| async move {
        let metadata = tokio::fs::metadata(&path).await.ok()?;
        let is_directory = metadata.is_dir();
        if !accepts_entry(kind, &path, metadata.is_file(), is_directory) {
            return None;
        }
        let name = path.file_name()?.to_string_lossy().into_owned();
        let modified_unix = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |duration| duration.as_secs());
        Some(ContentEntry {
            name,
            path,
            modified_unix,
            size: if is_directory { 0 } else { metadata.len() },
            is_directory,
        })
    }))
    .buffer_unordered(local_scan_workers())
    .filter_map(|entry| async move { entry })
    .collect();
    Ok(entries.await)
}

fn local_scan_workers() -> usize {
    std::thread::available_parallelism()
        .map_or(4, usize::from)
        .saturating_mul(2)
        .clamp(4, MAX_LOCAL_SCAN_WORKERS)
}

fn accepts_entry(kind: ContentKind, path: &Path, is_file: bool, is_directory: bool) -> bool {
    match kind {
        ContentKind::Worlds => is_directory,
        ContentKind::Mods => is_file && extension_is_one_of(path, &["jar", "zip", "disabled"]),
        ContentKind::ResourcePacks => {
            is_directory || (is_file && extension_is_one_of(path, &["zip"]))
        }
        ContentKind::ShaderPacks => {
            is_directory || (is_file && extension_is_one_of(path, &["zip"]))
        }
        ContentKind::DataPacks => is_directory || (is_file && extension_is_one_of(path, &["zip"])),
        ContentKind::Screenshots => is_file && extension_is_one_of(path, &["png", "jpg", "jpeg"]),
    }
}

fn extension_is_one_of(path: &Path, accepted: &[&str]) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            accepted
                .iter()
                .any(|candidate| extension.eq_ignore_ascii_case(candidate))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{InstanceColor, InstanceOrigin, InstanceSettings, LoaderKind, LoaderSpec};
    use uuid::Uuid;

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("azulc-content-test-{}", Uuid::new_v4()));
            std::fs::create_dir_all(&path).expect("create content test directory");
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn instance_at(game_dir: PathBuf) -> Instance {
        Instance {
            id: Uuid::new_v4(),
            name: "Test".into(),
            minecraft_version: "1.21.1".into(),
            version_id: "1.21.1".into(),
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

    #[test]
    fn mods_accept_archives_and_disabled_files_only() {
        assert!(accepts_entry(
            ContentKind::Mods,
            Path::new("example.jar"),
            true,
            false
        ));
        assert!(accepts_entry(
            ContentKind::Mods,
            Path::new("example.JAR.DISABLED"),
            true,
            false
        ));
        assert!(accepts_entry(
            ContentKind::Mods,
            Path::new("legacy.zip"),
            true,
            false
        ));
        assert!(!accepts_entry(
            ContentKind::Mods,
            Path::new("notes.txt"),
            true,
            false
        ));
        assert!(!accepts_entry(
            ContentKind::Mods,
            Path::new("unpacked"),
            false,
            true
        ));
    }

    #[test]
    fn each_content_kind_applies_its_own_file_policy() {
        assert!(accepts_entry(
            ContentKind::Worlds,
            Path::new("New World"),
            false,
            true
        ));
        assert!(!accepts_entry(
            ContentKind::Worlds,
            Path::new("level.dat"),
            true,
            false
        ));
        assert!(accepts_entry(
            ContentKind::ResourcePacks,
            Path::new("unpacked-pack"),
            false,
            true
        ));
        assert!(accepts_entry(
            ContentKind::ResourcePacks,
            Path::new("pack.ZIP"),
            true,
            false
        ));
        assert!(accepts_entry(
            ContentKind::Screenshots,
            Path::new("shot.JPEG"),
            true,
            false
        ));
        assert!(!accepts_entry(
            ContentKind::Screenshots,
            Path::new("shot.webp"),
            true,
            false
        ));
    }

    #[test]
    fn name_search_is_case_insensitive() {
        assert!(name_matches_query("Sodium-Fabric.jar", "sODium"));
    }

    #[test]
    fn empty_name_search_matches_every_entry() {
        assert!(name_matches_query("Any Mod.jar", "   "));
    }

    #[tokio::test]
    async fn scan_content_finds_instance_level_data_pack() {
        let directory = TestDirectory::new();
        let data_packs = directory.0.join("datapacks");
        std::fs::create_dir_all(&data_packs).expect("create instance data pack directory");
        std::fs::write(data_packs.join("global.zip"), []).expect("write data pack fixture");

        let entries = scan_content(&instance_at(directory.0.clone()), ContentKind::DataPacks)
            .await
            .expect("scan data packs");

        assert_eq!(
            entries.first().map(|entry| entry.name.as_str()),
            Some("global.zip")
        );
    }
}
