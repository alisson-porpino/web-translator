use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

use crate::jobs::{self, CreateJobRequest, ImageSource};
use crate::state::AppState;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateJobBody {
    #[serde(default)]
    image_url: Option<String>,
    /// Set instead of image_url for blob:/data: sources the extension had
    /// to read out of the page itself -- see entrypoints/content/index.ts.
    #[serde(default)]
    image_base64: Option<String>,
    #[serde(default)]
    mime_type: Option<String>,
    #[serde(default)]
    page_url: Option<String>,
    #[serde(default = "default_lang")]
    target_lang: String,
}

fn default_lang() -> String {
    "en".to_string()
}

#[derive(Serialize)]
struct CreateJobResponse {
    #[serde(rename = "jobId")]
    job_id: String,
}

async fn create_job(State(state): State<AppState>, Json(body): Json<CreateJobBody>) -> impl IntoResponse {
    let source = if let Some(b64) = body.image_base64 {
        match base64::engine::general_purpose::STANDARD.decode(b64) {
            Ok(bytes) => ImageSource::Bytes(bytes, body.mime_type),
            Err(e) => return (StatusCode::BAD_REQUEST, format!("invalid imageBase64: {e}")).into_response(),
        }
    } else if let Some(url) = body.image_url {
        ImageSource::Url(url)
    } else {
        return (StatusCode::BAD_REQUEST, "imageUrl or imageBase64 is required").into_response();
    };

    let job_id = jobs::create_job(
        state,
        CreateJobRequest {
            source,
            page_url: body.page_url,
            target_lang: body.target_lang,
        },
    );
    Json(CreateJobResponse { job_id }).into_response()
}

async fn get_job(State(state): State<AppState>, Path(job_id): Path<String>) -> impl IntoResponse {
    let job = state.jobs.lock().unwrap().get(&job_id).cloned();
    match job {
        Some(mut status) => {
            // If the job is currently processing and a model download is active,
            // report the live download progress so the client can show a progress bar.
            if status.status == "processing" {
                if let Some(dl) = crate::download_tracker::get_current_download() {
                    status.stage = Some("downloading".into());
                    status.progress_percent = Some(dl.percent);
                    let mb_done = dl.completed_bytes / (1024 * 1024);
                    let mb_total = dl.total_bytes / (1024 * 1024);
                    let is_pt = crate::settings::load(&state.app_handle).ui_language.starts_with("pt");
                    status.progress_message = Some(if is_pt {
                        format!(
                            "Baixando modelo {}: {:.1}% ({}/{} MB)... Isso ocorre apenas no primeiro uso.",
                            dl.name, dl.percent, mb_done, mb_total
                        )
                    } else {
                        format!(
                            "Downloading model {}: {:.1}% ({}/{} MB)... This only happens on first use.",
                            dl.name, dl.percent, mb_done, mb_total
                        )
                    });
                }
            }
            Json(status).into_response()
        }
        None => (StatusCode::NOT_FOUND, "Job not found").into_response(),
    }
}

async fn cancel_job(State(state): State<AppState>, Path(job_id): Path<String>) -> impl IntoResponse {
    if jobs::cancel_job(&state, &job_id) {
        StatusCode::NO_CONTENT.into_response()
    } else {
        (StatusCode::NOT_FOUND, "Job not found").into_response()
    }
}

async fn healthz() -> impl IntoResponse {
    Json(serde_json::json!({ "ok": true }))
}

/// Lets the browser extension's popup and settings window fetch the live
/// CPU/RAM/VRAM reading and model status.
async fn get_resource_usage(State(state): State<AppState>) -> impl IntoResponse {
    let settings = crate::settings::load(&state.app_handle);
    let models_loaded = crate::engines::koharu_engine::is_loaded().await;
    let active_jobs = state.active_jobs.load(std::sync::atomic::Ordering::Relaxed);
    Json(crate::resource_usage::snapshot(&settings, models_loaded, active_jobs))
}

/// Explicit endpoint to unload ML models from memory on demand.
async fn unload_pipeline(State(state): State<AppState>) -> impl IntoResponse {
    jobs::cancel_all_jobs(&state);
    let unloaded = crate::engines::koharu_engine::unload().await;
    Json(serde_json::json!({ "ok": true, "unloaded": unloaded })).into_response()
}

/// Binds the port up front so a conflict (another app already using it)
/// surfaces to the caller immediately, instead of only after `serve`
/// below has been running in the background for a while -- `start_server`
/// (main.rs) awaits this before telling the UI the server actually
/// started.
pub async fn bind(port: u16) -> std::io::Result<tokio::net::TcpListener> {
    tokio::net::TcpListener::bind(format!("127.0.0.1:{port}")).await
}

/// Runs until `shutdown` fires (see `stop_server` in main.rs) or the
/// listener errors out.
pub async fn serve(state: AppState, listener: tokio::net::TcpListener, shutdown: tokio::sync::oneshot::Receiver<()>) {
    let app = Router::new()
        .route("/api/jobs", post(create_job))
        .route("/api/jobs/{job_id}", get(get_job).delete(cancel_job))
        .route("/healthz", get(healthz))
        .route("/api/resources", get(get_resource_usage))
        .route("/api/pipeline/unload", post(unload_pipeline))
        .route("/api/resources/unload", post(unload_pipeline))
        .nest_service("/cache", ServeDir::new(state.cache_dir.clone()))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let _ = axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = shutdown.await;
        })
        .await;
}
