#!/usr/bin/env python3
"""
Manga Web Translator - Google Colab GPU Backend Server
Implements the exact API contract expected by the Manga Web Translator browser extension:
  - POST   /api/jobs          (Creates translation job)
  - GET    /api/jobs/{id}     (Polls job status)
  - DELETE /api/jobs/{id}     (Cancels job)
  - GET    /cache/{filename}  (Serves translated images)
  - GET    /healthz           (Health check)
  - GET    /api/resources     (Reports CPU, RAM, and GPU VRAM telemetry)
  - POST   /api/pipeline/unload (Reclaims GPU memory)
"""

from __future__ import annotations
import os

# Desabilita multi-threading paralelo em OpenCV e OpenMP no nível C/C++ para evitar
# corrupção de heap (free(): corrupted unsorted chunks) ao rodar com PyTorch CUDA
os.environ["OMP_NUM_THREADS"] = "1"
os.environ["MKL_NUM_THREADS"] = "1"
os.environ["OPENBLAS_NUM_THREADS"] = "1"
os.environ["VECLIB_MAXIMUM_THREADS"] = "1"
os.environ["NUMEXPR_NUM_THREADS"] = "1"

try:
    import cv2
    cv2.setNumThreads(0)
    if hasattr(cv2, "ocl"):
        cv2.ocl.setUseOpenCL(False)
except Exception:
    pass

import sys
import uuid
import base64
import time
import datetime
import asyncio
import threading
import traceback
import psutil
import torch
import shutil
import urllib.request
from pathlib import Path
from typing import Optional, Dict, Any, List
from pydantic import BaseModel
from fastapi import FastAPI, HTTPException, BackgroundTasks, Response
from fastapi.responses import FileResponse
from fastapi.middleware.cors import CORSMiddleware
from PIL import Image

# Adiciona o diretório do manga-image-translator ao sys.path para importação direta
mit_dir = Path("./manga-image-translator").resolve()
if mit_dir.exists() and str(mit_dir) not in sys.path:
    sys.path.insert(0, str(mit_dir))
elif Path("/content/manga-image-translator").exists() and "/content/manga-image-translator" not in sys.path:
    sys.path.insert(0, "/content/manga-image-translator")

try:
    from manga_translator import MangaTranslator, Config
    from manga_translator.config import Translator, Renderer
except ImportError:
    MangaTranslator = None
    Config = None
    Translator = None
    Renderer = None

def google_translate_batch(texts: List[str], target_lang: str = "pt", source_lang: str = "auto") -> List[str]:
    """
    Traduz uma lista de textos de mangá utilizando a API direta do Google Translate.
    - Zero download de modelos pesados (elimina o download de 1.09GB do M2M100).
    - Evita colisões de concorrência e corrupção de hash em requisições paralelas.
    - Resposta ultra-rápida (~150-250ms total) com excelente qualidade para Português.
    """
    if not texts:
        return []

    lang_map = {
        "PTB": "pt", "PT": "pt", "ENG": "en", "EN": "en", "ESP": "es",
        "ES": "es", "FRA": "fr", "FR": "fr", "DEU": "de", "DE": "de",
        "ITA": "it", "IT": "it", "RUS": "ru", "RU": "ru", "CHS": "zh-CN",
        "CHT": "zh-TW", "JPN": "ja", "JA": "ja", "KOR": "ko", "KO": "ko"
    }
    tl = lang_map.get(str(target_lang).upper(), str(target_lang).lower())
    sl = lang_map.get(str(source_lang).upper(), "auto")

    valid_indices = [i for i, t in enumerate(texts) if t and t.strip()]
    if not valid_indices:
        return list(texts)

    valid_texts = [texts[i].strip() for i in valid_indices]
    delimiter = "\n<<<MWT_SEP>>>\n"
    combined = delimiter.join(valid_texts)

    try:
        url = f"https://translate.googleapis.com/translate_a/single?client=gtx&sl={sl}&tl={tl}&dt=t&q={urllib.parse.quote(combined)}"
        req = urllib.request.Request(url, headers={"User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64)"})
        with urllib.request.urlopen(req, timeout=12) as resp:
            data = json.loads(resp.read().decode("utf-8"))
            translated_combined = "".join([seg[0] for seg in data[0] if seg and seg[0]])
            parts = [p.strip() for p in translated_combined.split("<<<MWT_SEP>>>")]
            if len(parts) == len(valid_texts):
                res = list(texts)
                for idx, translated_part in zip(valid_indices, parts):
                    res[idx] = translated_part
                return res
    except Exception as e:
        print(f"  [Tradução] Aviso no lote: {e}. Executando tradução individual...")

    # Fallback robusto item a item se o delimitador for alterado pela pontuação
    res = list(texts)
    for idx in valid_indices:
        try:
            q = urllib.parse.quote(texts[idx].strip())
            url = f"https://translate.googleapis.com/translate_a/single?client=gtx&sl={sl}&tl={tl}&dt=t&q={q}"
            req = urllib.request.Request(url, headers={"User-Agent": "Mozilla/5.0"})
            with urllib.request.urlopen(req, timeout=8) as resp:
                data = json.loads(resp.read().decode("utf-8"))
                res[idx] = "".join([seg[0] for seg in data[0] if seg and seg[0]]).strip()
        except Exception:
            pass
    return res

_PIPELINE_INSTRUMENTED = False

def instrument_declarative_pipeline():
    """
    Instrumenta a pipeline do manga_translator para relatar de forma declarativa
    cada etapa (Detecção, OCR, Tradução, Inpainting, Typesetting) no console da GPU.
    """
    global _PIPELINE_INSTRUMENTED
    if _PIPELINE_INSTRUMENTED:
        return
    try:
        import manga_translator.manga_translator as mt_mod
        
        orig_detect = getattr(mt_mod, "dispatch_detection", None)
        orig_ocr = getattr(mt_mod, "dispatch_ocr", None)
        orig_trans = getattr(mt_mod, "dispatch_translation", None)
        orig_inpaint = getattr(mt_mod, "dispatch_inpainting", None)
        orig_render = getattr(mt_mod, "dispatch_rendering", None)

        if orig_detect:
            async def wrapped_detect(*args, **kwargs):
                t_start = time.time()
                ts = datetime.datetime.now().strftime("%H:%M:%S.%f")[:-3]
                print(f"  [1/5 Detecção] 🔍 [{ts}] Localizando balões e regiões de texto...")
                res = await orig_detect(*args, **kwargs)
                dt = (time.time() - t_start) * 1000
                count = len(res) if isinstance(res, (list, tuple)) else "?"
                print(f"  [1/5 Detecção] ✅ Concluída em {dt:.0f}ms ({count} blocos detectados)")
                return res
            mt_mod.dispatch_detection = wrapped_detect

        if orig_ocr:
            async def wrapped_ocr(*args, **kwargs):
                t_start = time.time()
                ts = datetime.datetime.now().strftime("%H:%M:%S.%f")[:-3]
                print(f"  [2/5 OCR] 🔤 [{ts}] Extraindo texto dos balões (reconhecimento óptico)...")
                res = await orig_ocr(*args, **kwargs)
                dt = (time.time() - t_start) * 1000
                print(f"  [2/5 OCR] ✅ Concluído em {dt:.0f}ms")
                return res
            mt_mod.dispatch_ocr = wrapped_ocr

        if orig_trans:
            async def wrapped_trans(*args, **kwargs):
                t_start = time.time()
                ts = datetime.datetime.now().strftime("%H:%M:%S.%f")[:-3]
                queries = args[1] if len(args) > 1 else kwargs.get("queries", [])
                chain = args[0] if len(args) > 0 else kwargs.get("chain", None)
                trans_cfg = args[2] if len(args) > 2 else kwargs.get("translator_config", None)
                tgt = "PTB"
                if trans_cfg and hasattr(trans_cfg, "target_lang"):
                    tgt = trans_cfg.target_lang
                elif chain and hasattr(chain, "target_lang"):
                    tgt = chain.target_lang or "PTB"

                print(f"  [3/5 Tradução] 🧠 [{ts}] Traduzindo {len(queries)} blocos de texto (JA -> {tgt}) com IA neural na GPU (M2M100 1.2B / CTranslate2)...")
                try:
                    # Executa inferência do modelo neural local na GPU (CUDA)
                    res = await orig_trans(*args, **kwargs)
                    dt = (time.time() - t_start) * 1000
                    sample = res[0] if res else ""
                    if len(sample) > 40:
                        sample = sample[:37] + "..."
                    print(f"  [3/5 Tradução] ✅ Concluída em {dt:.0f}ms ({len(res)} blocos | IA: \"{sample}\")")
                    return res
                except Exception as e:
                    print(f"  [3/5 Tradução] ⚠️ Exceção no modelo neural ({e}). Ativando fallback de contingência...")
                    translated = await asyncio.to_thread(google_translate_batch, queries, tgt)
                    dt = (time.time() - t_start) * 1000
                    print(f"  [3/5 Tradução] ✅ Fallback concluído em {dt:.0f}ms")
                    return translated
            mt_mod.dispatch_translation = wrapped_trans

        if orig_inpaint:
            async def wrapped_inpaint(*args, **kwargs):
                t_start = time.time()
                ts = datetime.datetime.now().strftime("%H:%M:%S.%f")[:-3]
                print(f"  [4/5 Inpainting] 🎨 [{ts}] Removendo texto original e restaurando balões (LaMa)...")
                res = await orig_inpaint(*args, **kwargs)
                dt = (time.time() - t_start) * 1000
                print(f"  [4/5 Inpainting] ✅ Concluído em {dt:.0f}ms")
                return res
            mt_mod.dispatch_inpainting = wrapped_inpaint

        if orig_render:
            async def wrapped_render(*args, **kwargs):
                t_start = time.time()
                ts = datetime.datetime.now().strftime("%H:%M:%S.%f")[:-3]
                print(f"  [5/5 Typesetting] ✍️ [{ts}] Renderizando lettering nos balões...")
                res = await orig_render(*args, **kwargs)
                dt = (time.time() - t_start) * 1000
                print(f"  [5/5 Typesetting] ✅ Concluído em {dt:.0f}ms")
                return res
            mt_mod.dispatch_rendering = wrapped_render

        _PIPELINE_INSTRUMENTED = True
        print("⚡ Pipeline declarativa da GPU (Detecção -> OCR -> Tradução -> Inpainting -> Typesetting) ativada com sucesso!")
    except Exception as e:
        print(f"⚠️ Aviso: Não foi possível instrumentar pipeline declarativa: {e}")

app = FastAPI(title="Manga Web Translator Colab Server")

# Allow CORS for browser extension and external clients
app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

CACHE_DIR = Path("./cache").resolve()
CACHE_DIR.mkdir(parents=True, exist_ok=True)

# In-memory job store
jobs_db: Dict[str, Dict[str, Any]] = {}

# Concurrency permit: strictly 1 page at a time on GPU to eliminate C++ heap corruption and glibc free() crashes
GPU_SEMAPHORE = asyncio.Semaphore(1)
_GPU_EXECUTION_LOCK = threading.Lock()

TRANSLATOR_INSTANCE = None

def get_font_for_lang(lang_code: str) -> Optional[str]:
    # 1. Prioridade máxima: Wild Words (fonte canônica oficial do projeto)
    wild_candidates = [
        Path("/content/fonts/wildwords.ttf"),
        Path("/content/fonts/WildWords.ttf"),
        Path("/content/fonts/CCWildWords.ttf"),
        Path("/content/fonts/cc_wild_words.ttf"),
        Path("./fonts/wildwords.ttf"),
        Path("./wildwords.ttf"),
    ]
    for w in wild_candidates:
        if w.exists():
            return str(w.resolve())

    # 2. Fallbacks de lettering de quadrinhos/mangá
    candidates = [
        Path("/content/fonts/CCWildWords.ttf"),
        Path("/content/fonts/wildwords.ttf"),
        Path("/content/manga-image-translator/fonts/anime_ace_3.ttf"),
        Path("./manga-image-translator/fonts/anime_ace_3.ttf"),
        Path("/content/fonts/anime_ace_3.ttf"),
        Path("/content/fonts/comic_neue_bold.ttf"),
        Path("./fonts/comic_neue_bold.ttf"),
        Path("/content/manga-image-translator/fonts/anime_ace.ttf"),
        Path("./manga-image-translator/fonts/anime_ace.ttf"),
        Path("/content/manga-image-translator/fonts/comic shanns 2.ttf"),
        Path("./manga-image-translator/fonts/comic shanns 2.ttf"),
    ]
    for c in candidates:
        if c.exists():
            return str(c.resolve())
    return None

def get_translator():
    global TRANSLATOR_INSTANCE
    if TRANSLATOR_INSTANCE is None:
        if MangaTranslator is None:
            raise RuntimeError("MangaTranslator não encontrado. Verifique se o repositório manga-image-translator foi clonado no Colab.")
        instrument_declarative_pipeline()
        try:
            import manga_translator.utils.generic as generic_mod
            generic_mod.prompt_yes_no = lambda *args, **kwargs: True
        except Exception:
            pass
        try:
            for z in Path("/tmp").glob("**/*m2m100*.zip"):
                z.unlink(missing_ok=True)
            for z in Path("/content/manga-image-translator/models/translators/m2m_100").glob("*.zip"):
                z.unlink(missing_ok=True)
        except Exception:
            pass
        device_gpu = torch.cuda.is_available()
        print(f"🧠 Inicializando motor MangaTranslator na GPU (CUDA={device_gpu})...")
        init_font = get_font_for_lang("ENG")
        TRANSLATOR_INSTANCE = MangaTranslator({
            "use_gpu": device_gpu,
            "verbose": False,
            "kernel_size": 3,
            "models_ttl": 0,
            "batch_size": 4,
            "font_path": init_font
        })
        TRANSLATOR_INSTANCE.ignore_errors = True
        if init_font:
            TRANSLATOR_INSTANCE.font_path = init_font
            print(f"🔤 Fonte Manga inicializada: {init_font}")
    return TRANSLATOR_INSTANCE

def get_gpu_info():
    if torch.cuda.is_available():
        name = torch.cuda.get_device_name(0)
        total_vram = torch.cuda.get_device_properties(0).total_memory // (1024 * 1024)
        used_vram = torch.cuda.memory_allocated(0) // (1024 * 1024)
        return name, used_vram, total_vram
    return "CPU (GPU Unavailable)", 0, 0

class CreateJobRequest(BaseModel):
    imageUrl: Optional[str] = None
    imageBase64: Optional[str] = None
    mimeType: Optional[str] = None
    pageUrl: Optional[str] = None
    targetLang: Optional[str] = "pt"

def map_language(lang: str) -> str:
    l = (lang or "").lower().strip()
    if l in ["pt", "pt-br", "pt_br", "portuguese"]:
        return "PTB"
    if l in ["en", "en-us", "en_us", "english"]:
        return "ENG"
    if l in ["es", "es-es", "es_es", "spanish"]:
        return "ESP"
    if l in ["ja", "jp", "japanese"]:
        return "JPN"
    if l in ["ko", "kor", "korean"]:
        return "KOR"
    if l in ["zh", "zh-cn", "chinese"]:
        return "CHS"
    return "PTB"

def download_image_with_headers(url: str, referer: Optional[str] = None) -> bytes:
    req = urllib.request.Request(
        url,
        headers={
            "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
            "Referer": referer or url,
            "Accept": "image/avif,image/webp,image/apng,image/svg+xml,image/*,*/*;q=0.8",
        }
    )
    with urllib.request.urlopen(req, timeout=25) as resp:
        return resp.read()

async def process_translation(
    job_id: str,
    image_base64: Optional[str],
    image_url: Optional[str],
    page_url: Optional[str],
    target_lang: str
):
    input_file = CACHE_DIR / f"{job_id}_input.png"
    final_output = CACHE_DIR / f"{job_id}.png"

    try:
        # Step 1: Obtain image bytes
        if image_base64:
            try:
                data = base64.b64decode(image_base64)
                with open(input_file, "wb") as f:
                    f.write(data)
            except Exception as e:
                jobs_db[job_id]["status"] = "failed"
                jobs_db[job_id]["errorCode"] = "IMAGE_DECODE_FAILED"
                jobs_db[job_id]["errorMessage"] = f"Base64 decode failed: {e}"
                return
        elif image_url:
            jobs_db[job_id]["progressMessage"] = "Baixando imagem..."
            try:
                data = await asyncio.to_thread(download_image_with_headers, image_url, page_url)
                with open(input_file, "wb") as f:
                    f.write(data)
            except Exception as e:
                print(f"[{job_id}] Download direto falhou ({e}). Notificando extensão para enviar base64...")
                jobs_db[job_id]["status"] = "failed"
                jobs_db[job_id]["errorCode"] = "IMAGE_FETCH_FAILED"
                jobs_db[job_id]["errorMessage"] = f"Could not fetch source image: {e}"
                return
        else:
            jobs_db[job_id]["status"] = "failed"
            jobs_db[job_id]["errorCode"] = "IMAGE_FETCH_FAILED"
            jobs_db[job_id]["errorMessage"] = "No image source provided"
            return

        if jobs_db[job_id].get("cancelled"):
            jobs_db[job_id]["status"] = "failed"
            jobs_db[job_id]["errorCode"] = "CANCELLED"
            jobs_db[job_id]["errorMessage"] = "Cancelado pelo usuário"
            return

        # Step 2: Queue for GPU
        jobs_db[job_id]["progressMessage"] = "Na fila da GPU..."
        jobs_db[job_id]["progressPercent"] = 20

        async with GPU_SEMAPHORE:
            if jobs_db[job_id].get("cancelled"):
                jobs_db[job_id]["status"] = "failed"
                jobs_db[job_id]["errorCode"] = "CANCELLED"
                return

            jobs_db[job_id]["status"] = "processing"
            jobs_db[job_id]["progressPercent"] = 40
            jobs_db[job_id]["progressMessage"] = "Traduzindo na GPU do Colab..."

            lang_code = map_language(target_lang)
            gpu_name, _, _ = get_gpu_info()
            start_ts = datetime.datetime.now().strftime("%H:%M:%S.%f")[:-3]

            print(f"[{job_id}] ⏳ INÍCIO: {start_ts} | Processando na GPU ({gpu_name}) -> Idioma: {lang_code}...")
            t0 = time.time()

            # Executa a tradução na GPU em uma thread separada para não travar o event loop do FastAPI
            img = Image.open(input_file).convert("RGB")
            cfg = Config()
            cfg.translator.target_lang = lang_code

            # Seleção de tradutor:
            # Seleção de tradutor neural offline (CTranslate2 na GPU):
            # Prioriza o modelo neural M2M100 1.2B pré-carregado no Passo 2 na VRAM da A100
            if hasattr(Translator, "m2m100_big"):
                cfg.translator.translator = Translator.m2m100_big
            elif hasattr(Translator, "offline"):
                cfg.translator.translator = Translator.offline
            elif lang_code == "ENG" and hasattr(Translator, "sugoi"):
                cfg.translator.translator = Translator.sugoi

            # Tipografia inteligente: ajusta o texto perfeitamente aos balões sem estourar as margens
            if Renderer and hasattr(Renderer, "manga2Eng"):
                cfg.render.renderer = Renderer.manga2Eng
            cfg.render.font_size_offset = -1
            cfg.render.font_size_minimum = 7
            cfg.render.line_spacing = 1

            font_path = get_font_for_lang(lang_code)

            def _do_translate():
                with _GPU_EXECUTION_LOCK:
                    worker_loop = asyncio.new_event_loop()
                    asyncio.set_event_loop(worker_loop)
                    try:
                        t = get_translator()
                        if font_path:
                            t.font_path = font_path
                        try:
                            ctx = worker_loop.run_until_complete(t.translate(img, cfg))
                        except Exception as render_err:
                            print(f"[{job_id}] Aviso no motor ({render_err}). Usando renderizador padrão...")
                            if Renderer and hasattr(Renderer, "default"):
                                cfg.render.renderer = Renderer.default
                            ctx = worker_loop.run_until_complete(t.translate(img, cfg))

                        # Verificação de segurança: garante que os balões foram de fato traduzidos
                        if ctx and hasattr(ctx, "text_regions") and ctx.text_regions:
                            has_translation = any(bool(getattr(r, "translation", None)) for r in ctx.text_regions)
                            if not has_translation:
                                print(f"[{job_id}] ⚠️ Aviso: {len(ctx.text_regions)} balões detectados mas sem tradução. Forçando tradução direta...")
                                texts_to_trans = [r.text for r in ctx.text_regions]
                                direct_trans = google_translate_batch(texts_to_trans, lang_code)
                                for r, t_text in zip(ctx.text_regions, direct_trans):
                                    r.translation = t_text
                                    r.target_lang = lang_code
                                if hasattr(t, "_run_text_rendering"):
                                    ctx.img_rendered = worker_loop.run_until_complete(t._run_text_rendering(cfg, ctx))
                                    from manga_translator.utils import dump_image
                                    ctx.result = dump_image(ctx.input, ctx.img_rendered, ctx.img_alpha)

                        return ctx
                    finally:
                        worker_loop.close()
                        if torch.cuda.is_available():
                            try:
                                torch.cuda.empty_cache()
                            except Exception:
                                pass

            ctx = await asyncio.to_thread(_do_translate)

            if jobs_db[job_id].get("cancelled"):
                jobs_db[job_id]["status"] = "failed"
                jobs_db[job_id]["errorCode"] = "CANCELLED"
                return

            if ctx and ctx.result:
                import io
                buf = io.BytesIO()
                # Otimização de rede: salva em WebP (qualidade 90) para reduzir o payload
                # de ~5-6MB para ~250-350KB. Isso elimina a latência de ~20s de download
                # pelo túnel Cloudflare e é suportado nativamente pelo navegador.
                img_format = "WEBP"
                mime = "image/webp"
                try:
                    ctx.result.convert("RGB").save(buf, format="WEBP", quality=90)
                except Exception:
                    buf.seek(0)
                    buf.truncate(0)
                    ctx.result.save(buf, format="PNG")
                    img_format = "PNG"
                    mime = "image/png"

                raw_bytes = buf.getvalue()
                size_kb = len(raw_bytes) / 1024
                b64 = base64.b64encode(raw_bytes).decode("utf-8")
                data_url = f"data:{mime};base64,{b64}"

                ctx.result.save(str(final_output))
                elapsed = time.time() - t0
                end_ts = datetime.datetime.now().strftime("%H:%M:%S.%f")[:-3]
                print(f"[{job_id}] 🏁 FIM: {end_ts} (Duração GPU: {elapsed:.2f}s | Payload {img_format}: {size_kb:.1f} KB) -> {final_output.name}")

                jobs_db[job_id]["status"] = "succeeded"
                jobs_db[job_id]["translatedImageUrl"] = data_url
                jobs_db[job_id]["engineUsed"] = f"Colab ({gpu_name})"
                jobs_db[job_id]["progressPercent"] = 100
                jobs_db[job_id]["progressMessage"] = "Concluído"
                jobs_db[job_id]["timing"] = {
                    "start": start_ts,
                    "end": end_ts,
                    "gpuElapsedSeconds": round(elapsed, 2),
                    "imageSizeKb": round(size_kb, 1)
                }
            else:
                raise RuntimeError("Motor de IA não produziu imagem de resultado.")

    except Exception as e:
        print(f"[{job_id}] Erro na tradução: {e}")
        traceback.print_exc()
        jobs_db[job_id]["status"] = "failed"
        jobs_db[job_id]["errorCode"] = "TRANSLATION_FAILED"
        jobs_db[job_id]["errorMessage"] = f"Erro GPU: {str(e)[:120]}"

@app.post("/api/jobs")
async def create_job(req: CreateJobRequest, background_tasks: BackgroundTasks):
    job_id = str(uuid.uuid4())
    req_ts = datetime.datetime.now().strftime("%H:%M:%S.%f")[:-3]
    print(f"[{job_id}] 📥 [{req_ts}] Nova requisicao recebida da extensao (targetLang={req.targetLang})")

    jobs_db[job_id] = {
        "jobId": job_id,
        "status": "queued",
        "progressPercent": 10,
        "progressMessage": "Na fila do servidor...",
        "translatedImageUrl": None,
        "engineUsed": None,
        "errorCode": None,
        "errorMessage": None,
        "cancelled": False
    }

    background_tasks.add_task(
        process_translation,
        job_id,
        req.imageBase64,
        req.imageUrl,
        req.pageUrl,
        req.targetLang or "pt"
    )

    return {"jobId": job_id, "status": "queued"}

@app.get("/api/jobs/{job_id}")
async def get_job(job_id: str):
    job = jobs_db.get(job_id)
    if not job:
        raise HTTPException(status_code=404, detail="Job not found")
    return job

@app.delete("/api/jobs/{job_id}")
async def cancel_job(job_id: str):
    if job_id in jobs_db:
        jobs_db[job_id]["cancelled"] = True
        jobs_db[job_id]["status"] = "failed"
        jobs_db[job_id]["errorCode"] = "CANCELLED"
    return Response(status_code=204)

@app.get("/cache/{filename}")
async def get_cache(filename: str):
    p = CACHE_DIR / filename
    if not p.exists():
        raise HTTPException(status_code=404, detail="Not found")
    return FileResponse(
        str(p),
        media_type="image/png",
        headers={"Access-Control-Allow-Origin": "*", "Cache-Control": "public, max-age=86400"}
    )

@app.post("/api/pipeline/unload")
@app.post("/api/resources/unload")
async def unload_pipeline():
    if torch.cuda.is_available():
        torch.cuda.empty_cache()
    return {"status": "unloaded"}

@app.get("/healthz")
async def healthz():
    return {"status": "ok"}

@app.get("/api/resources")
async def get_resources():
    gpu_name, vram_used, vram_total = get_gpu_info()
    v_mem = psutil.virtual_memory()
    total_ram_mb = v_mem.total // (1024 * 1024)
    used_ram_mb = v_mem.used // (1024 * 1024)

    return {
        "cpuPercent": psutil.cpu_percent(interval=None),
        "systemCpuPercent": psutil.cpu_percent(interval=None),
        "usedMemoryMb": used_ram_mb,
        "processMemoryMb": used_ram_mb,
        "systemUsedMemoryMb": used_ram_mb,
        "systemTotalMemoryMb": total_ram_mb,
        "gpuName": gpu_name,
        "vramUsedMb": vram_used,
        "vramTotalMb": vram_total
    }

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8000)
