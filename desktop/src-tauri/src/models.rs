//! Generic, on-demand model registry. Any pipeline stage that needs
//! downloadable weights (OCR, inpainting, translation -- detection is
//! bundled in the binary, see assets/detector.fp16.onnx) draws from a
//! catalog of named options here, each downloaded individually only once
//! the user actually selects it -- never bundled, never fetched unasked.
//! This keeps the base app light even as more options get added per
//! category over time: picking Qwen2.5-VL as the translation model
//! doesn't touch OCR/inpainting's own downloads, and an option nobody
//! selects is never fetched at all.
//!
//! Detection and inpainting still list exactly one option each. OCR and
//! translation have grown past that (see engines/) as Koharu-style
//! breadth gets added one model at a time -- each still needs both a
//! `ModelOption` entry here *and* a matching engine implementation,
//! that's not something this registry can conjure on its own.

use serde::Serialize;
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, Manager};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelCategory {
    Detection,
    Ocr,
    Inpainting,
    Translation,
}

impl ModelCategory {
    pub fn all() -> &'static [ModelCategory] {
        &[ModelCategory::Detection, ModelCategory::Ocr, ModelCategory::Inpainting, ModelCategory::Translation]
    }

    pub fn id(self) -> &'static str {
        match self {
            ModelCategory::Detection => "detection",
            ModelCategory::Ocr => "ocr",
            ModelCategory::Inpainting => "inpainting",
            ModelCategory::Translation => "translation",
        }
    }

    pub fn options(self) -> &'static [ModelOption] {
        match self {
            ModelCategory::Detection => DETECTION_OPTIONS,
            ModelCategory::Ocr => OCR_OPTIONS,
            ModelCategory::Inpainting => INPAINTING_OPTIONS,
            ModelCategory::Translation => TRANSLATION_OPTIONS,
        }
    }
}

pub struct ModelFile {
    pub filename: &'static str,
    pub url: &'static str,
    /// Used only to size the progress bar before the real Content-Length is
    /// known; not load-bearing for correctness.
    pub approx_bytes: u64,
}

pub struct ModelOption {
    pub id: &'static str,
    pub label: &'static str,
    /// True for an option that ships inside the app binary itself (the
    /// bundled detector) -- always considered downloaded, never has files
    /// to fetch or delete.
    pub builtin: bool,
    pub files: &'static [ModelFile],
}

pub const DETECTION_OPTIONS: &[ModelOption] =
    &[ModelOption { id: "bundled-detector", label: "Comic text/bubble detector (bundled)", builtin: true, files: &[] }];

pub const OCR_OPTIONS: &[ModelOption] = &[
    ModelOption {
        id: "manga-ocr",
        label: "manga-ocr (Japanese only)",
        builtin: false,
        files: &[
            ModelFile {
                filename: "manga-ocr-encoder.onnx",
                url: "https://huggingface.co/hon-lu/manga-ocr-base-onnx-fp16/resolve/main/ocr-encoder.fp16.onnx",
                approx_bytes: 171_785_393,
            },
            ModelFile {
                filename: "manga-ocr-decoder-cross.onnx",
                url: "https://huggingface.co/hon-lu/manga-ocr-base-onnx-fp16/resolve/main/ocr-decoder-cross.fp16.onnx",
                approx_bytes: 4_732_209,
            },
            ModelFile {
                filename: "manga-ocr-decoder-step.onnx",
                url: "https://huggingface.co/hon-lu/manga-ocr-base-onnx-fp16/resolve/main/ocr-decoder-step.fp16.onnx",
                approx_bytes: 53_977_443,
            },
            ModelFile {
                filename: "manga-ocr-vocab.txt",
                url: "https://huggingface.co/hon-lu/manga-ocr-base-onnx-fp16/resolve/main/vocab.txt",
                approx_bytes: 24_072,
            },
        ],
    },
    ModelOption {
        // Koharu's default OCR engine. Official PaddlePaddle GGUF export
        // (Apache-2.0), runs via llama.cpp/mtmd -- same runtime as
        // engines/local_llm.rs, no new Rust dependency. Multilingual,
        // unlike manga-ocr's Japanese-only encoder-decoder. Pinned to the
        // revision confirmed live against the Hugging Face API on
        // 2026-10-20.
        id: "paddle-ocr-vl-1.6",
        label: "PaddleOCR-VL 1.6 (multilingual)",
        builtin: false,
        files: &[
            ModelFile {
                filename: "paddle-ocr-vl-1.6.gguf",
                url: "https://huggingface.co/PaddlePaddle/PaddleOCR-VL-1.6-GGUF/resolve/511b09642bb324401f15f97cc23bc67e8f0a291d/PaddleOCR-VL-1.6-GGUF.gguf",
                approx_bytes: 935_769_056,
            },
            ModelFile {
                filename: "paddle-ocr-vl-1.6-mmproj.gguf",
                url: "https://huggingface.co/PaddlePaddle/PaddleOCR-VL-1.6-GGUF/resolve/511b09642bb324401f15f97cc23bc67e8f0a291d/PaddleOCR-VL-1.6-GGUF-mmproj.gguf",
                approx_bytes: 881_770_560,
            },
        ],
    },
    ModelOption {
        // genshiai-daichi/baberu-ocr, Apache-2.0 -- small (115M), purpose-
        // built for manga speech bubbles (trained on Manga109-s, reads
        // sound effects/handwriting), multilingual (ja/zh/en), character-
        // level vocab. Koharu itself runs this via libtorch from the raw
        // safetensors checkpoint; we use the model author's own
        // self-contained ONNX export instead (same vision+split-decoder
        // shape as manga_ocr.rs), avoiding a libtorch dependency (see
        // DIRETRIZES.md Principio 1). fp16 vision + int8 decoders (the
        // author's "lossless, exact published nCER" tier -- there's also
        // a smaller int4-vision tier, not used here). Pinned to the
        // revision confirmed live against the Hugging Face API on
        // 2026-10-21.
        id: "baberu-ocr",
        label: "Baberu OCR (multilingual, manga-tuned)",
        builtin: false,
        files: &[
            ModelFile {
                filename: "baberu-ocr-vision.onnx",
                url: "https://huggingface.co/genshiai-daichi/baberu-ocr/resolve/b9868433dbda3ed9ba1357d008599ff4ada873d5/onnx/vision_fp16.onnx",
                approx_bytes: 172_917_304,
            },
            ModelFile {
                filename: "baberu-ocr-decoder-prefill.onnx",
                url: "https://huggingface.co/genshiai-daichi/baberu-ocr/resolve/b9868433dbda3ed9ba1357d008599ff4ada873d5/onnx/decoder_prefill_int8.onnx",
                approx_bytes: 35_133_596,
            },
            ModelFile {
                filename: "baberu-ocr-decoder-step.onnx",
                url: "https://huggingface.co/genshiai-daichi/baberu-ocr/resolve/b9868433dbda3ed9ba1357d008599ff4ada873d5/onnx/decoder_step_int8.onnx",
                approx_bytes: 34_715_466,
            },
            ModelFile {
                filename: "baberu-ocr-vocab.json",
                url: "https://huggingface.co/genshiai-daichi/baberu-ocr/resolve/b9868433dbda3ed9ba1357d008599ff4ada873d5/tokenizer/vocab.json",
                approx_bytes: 130_761,
            },
        ],
    },
];

pub const INPAINTING_OPTIONS: &[ModelOption] = &[ModelOption {
    id: "lama",
    label: "LaMa",
    builtin: false,
    files: &[ModelFile {
        // Apache-2.0. Carve-Photos' ONNX export of advimman/lama's
        // "big-lama" checkpoint -- a general-purpose photo inpainting
        // model (not manga-specific), used to reconstruct a bubble's real
        // background instead of flat-filling it; see engines/inpaint.rs.
        filename: "lama-inpainting-fp32.onnx",
        url: "https://huggingface.co/Carve/LaMa-ONNX/resolve/main/lama_fp32.onnx",
        approx_bytes: 208_044_816,
    }],
}];

pub const TRANSLATION_OPTIONS: &[ModelOption] = &[
    ModelOption {
        // Koharu's "lfm2.5-1.2b-instruct" -- smallest/fastest local
        // translation option, no vision tower (text-only, OCR transcript
        // only -- see engines/local_llm.rs's mtmd_ctx: None path). License
        // tag on HF is "other" (LiquidAI's own "lfm1.0" license, a custom
        // permissive license, not OSI-approved) -- runtime download
        // straight from Hugging Face, same reasoning as the Gemma entries
        // above (DIRETRIZES.md Principio 6 doesn't apply to these).
        id: "lfm2.5-1.2b-instruct",
        label: "LFM2.5 1.2B Instruct (text-only, fast)",
        builtin: false,
        files: &[ModelFile {
            filename: "lfm2.5-1.2b-instruct-q4_k_m.gguf",
            url: "https://huggingface.co/LiquidAI/LFM2.5-1.2B-Instruct-GGUF/resolve/afbd8eaeab5dd94ba0b079ebfb02517d19641e38/LFM2.5-1.2B-Instruct-Q4_K_M.gguf",
            approx_bytes: 730_895_168,
        }],
    },
    ModelOption {
        // Koharu's "ministral-3-8b-instruct" -- text-only (no vision
        // tower, same as above). Apache-2.0 on the HF card.
        id: "ministral-3-8b-instruct",
        label: "Ministral 3 8B Instruct (text-only)",
        builtin: false,
        files: &[ModelFile {
            filename: "ministral-3-8b-instruct-q4_k_m.gguf",
            url: "https://huggingface.co/mistralai/Ministral-3-8B-Instruct-2512-GGUF/resolve/0102285ad796bd99af90f58de616092e5630e970/Ministral-3-8B-Instruct-2512-Q4_K_M.gguf",
            approx_bytes: 5_198_911_904,
        }],
    },
    ModelOption {
        // Koharu's "gemma4-e2b-it" -- smallest vision-capable Gemma 4 in
        // its catalog. Apache-2.0 tag on HF (with the usual Gemma Terms of
        // Use link, same caveat as the 12B entry below).
        id: "gemma-4-e2b-it",
        label: "Gemma 4 E2B Instruct (vision-grounded)",
        builtin: false,
        files: &[
            ModelFile {
                filename: "gemma-4-e2b-it-q4_k_xl.gguf",
                url: "https://huggingface.co/unsloth/gemma-4-E2B-it-qat-GGUF/resolve/66a399f68ddd113b06dff02fca9523e55465d11d/gemma-4-E2B-it-qat-UD-Q4_K_XL.gguf",
                approx_bytes: 2_620_370_976,
            },
            ModelFile {
                filename: "gemma-4-e2b-it-mmproj.gguf",
                url: "https://huggingface.co/unsloth/gemma-4-E2B-it-qat-GGUF/resolve/66a399f68ddd113b06dff02fca9523e55465d11d/mmproj-F16.gguf",
                approx_bytes: 985_654_080,
            },
        ],
    },
    ModelOption {
        // Koharu's "gemma4-e2b-uncensored" -- uncensored twin of the
        // option above, same HauhauCS provenance caveat as the 12B
        // uncensored entry (see roadmap-modelos-futuros.md).
        id: "gemma-4-e2b-uncensored",
        label: "Gemma 4 E2B Uncensored (vision-grounded, uncensored finetune)",
        builtin: false,
        files: &[
            ModelFile {
                filename: "gemma-4-e2b-uncensored-q4_k_p.gguf",
                url: "https://huggingface.co/HauhauCS/Gemma-4-E2B-Uncensored-HauhauCS-Aggressive/resolve/da8593c3e407afcd3e7da94ff2d69d77e2a28a48/Gemma-4-E2B-Uncensored-HauhauCS-Aggressive-Q4_K_P.gguf",
                approx_bytes: 3_450_277_824,
            },
            ModelFile {
                filename: "gemma-4-e2b-uncensored-mmproj.gguf",
                url: "https://huggingface.co/HauhauCS/Gemma-4-E2B-Uncensored-HauhauCS-Aggressive/resolve/da8593c3e407afcd3e7da94ff2d69d77e2a28a48/mmproj-Gemma-4-E2B-Uncensored-HauhauCS-Aggressive-f16.gguf",
                approx_bytes: 985_570_240,
            },
        ],
    },
    ModelOption {
        // Koharu's "gemma4-e4b-it".
        id: "gemma-4-e4b-it",
        label: "Gemma 4 E4B Instruct (vision-grounded)",
        builtin: false,
        files: &[
            ModelFile {
                filename: "gemma-4-e4b-it-q4_k_xl.gguf",
                url: "https://huggingface.co/unsloth/gemma-4-E4B-it-qat-GGUF/resolve/8c5a9e4fd5482e2be20fe0bf013b4c262a8f4265/gemma-4-E4B-it-qat-UD-Q4_K_XL.gguf",
                approx_bytes: 4_215_695_776,
            },
            ModelFile {
                filename: "gemma-4-e4b-it-mmproj.gguf",
                url: "https://huggingface.co/unsloth/gemma-4-E4B-it-qat-GGUF/resolve/8c5a9e4fd5482e2be20fe0bf013b4c262a8f4265/mmproj-F16.gguf",
                approx_bytes: 990_372_672,
            },
        ],
    },
    ModelOption {
        // Koharu's "gemma4-e4b-uncensored" -- same HauhauCS caveat as the
        // other uncensored entries.
        id: "gemma-4-e4b-uncensored",
        label: "Gemma 4 E4B Uncensored (vision-grounded, uncensored finetune)",
        builtin: false,
        files: &[
            ModelFile {
                filename: "gemma-4-e4b-uncensored-q4_k_p.gguf",
                url: "https://huggingface.co/HauhauCS/Gemma-4-E4B-Uncensored-HauhauCS-Aggressive/resolve/45b6a334b4bcd1d7f37179df58b3b1d66a184e5d/Gemma-4-E4B-Uncensored-HauhauCS-Aggressive-Q4_K_P.gguf",
                approx_bytes: 5_369_246_688,
            },
            ModelFile {
                filename: "gemma-4-e4b-uncensored-mmproj.gguf",
                url: "https://huggingface.co/HauhauCS/Gemma-4-E4B-Uncensored-HauhauCS-Aggressive/resolve/45b6a334b4bcd1d7f37179df58b3b1d66a184e5d/mmproj-Gemma-4-E4B-Uncensored-HauhauCS-Aggressive-f16.gguf",
                approx_bytes: 990_288_832,
            },
        ],
    },
    ModelOption {
        // Koharu's "gemma4-26b-a4b-it". Much larger than the previous
        // Gemma 4 entries (~15.4GB combined) -- confirmed with the
        // Alisson before cadastering given the size jump.
        id: "gemma-4-26b-a4b-it",
        label: "Gemma 4 26B A4B Instruct (vision-grounded)",
        builtin: false,
        files: &[
            ModelFile {
                filename: "gemma-4-26b-a4b-it-q4_k_xl.gguf",
                url: "https://huggingface.co/unsloth/gemma-4-26B-A4B-it-qat-GGUF/resolve/7b92b5b28818151e8669af2e45e88d6086f490dd/gemma-4-26B-A4B-it-qat-UD-Q4_K_XL.gguf",
                approx_bytes: 14_249_047_104,
            },
            ModelFile {
                filename: "gemma-4-26b-a4b-it-mmproj.gguf",
                url: "https://huggingface.co/unsloth/gemma-4-26B-A4B-it-qat-GGUF/resolve/7b92b5b28818151e8669af2e45e88d6086f490dd/mmproj-F16.gguf",
                approx_bytes: 1_193_058_784,
            },
        ],
    },
    ModelOption {
        // Koharu's "gemma4-26b-a4b-uncensored" -- same HauhauCS caveat as
        // the other uncensored entries. ~16.7GB combined.
        id: "gemma-4-26b-a4b-uncensored",
        label: "Gemma 4 26B A4B Uncensored (vision-grounded, uncensored finetune)",
        builtin: false,
        files: &[
            ModelFile {
                filename: "gemma-4-26b-a4b-uncensored-q4_k_m.gguf",
                url: "https://huggingface.co/HauhauCS/Gemma4-26B-A4B-QAT-Uncensored-HauhauCS-Balanced-MTP/resolve/f9093662a2e7ae0503f637088bc96f77a1a70c83/Gemma4-26B-A4B-QAT-Uncensored-HauhauCS-Balanced-Q4_K_M.gguf",
                approx_bytes: 16_796_015_520,
            },
            ModelFile {
                filename: "gemma-4-26b-a4b-uncensored-mmproj.gguf",
                url: "https://huggingface.co/HauhauCS/Gemma4-26B-A4B-QAT-Uncensored-HauhauCS-Balanced-MTP/resolve/f9093662a2e7ae0503f637088bc96f77a1a70c83/mmproj-Gemma4-26B-A4B-QAT-Uncensored-HauhauCS-Balanced-BF16.gguf",
                approx_bytes: 1_194_827_776,
            },
        ],
    },
    ModelOption {
        // Koharu's "gemma4-31b-it" -- the largest Gemma 4 variant Koharu
        // offers, ~18.5GB combined.
        id: "gemma-4-31b-it",
        label: "Gemma 4 31B Instruct (vision-grounded)",
        builtin: false,
        files: &[
            ModelFile {
                filename: "gemma-4-31b-it-q4_k_xl.gguf",
                url: "https://huggingface.co/unsloth/gemma-4-31B-it-qat-GGUF/resolve/43cc1aeb31adf47ec06a854507ce552cd9862e6f/gemma-4-31B-it-qat-UD-Q4_K_XL.gguf",
                approx_bytes: 17_287_670_048,
            },
            ModelFile {
                filename: "gemma-4-31b-it-mmproj.gguf",
                url: "https://huggingface.co/unsloth/gemma-4-31B-it-qat-GGUF/resolve/43cc1aeb31adf47ec06a854507ce552cd9862e6f/mmproj-F16.gguf",
                approx_bytes: 1_198_957_024,
            },
        ],
    },
    ModelOption {
        // Koharu's "gemma4-31b-uncensored" -- same HauhauCS caveat as the
        // other uncensored entries. ~19.9GB combined.
        id: "gemma-4-31b-uncensored",
        label: "Gemma 4 31B Uncensored (vision-grounded, uncensored finetune)",
        builtin: false,
        files: &[
            ModelFile {
                filename: "gemma-4-31b-uncensored-q4_k_m.gguf",
                url: "https://huggingface.co/HauhauCS/Gemma4-31B-QAT-Uncensored-HauhauCS-Balanced-MTP/resolve/9654466e82d83f5ebfe1518a369bc5900873abb1/Gemma4-31B-QAT-Uncensored-HauhauCS-Balanced-Q4_K_M.gguf",
                approx_bytes: 18_687_062_176,
            },
            ModelFile {
                filename: "gemma-4-31b-uncensored-mmproj.gguf",
                url: "https://huggingface.co/HauhauCS/Gemma4-31B-QAT-Uncensored-HauhauCS-Balanced-MTP/resolve/9654466e82d83f5ebfe1518a369bc5900873abb1/mmproj-Gemma4-31B-QAT-Uncensored-HauhauCS-Balanced-BF16.gguf",
                approx_bytes: 1_200_726_016,
            },
        ],
    },
    ModelOption {
        id: "qwen2.5-vl-7b-instruct",
        label: "Qwen2.5-VL-7B-Instruct (vision-grounded)",
        builtin: false,
        files: &[
            ModelFile {
                filename: "qwen2.5-vl-7b-instruct-q4_k_m.gguf",
                url: "https://huggingface.co/unsloth/Qwen2.5-VL-7B-Instruct-GGUF/resolve/main/Qwen2.5-VL-7B-Instruct-Q4_K_M.gguf",
                approx_bytes: 4_683_072_384,
            },
            ModelFile {
                filename: "qwen2.5-vl-7b-instruct-mmproj.gguf",
                url: "https://huggingface.co/unsloth/Qwen2.5-VL-7B-Instruct-GGUF/resolve/main/mmproj-F16.gguf",
                approx_bytes: 1_354_163_040,
            },
        ],
    },
    ModelOption {
        // Koharu's own default local translation model (see
        // crates/koharu-translator/src/local/catalog.rs in mayocream/koharu
        // -- "gemma4-12b-it" / Q4_K_XL is its DEFAULT_MODEL /
        // DEFAULT_QUANTIZATION). Pinned to the exact revision Koharu itself
        // pins, confirmed live against the Hugging Face API before adding
        // here (sha + file sizes match). License: tagged apache-2.0 on the
        // HF card, but the card's license_link points at Google's own
        // "Gemma 4" terms of use page -- the usual Gemma pattern (permissive
        // but not OSI-approved; has a prohibited-use policy). This is a
        // runtime, user-initiated download straight from Hugging Face, not
        // an asset bundled into this app's own binary/repo, so it doesn't
        // need to clear the same bar as assets/ (see DIRETRIZES.md
        // Principio 6) -- but it's still the user's call to accept that
        // license, which the download confirmation step already surfaces.
        id: "gemma-4-12b-it",
        label: "Gemma 4 12B Instruct (vision-grounded, Koharu default)",
        builtin: false,
        files: &[
            ModelFile {
                filename: "gemma-4-12b-it-q4_k_xl.gguf",
                url: "https://huggingface.co/unsloth/gemma-4-12B-it-qat-GGUF/resolve/980b060c40a8539ac159e0501a3e0f66a6365af3/gemma-4-12B-it-qat-UD-Q4_K_XL.gguf",
                approx_bytes: 6_716_356_800,
            },
            ModelFile {
                filename: "gemma-4-12b-it-mmproj.gguf",
                url: "https://huggingface.co/unsloth/gemma-4-12B-it-qat-GGUF/resolve/980b060c40a8539ac159e0501a3e0f66a6365af3/mmproj-F16.gguf",
                approx_bytes: 175_115_840,
            },
        ],
    },
    ModelOption {
        // Koharu's "gemma4-12b-uncensored" -- a third-party (HauhauCS)
        // finetune of google/gemma-4-12B-it with safety alignment removed,
        // offered by Koharu alongside the official Gemma releases (its
        // catalog has several of these; see roadmap-modelos-futuros.md's
        // "Cautela com os modelos uncensored" note on provenance). Added
        // at the Alisson's explicit request, confirmed live against the
        // Hugging Face API (sha + file sizes match Koharu's pin). License
        // tag on the HF card is "gemma" (not apache-2.0 -- more honestly
        // labeled than the official unsloth repo's card), based on
        // google/gemma-4-12B-it -- same Gemma Terms of Use caveat as the
        // non-uncensored option above, same runtime-download reasoning
        // (not a bundled asset, see DIRETRIZES.md Principio 6).
        id: "gemma-4-12b-uncensored",
        label: "Gemma 4 12B Uncensored (vision-grounded, uncensored finetune)",
        builtin: false,
        files: &[
            ModelFile {
                filename: "gemma-4-12b-uncensored-q4_k_m.gguf",
                url: "https://huggingface.co/HauhauCS/Gemma4-12B-QAT-Uncensored-HauhauCS-Balanced/resolve/ae8045ac2bd216293ca49a3065da2c942dde4b68/Gemma4-12B-QAT-Uncensored-HauhauCS-Balanced-Q4_K_M.gguf",
                approx_bytes: 7_381_381_760,
            },
            ModelFile {
                filename: "gemma-4-12b-uncensored-mmproj.gguf",
                url: "https://huggingface.co/HauhauCS/Gemma4-12B-QAT-Uncensored-HauhauCS-Balanced/resolve/ae8045ac2bd216293ca49a3065da2c942dde4b68/mmproj-Gemma4-12B-QAT-Uncensored-HauhauCS-Balanced-BF16.gguf",
                approx_bytes: 175_115_264,
            },
        ],
    },
];

pub fn models_dir(app: &AppHandle) -> PathBuf {
    let dir = app.path().app_data_dir().expect("app data dir").join("models");
    std::fs::create_dir_all(&dir).ok();
    dir
}

pub fn model_path(app: &AppHandle, filename: &str) -> PathBuf {
    models_dir(app).join(filename)
}

pub fn find_option(id: &str) -> Option<&'static ModelOption> {
    ModelCategory::all().iter().flat_map(|c| c.options()).find(|o| o.id == id)
}

pub fn is_downloaded(app: &AppHandle, option: &ModelOption) -> bool {
    option.builtin || option.files.iter().all(|f| model_path(app, f.filename).is_file())
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OptionStatus {
    pub id: String,
    pub label: String,
    pub downloaded: bool,
    pub builtin: bool,
    pub approx_bytes: u64,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CategoryStatus {
    pub category: String,
    pub options: Vec<OptionStatus>,
}

/// Full catalog, annotated with each option's current downloaded state --
/// everything the settings UI needs to render every category's picker
/// plus its "Manage" (download/delete) view in one call.
pub fn catalog_status(app: &AppHandle) -> Vec<CategoryStatus> {
    ModelCategory::all()
        .iter()
        .map(|&cat| CategoryStatus {
            category: cat.id().to_string(),
            options: cat
                .options()
                .iter()
                .map(|o| OptionStatus {
                    id: o.id.to_string(),
                    label: o.label.to_string(),
                    downloaded: is_downloaded(app, o),
                    builtin: o.builtin,
                    approx_bytes: o.files.iter().map(|f| f.approx_bytes).sum(),
                })
                .collect(),
        })
        .collect()
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub option_id: String,
    pub file: String,
    pub file_index: usize,
    pub file_count: usize,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub done: bool,
    pub error: Option<String>,
}

/// Downloads every not-yet-present file for one catalog option, emitting
/// "model-download-progress" events the settings window listens for. Each
/// file lands at "<name>.part" first and is only renamed to its final name
/// once fully written, so a crash/quit mid-download can't leave a
/// truncated file that `is_downloaded()` would wrongly call present.
pub async fn download_option(app: AppHandle, option_id: String) -> Result<(), String> {
    let option = find_option(&option_id).ok_or_else(|| format!("unknown model option: {option_id}"))?;
    if option.builtin {
        return Ok(());
    }
    let dir = models_dir(&app);
    let to_fetch: Vec<&ModelFile> = option.files.iter().filter(|f| !dir.join(f.filename).is_file()).collect();
    let file_count = to_fetch.len();
    let client = reqwest::Client::new();

    for (index, file) in to_fetch.into_iter().enumerate() {
        let emit_progress = |downloaded: u64, total: u64, done: bool, error: Option<String>| {
            let _ = app.emit(
                "model-download-progress",
                DownloadProgress {
                    option_id: option_id.clone(),
                    file: file.filename.to_string(),
                    file_index: index,
                    file_count,
                    downloaded_bytes: downloaded,
                    total_bytes: total,
                    done,
                    error,
                },
            );
        };

        match download_one(&client, file, &dir).await {
            Ok(()) => emit_progress(file.approx_bytes, file.approx_bytes, true, None),
            Err(e) => {
                emit_progress(0, file.approx_bytes, true, Some(e.clone()));
                return Err(format!("Failed downloading {}: {e}", file.filename));
            }
        }
    }

    Ok(())
}

/// Downloads one file, resuming from an existing `.part` if one is
/// already there (e.g. the app was closed/killed mid-download last time
/// -- see DIRETRIZES.md's ".part file" pitfall, this is what fixes it).
/// Asks the server for only the missing tail via an HTTP Range request;
/// falls back to a full restart (truncating the stale partial) if the
/// server doesn't honor that, which is always safe, just slower.
async fn download_one(client: &reqwest::Client, file: &ModelFile, dir: &std::path::Path) -> Result<(), String> {
    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;

    let part_path = dir.join(format!("{}.part", file.filename));
    let already_downloaded = tokio::fs::metadata(&part_path).await.map(|m| m.len()).unwrap_or(0);

    let mut request = client.get(file.url);
    if already_downloaded > 0 {
        request = request.header(reqwest::header::RANGE, format!("bytes={already_downloaded}-"));
    }
    let resp = request.send().await.map_err(|e| e.to_string())?;

    if resp.status() == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
        // The existing .part is already >= what the server now reports
        // as the full file -- it shrank remotely, or the partial is
        // stale/corrupt either way. Discard it rather than risk writing
        // a mismatched file; the next download attempt starts clean.
        tokio::fs::remove_file(&part_path).await.ok();
        return Err(
            "the partially-downloaded file no longer matches what the server has; removed it, try downloading again"
                .into(),
        );
    }
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    // A 206 confirms the server actually honored the Range request --
    // a plain 200 here (still "successful" by is_success()'s definition)
    // means it ignored our resume attempt and is sending the whole file
    // from byte 0 again, so appending to the existing partial would
    // corrupt it. Not every static host supports Range, so this has to
    // be checked rather than assumed.
    let resuming = already_downloaded > 0 && resp.status() == reqwest::StatusCode::PARTIAL_CONTENT;
    if already_downloaded > 0 && !resuming {
        eprintln!(
            "[models] server didn't resume '{}' as requested (replied HTTP {} instead of 206); restarting this file from scratch.",
            file.filename,
            resp.status()
        );
    }

    let mut out = if resuming {
        tokio::fs::OpenOptions::new().append(true).open(&part_path).await.map_err(|e| e.to_string())?
    } else {
        tokio::fs::File::create(&part_path).await.map_err(|e| e.to_string())?
    };
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        out.write_all(&chunk).await.map_err(|e| e.to_string())?;
    }
    out.flush().await.map_err(|e| e.to_string())?;
    drop(out);
    let final_path = dir.join(file.filename);
    tokio::fs::rename(&part_path, &final_path).await.map_err(|e| e.to_string())?;
    Ok(())
}

/// Removes a downloaded option's files to free disk space -- the "Manage"
/// view's delete button. Refuses for a builtin option (nothing to delete)
/// and silently no-ops on a file that's already gone.
pub fn delete_option(app: &AppHandle, option_id: &str) -> Result<(), String> {
    let option = find_option(option_id).ok_or_else(|| format!("unknown model option: {option_id}"))?;
    if option.builtin {
        return Err("This is bundled with the app and can't be deleted.".into());
    }
    let dir = models_dir(app);
    for f in option.files {
        let path = dir.join(f.filename);
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod manual_tests {
    use super::*;

    /// Not run in normal `cargo test` (hits the real network every time).
    /// Set MWT_RUN_DOWNLOAD_RESUME_TEST=1 to run both tests below.
    ///
    /// Small, stable, already-verified-working URL (Baberu OCR's
    /// vocab.json, ~130KB) used as the test fixture -- big enough to
    /// meaningfully split into "already downloaded" vs "remaining" and
    /// small enough that re-fetching it a few times per test run is
    /// fast and polite to the host.
    fn test_file() -> ModelFile {
        ModelFile {
            filename: "test-resume-fixture.bin",
            url: "https://huggingface.co/genshiai-daichi/baberu-ocr/resolve/b9868433dbda3ed9ba1357d008599ff4ada873d5/tokenizer/vocab.json",
            approx_bytes: 130_761,
        }
    }

    fn should_run() -> bool {
        std::env::var("MWT_RUN_DOWNLOAD_RESUME_TEST").as_deref() == Ok("1")
    }

    /// Writing a *correct* prefix into `{filename}.part` before calling
    /// `download_one` should resume cleanly and produce a file
    /// byte-identical to a full fresh download.
    #[tokio::test]
    async fn resume_with_correct_partial_produces_correct_file() {
        if !should_run() {
            eprintln!("skipping: set MWT_RUN_DOWNLOAD_RESUME_TEST=1 to run this (hits the real network)");
            return;
        }
        let file = test_file();
        let client = reqwest::Client::new();

        let full = client.get(file.url).send().await.expect("fetch reference").bytes().await.expect("read reference");

        let dir = std::env::temp_dir().join(format!("mwt_resume_test_correct_{}", std::process::id()));
        tokio::fs::create_dir_all(&dir).await.expect("create temp dir");
        let part_path = dir.join(format!("{}.part", file.filename));
        tokio::fs::write(&part_path, &full[..full.len() / 2]).await.expect("write fake partial");

        download_one(&client, &file, &dir).await.expect("download_one with existing correct partial");

        let result = tokio::fs::read(dir.join(file.filename)).await.expect("read final file");
        assert_eq!(result, full.to_vec(), "resumed file does not match a full fresh download");

        tokio::fs::remove_dir_all(&dir).await.ok();
    }

    /// Writing a *wrong* prefix (all zero bytes, same length as a real
    /// prefix would be) into `{filename}.part` and getting back a
    /// corrupted result proves `download_one` really appended to the
    /// existing partial via a Range request, rather than silently
    /// discarding it and restarting clean -- if it had restarted, the
    /// result would be correct regardless of what garbage was in the
    /// stale `.part`, which would make this assertion fail.
    #[tokio::test]
    async fn resume_appends_to_existing_partial_rather_than_discarding_it() {
        if !should_run() {
            eprintln!("skipping: set MWT_RUN_DOWNLOAD_RESUME_TEST=1 to run this (hits the real network)");
            return;
        }
        let file = test_file();
        let client = reqwest::Client::new();

        let full = client.get(file.url).send().await.expect("fetch reference").bytes().await.expect("read reference");

        let dir = std::env::temp_dir().join(format!("mwt_resume_test_wrong_{}", std::process::id()));
        tokio::fs::create_dir_all(&dir).await.expect("create temp dir");
        let part_path = dir.join(format!("{}.part", file.filename));
        let fake_prefix = vec![0u8; full.len() / 2];
        tokio::fs::write(&part_path, &fake_prefix).await.expect("write fake wrong partial");

        download_one(&client, &file, &dir).await.expect("download_one with existing wrong partial");

        let result = tokio::fs::read(dir.join(file.filename)).await.expect("read final file");
        assert_ne!(
            result,
            full.to_vec(),
            "expected the bogus partial's bytes to still be in the result (proving resume actually \
             appended instead of silently restarting) -- got a byte-correct file instead"
        );
        assert_eq!(&result[..fake_prefix.len()], &fake_prefix[..], "expected the bogus prefix to survive untouched");

        tokio::fs::remove_dir_all(&dir).await.ok();
    }
}
