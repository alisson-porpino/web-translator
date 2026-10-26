pub mod compose;
pub mod detect;
pub mod google;
pub mod koharu_engine;
pub mod ort_env;

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
