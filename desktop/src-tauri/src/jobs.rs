use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::engines::{self, EngineError};
use crate::settings::{self, Settings};
use crate::state::{AppState, JobStatus};

const USER_AGENT: &str = "Mozilla/5.0 (compatible; manga-web-translator/0.1; self-hosted personal tool)";
const DEFAULT_MIME: &str = "image/png";

pub enum ImageSource {
    Url(String),
    /// Bytes the extension already read out of the page itself (blob:/data:
    /// sources, which only exist inside the page and can never be fetched
    /// by this server over the network).
    Bytes(Vec<u8>, Option<String>),
}

pub struct CreateJobRequest {
    pub source: ImageSource,
    pub page_url: Option<String>,
    pub target_lang: String,
}

pub fn create_job(state: AppState, req: CreateJobRequest) -> String {
    let job_id = Uuid::new_v4().simple().to_string();
    state
        .jobs
        .lock()
        .unwrap()
        .insert(job_id.clone(), JobStatus { status: "queued".into(), ..Default::default() });

    let job_id_clone = job_id.clone();
    tauri::async_runtime::spawn(async move {
        process_job(state, job_id_clone, req).await;
    });

    job_id
}

/// Marks a job cancelled. Returns false if the job id is unknown (already
/// finished and nobody's holding a reference anymore, or never existed).
///
/// A job not yet past its `local_engine_slots` wait sees this and bails
/// out before doing any real work. One already mid-inference can't be
/// safely interrupted -- it's a synchronous call inside
/// `spawn_blocking` -- so it keeps running in the background; its result
/// is simply never surfaced, since the extension has already stopped
/// polling for it by the time it finishes.
pub fn cancel_job(state: &AppState, job_id: &str) -> bool {
    if !state.jobs.lock().unwrap().contains_key(job_id) {
        return false;
    }
    state.cancelled_jobs.lock().unwrap().insert(job_id.to_string());
    set_status(state, job_id, JobStatus { status: "cancelled".into(), ..Default::default() });
    true
}

fn is_cancelled(state: &AppState, job_id: &str) -> bool {
    state.cancelled_jobs.lock().unwrap().contains(job_id)
}

async fn fetch_image(
    client: &reqwest::Client,
    image_url: &str,
    page_url: &Option<String>,
) -> Result<(Vec<u8>, String), EngineError> {
    let mut request = client.get(image_url).header("User-Agent", USER_AGENT);
    if let Some(page_url) = page_url {
        request = request.header("Referer", page_url.clone());
    }
    let resp = request
        .send()
        .await
        .map_err(|e| EngineError::new("IMAGE_FETCH_FAILED", e.to_string()))?;

    if !resp.status().is_success() {
        return Err(EngineError::new(
            "IMAGE_FETCH_FAILED",
            format!(
                "Could not fetch source image (HTTP {}). Some sites block server-side \
                 fetches of their images; the extension falls back to uploading bytes \
                 directly for blob:/data: sources, but not for ordinary protected URLs.",
                resp.status()
            ),
        ));
    }

    let mime_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(';').next().unwrap_or(s).trim().to_string())
        .filter(|s| s.starts_with("image/"))
        .unwrap_or_else(|| DEFAULT_MIME.to_string());

    let bytes = resp
        .bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(|e| EngineError::new("IMAGE_FETCH_FAILED", e.to_string()))?;

    Ok((bytes, mime_type))
}

fn engine_order(preference: &str) -> &'static [&'static str] {
    match preference {
        "local-only" => &["local"],
        "google-only" => &["google"],
        "google-then-local" => &["google", "local"],
        _ => &["local", "google"],
    }
}

async fn process_job(state: AppState, job_id: String, req: CreateJobRequest) {
    set_status(&state, &job_id, JobStatus { status: "processing".into(), ..Default::default() });

    let client = reqwest::Client::new();
    if is_cancelled(&state, &job_id) {
        return;
    }
    let (image_bytes, mime_type) = match req.source {
        ImageSource::Bytes(bytes, mime_type) => (bytes, mime_type.unwrap_or_else(|| DEFAULT_MIME.to_string())),
        ImageSource::Url(url) => match fetch_image(&client, &url, &req.page_url).await {
            Ok(result) => result,
            Err(err) => {
                set_status(
                    &state,
                    &job_id,
                    JobStatus {
                        status: "failed".into(),
                        error_code: Some(err.code),
                        error_message: Some(err.message),
                        ..Default::default()
                    },
                );
                return;
            }
        },
    };

    let sha256_hex = {
        let mut hasher = Sha256::new();
        hasher.update(&image_bytes);
        format!("{:x}", hasher.finalize())
    };
    let app_settings = settings::load(&state.app_handle);
    // Keyed on engine_preference + a version tag for the local pipeline
    // (bumped whenever its models/behavior change materially -- see
    // engines::local_pipeline::PIPELINE_VERSION) so switching engines or
    // upgrading the local models can never silently serve a stale result
    // from a previous engine/model generation. Pure image-hash keying
    // caused exactly that confusion once already: re-testing the same
    // page after swapping the local translation model kept returning the
    // old model's cached output.
    // Also folds in detection_score_threshold and the three per-stage
    // catalog selections (ocr/inpainting/translation_model). This matters
    // now that a stage can have more than one selectable option
    // (models.rs's catalog): PIPELINE_VERSION alone only changes when the
    // *code* changes, not when the user picks a different already-shipped
    // model for the same stage -- without these in the key, translating
    // the same image twice with two different selected translation models
    // would silently serve the first model's cached result for the
    // second, exactly the stale-cache bug this whole key format exists to
    // prevent. Unaffected by the engine being "google" for this job: the
    // fields are cheap to include and keep the key stable regardless of
    // which engine ends up used.
    let cache_filename = format!(
        "{}_{}_{}_{}_{:.2}_{}_{}_{}.png",
        sha256_hex,
        req.target_lang,
        app_settings.engine_preference,
        engines::local_pipeline::PIPELINE_VERSION,
        app_settings.detection_score_threshold,
        app_settings.ocr_model,
        app_settings.inpainting_model,
        app_settings.translation_model
    );
    let cache_path = state.cache_dir.join(&cache_filename);

    if cache_path.exists() {
        set_status(
            &state,
            &job_id,
            JobStatus {
                status: "succeeded".into(),
                translated_image_url: Some(cache_url(&app_settings, &cache_filename)),
                engine_used: Some("cache".into()),
                ..Default::default()
            },
        );
        return;
    }

    let mut last_error: Option<EngineError> = None;
    for engine_name in engine_order(&app_settings.engine_preference) {
        if is_cancelled(&state, &job_id) {
            return;
        }
        let result = match *engine_name {
            "local" => {
                // Back to "queued" (from "processing", set at the top of
                // this function) while waiting for a slot, so a client
                // polling this job can tell "still in line behind other
                // images" apart from "actually running".
                set_status(&state, &job_id, JobStatus { status: "queued".into(), ..Default::default() });
                let _permit =
                    state.local_engine_slots.clone().acquire_owned().await.expect("semaphore is never closed");
                // Re-check: may have been cancelled while waiting in line
                // for a slot -- no point starting a multi-minute local
                // translation for a job nobody's polling anymore.
                if is_cancelled(&state, &job_id) {
                    return;
                }
                set_status(&state, &job_id, JobStatus { status: "processing".into(), ..Default::default() });
                engines::local_pipeline::translate(state.app_handle.clone(), image_bytes.clone(), &req.target_lang).await
            }
            "google" => {
                engines::google::translate(
                    &client,
                    &app_settings,
                    image_bytes.clone(),
                    &mime_type,
                    &req.target_lang,
                    &state.google_usage,
                )
                .await
            }
            _ => continue,
        };

        match result {
            Ok(engine_result) => {
                if std::fs::write(&cache_path, &engine_result.image_bytes).is_ok() {
                    set_status(
                        &state,
                        &job_id,
                        JobStatus {
                            status: "succeeded".into(),
                            translated_image_url: Some(cache_url(&app_settings, &cache_filename)),
                            engine_used: Some(engine_result.engine_name.into()),
                            ..Default::default()
                        },
                    );
                    return;
                }
            }
            Err(err) => last_error = Some(err),
        }
    }

    let err = last_error.unwrap_or_else(|| EngineError::new("TRANSLATION_FAILED", "All configured engines failed."));
    set_status(
        &state,
        &job_id,
        JobStatus {
            status: "failed".into(),
            error_code: Some(err.code),
            error_message: Some(err.message),
            ..Default::default()
        },
    );
}

fn cache_url(settings: &Settings, filename: &str) -> String {
    format!("http://127.0.0.1:{}/cache/{}", settings.port, filename)
}

fn set_status(state: &AppState, job_id: &str, status: JobStatus) {
    state.jobs.lock().unwrap().insert(job_id.to_string(), status);
}
