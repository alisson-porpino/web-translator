# Jobs: cache, cancelamento e progresso

## Objetivo

Garantir que o usuário nunca veja resultado desatualizado (cache obsoleto)
e que ele tenha controle real sobre traduções em andamento (parar, ver
progresso) em vez de só esperar um timeout automático.

## Estado atual

### Cache de resultado por job

Nome do arquivo de cache (`jobs.rs`):
```
{sha256_da_imagem}_{idioma_alvo}_{engine_preference}_{PIPELINE_VERSION}_{detection_score_threshold}_{ocr_model}_{inpainting_model}_{translation_model}.png
```
Isso corrige o bug de "mesma coisa, mesmos erros": antes, o cache era só
`hash+idioma`, então trocar de motor/modelo não invalidava o cache e o
usuário continuava vendo a tradução antiga. Ver `DIRETRIZES.md` (Princípio
4) e `pipeline-local-traducao.md` para a disciplina de versionamento.

**Correção adicional (2026-10-20)**: quando o catálogo de modelos passou a
ter mais de uma opção por categoria (ver `catalogo-modelos.md`), o mesmo
bug voltou de um jeito mais sutil — `PIPELINE_VERSION` só muda quando o
*código* muda, não quando o usuário troca de modelo dentro do mesmo
código. Sem as três seleções (`ocr_model`/`inpainting_model`/
`translation_model`) na chave, traduzir a mesma imagem com dois modelos de
tradução diferentes (ex: Qwen2.5-VL vs. Gemma 4) serviria o resultado do
primeiro para o segundo. Corrigido adicionando as três seleções +
`detection_score_threshold` à chave. **Regra geral daqui pra frente**:
qualquer configuração nova que influencie a saída do pipeline local
(seleção de modelo por categoria, threshold, parâmetros de geração quando
forem implementados) precisa entrar nessa chave, não só esperar um bump de
`PIPELINE_VERSION`.

### Cancelamento de job

- `AppState.cancelled_jobs: Arc<Mutex<HashSet<String>>>`.
- `jobs::cancel_job(state, job_id)` marca o job como `cancelled` e insere
  no set; `jobs::is_cancelled(state, job_id)` é checado antes do fetch,
  antes do loop de motores, e antes/depois de adquirir o slot do semáforo
  do motor local (`AppState.local_engine_slots`, tamanho 1 — serializa jobs
  do motor local).
- Rota HTTP: `DELETE /api/jobs/:id`.
- **Limitação conhecida e aceita**: um job já em inferência (chamada
  síncrona dentro de `spawn_blocking`) não pode ser interrompido de fato —
  ele termina em background, só que o resultado nunca é exposto ao
  usuário.

### Progresso e parar, pelo lado da extensão

- Painel flutuante no canto inferior direito da página
  (`.mwt-progress-panel`) mostrando "Translating X/Y (N running)…" e um
  botão "Stop".
- Clicar em "Stop": descarta imagens enfileiradas que ainda não começaram
  (borda `.mwt-border--cancelled`), envia `STOP_TRANSLATIONS` ao
  background (sem precisar de `tabId` — o background lê `sender.tab.id`).
- Background marca os job ids rastreados como parados (`stoppedJobs`,
  checado dentro do loop de `pollJob`) e dispara `DELETE` no servidor para
  cada um.
- `POLL_TIMEOUT_MS = 1_800_000` (30 min) — bem maior que o necessário para
  um job individual, porque agora é só uma rede de segurança para motor
  realmente travado; o controle real do usuário é o botão de parar, não o
  timeout.

## Decisões de design

- **Timeout longo + botão de parar, em vez de timeout curto**: timeout
  curto gerava falso positivo quando havia fila atrás do semáforo de 1
  slot do motor local (imagens legítimas ainda esperando, não travadas).
- **Cancelamento best-effort, não preemptivo**: aceitável porque a
  alternativa (interromper de fato uma chamada de inferência síncrona em
  C/C++) exigiria uma arquitetura bem mais complexa só para economizar
  alguns segundos de CPU de um job que o usuário já decidiu descartar.

## Armadilhas conhecidas

- **Chrome "Local Network Access"** bloqueia silenciosamente
  `<img src="http://127.0.0.1:...">` no content script — o background
  precisa buscar a imagem (`fetch`) e converter para `data:` URL via
  `btoa()` (não `FileReader`: o contexto de service worker do MV3 não tem
  DOM).
- `sender` no listener de mensagens do background precisa de tipo
  explícito (`browser.Runtime.MessageSender`) — senão o TypeScript infere
  `any` e falha o build.

## Pendências / próximos passos

- Nenhuma pendência conhecida nesta área no momento — feature validada
  pelo Alisson em uso real (logs confirmando "stopped N job(s) for tab" e
  status `CANCELLED`).
