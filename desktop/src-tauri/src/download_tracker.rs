use serde::Serialize;
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DownloadStatus {
    pub active: bool,
    pub name: String,
    pub completed_bytes: u64,
    pub total_bytes: u64,
    pub percent: f32,
}

static CURRENT_DOWNLOAD: OnceLock<Arc<Mutex<Option<DownloadStatus>>>> = OnceLock::new();

fn current_download() -> &'static Arc<Mutex<Option<DownloadStatus>>> {
    CURRENT_DOWNLOAD.get_or_init(|| Arc::new(Mutex::new(None)))
}

pub fn get_current_download() -> Option<DownloadStatus> {
    current_download().lock().unwrap().clone()
}

pub fn start_download_listener() {
    tauri::async_runtime::spawn(async move {
        let mut rx = koharu_runtime::download::subscribe();
        while let Ok(event) = rx.recv().await {
            let mut state = current_download().lock().unwrap();
            match event {
                koharu_runtime::download::Event::Started { name, .. } => {
                    tracing::info!(%name, "download_tracker: download started");
                    *state = Some(DownloadStatus {
                        active: true,
                        name,
                        completed_bytes: 0,
                        total_bytes: 0,
                        percent: 0.0,
                    });
                }
                koharu_runtime::download::Event::Progress { name, completed, total, .. } => {
                    let percent = if total > 0 {
                        (completed as f32 / total as f32) * 100.0
                    } else {
                        0.0
                    };
                    *state = Some(DownloadStatus {
                        active: true,
                        name,
                        completed_bytes: completed,
                        total_bytes: total,
                        percent,
                    });
                }
                koharu_runtime::download::Event::Finished { .. } => {
                    tracing::info!("download_tracker: download finished");
                    *state = None;
                }
                koharu_runtime::download::Event::Failed { error, .. } => {
                    tracing::warn!(%error, "download_tracker: download failed");
                    *state = None;
                }
            }
        }
    });
}
