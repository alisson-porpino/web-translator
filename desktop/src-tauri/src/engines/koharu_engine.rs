//! Translation via Koharu's own pipeline (fork:
//! github.com/alisson-porpino/koharu), replacing this crate's own
//! detect/OCR/inpaint/translate engines (onnxruntime + llama-cpp-2) -- see
//! specs/DIRETRIZES.md's "Migração em andamento" section for why.

use std::sync::Arc;

use async_trait::async_trait;
use koharu_pipeline::{Committer, Pipeline, PipelineConfig, Request, RunStatus, StageOutput, StopToken};
use koharu_rasterizer::{RasterOptions, Rasterizer};
use koharu_renderer::Renderer;
use koharu_scene::{AssetInput, AssetMetadata, AssetRole, At, PageDraft, Session, Snapshot};
use tauri::AppHandle;
use tokio::sync::Mutex as TokioMutex;

use super::{EngineError, EngineResult};

/// Cache-key tag for this engine -- see jobs.rs's cache_filename
/// construction (DIRETRIZES.md Principio 4: "cache correto é
/// inegociável").
pub const ENGINE_VERSION: &str = "koharu-v1";

/// Pipeline singleton wrapped in a Tokio Mutex so it can be dynamically
/// unloaded on idle timeout or user request to reclaim RAM/VRAM.
static PIPELINE: TokioMutex<Option<Pipeline>> = TokioMutex::const_new(None);

/// Returns true if the ML pipeline models are currently loaded in RAM/VRAM.
pub async fn is_loaded() -> bool {
    let guard = PIPELINE.lock().await;
    guard.as_ref().map(|p| p.is_loaded()).unwrap_or(false)
}

/// Explicitly unloads all stage models (detection, OCR, Gemma LLM, inpainting)
/// from GPU VRAM and System RAM.
pub async fn unload() -> bool {
    let guard = PIPELINE.lock().await;
    if let Some(pipe) = guard.as_ref() {
        let unloaded = pipe.unload();
        tracing::info!("koharu_engine: pipeline stage models unloaded from memory (unloaded={unloaded})");
        #[cfg(target_os = "linux")]
        unsafe {
            libc::malloc_trim(0);
        }
        unloaded
    } else {
        false
    }
}

async fn pipeline(models_dir: Option<&str>) -> Result<Pipeline, String> {
    let mut guard = PIPELINE.lock().await;
    if let Some(pipe) = guard.as_ref() {
        return Ok(pipe.clone());
    }

    if let Some(dir) = models_dir {
        if let Err(e) = koharu_runtime::Store::configure(std::path::PathBuf::from(dir)) {
            return Err(format!("failed to configure Koharu's model storage directory '{dir}': {e:#}"));
        }
    }

    koharu_ml::init().await.map_err(|e| format!("failed to initialize Koharu's ML runtime: {e:#}"))?;
    let device = koharu_ml::device(false);
    tracing::info!(?device, "koharu_engine: device selected");
    let pipe = Pipeline::load(device).map_err(|e| format!("failed to load Koharu's pipeline: {e:#}"))?;
    *guard = Some(pipe.clone());
    Ok(pipe)
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

pub type ProgressCallback = Arc<dyn Fn(String, Option<f32>) + Send + Sync>;

pub async fn translate(
    app: AppHandle,
    image_bytes: Vec<u8>,
    target_lang: &str,
    stop: StopToken,
    on_progress: Option<ProgressCallback>,
) -> Result<EngineResult, EngineError> {
    if let Some(ref cb) = on_progress {
        cb("Inicializando pipeline de modelos...".to_string(), None);
    }

    let models_dir = crate::settings::load(&app).models_dir;
    let pipeline = pipeline(models_dir.as_deref()).await.map_err(|e| EngineError::new("ENGINE_UNAVAILABLE", e))?;

    let target_language: koharu_translator::Language = target_lang
        .parse()
        .map_err(|_| EngineError::new("TRANSLATION_FAILED", format!("unsupported target language '{target_lang}'")))?;

    let mut config = PipelineConfig::load()
        .map_err(|e| EngineError::new("ENGINE_UNAVAILABLE", e.to_string()))?
        .read()
        .map_err(|e| EngineError::new("ENGINE_UNAVAILABLE", e.to_string()))?
        .clone();
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

    let progress_callback = on_progress.clone();
    let progress_sink: Option<koharu_pipeline::ProgressSink> = progress_callback.map(|cb| {
        Arc::new(move |progress: koharu_pipeline::Progress| match progress {
            koharu_pipeline::Progress::Started { stages, .. } => {
                cb(format!("Iniciando processamento ({} estágios)...", stages.len()), None);
            }
            koharu_pipeline::Progress::Loading { stage, model, .. } => {
                let stage_name = match stage {
                    koharu_pipeline::Stage::Detection => "Detecção",
                    koharu_pipeline::Stage::Ocr => "OCR",
                    koharu_pipeline::Stage::Translation => "Tradução",
                    koharu_pipeline::Stage::Inpainting => "Inpainting",
                };
                cb(format!("Carregando {stage_name} ({model})..."), None);
            }
            koharu_pipeline::Progress::Running { stage, model, .. } => {
                let stage_name = match stage {
                    koharu_pipeline::Stage::Detection => "Detectando caixas e balões",
                    koharu_pipeline::Stage::Ocr => "Lendo texto original (OCR)",
                    koharu_pipeline::Stage::Translation => "Traduzindo texto",
                    koharu_pipeline::Stage::Inpainting => "Removendo texto (Inpainting)",
                };
                cb(format!("{stage_name} ({model})..."), None);
            }
            koharu_pipeline::Progress::Finished { stage, elapsed, .. } => {
                let stage_name = match stage {
                    koharu_pipeline::Stage::Detection => "Detecção",
                    koharu_pipeline::Stage::Ocr => "OCR",
                    koharu_pipeline::Stage::Translation => "Tradução",
                    koharu_pipeline::Stage::Inpainting => "Inpainting",
                };
                cb(format!("{stage_name} finalizado em {:.1}s", elapsed.as_secs_f32()), None);
            }
            koharu_pipeline::Progress::Skipped { stage, .. } => {
                cb(format!("Estágio {stage:?} ignorado"), None);
            }
        }) as koharu_pipeline::ProgressSink
    });

    let mut committer = JobCommitter { session };
    let request = Request { stop, progress: progress_sink, ..Request::default() };
    let report = pipeline
        .execute(committer.session.snapshot(), request, &mut committer)
        .await
        .map_err(|e| EngineError::new("TRANSLATION_FAILED", format!("{e:?}")))?;

    if report.status == RunStatus::Stopped {
        return Err(EngineError::new("TRANSLATION_FAILED", "pipeline execution was stopped"));
    }

    if let Some(ref cb) = on_progress {
        cb("Renderizando imagem traduzida...".to_string(), None);
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
