# Specs (SDD) — Manga Web Translator

Esta pasta existe para que o projeto siga **spec-driven development (SDD)**:
antes de qualquer agente (humano ou IA) implementar algo relevante, a
intenção, as decisões e os porquês ficam escritos aqui. O objetivo é que uma
sessão nova de agente consiga ler estes documentos e continuar o trabalho com
o mesmo contexto que uma conversa longa daria, sem precisar que o Alisson
reexplique tudo.

## Como esta pasta é organizada

- **`DIRETRIZES.md`** — a "constituição" do projeto: princípios que valem
  para qualquer feature (licenciamento, postura ética, trade-offs já
  decididos, armadilhas conhecidas). Qualquer spec abaixo deve respeitar o
  que está aqui; se um spec conflitar com as diretrizes, as diretrizes
  vencem — ou o conflito deve ser discutido com o Alisson antes de
  implementar.
- **Um arquivo por área funcional** (`pipeline-local-traducao.md`,
  `catalogo-modelos.md`, `limite-recursos.md`, `jobs-cache-e-controle.md`,
  `extensao-navegador.md`, `roadmap-modelos-futuros.md`) — cada um descreve:
  - **Objetivo**: por que essa parte existe.
  - **Estado atual**: o que já está implementado e como funciona.
  - **Decisões de design**: escolhas não-óbvias e o porquê (para não serem
    "corrigidas" de volta ao óbvio por um agente futuro sem contexto).
  - **Armadilhas conhecidas**: bugs reais já encontrados e corrigidos, para
    não serem reintroduzidos.
  - **Pendências / próximos passos**: o que falta, na ordem combinada.

## Convenção de uso para agentes

- Antes de tocar em uma área, leia o spec correspondente.
- Ao terminar uma mudança relevante, **atualize o spec** (estado atual,
  pendências) — spec desatualizado é pior que nenhum spec.
- Se uma decisão nova contradiz algo escrito aqui, isso é um sinal para
  perguntar ao Alisson antes de seguir, não para silenciosamente ignorar o
  spec.
- Estes documentos descrevem *intenção e arquitetura*; o código-fonte é
  sempre a fonte de verdade sobre o comportamento exato atual (nomes de
  função, caminhos de arquivo etc. podem ter mudado desde a última edição do
  spec).
