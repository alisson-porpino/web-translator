//! User-configurable caps on how much CPU and RAM the local engine is
//! allowed to use, so a page with many panels can't lock up the rest of
//! the computer while it runs. Two different mechanisms, because they
//! have very different failure modes:
//!
//! - **CPU**: a thread-count cap handed to every inference backend
//!   (llama.cpp, ONNX Runtime) instead of letting each one default to
//!   "every core". Safe and cooperative -- the backend just does less
//!   parallel work, nothing can go wrong from capping it.
//! - **Memory**: an OS-level hard cap -- `setrlimit(RLIMIT_AS, ..)` on
//!   macOS/Linux, a Win32 Job Object's `ProcessMemoryLimit` on Windows
//!   (see `apply_memory_limit` below for each). An allocation that would
//!   cross it fails outright, which typically aborts whatever just tried
//!   to allocate rather than degrading gracefully. That's intentional
//!   here: the goal is "this one job fails" instead of "the whole machine
//!   starts swapping and becomes unusable", which is what unbounded
//!   growth from one of the larger local models can otherwise do. 0 means
//!   no cap -- never lowers the OS default unless the user explicitly
//!   asks for one.
//!
//!   The Windows path (`apply_memory_limit`'s `#[cfg(windows)]` arm) was
//!   written and reasoned through carefully -- exact API signatures and
//!   required Cargo features verified against the actual published
//!   `windows` crate source (not guessed from memory, not docs.rs, whose
//!   rendered pages for this module 404 without the right feature
//!   selection) -- but this project is developed on macOS and has **never
//!   been compiled or run on real Windows**. Treat it as reasoned-through,
//!   not proven, until someone verifies it on an actual Windows box.

use std::sync::atomic::{AtomicU32, Ordering};

static MAX_CPU_THREADS: AtomicU32 = AtomicU32::new(0);

/// Applies both caps. Called once at startup with the saved settings, and
/// again every time settings are saved, so a change takes effect
/// immediately -- no restart needed.
pub fn set_limits(max_cpu_threads: u32, max_memory_mb: u32) {
    MAX_CPU_THREADS.store(max_cpu_threads, Ordering::Relaxed);
    apply_memory_limit(max_memory_mb);
}

/// How many threads an inference backend should ask for right now: the
/// configured cap if one is set (and only if it's not larger than what's
/// actually available), otherwise every logical core.
pub fn effective_thread_count() -> i32 {
    let total = std::thread::available_parallelism().map(|n| n.get() as u32).unwrap_or(4);
    let cap = MAX_CPU_THREADS.load(Ordering::Relaxed);
    let n = if cap > 0 { cap.min(total) } else { total };
    n.max(1) as i32
}

#[cfg(unix)]
fn apply_memory_limit(max_mb: u32) {
    if max_mb == 0 {
        // Explicitly not touching the OS's existing limit here: calling
        // setrlimit with "unlimited" would override a system-imposed
        // ceiling we have no business changing, not actually raise
        // anything useful.
        return;
    }
    let bytes = max_mb as u64 * 1024 * 1024;
    let limit = libc::rlimit { rlim_cur: bytes, rlim_max: bytes };
    let result = unsafe { libc::setrlimit(libc::RLIMIT_AS, &limit) };
    if result != 0 {
        eprintln!(
            "[resource_limits] could not set the {max_mb}MB memory limit: {}",
            std::io::Error::last_os_error()
        );
    }
}

/// A Job Object's `ProcessMemoryLimit`, same role as RLIMIT_AS on
/// Unix -- create an unnamed job, cap its process memory, assign this
/// process to it. The job handle is intentionally never closed: it must
/// keep the limit in force for the rest of this process's lifetime, and
/// Windows itself frees the job once the process holding its only handle
/// exits -- the standard "leak it on purpose" pattern every Win32 Job
/// Object example uses, there is nothing to release here.
#[cfg(windows)]
fn apply_memory_limit(max_mb: u32) {
    use windows::core::PCWSTR;
    use windows::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_PROCESS_MEMORY,
    };
    use windows::Win32::System::Threading::GetCurrentProcess;

    if max_mb == 0 {
        return;
    }
    let bytes = max_mb as usize * 1024 * 1024;

    // SAFETY: standard Win32 Job Object setup, as documented above --
    // create an unnamed job object, fill in an all-zeroed limit-info
    // struct with just the process memory cap set, apply it, then put
    // this process in the job. Every call here is a direct, unmodified
    // Win32 API invocation with no raw pointer arithmetic of our own
    // beyond taking `&info`'s address to hand to SetInformationJobObject,
    // which is the documented calling convention for that function.
    unsafe {
        let job = match CreateJobObjectW(None, PCWSTR::null()) {
            Ok(h) => h,
            Err(e) => {
                eprintln!("[resource_limits] could not create a Job Object for the {max_mb}MB memory limit: {e}");
                return;
            }
        };

        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_PROCESS_MEMORY;
        info.ProcessMemoryLimit = bytes;

        if let Err(e) = SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            std::ptr::addr_of!(info).cast(),
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        ) {
            eprintln!("[resource_limits] could not set the {max_mb}MB memory limit on the Job Object: {e}");
            return;
        }

        if let Err(e) = AssignProcessToJobObject(job, GetCurrentProcess()) {
            eprintln!(
                "[resource_limits] could not assign this process to the Job Object for the {max_mb}MB memory limit: {e}"
            );
        }
    }
}

#[cfg(not(any(unix, windows)))]
fn apply_memory_limit(max_mb: u32) {
    if max_mb > 0 {
        eprintln!(
            "[resource_limits] a {max_mb}MB memory limit is configured, but this platform doesn't support \
             enforcing one -- it has no effect here."
        );
    }
}

/// llama.cpp clamps any `n_gpu_layers` above a model's real layer count
/// down to that count automatically -- this sentinel is how `LocalLlm`/
/// `PaddleOcrVl` have always requested "every layer", before VRAM
/// budgets existed.
const OFFLOAD_ALL_LAYERS: u32 = 1000;

/// How many of a GGUF model's layers to offload to the GPU, given the
/// user's configured VRAM budget (`Settings.max_vram_mb`) -- 0 means no
/// budget set, which keeps the original "offload everything" behavior.
///
/// Unlike CPU/RAM, there is no OS-level "cap VRAM usage" API this can
/// call into, so this is an *estimate*, not an enforced limit: read the
/// model's real layer count straight from its own GGUF header (see
/// engines/gguf_meta.rs -- no need to load the whole model just to ask
/// it, which `LlamaModel::n_layer()` would otherwise require), and
/// assume `budget_bytes / file_size_bytes` of the file's layers fit in
/// that budget. That's a simple linear approximation (every layer
/// treated as roughly the same size) rather than llama.cpp's own far
/// more precise `common_fit_params` auto-fit, which reads actual
/// per-tensor sizes and KV-cache requirements -- picked for how much
/// simpler it is to reason about and wire up correctly, at the cost of
/// precision; good enough for a user-facing budget slider, not meant to
/// be exact down to the megabyte.
///
/// Falls back to "offload everything" on any read failure (missing
/// file, unparseable header, zero-byte file) -- a VRAM budget should
/// never be the reason a model fails to load.
pub fn gpu_layers_for_budget(model_path: &std::path::Path, max_vram_mb: u32) -> u32 {
    if max_vram_mb == 0 {
        return OFFLOAD_ALL_LAYERS;
    }
    let Ok(metadata) = std::fs::metadata(model_path) else { return OFFLOAD_ALL_LAYERS };
    let file_size = metadata.len();
    if file_size == 0 {
        return OFFLOAD_ALL_LAYERS;
    }
    let Some(total_layers) = crate::engines::gguf_meta::read_block_count(model_path) else {
        return OFFLOAD_ALL_LAYERS;
    };
    if total_layers == 0 {
        return OFFLOAD_ALL_LAYERS;
    }

    let budget_bytes = max_vram_mb as f64 * 1024.0 * 1024.0;
    let fraction = (budget_bytes / file_size as f64).clamp(0.0, 1.0);
    ((fraction * total_layers as f64).floor() as u64).min(total_layers) as u32
}
