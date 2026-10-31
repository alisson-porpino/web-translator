//! Google engine (read + translate stage only): local detection
//! (engines/detect.rs) finds exactly where the text is, for free, with a
//! model actually trained for that; this only reads and translates the
//! text inside boxes it's already given, via Gemini's structured JSON
//! output. This replaced an earlier design where Gemini was asked to both
//! find AND read text in one shot -- spatial detection is not an LLM's
//! strength (missed/overlapping/mis-sized boxes were nearly every bug seen
//! in testing), reading text inside a given, correctly-sized box is.

use base64::Engine as _;
use image::RgbaImage;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;

use super::compose::{self, TextRegion};
use super::detect::{self, DetectedRegion};
use super::{EngineError, EngineResult};
use crate::settings::Settings;
use crate::usage::UsageTracker;

/// Keep each Gemini call focused on a manageable number of regions --
/// asking it to read text out of too many scattered boxes in one pass
/// risks the same kind of "quietly skips some" behavior seen when it also
/// had to find them.
const MAX_REGIONS_PER_CALL: usize = 10;

fn lang_name(code: &str) -> &str {
    match code {
        "en" => "English",
        "pt" => "Portuguese",
        "es" => "Spanish",
        other => other,
    }
}

fn read_prompt(target_lang: &str, batch: &[(usize, &DetectedRegion)]) -> String {
    let lang = lang_name(target_lang);
    let mut list = String::new();
    for (id, r) in batch {
        list.push_str(&format!("- id {id}: box at x={}, y={}, width={}, height={}\n", r.x, r.y, r.width, r.height));
    }
    format!(
        "This image is a page from a manga, manhwa, or webtoon. The following regions have \
         already been located for you, by position, not by meaning -- most are real story \
         content (speech bubbles, narration, sound effects), but a detector can't tell apart a \
         narration box from a publisher's ad placed on the same page, so some may not be:\n\n\
         {list}\n\
         For each region, read the exact text inside that pixel box, decide if it's part of the \
         STORY, and act accordingly:\n\
         - Story content (anything a character says or thinks, narration about the plot, sound \
         effects drawn into the art): translate it into {lang} and report it normally. If it's \
         already in {lang}, set translated_text equal to original_text.\n\
         - NOT story content -- omit these from your answer entirely, don't translate them: \
         advertisements for other volumes/issues/merchandise (e.g. \"Volume 6 now on sale\", \
         release dates), author name/credits printed outside any panel, publisher names or \
         imprints, page numbers, website watermarks.\n\
         - Detection mistakes (the box doesn't actually contain readable text): also omit.\n\
         Report one entry per region you're translating, referencing it by the same id given \
         above."
    )
}

fn response_schema() -> Value {
    json!({
        "type": "OBJECT",
        "properties": {
            "results": {
                "type": "ARRAY",
                "items": {
                    "type": "OBJECT",
                    "properties": {
                        "id": {"type": "INTEGER"},
                        "original_text": {"type": "STRING"},
                        "translated_text": {"type": "STRING"}
                    },
                    "required": ["id", "original_text", "translated_text"]
                }
            }
        },
        "required": ["results"]
    })
}

#[derive(Debug, Deserialize)]
struct ReadResult {
    id: usize,
    translated_text: String,
}

#[derive(Debug, Deserialize, Default)]
struct ReadResponse {
    results: Vec<ReadResult>,
}

async fn read_batch(
    client: &reqwest::Client,
    settings: &Settings,
    usage: &UsageTracker,
    image_bytes: &[u8],
    mime_type: &str,
    target_lang: &str,
    batch: &[(usize, &DetectedRegion)],
) -> Result<Vec<ReadResult>, EngineError> {
    usage.check_and_record(settings.google_daily_request_cap)?;

    let image_b64 = base64::engine::general_purpose::STANDARD.encode(image_bytes);
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
        settings.gemini_model, settings.google_api_key
    );

    let body = json!({
        "contents": [{
            "parts": [
                { "inline_data": { "mime_type": mime_type, "data": image_b64 } },
                { "text": read_prompt(target_lang, batch) }
            ]
        }],
        "generationConfig": {
            "responseMimeType": "application/json",
            "responseSchema": response_schema(),
            "maxOutputTokens": 8192
        }
    });

    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| EngineError::new("TRANSLATION_FAILED", format!("Gemini request failed: {e}")))?;

    let status = resp.status();
    let payload: Value = resp
        .json()
        .await
        .map_err(|e| EngineError::new("TRANSLATION_FAILED", format!("Gemini response was not valid JSON: {e}")))?;

    if !status.is_success() {
        let message = payload
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(Value::as_str)
            .unwrap_or("unknown error");
        return Err(EngineError::new(
            "TRANSLATION_FAILED",
            format!("Gemini returned HTTP {status}: {message}"),
        ));
    }

    let candidate = payload.get("candidates").and_then(Value::as_array).and_then(|c| c.first());
    let text = candidate
        .and_then(|c| c.get("content"))
        .and_then(|c| c.get("parts"))
        .and_then(Value::as_array)
        .and_then(|p| p.first())
        .and_then(|p| p.get("text"))
        .and_then(Value::as_str);

    let Some(text) = text else {
        let finish_reason = candidate.and_then(|c| c.get("finishReason")).and_then(Value::as_str).unwrap_or("unknown");
        return Err(EngineError::new(
            "TRANSLATION_FAILED",
            format!("Gemini returned no text to parse (finishReason={finish_reason})."),
        ));
    };

    let parsed: ReadResponse = serde_json::from_str(text).map_err(|e| {
        EngineError::new(
            "OCR_FAILED",
            format!("Could not parse Gemini's JSON response: {e}. Raw (truncated): {}", &text[..text.len().min(300)]),
        )
    })?;

    Ok(parsed.results)
}

pub async fn translate(
    client: &reqwest::Client,
    settings: &Settings,
    image_bytes: Vec<u8>,
    mime_type: &str,
    target_lang: &str,
    usage: &UsageTracker,
) -> Result<EngineResult, EngineError> {
    if settings.google_api_key.trim().is_empty() {
        return Err(EngineError::new(
            "ENGINE_UNAVAILABLE",
            "No Google API key configured. Add one in the app's settings window \
             (get one at https://aistudio.google.com/apikey).",
        ));
    }

    let mut image: RgbaImage = image::load_from_memory(&image_bytes)
        .map_err(|e| EngineError::new("TRANSLATION_FAILED", format!("Could not decode source image: {e}")))?
        .to_rgba8();

    // Free, local, no API call: find exactly where the text is before
    // spending anything on reading it. The detector is trained to take the
    // whole page squashed to its input size -- no tiling/cropping here,
    // the reference implementation found that made detection *worse*.
    let detections = detect::detect_text_regions(&image, settings.detection_score_threshold)
        .map_err(|e| EngineError::new("OCR_FAILED", format!("Local text detection failed: {e}")))?;

    if detections.is_empty() {
        return Err(EngineError::new("NO_TRANSLATION_NEEDED", "No text regions detected in this image."));
    }

    let indexed: Vec<(usize, &DetectedRegion)> = detections.iter().enumerate().collect();
    let mut translated_by_id: HashMap<usize, String> = HashMap::new();
    for batch in indexed.chunks(MAX_REGIONS_PER_CALL) {
        let results = read_batch(client, settings, usage, &image_bytes, mime_type, target_lang, batch).await?;
        for r in results {
            translated_by_id.insert(r.id, r.translated_text);
        }
    }

    let regions: Vec<TextRegion> = detections
        .iter()
        .enumerate()
        .filter_map(|(id, d)| {
            translated_by_id.remove(&id).map(|translated_text| TextRegion {
                x: d.x,
                y: d.y,
                width: d.width,
                height: d.height,
                translated_text,
                is_round: d.kind == detect::RegionKind::TextInBubble,
            })
        })
        .collect();

    if regions.is_empty() {
        return Err(EngineError::new(
            "NO_TRANSLATION_NEEDED",
            "Detected regions, but Gemini found none of them to contain real text.",
        ));
    }

    let fonts = compose::load_font_stack(&settings.font_paths);
    // None: the inpainting model is only loaded as part of the local
    // engine's model set today (see local_pipeline.rs) -- this path still
    // gets the flat-fill erase. Worth sharing later if that's worth doing.
    compose::redraw_regions(&mut image, &regions, &fonts);

    let mut out = Vec::new();
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .map_err(|e| EngineError::new("TRANSLATION_FAILED", format!("Could not encode result image: {e}")))?;

    Ok(EngineResult { image_bytes: out, engine_name: "google" })
}
