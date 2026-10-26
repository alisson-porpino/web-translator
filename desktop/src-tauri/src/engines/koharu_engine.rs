//! Translation via Koharu's own pipeline (fork:
//! github.com/alisson-porpino/koharu), replacing this crate's own
//! detect/OCR/inpaint/translate engines (onnxruntime + llama-cpp-2) -- see
//! specs/DIRETRIZES.md's "Migração em andamento" section for why.
//!
//! Same contract as the engine it replaces (`local_pipeline::translate`):
//! `jobs.rs` calls this the same way, so nothing above this module changed
//! -- HTTP routes, job queue, cache keying, the browser extension are all
//! untouched. Built from `koharu-headless-spike` (a throwaway crate in the
//! fork itself, see its own doc comment), which validated this exact
//! approach -- an ephemeral in-memory project with one page, no desktop
//! GUI/CEF window, no Chrome Native Messaging -- against a real benchmark
//! page: 67.5s total (56s pipeline) vs. this app's previous ~7 minutes for
//! the same page, with no inpainting white-boxes and far better text
//! layout. That spike's `run_pipeline`-equivalent logic was itself ported
//! from the Koharu project's own (102-commits-stale, never-merged)
//! `web-extension` branch's `koharu-native-messaging` crate, which had
//! already solved the "ephemeral single-page project" construction this
//! would otherwise have needed real trial-and-error to figure out.

use std::sync::{Arc, OnceLock};

use async_trait::async_trait;
use koharu_pipeline::{Committer, Pipeline, PipelineConfig, Request, RunStatus, StageOutput, StopToken};
use koharu_rasterizer::{RasterOptions, Rasterizer};
use koharu_renderer::Renderer;
use koharu_scene::{AssetInput, AssetMetadata, AssetRole, At, PageDraft, Session, Snapshot};
use tauri::AppHandle;

use super::{EngineError, EngineResult};

/// Cache-key tag for this engine -- see jobs.rs's cache_filename
/// construction (DIRETRIZES.md Principio 4: "cache correto é
/// inegociável"). Bump this whenever this module's behavior changes
/// materially (same rule `local_pipeline::PIPELINE_VERSION` followed),
/// same reasoning as that constant: a stale cached PNG from a different
/// engine/behavior generation must never be served as if it came from
/// this one. jobs.rs switched to this constant (from
/// `local_pipeline::PIPELINE_VERSION`) the moment this module replaced
/// `local_pipeline` as the "local" engine -- the two could never share a
/// version namespace anyway, since they're different engines entirely,
/// not successive versions of the same one.
pub const ENGINE_VERSION: &str = "koharu-v1";

/// Loaded once per process -- rebuilding it would mean re-reading every
/// model from disk again (the exact per-call reload cost this migration
/// exists to avoid; see the module doc's timing numbers). `OnceLock`
/// instead of a plain `static Pipeline` because `Pipeline::load` is
/// fallible and needs an async `koharu_ml::init()` first.
static PIPELINE: OnceLock<Result<Pipeline, String>> = OnceLock::new();

async fn pipeline(models_dir: Option<&str>) -> Result<Pipeline, String> {
    if let Some(result) = PIPELINE.get() {
        return result.clone();
    }
    // Store::configure is itself OnceLock-protected (see koharu-runtime's
    // store.rs) -- must run before koharu_ml::init() touches it for the
    // first time in this process, same "takes effect only on next launch"
    // rule as everything else gated behind this module's own PIPELINE
    // OnceLock (e.g. the port setting). A relative path is rejected by
    // Store::configure itself; settings.rs's UI is expected to always save
    // an absolute path from the native folder picker.
    if let Some(dir) = models_dir {
        if let Err(e) = koharu_runtime::Store::configure(std::path::PathBuf::from(dir)) {
            return Err(format!("failed to configure Koharu's model storage directory '{dir}': {e:#}"));
        }
    }
    let result = async {
        koharu_ml::init().await.map_err(|e| format!("failed to initialize Koharu's ML runtime: {e:#}"))?;
        let device = koharu_ml::device(false);
        tracing::info!(?device, "koharu_engine: device selected");
        Pipeline::load(device).map_err(|e| format!("failed to load Koharu's pipeline: {e:#}"))
    }
    .await;
    // Another task may have raced us here; OnceLock keeps whichever
    // finished first and this clone just reflects that -- fine either way
    // since both would have built the same thing.
    PIPELINE.get_or_init(|| result).clone()
}

struct JobCommitter {
    session: Session,
}

#[async_trait]
impl Committer for JobCommitter {
    async fn commit(&mut self, output: StageOutput) -> anyhow::Result<Snapshot> {
        self.session.commit(output.patch).await?;
        Ok(self.session.snapshot())
    }
}

/// `stop`: jobs.rs creates this, keeps a clone to call `.stop()` on from the
/// `DELETE /api/jobs/:id` handler, and passes another clone in here --
/// lets an in-flight job actually be interrupted instead of always running
/// to completion in the background (koharu-pipeline checks it at each
/// stage boundary -- detection/OCR/translation/inpainting -- not mid-stage,
/// so Stop takes effect at the end of whichever stage is currently
/// running, not instantly, but without this it never took effect at all).
pub async fn translate(
    app: AppHandle,
    image_bytes: Vec<u8>,
    target_lang: &str,
    stop: StopToken,
) -> Result<EngineResult, EngineError> {
    let models_dir = crate::settings::load(&app).models_dir;
    let pipeline = pipeline(models_dir.as_deref()).await.map_err(|e| EngineError::new("ENGINE_UNAVAILABLE", e))?;

    let target_language: koharu_translator::Language = target_lang
        .parse()
        .map_err(|_| EngineError::new("TRANSLATION_FAILED", format!("unsupported target language '{target_lang}'")))?;

    // Per-job config, not the shared live-reloaded one Pipeline::from_config
    // watches: only `target_language` differs from the user's own Koharu
    // settings (same detection/OCR/inpainting/translation model choices
    // they already configured in the real Koharu app), and mutating a
    // *shared* config would trigger that watcher's StageRunner rebuild on
    // every single job -- reintroducing the reload-per-call cost this
    // migration exists to eliminate. `Pipeline::load`'s already-loaded
    // runner is reused as-is; only this one request's config copy changes.
    let mut config = PipelineConfig::load().map_err(|e| EngineError::new("ENGINE_UNAVAILABLE", e.to_string()))?.read().map_err(|e| EngineError::new("ENGINE_UNAVAILABLE", e.to_string()))?.clone();
    config.translation.target_language = target_language;

    let (width, height) = {
        let reader = image::ImageReader::new(std::io::Cursor::new(&image_bytes))
            .with_guessed_format()
            .map_err(|e| EngineError::new("TRANSLATION_FAILED", format!("could not read image: {e}")))?;
        let (w, h) = reader
            .into_dimensions()
            .map_err(|e| EngineError::new("TRANSLATION_FAILED", format!("could not read image dimensions: {e}")))?;
        (f64::from(w), f64::from(h))
    };

    let mut session = Session::memory().await.map_err(|e| EngineError::new("ENGINE_UNAVAILABLE", e.to_string()))?;
    let mut edit = session.snapshot().edit();
    let page = edit
        .add_page(PageDraft::new("web-capture", width, height), At::End)
        .map_err(|e| EngineError::new("TRANSLATION_FAILED", e.to_string()))?;
    edit.set_asset(
        page,
        &AssetRole::new("source").map_err(|e| EngineError::new("TRANSLATION_FAILED", e.to_string()))?,
        AssetInput::new(
            Arc::<[u8]>::from(image_bytes),
            "image/png",
            AssetMetadata { width: Some(width as u32), height: Some(height as u32), attributes: Default::default() },
        ),
    )
    .map_err(|e| EngineError::new("TRANSLATION_FAILED", e.to_string()))?;
    let patch = edit.finish().map_err(|e| EngineError::new("TRANSLATION_FAILED", e.to_string()))?;
    session.commit(patch).await.map_err(|e| EngineError::new("TRANSLATION_FAILED", e.to_string()))?;

    let mut committer = JobCommitter { session };
    let request = Request { stop, ..Request::default() }; // Operation::Full
    let report = pipeline
        .execute(committer.session.snapshot(), request, &mut committer)
        .await
        .map_err(|e| EngineError::new("TRANSLATION_FAILED", format!("{e:?}")))?;

    if report.status == RunStatus::Stopped {
        return Err(EngineError::new("TRANSLATION_FAILED", "pipeline execution was stopped"));
    }

    let final_snapshot = committer.session.snapshot();
    let renderer = Renderer::new().map_err(|e| EngineError::new("TRANSLATION_FAILED", e.to_string()))?;
    let rasterizer = Rasterizer::new().map_err(|e| EngineError::new("TRANSLATION_FAILED", e.to_string()))?;
    let frame = renderer
        .render(&final_snapshot, page)
        .await
        .map_err(|e| EngineError::new("TRANSLATION_FAILED", e.to_string()))?;
    let raster = rasterizer
        .rasterize(
            &frame.raster_frame().map_err(|e| EngineError::new("TRANSLATION_FAILED", e.to_string()))?,
            RasterOptions::default(),
        )
        .map_err(|e| EngineError::new("TRANSLATION_FAILED", e.to_string()))?;

    let mut image_bytes = Vec::new();
    raster
        .image
        .write_to(&mut std::io::Cursor::new(&mut image_bytes), image::ImageFormat::Png)
        .map_err(|e| EngineError::new("TRANSLATION_FAILED", format!("could not encode result image: {e}")))?;

    Ok(EngineResult { image_bytes, engine_name: "koharu" })
}
