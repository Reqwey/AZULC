//! In-process leases for shared destinations and installation directories.
use std::{
    collections::HashMap,
    io,
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex, OnceLock, Weak},
};
use tokio::sync::RwLock;

pub(crate) fn for_path(path: &Path) -> io::Result<Arc<RwLock<()>>> {
    static LOCKS: OnceLock<Mutex<HashMap<PathBuf, Weak<RwLock<()>>>>> = OnceLock::new();
    let absolute = std::path::absolute(path)?;
    let mut key = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::ParentDir => {
                key.pop();
            }
            Component::CurDir => {}
            part => key.push(part.as_os_str()),
        }
    }
    #[cfg(windows)]
    let key = PathBuf::from(key.to_string_lossy().to_lowercase());
    let mut locks = LOCKS
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if let Some(lock) = locks.get(&key).and_then(Weak::upgrade) {
        return Ok(lock);
    }
    locks.retain(|_, lock| lock.strong_count() > 0);
    let lock = Arc::new(RwLock::new(()));
    locks.insert(key, Arc::downgrade(&lock));
    Ok(lock)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn independent_paths_and_readers_overlap_but_writer_excludes_them() {
        let root = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        let shared = for_path(&root).unwrap();
        let same = for_path(&root.join("child/..")).unwrap();
        assert!(Arc::ptr_eq(&shared, &same));
        let first = shared.clone().read_owned().await;
        let second = shared.clone().read_owned().await;
        let _independent = for_path(&root.join("other")).unwrap().write_owned().await;
        assert!(
            tokio::time::timeout(Duration::from_millis(20), same.clone().write_owned())
                .await
                .is_err()
        );
        drop(first);
        drop(second);
        let writer = same.write_owned().await;
        assert!(shared.try_read().is_err());
        drop(writer);
        assert!(shared.try_read().is_ok());
    }
}
