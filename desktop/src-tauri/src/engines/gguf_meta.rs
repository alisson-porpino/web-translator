//! Minimal GGUF header reader -- extracts just the model's layer
//! ("block") count from a .gguf file's metadata, without loading the
//! model itself. Needed to turn a user-configured VRAM budget
//! (Settings.max_vram_mb) into a `n_gpu_layers` count *before* calling
//! `LlamaModel::load_from_file`: llama-cpp-2 only exposes the real layer
//! count via `LlamaModel::n_layer()`, which needs the model already
//! loaded -- and `n_gpu_layers` has to be decided at load time, not
//! after. Reading just the GGUF header avoids a wasteful "load once to
//! ask how big it is, load again with the right setting" and avoids
//! pulling in llama-cpp-2's `common` feature (llama.cpp's own
//! `common_fit_params` auto-fit) just for this one number.
//!
//! GGUF format (spec: https://github.com/ggml-org/ggml/blob/master/docs/gguf.md):
//! magic(u32) + version(u32) + tensor_count(u64) + metadata_kv_count(u64),
//! then that many metadata key/value pairs. Every architecture's block
//! count is stored under a key ending in ".block_count" (e.g.
//! "gemma4.block_count", "qwen3.block_count") -- this scans for that
//! suffix rather than hardcoding an architecture name, so it works for
//! any model without needing a per-architecture lookup table.

use std::io::{BufReader, Read};
use std::path::Path;

const GGUF_MAGIC: u32 = 0x4655_4747; // "GGUF" little-endian

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
enum GgufType {
    U8 = 0,
    I8 = 1,
    U16 = 2,
    I16 = 3,
    U32 = 4,
    I32 = 5,
    F32 = 6,
    Bool = 7,
    String = 8,
    Array = 9,
    U64 = 10,
    I64 = 11,
    F64 = 12,
}

impl GgufType {
    fn from_u32(v: u32) -> Option<Self> {
        Some(match v {
            0 => Self::U8,
            1 => Self::I8,
            2 => Self::U16,
            3 => Self::I16,
            4 => Self::U32,
            5 => Self::I32,
            6 => Self::F32,
            7 => Self::Bool,
            8 => Self::String,
            9 => Self::Array,
            10 => Self::U64,
            11 => Self::I64,
            12 => Self::F64,
            _ => return None,
        })
    }

    /// Byte width for every fixed-size scalar type; String/Array are
    /// handled separately since they're variable-length.
    fn scalar_size(self) -> Option<u64> {
        match self {
            Self::U8 | Self::I8 | Self::Bool => Some(1),
            Self::U16 | Self::I16 => Some(2),
            Self::U32 | Self::I32 | Self::F32 => Some(4),
            Self::U64 | Self::I64 | Self::F64 => Some(8),
            Self::String | Self::Array => None,
        }
    }
}

/// Reads `<any prefix>.block_count` from a GGUF file's metadata --
/// the model's layer count, same value `LlamaModel::n_layer()` would
/// report after a full load. Returns `None` on any parse failure or if
/// no such key exists (caller falls back to "offload every layer",
/// today's pre-VRAM-budget behavior) -- this is a best-effort read, not
/// something that should ever hard-fail a model load.
pub fn read_block_count(path: &Path) -> Option<u64> {
    let file = std::fs::File::open(path).ok()?;
    let mut r = BufReader::new(file);

    if read_u32(&mut r)? != GGUF_MAGIC {
        return None;
    }
    let version = read_u32(&mut r)?;
    if version < 2 {
        // Version 1 used u32 counts instead of u64; none of the models
        // this app's catalog downloads are that old, not worth supporting.
        return None;
    }
    let tensor_count = read_u64(&mut r)?;
    let metadata_kv_count = read_u64(&mut r)?;
    let _ = tensor_count; // not needed -- metadata comes first in the file, tensor info after

    for _ in 0..metadata_kv_count {
        let key = read_gguf_string(&mut r)?;
        let value_type = GgufType::from_u32(read_u32(&mut r)?)?;

        if key.ends_with(".block_count") {
            return read_scalar_as_u64(&mut r, value_type);
        }
        skip_value(&mut r, value_type)?;
    }
    None
}

fn read_u32(r: &mut impl Read) -> Option<u32> {
    let mut buf = [0u8; 4];
    r.read_exact(&mut buf).ok()?;
    Some(u32::from_le_bytes(buf))
}

fn read_u64(r: &mut impl Read) -> Option<u64> {
    let mut buf = [0u8; 8];
    r.read_exact(&mut buf).ok()?;
    Some(u64::from_le_bytes(buf))
}

fn read_gguf_string(r: &mut impl Read) -> Option<String> {
    let len = read_u64(r)?;
    // A corrupt/non-GGUF file could claim an absurd length -- cap it
    // well above any real GGUF key/string length (longest real keys and
    // string-typed metadata values are a few hundred bytes at most) so a
    // bad read fails fast instead of trying to allocate gigabytes.
    if len > 1_000_000 {
        return None;
    }
    let mut buf = vec![0u8; len as usize];
    r.read_exact(&mut buf).ok()?;
    String::from_utf8(buf).ok()
}

/// Reads a scalar value already known to be `value_type` and widens it
/// to u64 -- block_count is realistically always U32, but read
/// defensively in case some exporter used a different integer width.
fn read_scalar_as_u64(r: &mut impl Read, value_type: GgufType) -> Option<u64> {
    match value_type {
        GgufType::U8 | GgufType::Bool => {
            let mut b = [0u8; 1];
            r.read_exact(&mut b).ok()?;
            Some(b[0] as u64)
        }
        GgufType::I8 => {
            let mut b = [0u8; 1];
            r.read_exact(&mut b).ok()?;
            Some(b[0] as i8 as u64)
        }
        GgufType::U16 => {
            let mut b = [0u8; 2];
            r.read_exact(&mut b).ok()?;
            Some(u16::from_le_bytes(b) as u64)
        }
        GgufType::I16 => {
            let mut b = [0u8; 2];
            r.read_exact(&mut b).ok()?;
            Some(i16::from_le_bytes(b) as u64)
        }
        GgufType::U32 => Some(read_u32(r)? as u64),
        GgufType::I32 => Some(read_u32(r)? as i32 as u64),
        GgufType::U64 => read_u64(r),
        GgufType::I64 => Some(read_u64(r)? as i64 as u64),
        GgufType::F32 | GgufType::F64 | GgufType::String | GgufType::Array => None,
    }
}

/// Advances past a metadata value of `value_type` without interpreting
/// it -- needed for every key that isn't "*.block_count" so the scan can
/// keep walking the key/value list in order (GGUF has no "jump to next
/// key" offset table; the only way to find a later key is to correctly
/// walk past every earlier value).
fn skip_value(r: &mut impl Read, value_type: GgufType) -> Option<()> {
    match value_type {
        GgufType::String => {
            read_gguf_string(r)?;
            Some(())
        }
        GgufType::Array => {
            let element_type = GgufType::from_u32(read_u32(r)?)?;
            let count = read_u64(r)?;
            if count > 50_000_000 {
                // Same defensive cap as read_gguf_string's length check --
                // no real GGUF array metadata value is anywhere near this
                // large (tokenizer vocab arrays, the largest realistic
                // case, top out in the hundreds of thousands).
                return None;
            }
            for _ in 0..count {
                skip_value(r, element_type)?;
            }
            Some(())
        }
        other => {
            let size = other.scalar_size()?;
            let mut buf = vec![0u8; size as usize];
            r.read_exact(&mut buf).ok()?;
            Some(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a minimal, valid GGUF header (no tensors) with the given
    /// metadata key/value pairs already pre-encoded, for testing the
    /// scan logic without needing a real multi-gigabyte model file.
    fn write_gguf_header(kvs: &[(&str, Vec<u8>)]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&GGUF_MAGIC.to_le_bytes());
        out.extend_from_slice(&3u32.to_le_bytes()); // version
        out.extend_from_slice(&0u64.to_le_bytes()); // tensor_count
        out.extend_from_slice(&(kvs.len() as u64).to_le_bytes()); // metadata_kv_count
        for (key, encoded_value) in kvs {
            out.extend_from_slice(&(key.len() as u64).to_le_bytes());
            out.extend_from_slice(key.as_bytes());
            out.extend_from_slice(encoded_value);
        }
        out
    }

    fn encode_u32_kv(value_type: u32, value: u32) -> Vec<u8> {
        let mut out = value_type.to_le_bytes().to_vec();
        out.extend_from_slice(&value.to_le_bytes());
        out
    }

    fn encode_string_kv(s: &str) -> Vec<u8> {
        let mut out = 8u32.to_le_bytes().to_vec(); // GgufType::String
        out.extend_from_slice(&(s.len() as u64).to_le_bytes());
        out.extend_from_slice(s.as_bytes());
        out
    }

    #[test]
    fn finds_block_count_after_skipping_other_keys() {
        let data = write_gguf_header(&[
            ("general.architecture", encode_string_kv("gemma4")),
            ("general.name", encode_string_kv("Gemma 4 12B")),
            ("gemma4.block_count", encode_u32_kv(4, 42)), // GgufType::U32 = 4
            ("gemma4.context_length", encode_u32_kv(4, 8192)),
        ]);
        let path = std::env::temp_dir().join(format!("mwt_gguf_meta_test_{}.gguf", std::process::id()));
        std::fs::write(&path, &data).unwrap();

        assert_eq!(read_block_count(&path), Some(42));

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn returns_none_when_no_block_count_key_present() {
        let data = write_gguf_header(&[("general.architecture", encode_string_kv("mystery"))]);
        let path = std::env::temp_dir().join(format!("mwt_gguf_meta_test_none_{}.gguf", std::process::id()));
        std::fs::write(&path, &data).unwrap();

        assert_eq!(read_block_count(&path), None);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn returns_none_for_a_non_gguf_file() {
        let path = std::env::temp_dir().join(format!("mwt_gguf_meta_test_bad_{}.gguf", std::process::id()));
        std::fs::write(&path, b"not a gguf file").unwrap();

        assert_eq!(read_block_count(&path), None);

        std::fs::remove_file(&path).ok();
    }

    /// Not run in normal `cargo test`. Set MWT_GGUF_TEST_FILE to a real
    /// .gguf file (even just its first ~256KB via a Range request --
    /// metadata always comes before tensor data in GGUF, so a truncated
    /// file still has the full metadata section for any normal-sized
    /// model) and MWT_GGUF_EXPECTED_BLOCK_COUNT to the real layer count
    /// (from the model's own HF page or `inference_config.json`) to
    /// confirm this parses real-world files, not just the synthetic
    /// fixtures above.
    #[test]
    fn reads_block_count_from_a_real_gguf_file() {
        let (Ok(path), Ok(expected)) =
            (std::env::var("MWT_GGUF_TEST_FILE"), std::env::var("MWT_GGUF_EXPECTED_BLOCK_COUNT"))
        else {
            eprintln!("skipping: set MWT_GGUF_TEST_FILE and MWT_GGUF_EXPECTED_BLOCK_COUNT to run this");
            return;
        };
        let expected: u64 = expected.parse().expect("MWT_GGUF_EXPECTED_BLOCK_COUNT must be a number");
        let result = read_block_count(std::path::Path::new(&path));
        eprintln!("read_block_count({path}) = {result:?}");
        assert_eq!(result, Some(expected));
    }
}
