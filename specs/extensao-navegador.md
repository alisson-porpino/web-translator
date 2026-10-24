# Extensão do navegador

## Objetivo

Detectar imagens de manga/manhwa/webtoon em qualquer página, pedir a
tradução ao app desktop local, e sobrepor o resultado no lugar — com
feedback visual de progresso e controle de parar.

## Estado atual

- WXT, builda para Chrome (MV3) e Firefox/Zen (MV2) a partir do mesmo
  código-fonte (`npm run build` / `npm run build:firefox`).
- `entrypoints/content/index.ts`: `MutationObserver` observa toda
  `<img>` da página, aplica heurísticas de tamanho para decidir o que vale
  mandar traduzir (páginas de scan altas, tiras de webtoon, crops de bolha
  de fala) e ignora ícones/avatares/anúncios.
- Bordas de estado por imagem: pontilhada (processando), vermelha
  (concluído), `.mwt-border--cancelled` (cancelado pelo usuário).
- Painel de progresso flutuante + botão "Stop" (ver
  `jobs-cache-e-controle.md` para o fluxo completo de cancelamento).
- `entrypoints/background/index.ts`: enfileira jobs, faz polling
  (`pollJob`), busca a imagem traduzida via `fetch` e converte para
  `data:` URL antes de entregar ao content script (ver armadilha de
  "Local Network Access" em `jobs-cache-e-controle.md`).
- `lib/messages.ts`: tipos das mensagens entre content/background
  (`TRANSLATE_IMAGE`, `STOP_TRANSLATIONS`, etc).

## Decisões de design

- **Conversão para `data:` URL no background, não no content script**:
  necessário por causa do bloqueio de "Local Network Access" do Chrome a
  requisições de página para `127.0.0.1`. O background não tem essa
  restrição da mesma forma e consegue fazer o `fetch` direto.
- **Heurísticas de tamanho, não um classificador de imagem**: mais simples
  e suficiente até agora; se começar a dar muito falso positivo/negativo,
  considerar algo mais sofisticado — mas isso ainda não foi necessário.

## Pendências / próximos passos

- Nenhuma pendência grande conhecida hoje nesta camada especificamente —
  o trabalho pendente relevante agora é todo do lado do desktop (catálogo
  de modelos, ver `roadmap-modelos-futuros.md`) ou de config visual (UI de
  configurações no desktop, não na extensão).
- Se o desktop ganhar mais opções configuráveis por categoria (threshold de
  detecção, parâmetros de geração do LLM etc. — ver
  `pipeline-local-traducao.md`), avaliar se algum desses controles também
  faz sentido no popup da extensão, ou se fica só na janela de settings do
  desktop (hoje é tudo na janela de settings do desktop; não decidido se
  isso muda).
