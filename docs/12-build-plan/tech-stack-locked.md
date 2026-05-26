---
id: build-plan/tech-stack-locked
title: Stack tecnológico fijado
shard: 12-build-plan
tags: [stack, decisions, tech]
summary: Tecnologías acordadas tras las conversaciones de planning.
related: [build-plan/decisions-locked, build-plan/repo-layout]
sources: []
---

# Stack fijado

## Backend (Rust)

| Capa | Crate / herramienta | Notas |
|---|---|---|
| Framework HTTP | **axum** | + `tower-http` (CORS, trace, compression, timeout) |
| Runtime async | **tokio** (multi-thread) | feature `full` |
| Serialización | **serde** + **serde_json** + **toml** | |
| Type-sharing Rust→TS | **ts-rs** | derive en structs expuestas; output a `backend/bindings/` |
| Storage estructurado | **sqlx** (SQLite default) | postgres/mysql como features de `module-db` |
| Event log | escritura directa a `.jsonl` | append-only, rotación a `.jsonl.zst` |
| Validación schema | **jsonschema** + **schemars** | schemas en `crates/harness-core/schemas/` |
| Tracing | **tracing** + **tracing-subscriber** + **tracing-appender** | JSON formatter |
| Errores | **thiserror** (libs) + **anyhow** (bin/tests) | |
| PTY | **portable-pty** | cross-OS para F1 |
| SSH/SFTP | **russh** + **russh-sftp** + **russh-keys** | F4 |
| Sandbox | **seccompiler** (linux), `Command` + sandbox-exec (macOS) | F3 |
| Secrets | **keyring** | F2 |
| MCP | implementación propia en `harness-mcp-server` | stdio JSONL |
| Identificadores | **uuid** (v7 para orden temporal) | |
| Time | **time** (no chrono) | |
| Cancellation | **tokio-util::CancellationToken** | |

## Frontend (SvelteKit)

| Capa | Tecnología | Notas |
|---|---|---|
| Framework | **SvelteKit** (Svelte 5) | |
| Adapter | **`@sveltejs/adapter-node`** | corre como server Node dentro del container |
| Estilo | **TailwindCSS** | + tokens shadcn |
| Componentes UI | **shadcn-svelte** | añadidos selectivamente con CLI |
| Iconos | **lucide-svelte** | re-export centralizado en `lib/icons.ts` |
| Editor de código | **CodeMirror 6** | `@codemirror/lang-sql`, `@codemirror/lang-markdown` |
| Terminal | **xterm.js** + addons `fit`, `web-links`, `unicode11` | render del PTY |
| Listas grandes | **TanStack Virtual** | tablas de DB, listas SFTP |
| Markdown streaming | **`marked`** + sanitizer | render incremental de items |
| Validación runtime | **valibot** | F2+, donde sea relevante (ver [[#]]) |
| Estado | **stores Svelte nativos** | sin Redux ni similar |
| Package manager | **pnpm** | |
| Testing | **Vitest** + **Playwright** | Playwright para E2E desde F2+ |
| Lint/format | **eslint** + **prettier** + **svelte-check** | |

## Wire

- HTTP **REST** para CRUD bajo `/api/*`.
- **SSE** (Server-Sent Events) para streaming bajo `/api/events`.
- JSON con tipos generados por `ts-rs` (mismas formas en ambos lados).
- **CORS** habilitado en Axum para `http://localhost:8080` (frontend) en modo dev/local.
- `X-Protocol-Version` header en todas las requests.

## Deploy

- **Docker Compose** con dos servicios: `backend` + `frontend`.
- Backend: imagen distroless con binario static-musl. Bind-mount de `claude`/`codex` del host (ver [[build-plan/phase-1-sessions]]).
- Frontend: imagen `node:alpine` corriendo el server de `adapter-node`.
- Volumen `~/.harness/` mapeado a `/data` del backend.

## Dev

- **Justfile** orquesta tareas (`just dev`, `just gen-types`, `just docker-up`).
- `cargo watch` para backend con hot-reload.
- `vite dev` para frontend con HMR.
- Ambos pueden correr fuera de docker en modo `just dev-local`.

## Lo que **no** está en el stack

- ❌ Tauri (descartado).
- ❌ Electron.
- ❌ Provider clients de Anthropic/OpenAI directos en backend (el modelo lo invoca el CLI hijo).
- ❌ Zod (usamos valibot si necesitamos validador en TS).
- ❌ Turborepo / Nx (mono-repo simple con Justfile).
- ❌ tRPC / GraphQL (HTTP+SSE basta).
- ❌ Estado global cliente pesado (Pinia/Redux): stores Svelte nativos.
