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
//! The fixed set of valid model ids is hardcoded as <option> tags in
//! desktop/ui/index.html, mirroring koharu-pipeline's config.rs -- keep
//! both in sync if the pinned Koharu rev ever adds/renames a model.

use koharu_pipeline::{InpaintingModel, OcrModel, PipelineConfig};
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
