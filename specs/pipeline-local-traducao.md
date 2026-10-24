# Pipeline local de tradução

## Objetivo

Traduzir uma imagem de manga/manhwa/webtoon inteiramente offline, sem
depender de uma API paga (Google Gemini é o fallback opcional, não o
caminho principal), com qualidade comparável à de ferramentas de referência
como o Koharu.

## Estado atual

Pipeline, em ordem:

1. **Detecção de regiões de texto** (`engines/detect.rs`): modelo
   RT-DETR-v2 (`ogkalu/comic-text-and-bubble-detector`, fine-tuned em ~11k
   imagens de manga/manhwa/manhua/comics ocidentais), bundlado em
   `assets/detector.fp16.onnx` (Apache-2.0). Entrada 640x640, score mínimo
   0.3, deduplicação de caixas sobrepostas (bubble vs texto livre
   reportando a mesma região).
2. **OCR por região**: três motores arquiteturalmente diferentes,
   escolhidos via `Settings.ocr_model` e despachados por um enum
   `OcrModel` (`local_pipeline.rs`, sem `trait`/`dyn` — segue o estilo do
   projeto). **Padrão desde 2026-10-20: PaddleOCR-VL 1.6**
   (`engines/paddle_ocr_vl.rs`) — multilíngue, export GGUF oficial da
   PaddlePaddle, roda via llama.cpp/mtmd (mesmo runtime da tradução, sem
   dependência nova). **Baberu OCR** (`engines/baberu_ocr.rs`, desde
   2026-10-21) — multilíngue, modelo pequeno (115M) feito sob medida pra
   bolha de manga (treinado no Manga109-s, lê onomatopeia/texto à mão),
   via ONNX (export oficial do autor do modelo, três gráficos como
   manga-ocr mas com KV cache nomeado por camada). `manga-ocr`
   (`engines/manga_ocr.rs`, `kha-white/manga-ocr-base`, Apache-2.0, ONNX,
   **só japonês**) continua disponível como opção, só não é mais o
   pré-selecionado. Troca de padrão a pedido explícito do Alisson, mesmo
   padrão aplicado à tradução.
3. **Tradução** (`engines/local_llm.rs`): modelo de visão-linguagem local
   via llama.cpp (não um modelo de NMT dedicado — ver "Decisões de design").
   **Padrão: Gemma 4 12B Instruct** (`gemma-4-12b-it`) — trocado de
   Qwen2.5-VL-7B-Instruct em 2026-10-20 para casar com o padrão do
   próprio Koharu, a pedido explícito do Alisson. Qwen2.5-VL continua
   disponível como opção, só não é mais o pré-selecionado numa instalação
   nova. Mais opções disponíveis desde 2026-10-20 (Gemma 4 12B/E2B/E4B
   oficial + uncensored, LFM2.5 1.2B, Ministral 3 8B — ver
   `roadmap-modelos-futuros.md` para a lista completa de modelos de
   tradução do Koharu e quais ainda faltam). A integração é genérica
   (lê o chat template embutido no GGUF), então um modelo novo via
   llama.cpp/mtmd normalmente não exige código novo em `local_llm.rs`, só
   cadastro no catálogo + braço de `match`.
   **Visão é opcional**: `LocalLlm::load(model_path, mmproj_path: Option<&Path>)`
   — quando `None` (modelos sem projetor, ex: LFM2.5/Ministral),
   `translate_batch` tokeniza e decodifica o prompt manualmente (sem
   mtmd, sem a imagem da página), em vez de exigir visão de todo modelo.
   Todas as regiões de uma página são traduzidas em **uma única chamada**:
   com visão, a imagem inteira da página é passada junto com a lista
   numerada dos textos OCR'd; sem visão, só a lista numerada. O modelo
   sempre responde no mesmo formato numerado (`parse_numbered_reply`,
   que lê o dígito *logo antes* do `)` — não o prefixo inteiro — porque
   modelos pequenos às vezes ecoam o placeholder "N" da própria instrução
   do prompt ao invés do número real; visto de verdade no LFM2.5 1.2B).

   **Amostragem configurável desde 2026-10-21** (`GenerationParams` em
   `local_llm.rs`, `Settings.llm_*`): `temperature`/`top_k`/`top_p`/
   `min_p`/`max_tokens`/`repeat_penalty`/`frequency_penalty`/
   `presence_penalty`, todos `Option<T>` — em branco (`None`) continua
   sendo greedy/determinístico (comportamento padrão inalterado,
   `LlamaSampler::greedy()`); setar uma temperatura > 0 troca pra uma
   cadeia real (`penalties → top_k → top_p → min_p → temp → dist`),
   preenchendo com valores razoáveis o que ficar em branco. UI em
   "Translation sampling (local LLM, advanced)". Testado de verdade com
   o LFM2.5 e temperatura 0.7 — saída plausível, cadeia funcionando.
   Toggle de "reasoning" do Koharu **não implementado** — exigiria
   montar o prompt à mão por modelo (mesmo problema do
   `apply_chat_template` descrito abaixo), ver `roadmap-modelos-futuros.md`.
4. **Limpeza/inpainting** (`engines/inpaint.rs`): LaMa ("big-lama"), export
   ONNX da Carve (`Carve/LaMa-ONNX`, Apache-2.0). Janela(s) de 512x512
   centrada(s) em cada região (múltiplas, com stitching, para regiões
   maiores que um tile — ver abaixo), reconstrução do fundo sob o texto
   removido. **CoreML desabilitado de propósito** (ver "Bug real
   encontrado" abaixo) — CPU puro, mais lento mas correto.
5. **Composição/typeset** (`engines/compose.rs`): desenha o texto traduzido
   sobre a região limpa, com fallback de fonte por caractere (Noto Sans JP
   para CJK que sobrar sem tradução) para nunca mostrar "tofu boxes".

## Decisões de design

- **Por que um LLM de visão-linguagem, e não um modelo de tradução
  dedicado (NMT)**: já foi tentado M2M100 e depois NLLB-200-distilled-600M
  — qualidade estruturalmente abaixo do necessário para diálogo de manga
  (fragmentos de frase, sujeito implícito, tom casual). Um LLM generalista
  com contexto visual se aproxima muito mais do que o Gemini entregava.
- **Por que passar a imagem da página inteira, não só o texto OCR'd**:
  comparação direta com o Koharu mostrou que o pipeline dele usa a imagem
  da página durante a tradução ("Visão: Fornece imagens das páginas ao LLM
  durante a tradução"). Isso dá contexto de cena/personagem/tom que um
  transcript só-texto não tem (quem está falando, é diálogo ou aside etc).
- **Por que traduzir todas as regiões de uma página numa chamada só, e não
  uma chamada por região**: o modelo vê as traduções anteriores da mesma
  página enquanto segue gerando — mais parecido com como um tradutor humano
  lê a página inteira antes de decidir qualquer linha, e evita N chamadas
  caras ao LLM por página.
- **Inpainting por tile 512x512, não a página inteira**: é uma limitação do
  próprio modelo LaMa exportado (entrada fixa). **Até 2026-10-24**, uma
  região maior que 512x512 em qualquer dimensão falhava direto e caía pro
  preenchimento liso — comum em páginas densas (caixas de legenda largas
  cobrindo boa parte do painel). Corrigido: `reconstruct()` agora divide
  a região numa grade de sub-tiles 512x512, reconstrói cada um
  independentemente (com seu próprio contexto real ao redor) e junta o
  resultado — sem costura/blend nas emendas, mas incomparavelmente melhor
  que um retângulo branco sólido sobre a arte.

### Bug real encontrado e corrigido (2026-10-24): inpainting saindo branco liso

Reportado pelo Alisson comparando contra o Koharu: toda região "limpa"
saía como um retângulo branco sólido, sem nem a borda do balão, cortando
por cima da arte — bem diferente da reconstrução de fundo real que o
Koharu produz. Dois bugs distintos, achados nessa ordem:

1. **Pós-processamento duplicava a normalização**: o código multiplicava
   a saída do modelo por 255 (por simetria com o pré-processamento de
   entrada, que de fato divide por 255). Conferido ao vivo contra a demo
   de referência (`Carve/LaMa-Demo-ONNX`'s `app.py`): a saída do grafo
   ONNX **já vem em [0,255]** (`output.astype(np.uint8)`, sem nenhuma
   multiplicação lá) — só a entrada é normalizada pra [0,1]. Multiplicar
   de novo saturava praticamente todo pixel em 255 (branco).
2. **CoreML silenciosamente ativo mesmo sem ser pedido, e quebrado pra
   esse modelo específico**: mesmo comentando a chamada
   `.with_execution_providers([coreml])` do builder, o ONNX Runtime
   (`ort` 2.0.0-rc.14, feature `coreml` habilitada no Cargo.toml) ainda
   usava CoreML — só parar de verdade passando uma lista **vazia**
   explícita (`with_execution_providers(Vec::new())`), confirmado
   comparando os dois casos lado a lado. E o CoreML compilado pra esse
   modelo é o problema de verdade: com otimização de grafo desligada, a
   própria compilação falha (`Espresso exception: "Invalid blob shape":
   cannot broadcast: (1, 1, 512, 512) vs (1, 3, 0, 0)` — o compilador do
   CoreML resolve as dimensões espaciais da entrada "image" como 0x0
   internamente); com otimização normal (nível 3, o padrão), não dá erro
   nenhum — só devolve um tensor de saída **constante 255.0 em todo
   pixel**, silenciosamente. Como o output já saturado em 255 mascarava
   o bug #1 (255*255 clampado também dá 255), só ficou óbvio depois de
   forçar CPU puro de verdade e ver a saída variar de verdade
   (min=0, max=255, mean≈237 numa região real, reconstrução visualmente
   correta — balão com contorno preservado, sem texto, sem retângulo).
   **Consequência aceita**: inpainting roda só em CPU agora (os 10+
   segundos/região já documentados antes do CoreML existir aqui),
   trocando velocidade por corretude — não dá pra simplesmente "religar"
   o CoreML sem re-confirmar que ele passou a lidar direito com esse
   modelo (forma de entrada provavelmente declarada dinâmica no ONNX, que
   o compilador do CoreML não está resolvendo certo).

Processo de diagnóstico (caso precise repetir pra outro modelo ONNX no
futuro): reproduzir o job real → isolar com o teste manual
`reconstruct_real_region` (bypassa OCR/tradução, muito mais rápido pra
iterar) → logar estatísticas (min/max/mean) do tensor de saída bruto
antes de qualquer pós-processamento → comparar com/sem cada EP e nível de
otimização isoladamente. `output.min() == output.max()` (sem variância
nenhuma) é o sinal mais forte de "a sessão está devolvendo lixo/
constante", não um problema de escala.
- **Gemma 4 12B Instruct como padrão de tradução, PaddleOCR-VL 1.6 como
  padrão de OCR**: ambos trocados de um padrão mais simples (Qwen2.5-VL,
  manga-ocr) pra casar com os padrões do próprio Koharu, a pedido
  explícito do Alisson — não é um trade-off nosso de tamanho/qualidade,
  é alinhamento deliberado com a ferramenta de referência. Ver
  `roadmap-modelos-futuros.md` pra lista completa de opções do Koharu
  ainda não implementadas.

### Bug real encontrado e corrigido (2026-10-24): `eval_chunks`'s n_batch=1

Reportado pelo Alisson: a mesma página que o Koharu traduz em menos de um
minuto levava 5+ minutos aqui, com as mesmas configurações. Instrumentação
de tempo por etapa (`detect`/OCR por região/`translate_batch`/`compose`)
apontou `translate_batch` sozinho em **120s** pra 19 linhas.

Habilitar `print_timings: true` do mtmd (`MtmdContextParams`) revelou a
causa: a imagem da página inteira estava sendo decodificada **um token de
embedding por vez** — 81 chamadas sequenciais de ~65ms cada **só pra uma
imagem de teste sintética 64x64 em branco** (o smoke test existente), cada
uma pagando overhead fixo de dispatch do Metal independente de quanto
trabalho real aquele token precisava. `local_llm.rs::translate_batch` e
`paddle_ocr_vl.rs::read` chamam `mtmd::MtmdInputChunks::eval_chunks(...,
n_batch, ...)` — e o `n_batch` (5º parâmetro posicional) estava hardcoded
em `1` nos dois lugares. Conferido na fonte vendorizada do llama.cpp
(`mtmd-helper.cpp`): esse parâmetro controla diretamente quantos tokens de
embedding de imagem (ou de texto) entram em cada chamada de decode —
`n_batch=1` força processamento item-por-item sem motivo nenhum, já que o
contexto já suporta `n_batch`/`n_ubatch` padrão de 2048/512 do próprio
llama.cpp.

Corrigido trocando `1` por `MTMD_EVAL_BATCH = 512` (match do
`n_ubatch` padrão do llama.cpp, teto seguro) nos dois arquivos. Resultado
medido no mesmo smoke test: **81 chamadas → 1 chamada** (81 tokens
decodificados de uma vez em 7ms), tempo total do teste caindo de 19.7s
pra 6.1s — e esse ganho escala com o tamanho da imagem/número de tokens
de visão, então o efeito numa página real (muito mais tokens de imagem
que o teste sintético) é proporcionalmente maior. Esse mesmo bug também
afetava o OCR por região do PaddleOCR-VL (cada uma das ~19 chamadas por
página pagando o mesmo overhead), não só a tradução.

**Processo de diagnóstico** (reaproveitável pra outros bugs de performance
no pipeline local): nunca assumir que "modelo grande + imagem grande =
lento por natureza" sem medir — instrumentar tempo por etapa primeiro
(`eprintln!` com `std::time::Instant`, removido depois de diagnosticar),
depois, se uma etapa específica do llama.cpp/mtmd for a suspeita, ligar
`print_timings`/`MWT_DEBUG_PRINT_TIMINGS` (ver `local_llm.rs`) pra pegar
a granularidade real por chamada em vez de só o tempo total da função.

## Versionamento do pipeline (crítico)

`PIPELINE_VERSION` (hoje `"v7-inpaint-coreml-fix"`, em `local_pipeline.rs`)
entra na chave de cache de jobs (ver `jobs-cache-e-controle.md`). **Qualquer
mudança de comportamento do pipeline local precisa bumpar essa constante.**
Histórico: `v3-qwen2.5-7b-instruct` → `v4-lama-inpainting` →
`v5-vision-batch-translate` → `v6-selectable-models` → `v7-inpaint-coreml-fix`
(2026-10-24 — os dois bugs reais desta página: inpainting saindo branco
liso + `eval_chunks` n_batch=1. Sem isso, uma página já traduzida antes
desse fix continuaria servindo o PNG quebrado do cache, mesmo depois do
app atualizado, já que `ocr_model`/`inpainting_model`/`translation_model`
em si não mudaram — só o comportamento do código mudou).

## Armadilhas conhecidas

- Decoder start token: para modelos tipo M2M100/NLLB, o token inicial do
  decoder deve ser o EOS, e o token de idioma alvo deve ser *forçado* como
  primeira saída, não alimentado como entrada — erro já cometido e
  corrigido antes de abandonar essa abordagem.
- `ort::Environment` e `LlamaBackend` só podem existir uma vez por processo
  — ver `DIRETRIZES.md`.
- Inpainting em regiões grandes/texturizadas (ex: folhagem) pode gerar
  artefatos visíveis — aceito como caso de borda, não representativo do
  caso comum (bolhas de fala).
- **`Model::apply_chat_template()` (llama-cpp-2) não é um interpretador
  Jinja genérico** — ele chama a função C `llama_chat_apply_template`,
  que só reconhece um conjunto fixo/pequeno de famílias de template
  conhecidas (ChatML, Gemma clássico, etc. — casamento de padrão no
  texto, não avaliação real do Jinja). Modelos com `chat_template.jinja`
  fora desse conjunto retornam `FfiError(-1)`. Descoberto ao vivo
  implementando `engines/paddle_ocr_vl.rs` (formato próprio
  `"User: ...\nAssistant:\n"`) — qualquer modelo novo que não seja um
  dos "famosos" reconhecidos precisa montar o prompt à mão (ver
  `paddle_ocr_vl.rs::read()` pro exemplo — sem token BOS manual, já que
  `MtmdInputText { add_special: true, .. }` cuida disso no nível do
  tokenizer).

  **Correção (2026-10-24)**: essa mesma falha apareceu de verdade em
  produção no modelo de tradução *padrão* (Gemma 4 12B Instruct,
  `errorMessage: "ffi error -1"` reportado pelo Alisson) — a suposição
  acima de que "Qwen/Gemma funcionam normalmente" estava desatualizada:
  o GGUF do Gemma 4 vem com um `chat_template.jinja` novo, usando um DSL
  próprio (`<|turn>role\n...<turn|>`, mais marcação de tool-calling/
  thinking-channel que este pipeline nunca usa) que o `llama-chat.cpp`
  vendorizado só reconhece pelo marcador clássico `<start_of_turn>` do
  Gemma antigo — não reconhece o novo. Como a família Gemma 4 inteira
  (E2B/E4B/12B/26B-A4B/31B, oficial + uncensored) compartilha a mesma
  arquitetura/tokenizer, as 10 opções do catálogo tinham esse bug, não
  só a 12B.

  `engines/local_llm.rs::build_chat_prompt()` agora tenta
  `apply_chat_template()` normalmente e, só se isso falhar **e** o texto
  bruto do template (`model.chat_template(None)`) contiver os marcadores
  `<|turn>`/`<turn|>` (confirma que é especificamente esse DSL do Gemma
  4, não qualquer outra falha), cai para `build_gemma4_chat_prompt()` —
  um prompt montado à mão cobrindo só o formato de turno único (system +
  user + início do turno model) que este pipeline sempre envia, no mesmo
  espírito do que `paddle_ocr_vl.rs` já fazia. Qualquer outra falha de
  `apply_chat_template` (modelo genuinamente não suportado) continua
  retornando erro normalmente, em vez de aplicar esse formato errado por
  engano. Validado de ponta a ponta com um job real contra
  `gemma-4-12b-it` (antes: `TRANSLATION_FAILED`/"ffi error -1"; depois:
  tradução correta, conferida visualmente no resultado).

## Pendências / próximos passos

- Ver `roadmap-modelos-futuros.md`: adicionar opções alternativas de OCR
  (PaddleOCR-VL, Baberu OCR, Hayai OCR), inpainting (AOT, FLUX.2 Klein,
  RORem Mixed) e mais modelos de tradução, todas passando pelo catálogo
  agnóstico (`catalogo-modelos.md`).
- ~~Threshold de detecção configurável pelo usuário~~ **implementado**:
  `Settings.detection_score_threshold` (padrão 0.3, igual ao valor fixo
  anterior), campo em "Advanced" na UI, entra na chave de cache do job
  (`jobs.rs`) para nunca servir um resultado detectado com outro limiar.
  `detect::detect_text_regions` agora recebe o valor como parâmetro em vez
  de usar a constante `SCORE_FLOOR` fixa. `MIN_SIDE` e
  `DUPLICATE_OVERLAP` continuam fixos (detalhe interno de implementação,
  não um "threshold" no sentido que o usuário pediu).
- ~~Parâmetros de geração do LLM configuráveis~~ **implementado**
  (2026-10-21, ver acima) — exceto o toggle de "reasoning" do Koharu,
  ainda pendente.
- ~~Pilha de fontes editável~~ **implementado** (2026-10-21) —
  `Settings.font_paths: Vec<String>` (migrado automaticamente do antigo
  `font_path: String` ao carregar, sem perder a fonte já configurada),
  `compose::load_font_stack` carrega todas em ordem + sempre acrescenta o
  fallback Noto Sans JP bundlado por último. `pick_font`/`measure_mixed`/
  `draw_mixed`/`wrap_text`/`draw_fitted_text` generalizados de "primary +
  fallback" pra "primeira fonte da pilha com o glifo" (`&[FontArc]`, N
  fontes). UI: lista reordenável (↑/↓/×) em vez de um campo só. Testado
  com uma pilha sintética de 3 fontes (duas sem glifo de kanji + o
  fallback) pra confirmar que a busca passa por mais de uma entrada sem
  glifo corretamente, não só o caso de 2 elementos que já existia.
