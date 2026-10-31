//! Exposes Koharu's own OCR/inpainting model selection to this app's
//! settings UI -- these live in Koharu's own shared config file
//! (`~/.koharu/config.toml`, see koharu_config::path()), the same file
//! its real desktop app reads/writes, not in our own settings.json.
//! `PipelineConfig::load()` returns a live, process-wide handle the
//! pipeline itself watches for changes (see koharu_engine.rs's comment on
//! "the shared live-reloaded config"), so saving here takes effect on the
//! *next* translation job -- no app restart needed, unlike this app's own
//! settings.json fields that say "restart required".
//!
//! Detection and translation-model selection aren't exposed here: Koharu
//! only has one detection model today (nothing to choose), and the
//! translation model carries a much bigger surface (provider, quantized
//! local weights, cloud API keys) that's out of scope for this pass --
//! this covers exactly what used to exist in this app's own (now-deleted)
//! per-stage model catalog, e.g. LaMa for inpainting.
//!
//! The fixed set of valid OCR/inpainting model ids is hardcoded as
//! <option> tags in desktop/ui/index.html, mirroring koharu-pipeline's
//! config.rs -- keep both in sync if the pinned Koharu rev ever
//! adds/renames a model.
//!
//! Translation-model selection is handled separately below
//! (list_translation_models/get_translation_selection/
//! set_translation_model): unlike OCR/inpainting, Koharu's local
//! translation catalog is large (~27 models, each with several
//! quantizations) and changes with the pinned rev, so it's listed
//! dynamically via koharu_translator::Translator::models() rather than
//! hardcoded, and only the `Provider::Local` entries are exposed here --
//! Koharu also supports many cloud providers (OpenRouter, Claude, Grok,
//! DeepSeek, etc.) but wiring API keys for each is out of scope; this
//! app's own "Engine" setting + Google API key already cover the
//! local-vs-cloud choice this app cares about.

use koharu_pipeline::{InpaintingModel, OcrModel, PipelineConfig};
use koharu_translator::{Model, Provider, Translator};
use serde::Serialize;

#[derive(Serialize)]
pub struct PipelineModels {
    pub ocr: String,
    pub inpainting: String,
}

fn ocr_id(model: &OcrModel) -> &'static str {
    match model {
        OcrModel::PaddleOcrVl1_6 => "paddleocr-vl-1.6",
        OcrModel::MangaOcr => "manga-ocr",
        OcrModel::BaberuOcr => "baberu-ocr",
        OcrModel::HayaiOcr => "hayai-ocr",
    }
}

fn inpainting_id(model: &InpaintingModel) -> &'static str {
    match model {
        InpaintingModel::LaMa {} => "lama",
        InpaintingModel::AotInpainting {} => "aot-inpainting",
        InpaintingModel::Flux2Klein(_) => "flux2-klein",
        InpaintingModel::RoremMixed(_) => "rorem-mixed",
    }
}

pub fn get() -> Result<PipelineModels, String> {
    let config = PipelineConfig::load().map_err(|e| e.to_string())?;
    let value = config.read().map_err(|e| e.to_string())?;
    Ok(PipelineModels { ocr: ocr_id(&value.ocr).to_owned(), inpainting: inpainting_id(&value.inpainting).to_owned() })
}

pub fn set(ocr_model: &str, inpainting_model: &str) -> Result<(), String> {
    let config = PipelineConfig::load().map_err(|e| e.to_string())?;
    let mut write = config.write().map_err(|e| e.to_string())?;

    write.ocr = match ocr_model {
        "paddleocr-vl-1.6" => OcrModel::PaddleOcrVl1_6,
        "manga-ocr" => OcrModel::MangaOcr,
        "baberu-ocr" => OcrModel::BaberuOcr,
        "hayai-ocr" => OcrModel::HayaiOcr,
        other => return Err(format!("unknown OCR model '{other}'")),
    };
    write.inpainting = match inpainting_model {
        "lama" => InpaintingModel::LaMa {},
        "aot-inpainting" => InpaintingModel::AotInpainting {},
        // Reuse whatever per-model settings (e.g. a custom prompt) are
        // already saved under [processor.*] -- possibly set by the real
        // Koharu app, or by a previous selection here -- instead of
        // resetting them to blank defaults every time this is switched
        // back to. Mirrors the "settings for every model are kept
        // independently of the active model" contract koharu-pipeline's
        // own config.rs documents on PipelineConfig.
        "flux2-klein" => InpaintingModel::Flux2Klein(write.processor.flux2_klein.clone().unwrap_or_default()),
        "rorem-mixed" => InpaintingModel::RoremMixed(write.processor.rorem_mixed.clone().unwrap_or_default()),
        other => return Err(format!("unknown inpainting model '{other}'")),
    };
    write.save().map_err(|e| e.to_string())
}

/// Lists only the locally-run translation models (filters out Koharu's
/// cloud providers -- see this module's doc comment). Each `Model` carries
/// its own `quantizations`, each flagged `downloaded` based on whether
/// that file already exists on disk -- the UI uses this to show the user
/// what picking a given option will or won't trigger a fresh download for.
pub async fn list_translation_models() -> Result<Vec<Model>, String> {
    let models = Translator::models().await.map_err(|e| e.to_string())?;
    Ok(models.into_iter().filter(|m| m.provider == Provider::Local).collect())
}

#[derive(Serialize)]
pub struct TranslationSelection {
    pub model: Option<String>,
    pub quantization: Option<String>,
}

pub fn get_translation_selection() -> Result<TranslationSelection, String> {
    let config = PipelineConfig::load().map_err(|e| e.to_string())?;
    let value = config.read().map_err(|e| e.to_string())?;
    Ok(TranslationSelection {
        model: value.translation.model.model.clone(),
        quantization: value.translation.model.quantization.clone(),
    })
}

/// Switches the translation model/quantization, keeping the provider
/// pinned to Local (this app doesn't expose Koharu's cloud providers --
/// see this module's doc comment) and leaving the existing vision/
/// reasoning toggles untouched, so switching models doesn't silently
/// reset preferences the user set some other way (e.g. the real Koharu
/// app, if installed).
pub fn set_translation_model(model_id: &str, quantization: Option<&str>) -> Result<(), String> {
    let config = PipelineConfig::load().map_err(|e| e.to_string())?;
    let mut write = config.write().map_err(|e| e.to_string())?;
    write.translation.model.provider = Provider::Local;
    write.translation.model.model = Some(model_id.to_owned());
    write.translation.model.quantization = quantization.map(ToOwned::to_owned);
    write.save().map_err(|e| e.to_string())
}
