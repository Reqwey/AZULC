use crate::domain::cpu_thread_count;
use sysinfo::{MemoryRefreshKind, RefreshKind, System};

const MIB: u64 = 1024 * 1024;
pub const MIN_GAME_MEMORY_MB: u32 = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemResources {
    pub available_memory_mb: u32,
    pub total_memory_mb: u32,
    pub cpu_threads: usize,
}

impl Default for SystemResources {
    fn default() -> Self {
        Self {
            available_memory_mb: MIN_GAME_MEMORY_MB,
            total_memory_mb: MIN_GAME_MEMORY_MB,
            cpu_threads: cpu_thread_count(),
        }
    }
}

impl SystemResources {
    pub fn memory_limit_mb(self) -> u32 {
        self.available_memory_mb.max(MIN_GAME_MEMORY_MB)
    }
}

pub async fn read() -> SystemResources {
    tokio::task::spawn_blocking(read_blocking)
        .await
        .unwrap_or_default()
}

pub fn read_blocking() -> SystemResources {
    let mut system = System::new_with_specifics(
        RefreshKind::nothing().with_memory(MemoryRefreshKind::nothing().with_ram()),
    );
    system.refresh_memory_specifics(MemoryRefreshKind::nothing().with_ram());
    SystemResources {
        available_memory_mb: bytes_to_mb(system.available_memory()),
        total_memory_mb: bytes_to_mb(system.total_memory()),
        cpu_threads: cpu_thread_count(),
    }
}

fn bytes_to_mb(bytes: u64) -> u32 {
    u32::try_from(bytes / MIB).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_limit_always_keeps_the_slider_range_valid() {
        let resources = SystemResources {
            available_memory_mb: 0,
            total_memory_mb: 0,
            cpu_threads: 1,
        };
        assert_eq!(resources.memory_limit_mb(), MIN_GAME_MEMORY_MB);
    }
}
