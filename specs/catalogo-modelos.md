# Catálogo agnóstico de modelos

## Objetivo

Dar ao usuário liberdade total de escolha de qual modelo/motor usar em cada
etapa do pipeline (detecção, OCR, inpainting, tradução — e, no futuro,
qualquer outra categoria plugável), sem que a aplicação fique pesada por
padrão. Inspirado diretamente na lista de opções do Koharu.

## Requisito do usuário (citação, para não perder a intenção original)

> "Quero a primeira opção, mas eu quero também ter a possibilidade para
> escolher o que eu não tenho (...) a diferença para que isso não fique
> pesado, é que ele baixe à medida que ele vai usar (...) uma vez que ele
> já esteja baixado, aí o usuário só usa na hora mesmo. Essa escolha pode
> ser mesmo naquela lista que temos na aplicação, se já tiver baixado por
> exemplo, ele pode ter um ✓ do lado que marque que está baixado, se não
> tiver, quando ele selecionar outra, deve dar um aviso dizendo: 'esse
> recurso ainda não está disponível, deseja realizar o download?' (...) a
> própria lista também deveria ter um botão (...) gerenciar/editar/
> engrenagem (...) escolher de antemão o que baixar, e também excluir o
> que já está baixado (...) isso tem que funcionar para qualquer opção,
> seja OCR, pintura, limpeza, fonte, LLM, etc. Não quero nada fixo, quero
> algo mais agnóstico que dê liberdade para o usuário, que nem o Koharu
> faz."

## Estado atual

Arquitetura genérica em `models.rs`:

- `ModelCategory` (hoje: `Detection`, `Ocr`, `Inpainting`, `Translation`).
- `ModelOption { id, label, builtin, files }` — cada opção sabe seus
  arquivos (`ModelFile { filename, url, approx_bytes }`).
- Funções genéricas, nenhuma hardcoded por categoria:
  `find_option(id)`, `is_downloaded(app, option)`,
  `catalog_status(app) -> Vec<CategoryStatus>`,
  `download_option(app, option_id)`, `delete_option(app, option_id)`.
- Comandos Tauri: `get_model_catalog`, `download_model_option`,
  `delete_model_option`.
- `Settings` guarda qual opção está selecionada por categoria
  (`ocr_model`, `inpainting_model`, `translation_model` — strings livres,
  casam com `ModelOption::id`).
- `local_pipeline.rs` mantém **no máximo uma combinação carregada** em
  memória (`static MODELS: Mutex<Option<(ModelsCacheKey, Arc<LocalModels>)>>`)
  — trocar a seleção libera a combinação antiga (vários GB) e recarrega;
  voltar a uma combinação anterior paga o custo de carregar de novo
  (trade-off deliberado).
- UI (`desktop/ui/index.html` + `main.js`): uma linha por categoria com
  `<select>` (opções já baixadas marcadas com ✓) e um botão "⚙ Gerenciar".
  Trocar para uma opção não baixada pede confirmação (`confirm()`) antes de
  baixar; decline reverte a seleção. O modal de gerenciar lista
  Download/Delete por opção (Delete desabilitado para opções `builtin` ou
  atualmente selecionadas).

## Decisões de design

- **Download por opção, não por categoria inteira**: cada `ModelOption`
  baixa só os próprios arquivos — nunca baixa tudo de uma categoria de
  uma vez.
- **Nunca baixa automaticamente**: toda troca de seleção para algo ainda
  não baixado passa por confirmação explícita do usuário.
- **`builtin: true`** marca opções embutidas no binário (ex: o detector
  hoje) que não podem ser excluídas — `delete_option` recusa para essas.
- **Cache de combinação única (não multi-combinação)**: escolhido para não
  deixar vários modelos grandes em RAM ao mesmo tempo; o custo é recarregar
  ao alternar entre combinações usadas recentemente. Se isso incomodar na
  prática, considerar um cache LRU de N combinações — mas isso é uma
  mudança de design, não fazer sem alinhar antes.

## Pendências / próximos passos

- Detecção e inpainting ainda têm **apenas uma opção real** cada (detector
  bundlado, LaMa). OCR tem três (manga-ocr, **PaddleOCR-VL 1.6** — padrão
  desde 2026-10-20, multilíngue via llama.cpp/mtmd; **Baberu OCR** — desde
  2026-10-21, multilíngue via ONNX, manga-tuned; falta Hayai OCR, ver
  `pipeline-local-traducao.md`/`roadmap-modelos-futuros.md`). Tradução já
  tem treze: Qwen2.5-VL-7B-Instruct, **a família Gemma 4 inteira**
  (E2B/E4B/12B/26B A4B/31B, oficial + uncensored cada — 5 pares, **12B é
  o padrão**), LFM2.5 1.2B Instruct e Ministral 3 8B Instruct (ver
  `roadmap-modelos-futuros.md` para a lista completa de 27 modelos de
  tradução do Koharu — restam só os 15 Qwen). LFM2.5 e Ministral são
  texto-apenas — primeiros a usar o suporte a visão opcional em `LocalLlm` (ver
  `pipeline-local-traducao.md`). OCR com motores arquiteturalmente
  diferentes (ONNX encoder-decoder, ONNX com KV cache nomeado por camada,
  llama.cpp/mtmd) usa um enum `OcrModel` com despacho via `match` em
  `local_pipeline.rs` em vez de um tipo concreto único — padrão já
  comprovado em 3 motores. Hayai OCR fica de lado por decisão do Alisson
  (2026-10-21).
- **Pilha de fontes editável implementada (2026-10-21)** — mas como uma
  lista simples de caminhos (`Settings.font_paths`), *não* como mais uma
  categoria do catálogo baixável (`models.rs`). Diferença importante:
  fonte comercial de lettering (Wild Words, Anime Ace) não pode ser
  baixada por nós de jeito nenhum (ver `DIRETRIZES.md` Princípio 6) — o
  usuário tem que obter e apontar o caminho por conta própria, então não
  faz sentido como opção de catálogo com download. Ver
  `pipeline-local-traducao.md` e `engines/compose.rs` para os detalhes.
  **Também corrigido (2026-10-21)**: download de modelo agora retoma de
  onde parou em vez de reiniciar do zero (`models.rs`'s `download_one`,
  ver `DIRETRIZES.md`).
- Ao adicionar uma nova opção a uma categoria existente, o padrão a seguir
  é: 1) cadastrar o `ModelOption` com seus `ModelFile`s no catálogo
  constante da categoria certa em `models.rs`; 2) implementar o motor de
  fato (novo arquivo em `engines/` se for um motor novo) e registrar o
  `match` correspondente em `build_models()` (`local_pipeline.rs`); 3)
  bumpar `PIPELINE_VERSION` **só** se a mudança for no código do pipeline
  em si — a seleção da opção em si já entra automaticamente na chave de
  cache (`jobs.rs` inclui `ocr_model`/`inpainting_model`/
  `translation_model`, corrigido em 2026-10-20 — ver
  `jobs-cache-e-controle.md`). Checklist completo e fonte oficial da lista
  de modelos do Koharu em `roadmap-modelos-futuros.md`.
