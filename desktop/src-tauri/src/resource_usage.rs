//! Real-time CPU/RAM/VRAM usage snapshot for the resource-usage display
//! (polled by both the desktop settings UI and the browser extension popup).
//! Reports system RAM, app process RAM, and GPU VRAM via DXGI on Windows.

use serde::Serialize;
use std::sync::{Mutex, OnceLock};
use sysinfo::{get_current_pid, ProcessesToUpdate, System};

use crate::download_tracker::{self, DownloadStatus};

/// One persistent `System` instance, refreshed on every snapshot rather
/// than recreated -- sysinfo's CPU-usage percentage is a delta since the
/// last refresh, so a fresh instance each call would always report 0%.
fn system() -> &'static Mutex<System> {
    static SYSTEM: OnceLock<Mutex<System>> = OnceLock::new();
    SYSTEM.get_or_init(|| Mutex::new(System::new_all()))
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ResourceUsage {
    /// Whole-system CPU load, 0-100.
    pub cpu_percent: f32,
    /// CPU percentage used by this process specifically.
    pub process_cpu_percent: f32,
    /// RAM consumed by this application process in MB.
    pub process_memory_mb: u64,
    /// Total RAM currently used across the whole operating system in MB.
    pub system_used_memory_mb: u64,
    /// Total physical RAM installed on the machine in MB.
    pub system_total_memory_mb: u64,
    /// Legacy compatibility alias for system_used_memory_mb.
    pub used_memory_mb: u64,
    /// Legacy compatibility alias for system_total_memory_mb.
    pub total_memory_mb: u64,
    /// Dedicated GPU model name if detected (e.g. "NVIDIA GeForce RTX 3060").
    pub gpu_name: Option<String>,
    /// VRAM used in MB on the active GPU adapter.
    pub vram_used_mb: Option<u64>,
    /// Total VRAM budget or capacity in MB on the active GPU adapter.
    pub vram_total_mb: Option<u64>,
    /// True if the ML inference pipeline is currently loaded in memory.
    pub models_loaded: bool,
    /// How many translation jobs are currently queued or actively processing.
    pub active_jobs: usize,
    /// Echoes the user's configured caps/budget (settings.rs).
    pub max_cpu_threads: u32,
    pub max_memory_mb: u32,
    /// Current model download progress if a download is active.
    pub download: Option<DownloadStatus>,
}

#[cfg(windows)]
fn query_dxgi_vram() -> Option<(String, u64, u64)> {
    use windows::core::Interface as _;
    use windows::Win32::Graphics::Dxgi::{
        CreateDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE, DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE,
        DXGI_MEMORY_SEGMENT_GROUP_LOCAL, DXGI_MEMORY_SEGMENT_GROUP_NON_LOCAL, IDXGIAdapter1,
        IDXGIAdapter3, IDXGIFactory6,
    };

    unsafe {
        let factory: IDXGIFactory6 = CreateDXGIFactory1().ok()?;
        let adapter = factory
            .EnumAdapterByGpuPreference::<IDXGIAdapter1>(0, DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE)
            .ok()?;
        let desc = adapter.GetDesc1().ok()?;
        if desc.Flags & (DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32) != 0 {
            return None;
        }
        let adapter3: IDXGIAdapter3 = adapter.cast().ok()?;
        let segment = if desc.DedicatedVideoMemory > 0 {
            DXGI_MEMORY_SEGMENT_GROUP_LOCAL
        } else {
            DXGI_MEMORY_SEGMENT_GROUP_NON_LOCAL
        };
        let mut memory = Default::default();
        adapter3.QueryVideoMemoryInfo(0, segment, &mut memory).ok()?;
        let name_len = desc.Description.iter().position(|&c| c == 0).unwrap_or(desc.Description.len());
        let name = String::from_utf16_lossy(&desc.Description[..name_len]).trim().to_string();
        let used_mb = memory.CurrentUsage / (1024 * 1024);
        let total_mb = if desc.DedicatedVideoMemory > 0 {
            desc.DedicatedVideoMemory as u64 / (1024 * 1024)
        } else if memory.Budget > 0 {
            memory.Budget / (1024 * 1024)
        } else {
            0
        };
        Some((name, used_mb, total_mb))
    }
}

#[cfg(not(windows))]
fn query_dxgi_vram() -> Option<(String, u64, u64)> {
    None
}

pub fn snapshot(settings: &crate::settings::Settings, models_loaded: bool, active_jobs: usize) -> ResourceUsage {
    let pid = get_current_pid().ok();
    let mut sys = system().lock().unwrap();
    sys.refresh_cpu_usage();
    sys.refresh_memory();
    if let Some(pid) = pid {
        sys.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
    }

    let proc = pid.and_then(|p| sys.process(p));
    let process_memory_mb = proc.map(|p| p.memory() / (1024 * 1024)).unwrap_or(0);
    let process_cpu_percent = proc.map(|p| p.cpu_usage()).unwrap_or(0.0);
    let system_used_memory_mb = sys.used_memory() / (1024 * 1024);
    let system_total_memory_mb = sys.total_memory() / (1024 * 1024);

    let (gpu_name, vram_used_mb, vram_total_mb) = match query_dxgi_vram() {
        Some((name, used, total)) => (Some(name), Some(used), Some(total)),
        None => (None, None, None),
    };

    ResourceUsage {
        cpu_percent: sys.global_cpu_usage(),
        process_cpu_percent,
        process_memory_mb,
        system_used_memory_mb,
        system_total_memory_mb,
        used_memory_mb: system_used_memory_mb,
        total_memory_mb: system_total_memory_mb,
        gpu_name,
        vram_used_mb,
        vram_total_mb,
        models_loaded,
        active_jobs,
        max_cpu_threads: settings.max_cpu_threads,
        max_memory_mb: settings.max_memory_mb,
        download: download_tracker::get_current_download(),
    }
}
