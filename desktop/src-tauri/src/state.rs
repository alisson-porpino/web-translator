use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::AppHandle;
use tokio::sync::Semaphore;

use crate::usage::UsageTracker;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct JobStatus {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress_percent: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub translated_image_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engine_used: Option<String>,
}

pub type JobsMap = Arc<Mutex<HashMap<String, JobStatus>>>;

#[derive(Clone)]
pub struct AppState {
    pub app_handle: AppHandle,
    pub jobs: JobsMap,
    pub cache_dir: PathBuf,
    pub google_usage: Arc<UsageTracker>,
    /// How many jobs are actively queued or in-flight across all engines.
    /// Used by the idle timer to know when the server is fully quiet so it
    /// can unload heavy ML models from memory after a cooldown period.
    pub active_jobs: Arc<std::sync::atomic::AtomicUsize>,
    /// Cancellation channel for a pending idle-unload timer. If a new job
    /// arrives while the idle countdown is ticking, this aborts the unload.
    pub idle_timer_cancel: Arc<Mutex<Option<tokio::sync::oneshot::Sender<()>>>>,
    /// Every "translate this page" scan fires one job per detected image at
    /// once, with no limit -- a page with 20+ panels used to spawn 20+
    /// concurrent local-engine pipelines, all fighting over the same
    /// Mutex-guarded ONNX sessions (detect.rs/manga_ocr.rs/nllb.rs each
    /// hold one). That's pure thread-pool thrashing, not real parallelism:
    /// everything slows down together and jobs queued behind the pile-up
    /// blow past the extension's poll timeout. This caps how many local
    /// jobs actually run inference at once; the rest wait their turn.
    pub local_engine_slots: Arc<Semaphore>,
    /// Job IDs the user asked to stop (DELETE /api/jobs/:id). A job not
    /// yet past the local_engine_slots wait checks this and bails out
    /// instead of starting real work.
    pub cancelled_jobs: Arc<Mutex<HashSet<String>>>,
    /// Koharu's own cancellation handle (`koharu_pipeline::StopToken`) for
    /// each job currently running the "local" (Koharu) engine, keyed by
    /// job id.
    pub running_stops: Arc<Mutex<HashMap<String, koharu_pipeline::StopToken>>>,
    pub server: Arc<Mutex<Option<ServerHandle>>>,
}

pub struct ServerHandle {
    pub port: u16,
    pub shutdown: tokio::sync::oneshot::Sender<()>,
}
