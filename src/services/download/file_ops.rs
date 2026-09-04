//! Atomic file creation, replacement, and copy primitives.

use std::{
    io,
    io::Write as _,
    path::{Path, PathBuf},
};

use tokio::io::AsyncWriteExt;

/// Returns a short, unique staging path beside the destination.
///
/// Keeping the temporary name independent of the destination avoids exceeding
/// platform component limits when a provider publishes a long, valid file name.
pub(crate) fn staging_path(destination: &Path) -> io::Result<PathBuf> {
    let parent = destination.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "destination has no parent directory",
        )
    })?;
    Ok(parent.join(format!(".azulc-{}.part", uuid::Uuid::new_v4())))
}

/// Atomically publishes a fully-written sibling file at its final path.
///
/// Existing destination data remains intact if replacement fails.
pub(crate) async fn replace_file(source: &Path, destination: &Path) -> io::Result<()> {
    let source = source.to_path_buf();
    let destination = destination.to_path_buf();
    tokio::task::spawn_blocking(move || replace_file_sync(&source, &destination))
        .await
        .map_err(|error| io::Error::other(format!("atomic replacement worker failed: {error}")))?
}

/// Writes bytes to a sibling staging file and atomically publishes them.
pub(crate) async fn write_atomic(destination: &Path, contents: &[u8]) -> io::Result<()> {
    if let Some(parent) = destination.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let staging = staging_path(destination)?;
    let result = async {
        let mut file = tokio::fs::File::create(&staging).await?;
        file.write_all(contents).await?;
        file.flush().await?;
        file.sync_all().await?;
        drop(file);
        replace_file(&staging, destination).await
    }
    .await;
    if result.is_err() {
        let _ = tokio::fs::remove_file(&staging).await;
    }
    result
}

/// Copies a blocking reader to a sibling staging file, then publishes it.
pub(crate) fn copy_reader_atomic(
    destination: &Path,
    reader: &mut impl io::Read,
) -> io::Result<u64> {
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let staging = staging_path(destination)?;
    let result = (|| {
        let mut file = std::fs::File::create(&staging)?;
        let written = io::copy(reader, &mut file)?;
        file.flush()?;
        file.sync_all()?;
        drop(file);
        replace_file_sync(&staging, destination)?;
        Ok(written)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&staging);
    }
    result
}

#[cfg(not(target_os = "windows"))]
fn replace_file_sync(source: &Path, destination: &Path) -> io::Result<()> {
    std::fs::rename(source, destination)
}

#[cfg(target_os = "windows")]
fn replace_file_sync(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };

    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    // SAFETY: Both paths are encoded as owned, NUL-terminated UTF-16 buffers
    // and remain alive for the duration of the call.
    let result = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn staging_paths_are_unique_short_siblings_for_long_destinations() {
        let destination = PathBuf::from("mods").join(format!("{}.jar", "a".repeat(251)));
        let first = staging_path(&destination).unwrap();
        let second = staging_path(&destination).unwrap();

        assert_eq!(first.parent(), destination.parent());
        assert_eq!(second.parent(), destination.parent());
        assert_ne!(first, second);
        assert!(first.file_name().unwrap().len() < 64);
    }

    #[tokio::test]
    async fn atomic_write_replaces_complete_contents() {
        let fixture =
            std::env::temp_dir().join(format!("azulc-atomic-write-{}", uuid::Uuid::new_v4()));
        let destination = fixture.join("value.json");
        write_atomic(&destination, b"first").await.unwrap();
        write_atomic(&destination, b"second").await.unwrap();

        assert_eq!(std::fs::read(&destination).unwrap(), b"second");
        std::fs::remove_dir_all(fixture).unwrap();
    }
}
