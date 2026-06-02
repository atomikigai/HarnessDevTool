---
id: agents/supported-clis
title: CLIs soportados por el harness
shard: 13-agents
tags: [cli, agents, claude, codex, cursor, antigravity, spawn]
summary: Set fijo de 4 CLIs que el harness sabe spawnear, con la matriz de features por CLI.
related: [agents/spawn-lifecycle, build-plan/decisions-locked, harness-core/mcp-integration]
sources: []
---

# CLIs soportados

> Cierre de **Q5**. Set fijo de 4 CLIs reales + 1 virtual (Zeus). No hay `agent_kind: custom`.

## Set canónico

| CLI            | `AgentKind` enum  | Binario default     | Notas                                                              |
|----------------|-------------------|---------------------|--------------------------------------------------------------------|
| Claude Code    | `Claude`          | `claude`            | Reference implementation; MCP wired.                               |
| Codex          | `Codex`           | `codex`             | OpenAI; MCP wired via per-invocation `-c mcp_servers.*` overrides. |
| Cursor Agent   | `Cursor`          | `cursor-agent`      | Primario para frontend visual en Zeus; sin MCP injection todavía.  |
| Antigravity    | `Antigravity`     | `agy`               | Cubre el rol de cloud/Workspace/context (sin MCP injection).       |
| **Zeus**       | `Zeus`            | *(virtual → Claude)*| **No es un CLI** — orquestador. Corre un Claude PTY con el briefing de Zeus hasta F3. Ver [[agents/zeus-orchestrator]]. |

## Matriz de features

| Feature                          | Claude | Codex | Cursor | Antigravity | Zeus (→ Claude) |
|----------------------------------|--------|-------|--------|-------------|-----------------|
| Spawn vía PTY                    | ✅     | ✅    | ✅     | ✅          | ✅ (via Claude) |
| `--session-id` pin               | ✅     | ✗     | ✗      | ✗           | ✅              |
| Default model                    | `claude-opus-4-7` | `gpt-5.5` | CLI default | CLI default | `claude-opus-4-7` |
| Default effort                   | `medium` | `medium` | CLI default | CLI default | `medium` |
| MCP injection                    | ✅ `--mcp-config` | ✅ `-c mcp_servers.*` | ✗      | ✗           | ✅              |
| `--append-system-prompt` silent  | ✅     | ✗     | ✗      | ✗           | ✅              |
| `--disallowed-tools`             | ✅     | ✗     | ✗      | ✗           | ✅              |
| `--dangerously-skip-permissions` | ✅     | ✗     | ✗      | ✗           | ✅              |
| Auth bind-mount (`~/.X/`)        | `.claude` | `.codex` | `.cursor` | `.antigravity` | `.claude`  |

`✗` = no soportado por el CLI o aún no investigado. Zeus hoy hereda todas las features de Claude (su underlying CLI); F3 cambiará esto al introducir delegation real.

Nota de routing: aunque Cursor todavía no tenga MCP injection, Zeus debe
preferirlo para implementación visual de pantallas, CSS, layout, responsive y
polish cuando pueda darle contexto suficiente por prompt y verificar el
resultado con tests/screenshot/handoff. Para frontend lógico o cambios de datos,
Codex/Claude siguen siendo mejores hasta completar la integración de Cursor.

## Cómo el harness los spawnea

Patrón uniforme (ver [[agents/spawn-lifecycle]]):

1. Resolver binario por path discovery (`which $bin`); si falta, devolver `install_hint`.
2. Construir `cmd` con env vars (`LANG=C.UTF-8`, `LC_ALL=C.UTF-8`, bind-mount auth dir).
3. Append flags específicos por CLI (ver `build_extra_args` en `harness-session::manager`).
4. Spawn en PTY; emitir `session.started { session_id, pid }`.
5. El `session_id` es UUID v4 y se usa como `spawn_id` para tracing y log paths (cierre de Q3).

Claude se lanza con `--model claude-opus-4-7 --effort medium`. Codex se lanza
con `--model gpt-5.5 -c model_reasoning_effort="medium"`. El modelo Codex se
confirmó contra `codex debug models` en `codex-cli 0.134.0`; el CLI lo lista
como `GPT-5.5` con reasoning default `medium`.

## Inyección del prompt inicial

Solo `claude` soporta hoy `--append-system-prompt` para inyección silenciosa. Para los demás, el harness escribe el prompt al PTY 200ms después del spawn (visible al usuario como primer turn). Documentado en [[build-plan/open-questions]] N2.

## Auth y refresh tokens

Cada CLI mantiene su token store en un directorio del home:
- `claude` → `~/.claude/`
- `codex` → `~/.codex/`
- `cursor` → `~/.cursor/`
- `antigravity` → `~/.antigravity/` *(verificar path real al integrar)*

El container del harness hace bind-mount **RW** de estos dirs (cierre N4). Restricción: el host no debe correr el mismo CLI con otra cuenta en paralelo mientras hay sesión activa en el harness.

## Selector en la UI

`NewSessionDialog` muestra las 4 opciones como radio group. Al seleccionar, el form pasa `kind: AgentKind` al endpoint `POST /api/threads/:tid/sessions`.

## Cómo añadir un CLI nuevo

1. Variant nuevo en `AgentKind` (Rust) + actualizar `as_str()` e `install_hint()`.
2. Si soporta flags MCP/system-prompt, extender `build_extra_args`.
3. Añadir opción al `NewSessionDialog` (frontend).
4. Bind-mount del dir de auth en el Dockerfile/compose.
5. Smoke test: spawn + saludo + exit.
6. Fila en la matriz de este shard.
