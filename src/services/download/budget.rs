//! One transfer budget shared by all batches, adjustable without cancelling work.
use std::sync::{Mutex, OnceLock};
use tokio::sync::Notify;

pub(super) struct Budget {
    state: Mutex<(usize, usize)>, // running, limit
    changed: Notify,
}

impl Budget {
    fn new(limit: usize) -> Self {
        Self {
            state: Mutex::new((0, limit.max(1))),
            changed: Notify::new(),
        }
    }
    fn set_limit(&self, limit: usize) {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).1 = limit.max(1);
        self.changed.notify_waiters();
    }
    pub(super) async fn acquire(&self) -> Permit<'_> {
        loop {
            let notified = self.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            {
                let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
                if state.0 < state.1 {
                    state.0 += 1;
                    return Permit(self);
                }
            }
            notified.await;
        }
    }
}

pub(super) struct Permit<'a>(&'a Budget);
impl Drop for Permit<'_> {
    fn drop(&mut self) {
        self.0.state.lock().unwrap_or_else(|e| e.into_inner()).0 -= 1;
        self.0.changed.notify_waiters();
    }
}

pub(super) fn global() -> &'static Budget {
    static BUDGET: OnceLock<Budget> = OnceLock::new();
    BUDGET.get_or_init(|| Budget::new(crate::domain::cpu_thread_count()))
}

pub(crate) fn set_concurrency(limit: usize) {
    global().set_limit(limit.clamp(1, crate::domain::cpu_thread_count()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };

    #[tokio::test]
    async fn batches_share_one_limit_and_return_permits_on_cancel() {
        let budget = Arc::new(Budget::new(2));
        let peak = Arc::new(AtomicUsize::new(0));
        let active = Arc::new(AtomicUsize::new(0));
        let mut tasks = Vec::new();
        for _ in 0..8 {
            let (budget, peak, active) = (budget.clone(), peak.clone(), active.clone());
            tasks.push(tokio::spawn(async move {
                let _permit = budget.acquire().await;
                let running = active.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(running, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(10)).await;
                active.fetch_sub(1, Ordering::SeqCst);
            }));
        }
        for task in tasks {
            task.await.unwrap();
        }
        assert_eq!(peak.load(Ordering::SeqCst), 2);
        let first = budget.acquire().await;
        let second = budget.acquire().await;
        budget.set_limit(1);
        drop(first);
        assert!(
            tokio::time::timeout(Duration::from_millis(20), budget.acquire())
                .await
                .is_err()
        );
        drop(second);
        let _permit = tokio::time::timeout(Duration::from_secs(1), budget.acquire())
            .await
            .unwrap();
    }
}
