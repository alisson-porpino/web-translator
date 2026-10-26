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
/// A job not yet past its `local_engine_slots` wait sees `cancelled_jobs`
/// and bails out before doing any real work. One already mid-pipeline gets
/// a real interrupt too, via `running_stops`' `StopToken` (see
/// state.rs's docs on that field and engines/koharu_engine.rs) -- not
/// instant (Koharu checks it between pipeline stages, not mid-stage), but
/// a real interrupt now instead of always running to completion in the
/// background and holding `local_engine_slots`' one permit hostage until
/// it finishes on its own.
pub fn cancel_job(state: &AppState, job_id: &str) -> bool {
    // A job that already reached a terminal state (succeeded/failed/
    // cancelled -- e.g. it finished naturally in the gap between the
    // client deciding to cancel and this call landing) keeps that real
    // outcome. Without this check, cancelling a job that already
    // succeeded silently flipped its status backward to "cancelled",
    // hiding a result that's sitting right there in the cache -- found
    // 2026-10-25 testing the running_stops interrupt above against a job
    // that turned out to finish before the cancel request arrived.
    let already_terminal = state
        .jobs
        .lock()
        .unwrap()
        .get(job_id)
        .map(|s| matches!(s.status.as_str(), "succeeded" | "failed" | "cancelled"))
        .unwrap_or(true); // unwrap_or(true): unknown id, nothing to cancel either way
    if already_terminal {
        return false;
    }
    state.cancelled_jobs.lock().unwrap().insert(job_id.to_string());
    if let Some(stop) = state.running_stops.lock().unwrap().get(job_id) {
        stop.stop();
    }
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
    // engines::koharu_engine::ENGINE_VERSION) so switching engines or
    // upgrading the local models can never silently serve a stale result
    // from a previous engine/model generation. Pure image-hash keying
    // caused exactly that confusion once already: re-testing the same
    // page after swapping the local translation model kept returning the
    // old model's cached output.
    //
    // ENGINE_VERSION ("koharu-v1") replaced the old engine's
    // PIPELINE_VERSION ("v7-inpaint-coreml-fix") here on 2026-10-24, when
    // this app's local engine switched from its own onnxruntime +
    // llama-cpp-2 pipeline to Koharu's (see specs/DIRETRIZES.md's
    // "Migração em andamento" section) -- a different tag namespace
    // entirely, since these are two different engines, not successive
    // versions of the same one; any filename under the old tag is from
    // the retired engine and must never be matched again.
    // Also folds in detection_score_threshold (only affects the Google
    // engine's region detection -- the Koharu engine does its own
    // detection, unaffected by this setting) so changing it can't
    // silently serve a stale result from before the change either.
    let cache_filename = format!(
        "{}_{}_{}_{}_{:.2}.png",
        sha256_hex,
        req.target_lang,
        app_settings.engine_preference,
        engines::koharu_engine::ENGINE_VERSION,
        app_settings.detection_score_threshold,
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
                let stop = koharu_pipeline::StopToken::default();
                state.running_stops.lock().unwrap().insert(job_id.clone(), stop.clone());
                let result = engines::koharu_engine::translate(
                    state.app_handle.clone(),
                    image_bytes.clone(),
                    &req.target_lang,
                    stop,
                )
                .await;
                state.running_stops.lock().unwrap().remove(&job_id);
                result
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

    // A job stopped mid-pipeline (see running_stops above) surfaces here as
    // a plain Err from translate() like any other failure -- without this
    // check, the "failed" status set below would overwrite the "cancelled"
    // status cancel_job already set, and the extension would show the
    // wrong outcome for a job the user explicitly stopped.
    if is_cancelled(&state, &job_id) {
        return;
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
