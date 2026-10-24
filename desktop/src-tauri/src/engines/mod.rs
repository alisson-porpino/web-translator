pub mod baberu_ocr;
pub mod compose;
pub mod detect;
pub mod gguf_meta;
pub mod google;
pub mod inpaint;
pub mod local_llm;
pub mod local_pipeline;
pub mod manga_ocr;
pub mod ort_env;
pub mod paddle_ocr_vl;

#[derive(Debug, Clone)]
pub struct EngineError {
    pub code: String,
    pub message: String,
}

impl EngineError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
        }
    }
}

pub struct EngineResult {
    pub image_bytes: Vec<u8>,
    pub engine_name: &'static str,
}
