# Limite de CPU, RAM e VRAM + uso em tempo real

## Objetivo

Deixar o usuário definir um limite máximo de uso de CPU/RAM durante o
processamento local, para que páginas com muitos painéis não travem o
resto do computador — especialmente depois do inpainting (LaMa) ter
aumentado bastante o custo de processamento por página.

## Requisito do usuário (citação)

> "Quero também adicionar um limitador de CPU e RAM que o usuário possa
> definir o limite máximo de uso durante o processo da aplicação para que
> ele não acabe travando tanto o computador."

Depois, pedido de continuação (VRAM + tempo real):

> "Vamos voltar com isso. Primeiro quero que ele me mostre em tempo real
> os recursos utilizados, e que eu possa eu mesmo determinar qual o
> limite desses recursos. Então vamos ter valores padrão como agora, mas
> quer poder escolher o que gastar desses recursos. Como por exemplo o
> quanto usar de CPU, de VRAM e RAM"

E, ao ser perguntado como lidar com VRAM (não existe API de SO pra
limitar VRAM do jeito que `setrlimit`/Job Object limitam RAM):

> "Quero a opção 1, mas quero que ela seja mostrada o uso em tempo real,
> tanto no próprio navegador, quanto no próprio aplicativo"

("Opção 1" = orçamento de VRAM em MB, estimando quantas camadas do
modelo cabem nesse orçamento — não um limite rígido do SO como RAM.)

## Estado atual (implementado)

Módulo `resource_limits.rs`, dois mecanismos distintos (motivo: têm modos
de falha muito diferentes):

- **CPU**: `effective_thread_count()` devolve o número de threads que
  qualquer backend de inferência deve pedir — o limite configurado (se > 0
  e menor que o total de cores) ou todos os cores disponíveis. Aplicado via
  `with_intra_threads(...)` em todas as `Session` do ONNX Runtime
  (`detect.rs`, `manga_ocr.rs`, `inpaint.rs`) e via `with_n_threads`/
  `with_n_threads_batch` no llama.cpp (`local_llm.rs`). É cooperativo — só
  reduz paralelismo, nunca falha.
- **RAM**: `set_limits()` aplica `setrlimit(RLIMIT_AS, ...)` em Unix
  (macOS/Linux) via a crate `libc`, ou (**desde 2026-10-21**) o
  `ProcessMemoryLimit` de um Job Object no Windows via a crate `windows`
  (`0.62`, features `Win32_Foundation`/`Win32_Security`/
  `Win32_System_JobObjects`/`Win32_System_Threading`). Mesmo conceito nos
  dois: cria um objeto do SO com o teto, associa o processo atual a ele,
  nunca fecha o handle de propósito (o Windows libera o Job Object sozinho
  quando o processo termina). Em ambos os casos é um limite **rígido**:
  uma alocação que ultrapasse o teto falha e tipicamente aborta o que
  estava alocando. Proposital — o objetivo é "esse job falha" em vez de
  "a máquina toda trava trocando memória para disco".

  **Importante sobre o lado Windows**: as assinaturas exatas da API e as
  features de Cargo necessárias foram conferidas contra o código-fonte
  publicado da crate `windows` no GitHub (tag `0.62.0`, não contra a
  documentação renderizada no docs.rs, que retorna 404 pra esse módulo
  sem a seleção de features certa) — e validadas via `cargo check
  --target x86_64-pc-windows-msvc` num crate isolado, que compilou limpo
  contra a versão real publicada (0.62.2). Isso dá confiança de que o
  código está sintaticamente/tipicamente correto, mas **este projeto é
  desenvolvido no macOS e esse código nunca rodou de verdade em
  Windows** — uma compilação cruzada do crate inteiro trava antes mesmo
  de chegar nesse arquivo (o `llama-cpp-sys-2` precisa do SDK do Windows
  pra gerar bindings, que não existe aqui). Tratar como "raciocinado com
  cuidado", não "comprovado", até alguém confirmar numa máquina Windows
  de verdade.
- `0` em qualquer um dos dois campos significa "sem limite" — nunca abaixa
  o padrão do sistema operacional por conta própria.
- Configuração: `Settings.max_cpu_threads` / `Settings.max_memory_mb`
  (`settings.rs`), com `#[serde(default)]` para compatibilidade com
  `settings.json` salvos antes dessa feature existir.
- Aplicado no `main.rs`: uma vez no startup (`setup()`, com os settings
  salvos) e de novo a cada `save_settings` — a mudança vale imediatamente,
  sem precisar reiniciar o app.
- UI: dois campos numéricos em "Avançado" no `desktop/ui/index.html`
  ("Max CPU threads for local models", "Max memory for local models (MB)"),
  0 = sem limite, wireados em `main.js`.

### VRAM (orçamento estimado, 2026-10-24)

Diferente de CPU/RAM, **não existe API de SO multiplataforma pra
limitar VRAM** do jeito que `setrlimit`/Job Object limitam RAM — então
este não é um limite rígido, é uma estimativa de quantas camadas do
modelo GGUF offloadar pra GPU dado um orçamento em MB:

- `engines/gguf_meta.rs` (novo) — lê só a chave `*.block_count` do
  cabeçalho GGUF (sem carregar o modelo inteiro) pra descobrir o total
  de camadas de um modelo. Necessário porque `n_gpu_layers` precisa ser
  decidido **antes** de `LlamaModel::load_from_file`, mas
  `LlamaModel::n_layer()` só existe **depois** de carregar — e carregar
  duas vezes (uma só pra perguntar o tamanho) seria um desperdício.
  Validado contra cabeçalhos GGUF sintéticos e também contra um arquivo
  real baixado (LFM2.5, 16 camadas, confirmado batendo com o
  `num_hidden_layers` do `config.json` do modelo base no Hugging Face).
- `resource_limits::gpu_layers_for_budget(model_path, max_vram_mb)` —
  aproximação linear: `camadas_offloaded ≈ (orçamento_bytes /
  tamanho_do_arquivo) × total_de_camadas`, assumindo camadas de tamanho
  parecido. Menos preciso que o `common_fit_params` auto-fit do próprio
  llama.cpp (que lê tamanho real por tensor + KV-cache), escolhido de
  propósito por ser muito mais simples de implementar/raciocinar
  corretamente — decisão explícita de reduzir escopo depois do Alisson
  avisar que estava gastando bastante tokens na sessão. `0` = sem
  orçamento, offload tudo (comportamento de sempre, inalterado).
  Nunca falha o carregamento por causa do orçamento: qualquer erro de
  leitura (arquivo ausente, cabeçalho corrompido) cai em "offload tudo".
- `Settings.max_vram_mb` (novo campo, `#[serde(default)]`), parte da
  `ModelsCacheKey` do `local_pipeline.rs` — mudar o orçamento força
  recarregar a combinação atual (senão o modelo já carregado ficaria
  rodando com o `n_gpu_layers` antigo até a próxima troca de seleção).
- `LocalLlm::load`/`PaddleOcrVl::load` ganharam um parâmetro
  `gpu_layers: u32` (era hardcoded em `1000` = "todas as camadas" pros
  dois); `build_models()` resolve esse valor por modelo via
  `gpu_layers_for_budget` antes de cada chamada.
- UI: campo "Max VRAM budget for local models (MB)" em "Avançado",
  mesmo padrão dos outros dois campos.

### Uso em tempo real (2026-10-24)

Novo módulo `resource_usage.rs` — CPU%/RAM usado/total ao vivo via a
crate `sysinfo` (0.39), um `System` persistente atrás de um
`Mutex`/`OnceLock` (precisa ser persistente: a % de CPU da `sysinfo` é
um delta desde o último refresh, uma instância nova a cada chamada
sempre devolveria 0%). **Sem número de VRAM ao vivo** — mesma limitação
de não existir API de SO multiplataforma pra isso; o que aparece é o
orçamento *configurado* (`max_vram_mb`), não uma leitura real de
hardware.

- Comando Tauri `get_resource_usage` — usado pela própria janela de
  configurações do desktop (`desktop/ui/main.js`'s `refreshResourceUsage`,
  polling a cada 2s).
- Rota HTTP `GET /api/resources` (`http_server.rs`) — mesma leitura,
  exposta pro popup da extensão poder mostrar sem precisar abrir a
  janela do app desktop (`extension/entrypoints/popup/main.ts`'s
  `refreshResourceUsage`, mesmo polling de 2s, usando a `serverUrl` já
  configurada). `host_permissions: ['<all_urls>']` no `wxt.config.ts` já
  cobre `http://127.0.0.1:*`, nenhuma permissão nova precisou ser
  adicionada.

## Decisões de design

- **CPU cooperativo, RAM rígido**: são naturezas de recurso diferentes.
  Reduzir threads nunca quebra nada; já alocar memória sem controle pode
  degradar o sistema inteiro, então ali vale a pena aceitar uma falha dura
  e localizada.
- **Aplicado de forma transversal a todo motor local**, não feature por
  feature — qualquer novo motor adicionado ao catálogo (ver
  `catalogo-modelos.md` / `roadmap-modelos-futuros.md`) deve chamar
  `resource_limits::effective_thread_count()` para sua configuração de
  threads, em vez de `std::thread::available_parallelism()` direto.

## Pendências / próximos passos

- ~~Equivalente no Windows via Job Objects~~ **implementado, não
  verificado em hardware real** (2026-10-21) — ver nota acima. Alguém
  com uma máquina Windows precisa confirmar que o limite realmente é
  aplicado (abrir o Gerenciador de Tarefas, setar um teto baixo, ver se
  o app realmente é limitado/aborta como esperado).
- Validar na prática se o limite de RAM realmente evita o travamento em
  uma sessão longa com o LLM de 7B carregado — ainda não testado sob
  limite ativo em uso real pelo Alisson (macOS).
