//! Real-time CPU/RAM usage snapshot for the resource-usage display (both
//! the desktop settings window and the browser extension's popup poll
//! this). Separate from resource_limits.rs on purpose: that module
//! enforces the user's configured caps, this one just reports what's
//! actually happening right now -- no VRAM number here, because unlike
//! CPU/RAM there's no cross-platform OS API to read actual GPU memory
//! use (see resource_limits::gpu_layers_for_budget's docs); the VRAM
//! field below is the user's configured *budget*, not a live reading.

use serde::Serialize;
use std::sync::{Mutex, OnceLock};
use sysinfo::System;

/// One persistent `System` instance, refreshed on every snapshot rather
/// than recreated -- sysinfo's CPU-usage percentage is a delta since the
/// last refresh, so a fresh instance each call would always report 0%.
fn system() -> &'static Mutex<System> {
    static SYSTEM: OnceLock<Mutex<System>> = OnceLock::new();
    SYSTEM.get_or_init(|| Mutex::new(System::new_all()))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceUsage {
    /// Whole-system CPU load, 0-100. Only accurate from the second call
    /// onward within a process's lifetime (sysinfo needs a prior refresh
    /// to compute a delta) -- callers that poll repeatedly (as both UIs
    /// here do) see correct values after the first tick.
    pub cpu_percent: f32,
    pub used_memory_mb: u64,
    pub total_memory_mb: u64,
    /// Echoes the user's configured caps/budget (settings.rs) alongside
    /// the live reading, so a single poll has everything a usage bar
    /// needs without a second request.
    pub max_cpu_threads: u32,
    pub max_memory_mb: u32,
    pub max_vram_mb: u32,
}

pub fn snapshot(settings: &crate::settings::Settings) -> ResourceUsage {
    let mut sys = system().lock().unwrap();
    sys.refresh_cpu_usage();
    sys.refresh_memory();
    ResourceUsage {
        cpu_percent: sys.global_cpu_usage(),
        used_memory_mb: sys.used_memory() / 1024 / 1024,
        total_memory_mb: sys.total_memory() / 1024 / 1024,
        max_cpu_threads: settings.max_cpu_threads,
        max_memory_mb: settings.max_memory_mb,
        max_vram_mb: settings.max_vram_mb,
    }
}
