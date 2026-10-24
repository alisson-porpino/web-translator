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
        Some(status) => Json(status).into_response(),
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

/// Lets the browser extension's popup show the same live CPU/RAM reading
/// (and configured caps/VRAM budget) as the desktop settings window --
/// see resource_usage.rs.
async fn get_resource_usage(State(state): State<AppState>) -> impl IntoResponse {
    let settings = crate::settings::load(&state.app_handle);
    Json(crate::resource_usage::snapshot(&settings))
}

pub async fn run(state: AppState, port: u16) {
    let app = Router::new()
        .route("/api/jobs", post(create_job))
        .route("/api/jobs/{job_id}", get(get_job).delete(cancel_job))
        .route("/healthz", get(healthz))
        .route("/api/resources", get(get_resource_usage))
        .nest_service("/cache", ServeDir::new(state.cache_dir.clone()))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = format!("127.0.0.1:{port}");
    match tokio::net::TcpListener::bind(&addr).await {
        Ok(listener) => {
            let _ = axum::serve(listener, app).await;
        }
        Err(e) => {
            eprintln!("Failed to bind local server on {addr}: {e}");
        }
    }
}
