# Diretrizes do projeto

Princípios que valem para o projeto inteiro, não só para uma feature. Leia
isto antes de qualquer spec de área.

## O que é este projeto

Uma alternativa pessoal, open-source e self-hosted a extensões comerciais de
"tradutor de manga": detecta imagens de manga/manhwa/webtoon numa página
qualquer e sobrepõe uma versão traduzida, usando um pipeline que o próprio
usuário roda e controla (sem conta, sem créditos, sem servidor de terceiros
vendo o que ele está lendo).

Duas peças:
- **`desktop/`** — app Tauri (Rust): servidor HTTP local + pipeline de
  ML (detecção, OCR, inpainting, tradução) + janela de configurações.
- **`extension/`** — extensão WXT (Chrome/Firefox): detecta imagens na
  página, pede a tradução ao app desktop, sobrepõe o resultado.

## Migração em andamento: motor próprio → motor do Koharu (2026-10-24)

**Contexto importante para quem ler os specs de pipeline/catálogo de
modelos depois desta data**: o pipeline local descrito em
`pipeline-local-traducao.md`, `catalogo-modelos.md` e
`limite-recursos.md` (onnxruntime + llama.cpp, motores próprios em
`engines/*`) foi a **primeira implementação real e funcional** do
projeto, e funcionou — mas depois de uma sessão inteira corrigindo bugs
reais nela (template de chat do Gemma 4 não reconhecido por
`apply_chat_template`, inpainting saindo em branco liso por CoreML
silenciosamente quebrado + normalização de saída dobrada, decodificação
de imagem via `eval_chunks` travada em `n_batch=1` fazendo um token por
vez), ficou claro que o resultado prático continuava muito atrás do
Koharu: o mesmo teste real que o Koharu faz em **~1 minuto** levou
**6:57** no nosso app, mesmo depois de todas as correções.

Decisão explícita do Alisson, depois de ver os números lado a lado: usar
o motor de processamento real do Koharu (`github.com/koharu-rs/koharu`,
MIT OR Apache-2.0 — licença permissiva, fork/vendoring sem problema) em
vez de continuar reimplementando cada detalhe de inferência do zero numa
pilha diferente e descobrindo os bugs um por um. Escopo da migração,
também decidido explicitamente:

- **Não** herdar a GUI desktop completa do Koharu (roda num Chromium
  embutido via CEF — dependência pesada, pensada pra edição manual de
  projeto, não é o que este app precisa).
- **Sim** herdar o motor de processamento (`koharu-pipeline` e as
  crates de ML por trás) — é aí que está o ganho de performance e
  corretude medido na prática.
- O *conceito* de uma funcionalidade específica da GUI do Koharu (o
  painel "Páginas" com miniaturas das imagens + lista de camadas) deve
  ser **reimplementado dentro da nossa própria extensão de navegador**
  — não rodando o frontend dele — como forma de deixar o usuário
  escolher quais imagens de uma página realmente devem ser traduzidas
  (resolve também falso positivo da heurística de detecção atual).
- Nosso servidor HTTP local + extensão (a parte "web" do projeto) **não
  tem equivalente no Koharu** — ele não tem nenhum servidor HTTP/web
  hoje, é um app desktop puro. Essa continua sendo a nossa contribuição
  real, só o motor por baixo muda.

O pipeline local atual (`engines/detect.rs`, `manga_ocr.rs`,
`baberu_ocr.rs`, `paddle_ocr_vl.rs`, `local_llm.rs`, `inpaint.rs`,
`compose.rs`, o catálogo de modelos em `models.rs`, o limitador de
recursos em `resource_limits.rs`/`resource_usage.rs`) **fica preservado
no histórico do git como referência** — não foi abandonado por estar
"errado", a migração é incremental e os specs que o documentam continuam
valendo como registro histórico até serem explicitamente substituídos.
Ver o plano de migração completo (fases, riscos, critérios de sucesso)
no histórico de planejamento da sessão de 2026-10-24.

## Princípio 1: leve por padrão, pesado por escolha

A aplicação em si deve ser **leve** — nada de baixar modelos grandes só por
estarem na lista. Qualquer recurso plugável (OCR, limpeza/inpainting, fonte,
modelo de tradução/LLM, etc.) segue o mesmo padrão agnóstico:

- Existe um catálogo de opções por categoria (ver `catalogo-modelos.md`).
- Nada baixa automaticamente — o download só acontece quando o usuário
  escolhe explicitamente uma opção que ainda não está no disco, e aí só
  depois de confirmar um aviso tipo "esse recurso ainda não está disponível,
  deseja realizar o download?".
- Uma vez baixado, o uso é local e imediato — sem rebaixar, sem depender de
  rede depois disso.
- Deve ser possível gerenciar (baixar ou excluir) qualquer opção de antemão,
  por categoria, não só no momento da seleção.
- Isso vale para **qualquer** categoria — OCR, pintura/limpeza, fonte, LLM
  de tradução, detecção — não é um comportamento especial de uma única
  feature. Nada fixo/hardcoded sem passar por esse catálogo.

**Por quê**: pedido explícito do Alisson, inspirado no Koharu — ele quer a
liberdade de escolher o que baixar e quando, sem a aplicação ficar pesada
por padrão nem surpreender o usuário com downloads grandes não solicitados.

## Princípio 2: priorizar qualidade sobre desempenho quando o usuário pedir isso

Já tivemos o caso do inpainting via LaMa: ficou bem mais lento (175s/página)
que a abordagem anterior (preenchimento de cor sólida), mas a qualidade do
resultado é muito melhor. Decisão explícita do Alisson: **manter a
qualidade mesmo com desempenho ruim**, em vez de voltar para uma solução
mais rápida e pior.

**Como isso se aplica a decisões futuras**: não "otimizar" silenciosamente
trocando um modelo/abordagem de melhor qualidade por um mais rápido e
pior sem perguntar antes. A compensação para o problema de desempenho não é
"usar algo mais fraco", é dar ao usuário controle (botão de parar, limite de
CPU/RAM — ver `limite-recursos.md`), não reduzir a qualidade por padrão.

## Princípio 3: não travar a máquina do usuário

Qualquer feature cujo custo de CPU/RAM possa ser alto (modelos locais
grandes, inpainting, LLM de tradução) deve respeitar os limites
configuráveis de CPU/RAM (ver `limite-recursos.md`) e, quando for uma
operação longa, deve ser cancelável (ver `jobs-cache-e-controle.md`).

## Princípio 4: cache correto é inegociável

Já tivemos um bug grave de "mesma coisa, mesmos erros" causado por cache
obsoleto — o servidor cacheava imagens traduzidas por hash+idioma, sem
invalidar quando o motor/modelo mudava. Qualquer mudança que altere o
comportamento do pipeline local (troca de modelo, de versão de prompt, de
lógica de pós-processamento etc.) **precisa** bumpar a versão de pipeline
usada na chave de cache (hoje `PIPELINE_VERSION` em `local_pipeline.rs`).
Esquecer isso faz o usuário ver resultados antigos achando que é um bug
novo — já aconteceu uma vez e custou tempo de diagnóstico.

## Princípio 5: limites éticos sobre conteúdo com direitos autorais

Esta aplicação traduz imagens individuais enquanto o usuário está lendo uma
página — uso transitório e transformativo, análogo ao que a extensão
comercial original faz. Isso é diferente de:

- **Construir um downloader em massa/arquivista** de páginas de manga de um
  agregador não autorizado, mesmo que a intenção declarada seja só
  "comparar" ou "testar" — isso já foi pedido e **recusado** nesta
  conversa, e a recusa deve continuar valendo para pedidos parecidos no
  futuro.
- É permitido e incentivado, porém, **se inspirar em técnicas e arquitetura
  de ferramentas open-source como o Koharu** (ex: tradução com contexto
  visual, catálogo de modelos agnóstico, pipeline de detecção → OCR →
  limpeza → tradução → composição) — a linha é "aprender a técnica e
  reimplementar", não "baixar o software/conteúdo de terceiros em massa".

Qualquer pedido futuro de scraping/download em massa de conteúdo protegido
deve ser tratado com a mesma cautela, independentemente de como for
justificado.

## Princípio 6: licenciamento dos modelos/assets embutidos

Todo modelo ou asset embutido no app (detector, OCR, inpainting, fontes)
precisa ter licença compatível com redistribuição livre (Apache-2.0, SIL
OFL etc.) e a licença deve ser documentada junto do asset (ex:
`assets/LICENSE-*.txt`). Fontes comerciais de lettering (Wild Words, Anime
Ace) **não são bundladas** — o app aceita um caminho de fonte configurável
pelo próprio usuário, que precisa obtê-la por conta própria.

## Armadilhas conhecidas (válidas para o projeto inteiro)

- **ort (ONNX Runtime) só permite um `Environment` por processo** — use o
  singleton compartilhado (`engines/ort_env.rs`), nunca crie um novo.
- **`LlamaBackend::init()` só pode ser chamado uma vez por processo** —
  mesmo padrão de singleton (`local_llm.rs`'s `backend()`).
- **Chrome "Local Network Access"** bloqueia silenciosamente
  `<img src="http://127.0.0.1:...">` no content script — a extensão precisa
  buscar a imagem no background script e converter para `data:` URL.
- **Fontes latinas não têm glifo para CJK** — texto não traduzido (nomes
  próprios, títulos) usando só a fonte principal gera caixas vazias
  ("tofu boxes"); é necessário fallback de fonte por caractere (ver
  `engines/compose.rs`).
- **Timeout de polling da extensão precisa ser bem maior que o tempo normal
  de um único job** quando há fila (semáforo de 1 slot para o motor local)
  — o timeout automático é uma rede de segurança para motor realmente
  travado, não o controle principal; o controle principal é o botão de
  parar.
- **Toggle de `hidden` via JS não tem efeito se o elemento tiver uma regra
  CSS própria de `display`** (ex: `.manage-overlay { display: flex }`): a
  regra de classe do autor e o `[hidden] { display: none }` padrão do
  navegador têm a mesma especificidade, e a do autor sempre vence — o
  elemento fica visível (e "intravável") independente do atributo. Bug
  real: o modal "Manage models" ficava sempre visível e o X não fechava
  (`desktop/ui/style.css`). Qualquer elemento novo controlado por
  `el.hidden = true/false` que também tenha seu próprio `display` no CSS
  precisa de uma regra `[hidden] { display: none }` explícita para esse
  seletor.
- **Download de modelo não tinha retomada** (`models.rs`'s
  `download_one`) — **corrigido em 2026-10-21**. Escrevia direto num
  `.part`, sem checkpoint: matar o processo do app no meio de um
  download perdia todo o progresso. **Aconteceu de verdade** (2026-10-20):
  matei o app pra aplicar uma mudança de settings enquanto o Alisson
  tinha ~4.5GB de um download de 6.7GB em andamento; perdeu tudo. Agora
  `download_one` checa o tamanho do `.part` existente e manda um `Range:
  bytes=<n>-` — se o servidor responde `206 Partial Content`, abre o
  arquivo em modo append em vez de truncar; se não honrar o Range
  (responde `200` normal), recomeça do zero (sempre seguro, só mais
  lento). Validado com dois testes reais contra a rede (não só
  compilado): um confirma que um `.part` com prefixo correto retoma e dá
  um arquivo idêntico a um download completo do zero; o outro escreve um
  prefixo *errado* de propósito e confirma que ele sobrevive no resultado
  final — prova que a retomada está de fato acontecendo (anexando), não
  só silenciosamente reiniciando (ver `models::manual_tests` — rodar com
  `MWT_RUN_DOWNLOAD_RESUME_TEST=1`). **Mesmo assim**, ainda vale
  perguntar/checar se há download em andamento antes de matar o processo
  do app por qualquer motivo (arquivo `.part` recente em `models_dir`) —
  a retomada evita perder o progresso, mas ainda reinicia do ponto onde
  parou, não é instantâneo.
- **`Model::apply_chat_template()` (crate `llama-cpp-2`) não interpreta
  Jinja de verdade** — chama a função C `llama_chat_apply_template`, que
  só reconhece um conjunto fixo e pequeno de famílias de template
  conhecidas (casamento de padrão no texto do template, não avaliação
  real). Um modelo GGUF com `chat_template.jinja` fora desse conjunto
  (visto ao vivo com PaddleOCR-VL, formato próprio
  `"User: ...\nAssistant:\n"`) retorna `FfiError(-1)` em vez de um erro
  claro. **Correção importante (2026-10-24)**: a suposição anterior aqui
  de que "Qwen/Gemma funcionam porque seus templates batem com os
  padrões reconhecidos" estava errada — o Gemma 4 (modelo de tradução
  *padrão* do catálogo) também caía nesse mesmo erro, porque seu
  `chat_template.jinja` usa um DSL novo (`<|turn>role\n...<turn|>`) que
  o `llama_chat_apply_template` vendorizado só reconhece pelo marcador
  clássico `<start_of_turn>` do Gemma antigo. **Nunca assumir que um
  modelo "famoso" está safe sem testar de verdade.** Quando der esse
  erro, montar o prompt à mão (ler o `chat_template.jinja` do próprio
  modelo na página do Hugging Face pra replicar a estrutura de turno) em
  vez de depender de `apply_chat_template` — ver
  `engines/paddle_ocr_vl.rs::read()` pro exemplo original e
  `engines/local_llm.rs::build_chat_prompt()` pro fallback automático
  (detecta a falha e monta o prompt do Gemma 4 na hora, sem precisar
  saber de antemão qual modelo vai dar erro) — detalhes completos em
  `pipeline-local-traducao.md`.

## Como a camada de performance é estruturada hoje

- **CPU**: contagem de threads configurável, aplicada de forma cooperativa
  a todo backend de inferência (llama.cpp e ONNX Runtime via
  `with_intra_threads`). Nunca falha — só reduz paralelismo.
- **RAM**: limite rígido via `setrlimit(RLIMIT_AS, ...)` em macOS/Linux,
  **e via Job Object (`ProcessMemoryLimit`) no Windows desde 2026-10-21**
  (raciocinado com cuidado contra o código-fonte real da crate
  `windows`, mas nunca testado num Windows de verdade). É proposital ser
  rígido: o objetivo é "esse job falha" em vez de "a máquina toda trava
  trocando memória para disco".
- **VRAM (desde 2026-10-24)**: não existe API de SO multiplataforma pra
  limitar isso como RAM/CPU — é um orçamento *estimado* em MB
  (`Settings.max_vram_mb`), convertido em quantas camadas GGUF offloadar
  pra GPU lendo só o cabeçalho do modelo (`engines/gguf_meta.rs`), sem
  carregar o modelo inteiro. `0` = sem orçamento, offload tudo (padrão).
- **Uso em tempo real (desde 2026-10-24)**: CPU%/RAM usado-total ao vivo
  via `sysinfo`, exposto tanto na janela do app (`get_resource_usage`)
  quanto pra extensão via `GET /api/resources` no servidor HTTP — ver
  `resource_usage.rs`.
- Ver `limite-recursos.md` para detalhes. **Nota**: toda essa camada é
  específica do motor local atual (`engines/*`) e deve ser reavaliada
  junto da migração pro motor do Koharu (ver seção acima) — o
  `koharu-pipeline` já tem seu próprio `subscribe_resources()`
  equivalente.
