# HarnessDevTool — Documentación Shardeada

> Índice maestro. Cada entrada apunta a un **shard** corto (un archivo, un concepto). Los shards usan frontmatter YAML para que un LLM pueda filtrar por `id`, `tags`, `shard` (grupo) o `related`. Lee primero [[meta/shard-format]] para entender la convención.

## Cómo navegar
- **Por ID**: cada shard tiene un `id` único tipo `harness-core/agent-loop`. Busca por id exacto.
- **Por tag**: filtra el frontmatter por etiquetas (`rust`, `ipc`, `sveltekit`, `sandbox`, ...).
- **Por grupo**: los directorios numerados (`01-foundations`, `02-architecture`, ...) agrupan por dominio.
- **Cross-refs**: enlaces `[[id]]` apuntan a otros shards. Sigue el grafo, no leas todo.

## Grupos

### 00 · Meta
- [[meta/shard-format]] — Convención de frontmatter y enlaces.
- [[meta/conventions]] — Estilo de redacción, nombres, idioma.
- [[meta/glossary]] — Términos: harness, turn, item, thread, sandbox, surface.

### 01 · Fundamentos (qué es un harness y por qué)
- [[foundations/harness-concept]] — Definición y motivación.
- [[foundations/anthropic-principles]] — GAN-inspired tri-agent, context anxiety, evaluator bias.
- [[foundations/openai-codex-architecture]] — Codex core en Rust, App Server, JSON-RPC.
- [[foundations/lessons-learned]] — **Síntesis prescriptiva** OpenAI + Anthropic → este repo. Empezar aquí si vienes nuevo al "porqué".
- [[foundations/design-tradeoffs]] — Cuándo simplificar; qué componente es "load-bearing".

> **Vista visual**: `docs/architecture.html` — diagrama en HTML del sistema completo (abrir en navegador).

### 02 · Arquitectura del sistema
- [[architecture/system-overview]] — Diagrama: surfaces → App Server → harness core → herramientas.
- [[architecture/layered-architecture]] — Capas y responsabilidades.
- [[architecture/process-model]] — Procesos, hilos, child processes.
- [[architecture/ipc-protocol]] — JSON-RPC sobre stdio (JSONL).
- [[architecture/state-persistence]] — Disco, thread history, resume/fork.

### 03 · Harness Core (Rust)
- [[harness-core/rust-crate-layout]] — Workspace Cargo, crates internos.
- [[harness-core/agent-loop]] — Bucle: prompt → modelo → tool calls → repetir.
- [[harness-core/thread-lifecycle]] — create / resume / fork / archive.
- [[harness-core/turn-and-item-primitives]] — Unidad de trabajo y de I/O.
- [[harness-core/prompt-construction]] — Orden estricto para caché.
- [[harness-core/prompt-caching]] — Prefix caching, qué invalida.
- [[harness-core/context-compaction]] — Auto-compact y reset.
- [[harness-core/tool-execution]] — FuturesOrdered, paralelismo.
- [[harness-core/sandbox]] — Permisos de escritura, FS jails.
- [[harness-core/mcp-integration]] — Servidores MCP externos.
- [[harness-core/approval-flow]] — `approval_request`, allow/deny.
- [[harness-core/streaming-events]] — SSE / item delta.
- [[harness-core/auth-and-config]] — `~/.harness/config.toml`, claves.

### 04 · harness-server (Axum, HTTP+SSE)
- [[app-server/overview]] — harness-server: Axum, routes, SSE hub.
- [[app-server/backward-compat]] — Versionado de API (`X-Protocol-Version`).
- [[app-server/web-deployment]] — Docker compose, bind-mounts de claude/codex.
- [[app-server/jsonrpc-transport]] — *[tombstone]* JSON-RPC stdio obsoleto.
- [[app-server/message-processor]] — *[tombstone]* dispatcher obsoleto.

### 05 · Frontend Shell (SvelteKit, sin Tauri)
- [[frontend-shell/tech-stack]] — SvelteKit + adapter-node + shadcn-svelte + valibot.
- [[frontend-shell/sveltekit-integration]] — Cliente HTTP+SSE tipado vía ts-rs.
- [[frontend-shell/event-stream-ui]] — xterm.js + items estructurados.
- [[frontend-shell/state-store]] — Stores Svelte nativos.
- [[frontend-shell/routing-shell]] — Sidebar dinámica, command palette, atajos.
- [[frontend-shell/tauri-vs-app-server]] — *[tombstone]* Tauri descartado.

### 06 · *(obsoleto)* Módulo Agentes
Promovido a runtime principal en la sección 13. Los shards en este dir son tombstones:
- [[module-agents/overview]], [[module-agents/claude-cli-bootstrap]], [[module-agents/session-pty]], [[module-agents/multi-agent]] → ver **[[agents/overview]]** y compañía.

### 07 · Módulo DB Manager (lite)
- [[module-db-manager/overview]] — Alcance: SQLite, Postgres, MySQL.
- [[module-db-manager/supported-engines]] — Drivers Rust (sqlx).
- [[module-db-manager/connection-pool]] — Pool por conexión guardada.
- [[module-db-manager/query-runner]] — Ejecutar / paginar / cancelar.
- [[module-db-manager/schema-introspection]] — Árbol DB → tabla → columna.
- [[module-db-manager/sveltekit-views]] — Tabla virtualizada, editor SQL.

### 08 · Módulo SSH Manager (FileZilla-style)
- [[module-ssh-manager/overview]] — Alcance: SSH + SFTP.
- [[module-ssh-manager/ssh-backend]] — `russh` / `russh-sftp`.
- [[module-ssh-manager/sftp-transfer]] — Cola de transferencias, resumable.
- [[module-ssh-manager/sessions-and-keys]] — Identidades, agente, host keys.
- [[module-ssh-manager/sveltekit-views]] — Dos paneles, drag&drop.
- [[module-ssh-manager/transfer-queue]] — Estado, progreso, retry.

### 09 · Cross-cutting
- [[cross-cutting/logging-tracing]] — `tracing`, spans por turn.
- [[cross-cutting/error-model]] — `thiserror` + códigos JSON-RPC.
- [[cross-cutting/config-files]] — Layout en disco.
- [[cross-cutting/security-model]] — Sandbox, secretos, host keys.
- [[cross-cutting/testing-strategy]] — Unit, integration, eval.
- [[cross-cutting/telemetry]] — Métricas opt-in.
- [[cross-cutting/profiles]] — Aislamiento por contexto (dos trabajos, mismo stack).

### 10 · Recetas
- [[recipes/bootstrap-new-tool]] — Cómo añadir un módulo nuevo.
- [[recipes/add-mcp-server]] — Conectar un MCP server.
- [[recipes/add-frontend-route]] — Nueva vista SvelteKit.

### 11 · Referencias
- [[references/sources]] — URLs fuente con notas.
- [[references/rust-crates]] — Crates externos recomendados.
- [[references/file-tree]] — Layout sugerido del repo.

### 12 · Plan de construcción (vivo) ⭐
- [[build-plan/overview]] — F0–F6, criterios de "done".
- [[build-plan/tech-stack-locked]] — Stack final (Axum · SvelteKit · ts-rs · valibot · Justfile · Docker).
- [[build-plan/repo-layout]] — Estructura definitiva del repo.
- [[build-plan/phase-0-skeleton]] — Server arranca, persiste, sirve shell.
- [[build-plan/phase-1-sessions]] — PTY de claude/codex visible en UI.
- [[build-plan/phase-2-tasks-mcp]] — Máquina de tareas + MCP bridge.
- [[build-plan/phase-3-team]] — Planner/Generator/Evaluator + budget + sandbox.
- [[build-plan/phase-4-modules]] — DB Manager + SSH Manager.
- [[build-plan/phase-5-skills]] — Skills + Learner (proposed) + Curator determinístico + FTS5.
- [[build-plan/phase-6-polish]] — Curator LLM + GEPA + USER.md + packaging.
- [[build-plan/decisions-locked]] — Decisiones fijadas (no re-abrir sin justificación).
- [[build-plan/open-questions]] — Preguntas abiertas por fase, lo siguiente a aclarar.
- [[build-plan/risks]] — Matriz de riesgos con mitigaciones.

### 13 · Agentes (runtime) ⭐
**Mecanismo**
- [[agents/overview]] — Roles, mapa de agentes, diagrama del loop.
- [[agents/spawn-lifecycle]] — Efímero, lease, recovery.
- [[agents/smart-loading]] — 3 niveles de decisión (declaración, recomendación, runtime).
- [[agents/capability-registry]] — Catálogo canónico de MCPs, skill-tags, tools.
- [[agents/rust-rails]] — Funciones determinísticas Rust que el LLM elige (no inventa).

**Agentes — runtime principal**
- [[agents/orchestrator]] — Planner: analiza, clarifica, descompone, declara contratos.
- [[agents/frontend]] — Generator SvelteKit/Tailwind/shadcn.
- [[agents/backend]] — Generator Rust/Axum.
- [[agents/database]] — Generator SQL/sqlx/migrations.
- [[agents/devops]] — Generator Docker/CI/deploy.
- [[agents/qa]] — Evaluator (escribe tests; Rust los corre).
- [[agents/generic]] — Generator fallback sin dominio.
- [[agents/arbitrator]] — Resuelve drift_minor (call corto, barato).

**Agentes — auto-mejora (F5/F6)**
- [[agents/learner]] — Async batch; propone skills en `proposed/`.
- [[agents/curator]] — Background; mantiene corpus de skills, nunca borra.
- [[agents/psychologist]] — Actualiza `USER.md` con preferencias persistentes.

### 14 · Memoria (continuidad) ⭐
- [[memory/overview]] — Las 7 capas, filosofía, decisiones locked.
- [[memory/layout]] — Layout en disco (profiles + shared + memory + skills + threads).
- [[memory/entry-format]] — Frontmatter YAML + Markdown body; kinds y status.
- [[memory/lifecycle]] — Transiciones; approval del humano para escritos de agentes.
- [[memory/continuity]] — `CONTINUITY.md` auto + UI banner + inyección selectiva al resume.
- [[memory/search-and-index]] — SQLite FTS5 + tool `memory.search`.
- [[memory/git]] — Git por profile + shared; commits automáticos; remotes opcionales.
