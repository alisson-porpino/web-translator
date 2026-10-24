# Roadmap: opções adicionais de modelos (paridade com Koharu)

## Objetivo

Preencher o catálogo agnóstico (ver `catalogo-modelos.md`) com as
alternativas reais que o Koharu oferece em cada categoria. Esta é a "outra
parte" mencionada explicitamente pelo Alisson, feita depois do limitador
de CPU/RAM (concluído — ver `limite-recursos.md`) e do threshold de
detecção (concluído — ver `pipeline-local-traducao.md`).

**Status: em andamento.** Tradução: 12 de 27 modelos do Koharu já
implementados — **toda a família Gemma 4 completa** (E2B, E4B, 12B, 26B
A4B, 31B, oficial + uncensored cada), mais LFM2.5 1.2B e Ministral 3 8B.
Seguindo a ordem da tabela, por pedido explícito do Alisson (2026-10-20:
"Pode seguir a ordem da tabela"). Restam só os Qwen (3.5/3.6/3.8, 15
modelos). OCR: 2 de 4 motores do Koharu implementados (**PaddleOCR-VL
1.6**, agora o padrão — 2026-10-20; **Baberu OCR** — 2026-10-21). Hayai
OCR fica de lado por ora (2026-10-21, decisão do Alisson) — complexidade
bem maior, ver seção própria abaixo se for retomado.

## Fonte oficial da lista do Koharu

A lista abaixo não é mais "o que o Alisson descreveu de memória" — foi
confirmada direto no código-fonte real do Koharu
(`github.com/mayocream/koharu`, licença Apache-2.0/MIT dual):
`crates/koharu-translator/src/local/catalog.rs` lista os modelos de
tradução locais com repositório HF, revisão fixada e nome exato do arquivo
GGUF. Reconsultar esse arquivo (via `gh api repos/mayocream/koharu/...`)
antes de cadastrar qualquer modelo novo — não confiar em resumos de busca
na web para isso, só na fonte.

### Detecção

- **Koharu Layout RF-DETR Seg 2XL** — **avaliado e descartado em
  2026-10-21**. Levantado em detalhe
  (`mayocream/koharu-layout-rfdetr-seg-2xl-1152`), três problemas reais,
  não só "achar o export leve":
  1. **Licença incerta**: os pesos foram treinados com imagens do
     Manga109, que tem termos de **uso acadêmico apenas** — o próprio
     model card diz: *"Users are responsible for obtaining Manga109 and
     complying with its terms... This repository does not redistribute
     Manga109 images."* O código RF-DETR em si é Apache-2.0, mas os
     pesos carregam essa obrigação do dado de treino — tag de licença no
     HF é `other`, sem SPDX definido. Bem mais incerto que qualquer
     modelo já integrado neste projeto (todos Apache-2.0 ou licença
     comercial clara tipo Gemma Terms of Use).
  2. **Sem export leve**: só `model.safetensors` — sem ONNX, sem GGUF.
     Rodar exigiria libtorch (contra `DIRETRIZES.md` Princípio 1) ou uma
     conversão pra ONNX feita por nós mesmos (arriscado, sem garantia de
     fidelidade).
  3. **Capacidade que não aproveitaríamos mesmo assim**: esse modelo faz
     segmentação de instância (máscaras, não só caixas) em 4 classes
     (text/onomatopoeia/bubble/panel) a 1152x1152 — mas nosso pipeline
     (`detect.rs` → recorta região → OCR/traduz/pinta) só usa caixas
     delimitadoras hoje. Aproveitar máscaras de verdade exigiria mudar
     `compose.rs`/`inpaint.rs` também, não é só trocar o detector.

  **Decisão do Alisson**: manter o detector atual
  (`ogkalu/comic-text-and-bubble-detector`, Apache-2.0, bundlado,
  3 classes, 640x640 — ver `pipeline-local-traducao.md`). Item fechado,
  não retomar sem uma mudança real de contexto (ex: surgir um export
  leve com licença clara).

### OCR

- **PaddleOCR-VL 1.6** — **feito** (id `paddle-ocr-vl-1.6`, agora o
  padrão). Ver seção "Modelos de OCR já implementados" abaixo.
- **Baberu OCR** — **feito** (id `baberu-ocr`). Ver seção "Modelos de OCR
  já implementados" abaixo.
- **manga-ocr** — já tínhamos (`kha-white/manga-ocr-base`, Apache-2.0,
  japonês apenas, não é mais o padrão).
- **Hayai OCR** — levantado em detalhe (2026-10-21), **ainda não
  implementado** por ser bem mais complexo que os outros três. O Koharu
  roda seus motores de visão via libtorch (crate própria `koharu-torch`,
  reimplementação da arquitetura à mão em Rust) — a lição do PaddleOCR-VL
  e do Baberu OCR foi **procurar primeiro se existe export compatível com
  um runtime mais leve** (GGUF/llama.cpp ou ONNX) antes de ir pro caminho
  pesado do libtorch (ver `DIRETRIZES.md` Princípio 1). Pro Hayai existe
  um export ONNX oficial (`JustANormalTinkerer/hayai-ocr-v2-onnx`,
  Apache-2.0) — o runtime não é o problema. **O problema é o que o
  gráfico ONNX espera que o chamador já tenha pré-computado**, bem mais
  do que PaddleOCR-VL/Baberu exigiam:
  - Patchificação dinâmica: a imagem é redimensionada pra um grid de
    patches 16x16 que varia por imagem (busca binária pra caber em até
    256 patches preservando proporção), não um resize fixo 224x224 como
    manga-ocr/Baberu.
  - Embeddings de posição 2D: uma grade-base 16x16x768 pré-computada
    (`position_base.npy`) precisa ser reamostrada por bilinear
    interpolation pro tamanho real do grid de patches de cada imagem —
    768 canais, cada um reamostrado separadamente.
  - RoPE 2D customizado: tabelas seno/cosseno calculadas à mão por
    frequência, concatenando eixos y/x pros tokens visuais e RoPE 1D
    padrão (duplicado) pros tokens de texto, numa única tabela
    (1, 320, 32).
  - Máscara de atenção bloco-causal construída manualmente a cada passo
    de geração (visão vê toda visão, texto vê toda visão + causal entre
    si).

  Nada disso é código novo pro gráfico ONNX em si (ele só multiplica
  matrizes) — é lógica de pré/pós-processamento que normalmente o
  `transformers`/PyTorch faz por baixo dos panos, e que o script de
  referência do próprio autor (`inference_onnx.py`) implementa à mão em
  Python/numpy porque o gráfico ONNX não embute isso. Portar isso pra Rust
  com fidelidade é um esforço de implementação bem maior que qualquer
  motor já adicionado até agora — **perguntar ao Alisson antes de
  começar**, já que o custo de engenharia é claramente mais alto que o dos
  outros três OCRs.

### Inpainting/limpeza

- **LaMa** — já temos.
- **FLUX.2 Klein**, **RORem mixed**, **AOT GAN** — ainda não levantados em
  detalhe (formato, licença, viabilidade via ONNX/outro runtime).

### Tradução — lista completa confirmada no código do Koharu (27 modelos)

Todos via llama.cpp/mtmd (mesmo runtime que já usamos em `local_llm.rs`),
a maioria com projetor de visão (`mmproj-F16.gguf` ou equivalente) —
compatíveis com a mesma integração que já existe, sem código novo em
`local_llm.rs`, só um `ModelOption` + um braço de `match` em
`build_models()` cada. `DEFAULT_MODEL` do Koharu é `gemma4-12b-it` /
`Q4_K_XL`.

| id (Koharu) | Nome | Repositório HF | Quant. padrão | Visão | Status |
|---|---|---|---|---|---|
| `lfm2.5-1.2b-instruct` | LFM2.5 1.2B Instruct | `LiquidAI/LFM2.5-1.2B-Instruct-GGUF` | Q4_K_M | não | **feito** (id `lfm2.5-1.2b-instruct`) |
| `ministral-3-8b-instruct` | Ministral 3 8B Instruct | `mistralai/Ministral-3-8B-Instruct-2512-GGUF` | Q4_K_M | não | **feito** (id `ministral-3-8b-instruct`) |
| `gemma4-e2b-it` | Gemma 4 E2B Instruct | `unsloth/gemma-4-E2B-it-qat-GGUF` | Q4_K_XL | sim | **feito** (id `gemma-4-e2b-it`) |
| `gemma4-e4b-it` | Gemma 4 E4B Instruct | `unsloth/gemma-4-E4B-it-qat-GGUF` | Q4_K_XL | sim | **feito** (id `gemma-4-e4b-it`) |
| `gemma4-12b-it` | Gemma 4 12B Instruct | `unsloth/gemma-4-12B-it-qat-GGUF` | Q4_K_XL | sim | **feito** (id `gemma-4-12b-it`) |
| `gemma4-26b-a4b-it` | Gemma 4 26B A4B Instruct | `unsloth/gemma-4-26B-A4B-it-qat-GGUF` | Q4_K_XL | sim | **feito** (id `gemma-4-26b-a4b-it`) |
| `gemma4-31b-it` | Gemma 4 31B Instruct | `unsloth/gemma-4-31B-it-qat-GGUF` | Q4_K_XL | sim | **feito** (id `gemma-4-31b-it`) |
| `gemma4-e2b-uncensored` | Gemma 4 E2B Uncensored | `HauhauCS/Gemma-4-E2B-Uncensored-HauhauCS-Aggressive` | Q4_K_P | sim | **feito** (id `gemma-4-e2b-uncensored`) |
| `gemma4-e4b-uncensored` | Gemma 4 E4B Uncensored | `HauhauCS/Gemma-4-E4B-Uncensored-HauhauCS-Aggressive` | Q4_K_P | sim | **feito** (id `gemma-4-e4b-uncensored`) |
| `gemma4-12b-uncensored` | Gemma 4 12B Uncensored | `HauhauCS/Gemma4-12B-QAT-Uncensored-HauhauCS-Balanced` | Q4_K_M | sim | **feito** (id `gemma-4-12b-uncensored`) |
| `gemma4-26b-a4b-uncensored` | Gemma 4 26B A4B Uncensored | `HauhauCS/Gemma4-26B-A4B-QAT-Uncensored-HauhauCS-Balanced-MTP` | Q4_K_M | sim | **feito** (id `gemma-4-26b-a4b-uncensored`) |
| `gemma4-31b-uncensored` | Gemma 4 31B Uncensored | `HauhauCS/Gemma4-31B-QAT-Uncensored-HauhauCS-Balanced-MTP` | Q4_K_M | sim | **feito** (id `gemma-4-31b-uncensored`) |
| `qwen3.5-0.8b` | Qwen 3.5 0.8B | `unsloth/Qwen3.5-0.8B-GGUF` | Q4_K_XL | sim | pendente |
| `qwen3.5-2b` | Qwen 3.5 2B | `unsloth/Qwen3.5-2B-GGUF` | Q4_K_XL | sim | pendente |
| `qwen3.5-4b` | Qwen 3.5 4B | `unsloth/Qwen3.5-4B-GGUF` | Q4_K_XL | sim | pendente |
| `qwen3.5-9b` | Qwen 3.5 9B | `unsloth/Qwen3.5-9B-GGUF` | Q4_K_XL | sim | pendente |
| `qwen3.5-27b` | Qwen 3.5 27B | `unsloth/Qwen3.5-27B-GGUF` | Q4_K_XL | sim | pendente |
| `qwen3.5-35b-a3b` | Qwen 3.5 35B A3B | `unsloth/Qwen3.5-35B-A3B-GGUF` | Q4_K_XL | sim | pendente |
| `qwen3.6-27b` | Qwen 3.6 27B | `unsloth/Qwen3.6-27B-GGUF` | Q4_K_XL | sim | pendente |
| `qwen3.6-35b-a3b` | Qwen 3.6 35B A3B | `unsloth/Qwen3.6-35B-A3B-GGUF` | Q4_K_XL | sim | pendente |
| `qwen3.8-27b` | Qwen 3.8 27B | `unsloth/Qwen3.8-27B-GGUF` | Q4_K_XL | sim | pendente |
| `qwen3.5-2b-uncensored` | Qwen 3.5 2B Uncensored | `HauhauCS/Qwen3.5-2B-Uncensored-HauhauCS-Aggressive` | Q4_K_M | sim | pendente |
| `qwen3.5-4b-uncensored` | Qwen 3.5 4B Uncensored | `HauhauCS/Qwen3.5-4B-Uncensored-HauhauCS-Aggressive` | Q4_K_M | sim | pendente |
| `qwen3.5-9b-uncensored` | Qwen 3.5 9B Uncensored | `HauhauCS/Qwen3.5-9B-Uncensored-HauhauCS-Aggressive` | Q4_K_M | sim | pendente |
| `qwen3.6-27b-uncensored` | Qwen 3.6 27B Uncensored | `HauhauCS/Qwen3.6-27B-Uncensored-HauhauCS-Balanced` | Q4_K_P | sim | pendente |
| `qwen3.6-35b-a3b-uncensored` | Qwen 3.6 35B A3B Uncensored | `HauhauCS/Qwen3.6-35B-A3B-Uncensored-HauhauCS-Aggressive` | Q4_K_P | sim | pendente |
| `qwen3.8-27b-uncensored` | Qwen 3.8 27B Uncensored | `HauhauCS/Qwen3.8-27B-Uncensored-HauhauCS-Aggressive-MTP-GGUF` | Q4_K_P | sim | pendente |

\* `lfm2.5-1.2b-instruct` e `ministral-3-8b-instruct` não têm projetor de
visão no Koharu (`reasoning: false`, sem `mmproj`) — texto-apenas.
**Feito em 2026-10-20**: `LocalLlm` ganhou suporte a modelo sem visão
(`mtmd_ctx: Option<Mutex<MtmdContext>>`, `load(model_path, mmproj_path:
Option<&Path>)`) — quando `None`, `translate_batch` tokeniza e decodifica
o prompt manualmente (sem mtmd, sem a imagem da página) em vez de exigir
um projetor. Testado de verdade (não só compilado): baixei o LFM2.5
1.2B e rodei `translate_batch_text_only_smoke_test` contra ele.

**Nota sobre licença**: os modelos `unsloth/*` espelham a licença do modelo
base (Gemma: licença própria do Google "Gemma Terms of Use", permissiva
mas não OSI-approved, com política de uso proibido; Qwen: Apache-2.0 na
maioria dos tamanhos; Mistral/Ministral: licença própria da Mistral;
LiquidAI/LFM: licença própria). Os `HauhauCS/*` são finetunes comunitários
"uncensored" de proveniência/licença não verificada por nós — ver seção
"Cautela com os modelos 'uncensored'" abaixo antes de cadastrar qualquer
um deles.

**Nota importante sobre licenciamento neste contexto**: diferente dos
assets bundlados no binário (`DIRETRIZES.md` Princípio 6), estes são
downloads que o próprio usuário inicia, direto do Hugging Face, na
máquina dele — não ficam no nosso repositório nem no nosso binário. Isso
muda o cálculo de risco: não precisamos nos restringir a Apache-2.0/OFL
como nos assets bundlados, mas ainda é correto que o usuário saiba sob
que licença está baixando algo antes de baixar (o aviso de confirmação de
download já existente no catálogo é o lugar certo pra isso — considerar
incluir a licença no texto do aviso quando não for um permissivo comum).

**Parâmetros de geração por modelo**: o Koharu define `temperature`,
`top_k`, `top_p`, `min_p`, `repeat_penalty` *por modelo* (valores
ligeiramente diferentes por família) — hoje nosso `local_llm.rs` usa
`LlamaSampler::greedy()` fixo, sem nenhum desses parâmetros. Quando a
pendência "parâmetros de geração configuráveis" for feita, considerar
também valores padrão por modelo (não só um único padrão global), seguindo
o exemplo do Koharu.

## Modelos de tradução já implementados neste app

1. **Qwen2.5-VL-7B-Instruct** — padrão atual deste app (não está na lista
   do Koharu; foi nossa escolha antes de termos a lista real dele).
2. **Gemma 4 12B Instruct** (`gemma-4-12b-it`, Q4_K_XL) — adicionado em
   2026-10-20. Usa exatamente o repositório/revisão que o Koharu fixa
   (`unsloth/gemma-4-12B-it-qat-GGUF` @ `980b060c40a8539ac159e0501a3e0f66a6365af3`),
   confirmado ao vivo contra a API do Hugging Face antes de cadastrar
   (sha e tamanhos de arquivo batem). ~6.26GB modelo + ~167MB mmproj.
   Suporte a visão confirmado no llama.cpp (vendorizado pela
   `llama-cpp-2` 0.1.160, lançada 2026-10-15 — bem recente). Nenhum
   código novo precisou ser escrito em `local_llm.rs`: a integração já é
   genérica (lê o chat template embutido no próprio GGUF), só precisou de
   um `ModelOption` em `models.rs` e um braço de `match` em
   `build_models()` (`local_pipeline.rs`).
3. **Gemma 4 12B Uncensored** (`gemma-4-12b-uncensored`, Q4_K_M) —
   adicionado em 2026-10-20, a pedido explícito do Alisson ("quero que
   tenha esses uncensored também"). Finetune de terceiro
   (`HauhauCS/Gemma4-12B-QAT-Uncensored-HauhauCS-Balanced`, revisão
   `ae8045ac2bd216293ca49a3065da2c942dde4b68`) a partir de
   `google/gemma-4-12B-it`, confirmado ao vivo contra a API do Hugging
   Face. Tag de licença no HF é `gemma` (etiquetado mais honestamente que
   o repositório oficial da unsloth, que usa a tag `apache-2.0` apesar do
   link apontar pros termos do Gemma). ~6.87GB modelo + ~167MB mmproj.
4. **LFM2.5 1.2B Instruct** (`lfm2.5-1.2b-instruct`, Q4_K_M) — adicionado
   em 2026-10-20 (`LiquidAI/LFM2.5-1.2B-Instruct-GGUF` @
   `afbd8eaeab5dd94ba0b079ebfb02517d19641e38`, confirmado ao vivo, ~697MB).
   Texto-apenas (sem visão) — primeiro modelo a usar o novo caminho
   `mtmd_ctx: None` de `local_llm.rs`. Licença `lfm1.0` (própria da
   LiquidAI, "other" no HF, não OSI-approved). **Testado de verdade**:
   baixei o arquivo e rodei a tradução contra um texto real, não só
   compilei o código.
5. **Ministral 3 8B Instruct** (`ministral-3-8b-instruct`, Q4_K_M) —
   adicionado em 2026-10-20 (`mistralai/Ministral-3-8B-Instruct-2512-GGUF`
   @ `0102285ad796bd99af90f58de616092e5630e970`, confirmado ao vivo,
   ~4.84GB). Texto-apenas, Apache-2.0. Não testado ao vivo (só o LFM2.5
   foi, por ser bem menor para baixar) — mesma integração de código que o
   LFM2.5, risco baixo, mas vale rodar o smoke test manual antes de
   confiar nele em produção se possível.
6. **Gemma 4 E2B Instruct** (`gemma-4-e2b-it`, Q4_K_XL) — adicionado em
   2026-10-20 (`unsloth/gemma-4-E2B-it-qat-GGUF` @
   `66a399f68ddd113b06dff02fca9523e55465d11d`, confirmado ao vivo,
   ~2.44GB modelo + ~940MB mmproj). Apache-2.0 na tag (mesmo caveat do
   Gemma Terms of Use que o 12B). Menor dos Gemma 4 com visão do Koharu —
   mesma integração de código que o 12B, não testado ao vivo (código
   idêntico, já validado).
7. **Gemma 4 E2B Uncensored** (`gemma-4-e2b-uncensored`, Q4_K_P) —
   adicionado em 2026-10-20 junto com o oficial acima
   (`HauhauCS/Gemma-4-E2B-Uncensored-HauhauCS-Aggressive` @
   `da8593c3e407afcd3e7da94ff2d69d77e2a28a48`, confirmado ao vivo, ~3.21GB
   modelo + ~940MB mmproj). Tag de licença `gemma`.
8. **Gemma 4 E4B Instruct** (`gemma-4-e4b-it`, Q4_K_XL) — adicionado em
   2026-10-20 (`unsloth/gemma-4-E4B-it-qat-GGUF` @
   `8c5a9e4fd5482e2be20fe0bf013b4c262a8f4265`, confirmado ao vivo, ~3.93GB
   modelo + ~945MB mmproj). Apache-2.0 na tag.
9. **Gemma 4 E4B Uncensored** (`gemma-4-e4b-uncensored`, Q4_K_P) —
   adicionado em 2026-10-20 junto com o oficial acima
   (`HauhauCS/Gemma-4-E4B-Uncensored-HauhauCS-Aggressive` @
   `45b6a334b4bcd1d7f37179df58b3b1d66a184e5d`, confirmado ao vivo, ~5.00GB
   modelo + ~945MB mmproj). Tag de licença `gemma`.
10. **Gemma 4 26B A4B Instruct** (`gemma-4-26b-a4b-it`, Q4_K_XL) —
    adicionado em 2026-10-21 (`unsloth/gemma-4-26B-A4B-it-qat-GGUF` @
    `7b92b5b28818151e8669af2e45e88d6086f490dd`, confirmado ao vivo,
    ~14.25GB modelo + ~1.19GB mmproj). Apache-2.0 na tag. Salto de
    tamanho confirmado com o Alisson antes de cadastrar (bem maior que
    os Gemma 4 anteriores). Não testado ao vivo (código idêntico aos
    outros Gemma 4, já validado — baixar ~15GB só pra reconfirmar o
    mesmo código não compensava).
11. **Gemma 4 26B A4B Uncensored** (`gemma-4-26b-a4b-uncensored`,
    Q4_K_M) — adicionado em 2026-10-21 junto com o oficial acima
    (`HauhauCS/Gemma4-26B-A4B-QAT-Uncensored-HauhauCS-Balanced-MTP` @
    `f9093662a2e7ae0503f637088bc96f77a1a70c83`, confirmado ao vivo,
    ~16.80GB modelo + ~1.19GB mmproj). Tag de licença `gemma`.
12. **Gemma 4 31B Instruct** (`gemma-4-31b-it`, Q4_K_XL) — adicionado em
    2026-10-21 (`unsloth/gemma-4-31B-it-qat-GGUF` @
    `43cc1aeb31adf47ec06a854507ce552cd9862e6f`, confirmado ao vivo,
    ~17.29GB modelo + ~1.20GB mmproj). Apache-2.0 na tag. Maior variante
    Gemma 4 que o Koharu oferece — **com essa, a família Gemma 4 do
    Koharu está completa** (E2B, E4B, 12B, 26B A4B, 31B, oficial +
    uncensored cada). Não testado ao vivo (código idêntico aos demais
    Gemma 4).
13. **Gemma 4 31B Uncensored** (`gemma-4-31b-uncensored`, Q4_K_M) —
    adicionado em 2026-10-21 junto com o oficial acima
    (`HauhauCS/Gemma4-31B-QAT-Uncensored-HauhauCS-Balanced-MTP` @
    `9654466e82d83f5ebfe1518a369bc5900873abb1`, confirmado ao vivo,
    ~18.69GB modelo + ~1.20GB mmproj). Tag de licença `gemma`.

### Bug encontrado e corrigido durante o teste real do LFM2.5

`parse_numbered_reply` (`local_llm.rs`) exigia que a linha começasse
exatamente com o número (`"1) texto"`). O LFM2.5 1.2B devolveu
`"N1) That's it..."` — interpretou literalmente o placeholder `"N"` da
instrução do prompt (`"formatted as \"N) translation\""`) em vez de
substituí-lo pelo número real. Corrigido: o parser agora lê a sequência de
dígitos *logo antes* do `)` (sufixo, não o prefixo inteiro), então
`"N1)"`, `"1)"` e variantes como `"No. 1)"` funcionam igual. Teste
`strips_a_literal_n_placeholder_prefix` cobre esse caso. **Isso pode
acontecer com qualquer modelo mais fraco** — vale ficar atento ao testar
os próximos modelos pequenos da lista (ex: Qwen 3.5 0.8B/2B).

## Modelos de OCR já implementados neste app

1. **PaddleOCR-VL 1.6** (`paddle-ocr-vl-1.6`) — adicionado em 2026-10-20,
   agora o padrão (trocado de manga-ocr, mesmo padrão aplicado à
   tradução). Export GGUF **oficial da própria PaddlePaddle**
   (`PaddlePaddle/PaddleOCR-VL-1.6-GGUF` @
   `511b09642bb324401f15f97cc23bc67e8f0a291d`, confirmado ao vivo,
   Apache-2.0, ~936MB modelo + ~882MB mmproj). Multilíngue — diferente de
   manga-ocr, que só lê japonês. Roda via llama.cpp/mtmd (mesmo runtime
   da tradução, suporte nativo mainline desde
   ggml-org/llama.cpp#18825, fev/2026) — **sem precisar de libtorch**,
   diferente de como o próprio Koharu roda esse modelo (reimplementação
   manual via `koharu-torch`). Novo arquivo `engines/paddle_ocr_vl.rs`,
   despachado via o enum `OcrModel` em `local_pipeline.rs` (primeira vez
   que OCR deixa de ser um tipo concreto único).

   **Testado de verdade**: baixei os arquivos reais (~1.8GB), renderizei
   um texto conhecido ("HELLO FRIEND") numa imagem sintética (evita
   qualquer questão de direitos autorais — nenhum crop de manga real
   usado) e confirmei que o OCR reconheceu o texto corretamente.

   **Bug real encontrado e corrigido nesse processo**:
   `Model::apply_chat_template()` (crate `llama-cpp-2`) retornou
   `FfiError(-1)` — a função C que ela chama só reconhece um conjunto
   fixo de templates "famosos" (ChatML, Gemma etc.), não interpreta o
   Jinja customizado do PaddleOCR-VL (`"User: ...\nAssistant:\n"`).
   Corrigido montando o prompt à mão em vez de usar
   `apply_chat_template` — ver `paddle_ocr_vl.rs::read()` e a armadilha
   registrada em `DIRETRIZES.md`. **Vale conferir isso em qualquer
   modelo novo que não seja Qwen/Gemma.**

2. **Baberu OCR** (`baberu-ocr`) — adicionado em 2026-10-21. Modelo
   pequeno (115M params) **feito sob medida pra bolha de manga**
   (`genshiai-daichi/baberu-ocr`, Apache-2.0, pinado em
   `b9868433dbda3ed9ba1357d008599ff4ada873d5`, confirmado ao vivo):
   DINOv2 + decoder próprio de 6 camadas, vocabulário por caractere
   (14.630 símbolos), multilíngue (ja/zh/en), treinado no Manga109-s
   (lê até onomatopeia e texto à mão, não só diálogo limpo). O Koharu
   roda isso via libtorch a partir do `model.safetensors`; usamos em vez
   disso o **export ONNX que o próprio autor do modelo publica no mesmo
   repositório** (`onnx/vision_fp16.onnx` + `decoder_prefill_int8.onnx` +
   `decoder_step_int8.onnx`, ~243MB total) — mesmo formato de três
   gráficos (visão → prefill → step com KV cache) que `manga_ocr.rs` já
   usa, só que com tensores K/V nomeados individualmente por camada
   (`past_k0..5`/`past_v0..5`) em vez de empilhados num tensor só. Novo
   arquivo `engines/baberu_ocr.rs`, novo braço no enum `OcrModel`.

   Decodificação replicada fielmente do próprio script de referência do
   autor (`onnx_infer.py`): greedy + `repetition_penalty` 1.2 (logit
   dividido se positivo, multiplicado se negativo, pra todo token já
   gerado) + limite de repetição em sequência (12 pra caracteres de
   conteúdo — letra/dígito —, 16 pra símbolos, incluindo os marcadores de
   vogal longa "ー"/"〜" que legitimamente se repetem bastante em manga).
   3 testes unitários cobrem essa lógica sem precisar do modelo real.

   **Testado de verdade**: baixei os 4 arquivos reais (~243MB) e rodei
   contra texto renderizado sinteticamente. Resultado interessante: o
   modelo lê "H","E","L","L" com 100% de acerto e consistência em três
   tentativas diferentes de canvas/padding (prova que o encadeamento
   visão → prefill → 4 chamadas de `decoder_step` com KV cache está
   correto), mas hesita no 5º caractere — uma decisão de margem
   mínima (<1% da escala do logit) entre 'O' (certo), quebra de linha e
   fim de sequência. Investigado a fundo (instrumentação temporária
   mostrando os top-3 candidatos por passo) antes de concluir que é
   comportamento genuíno do modelo em uma imagem sintética fora da
   distribuição de treino dele (bolhas de manga reais, não texto Latin
   genérico num fundo branco) — não um bug de código. Teste ajustado pra
   exigir "HELL" (evidência forte de plumbing correto) em vez da palavra
   inteira, com o raciocínio documentado no próprio teste.

## Cautela com os modelos "uncensored" (`HauhauCS/*`)

São finetunes de terceiros não verificados, com proveniência e processo de
"descensura" não documentados publicamente da mesma forma que os releases
oficiais (Google/Alibaba/Mistral/LiquidAI). O Alisson confirmou
explicitamente (2026-10-20) que quer essas opções disponíveis — faz
sentido para um tradutor de manga/manhwa que pode conter conteúdo adulto,
e o Koharu as oferece da mesma forma. **Decisão já tomada**: pode
cadastrar os `HauhauCS/*` junto com os modelos "oficiais" ao trazer cada
família de modelo (ex: trazer `gemma4-26b-a4b-it` e
`gemma4-26b-a4b-uncensored` juntos, não como pendência separada) — não
precisa mais pedir confirmação a cada um, a menos que surja algo
diferente desse padrão (outro mantenedor, outra base, etc.).

## Como adicionar cada nova opção (checklist, atualizado)

Para qualquer motor novo (OCR, inpainting ou tradução):

1. **Levantar a fonte direto do código do Koharu** (não de busca na web) —
   repositório HF, revisão fixada, nome exato do(s) arquivo(s). Confirmar
   ao vivo contra a API do Hugging Face (`huggingface.co/api/models/...`)
   que o repositório/arquivo/sha ainda existem e pegar o tamanho real em
   bytes antes de cadastrar.
2. **Cadastrar no catálogo** (`models.rs`): novo `ModelOption` com seus
   `ModelFile`s na categoria certa, URL apontando para a revisão fixada
   (`resolve/<sha>/<arquivo>`, não `resolve/main/...`) sempre que o Koharu
   também fixar uma revisão — evita que o arquivo mude de baixo dos pés
   depois de cadastrado.
3. **Implementar o motor**, se for um runtime diferente do que já existe
   (novo arquivo em `engines/`). Para modelos de tradução via llama.cpp
   com chat template embutido no GGUF (como todos os da tabela acima),
   normalmente **não precisa de código novo** — só o cadastro no catálogo
   e o braço de `match`. Pra qualquer modelo novo de visão fora do padrão
   Qwen/Gemma (ex: próximos de OCR — Baberu, Hayai), **procurar primeiro
   se existe export GGUF** (evita precisar de libtorch, ver
   `DIRETRIZES.md` Princípio 1) e **testar `apply_chat_template` cedo** —
   pode falhar com `FfiError(-1)` se o template do modelo não for um dos
   "famosos" reconhecidos pelo llama.cpp (aconteceu com o PaddleOCR-VL,
   ver seção acima).
4. **Registrar no `build_models()`** (`engines/local_pipeline.rs`): novo
   braço de `match` mapeando o id da opção para a instância do motor.
5. **A seleção já entra automaticamente na chave de cache** (`jobs.rs`,
   via `ocr_model`/`inpainting_model`/`translation_model`) — não precisa
   de ação extra por opção nova. Só bumpar `PIPELINE_VERSION` se a
   mudança for no *código* do pipeline em si (não só adicionar uma opção
   ao catálogo).
6. **Testar manualmente** com uma página real antes de considerar
   concluído — qualidade de tradução/OCR/limpeza é subjetiva e não é
   coberta só por testes automatizados de plumbing.
7. **Atualizar este spec**: mover o modelo de "pendências" para
   "implementados", com data.

## Pendências específicas

- OCR: ~~PaddleOCR-VL 1.6~~ e ~~Baberu OCR~~ **feitos**. Hayai OCR **fica
  de lado por decisão do Alisson** (2026-10-21) — complexidade de
  implementação bem maior (RoPE 2D customizado, patches dinâmicos,
  posições interpoladas — ver seção "OCR" acima). Não retomar sem
  confirmar de novo.
- Inpainting: FLUX.2 Klein, RORem mixed, AOT GAN.
- Tradução: os 15 modelos Qwen restantes (12 de 27 já feitos — a família
  Gemma 4 inteira está completa, seguindo a ordem da tabela, uncensored
  incluído junto quando fizer parte da mesma família sem precisar
  perguntar — ver seção acima). Próximo na ordem:
  `qwen3.5-0.8b`/`qwen3.5-2b`/`qwen3.5-4b`/`qwen3.5-9b`/`qwen3.5-27b`/
  `qwen3.5-35b-a3b` (+ uncensored dos pequenos), depois `qwen3.6-*` e
  `qwen3.8-27b`. Tamanhos variam bastante (0.8B a 35B) — os pequenos
  (0.8B-4B) são leves, os grandes (27B+) têm o mesmo salto de tamanho dos
  últimos Gemma, confirmar antes.
- ~~Detecção: avaliar troca pelo Koharu Layout RF-DETR Seg 2XL~~
  **avaliado e descartado** (2026-10-21) — licença incerta (Manga109,
  uso acadêmico), sem export leve, pipeline não aproveitaria a
  segmentação mesmo assim. Ver seção "Detecção" acima.
- **Em andamento (2026-10-21)**: parâmetros de geração do LLM
  configuráveis (hoje greedy fixo, sem temperature/top-p/top-k/min-p/
  penalidades) — valores padrão por modelo, como o Koharu faz.
- **Em andamento (2026-10-21)**: pilha de fontes editável (múltiplas
  fontes, não uma só).
