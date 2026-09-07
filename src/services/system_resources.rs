use crate::domain::{DEFAULT_GAME_MEMORY_MB, cpu_thread_count};
use sysinfo::{MemoryRefreshKind, RefreshKind, System};

const MIB: u64 = 1024 * 1024;
pub const MIN_GAME_MEMORY_MB: u32 = 512;
const SYSTEM_MEMORY_RESERVE_MB: u32 = 1024;

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
        self.total_memory_mb
            .saturating_sub(SYSTEM_MEMORY_RESERVE_MB)
            .max(MIN_GAME_MEMORY_MB)
    }

    pub fn game_memory_mb(self, automatic: bool, configured_mb: u32) -> u32 {
        let requested = if automatic {
            DEFAULT_GAME_MEMORY_MB
        } else {
            configured_mb
        };
        requested.clamp(MIN_GAME_MEMORY_MB, self.memory_limit_mb())
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

    #[test]
    fn automatic_memory_is_not_reduced_by_temporarily_low_available_memory() {
        let resources = SystemResources {
            available_memory_mb: 1800,
            total_memory_mb: 16 * 1024,
            cpu_threads: 1,
        };

        assert_eq!(resources.game_memory_mb(true, 8192), DEFAULT_GAME_MEMORY_MB);
    }

    #[test]
    fn memory_limit_reserves_space_for_the_operating_system() {
        let resources = SystemResources {
            available_memory_mb: 4096,
            total_memory_mb: 4096,
            cpu_threads: 1,
        };

        assert_eq!(resources.memory_limit_mb(), 3072);
    }

    #[test]
    fn manual_memory_is_capped_by_the_stable_system_limit() {
        let resources = SystemResources {
            available_memory_mb: 1024,
            total_memory_mb: 8192,
            cpu_threads: 1,
        };

        assert_eq!(resources.game_memory_mb(false, 8192), 7168);
    }
}
