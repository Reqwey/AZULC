use std::sync::atomic::{AtomicUsize, Ordering};

static PENDING: AtomicUsize = AtomicUsize::new(0);

struct Worker;
impl Worker {
    fn new() -> Self {
        PENDING.fetch_add(1, Ordering::SeqCst);
        Self
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        PENDING.fetch_sub(1, Ordering::SeqCst);
    }
}

pub(crate) fn has_pending_work() -> bool {
    PENDING.load(Ordering::SeqCst) != 0
}

pub(super) async fn run<T: Send + 'static>(
    work: impl Future<Output = T> + Send + 'static,
) -> Result<T, tokio::task::JoinError> {
    let worker = Worker::new();
    tokio::spawn(async move {
        let _worker = worker;
        work.await
    })
    .await
}

/// Only for network/read-only work or async atomic writes. Blocking mutations
/// and Java processes must finish their own cleanup instead of being dropped.
pub(super) async fn until_cancelled<T>(
    tx: &tokio::sync::mpsc::UnboundedSender<crate::domain::PipelineEvent>,
    work: impl Future<Output = T>,
) -> Result<T, super::InstallError> {
    tokio::select! {
        biased;
        () = tx.closed() => Err(super::InstallError::Cancelled),
        result = work => Ok(result),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::Arc, time::Duration};

    #[tokio::test]
    async fn cancelled_metadata_wait_releases_the_shared_cache_lease() {
        let lock = Arc::new(tokio::sync::RwLock::new(()));
        let writer_lock = lock.clone();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let (started, ready) = tokio::sync::oneshot::channel();
        let worker = tokio::spawn(run(async move {
            let _lease = writer_lock.write_owned().await;
            started.send(()).unwrap();
            until_cancelled(&tx, std::future::pending::<()>()).await
        }));
        ready.await.unwrap();
        drop(rx);
        let result = tokio::time::timeout(Duration::from_secs(1), worker)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(matches!(result, Err(super::super::InstallError::Cancelled)));
        assert!(lock.try_write().is_ok());
    }

    #[tokio::test]
    async fn cancelled_observer_keeps_write_lease_until_blocking_work_finishes() {
        let lock = Arc::new(tokio::sync::RwLock::new(()));
        let writer_lock = lock.clone();
        let (started, ready) = tokio::sync::oneshot::channel();
        let (finish, wait) = std::sync::mpsc::channel();
        let observer = tokio::spawn(run(async move {
            let _lease = writer_lock.write_owned().await;
            tokio::task::spawn_blocking(move || {
                started.send(()).unwrap();
                wait.recv_timeout(Duration::from_secs(3)).unwrap();
            })
            .await
            .unwrap();
        }));
        ready.await.unwrap();
        observer.abort();
        assert!(observer.await.unwrap_err().is_cancelled());
        assert!(lock.try_write().is_err());
        finish.send(()).unwrap();
        let _lease = tokio::time::timeout(Duration::from_secs(3), lock.write_owned())
            .await
            .unwrap();
    }
}
