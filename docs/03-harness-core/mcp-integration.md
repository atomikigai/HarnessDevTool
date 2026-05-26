---
id: harness-core/mcp-integration
title: MCP — somos servidor (no cliente)
shard: 03-harness-core
tags: [mcp, server, bridge, harness-bridge]
summary: harness-mcp-server expone rails Rust como tools MCP al CLI hijo. Cliente MCP es opcional para externos.
related: [agents/rust-rails, agents/capability-registry, agents/spawn-lifecycle, recipes/add-mcp-server]
sources: []
---

# MCP en el harness — somos SERVER

> Cambio importante respecto al modelo Codex: nosotros **exponemos** tools via MCP, no las **consumimos**.

## Roles

| Componente | Rol MCP | Ejemplo |
|---|---|---|
| `harness-mcp-server` | **Server** | Expone `task.*`, `memory.*`, `skills.*` al CLI hijo |
| `claude` / `codex` (CLI hijo) | **Client** | Llama nuestras tools |
| Externos (context7, playwright, ...) | **Server externo** | El CLI hijo decide cargarlos según `spawn_hint` |

El backend no es **cliente** de MCPs externos en el camino crítico. Si configuras context7, lo carga el CLI hijo, no `harness-server`.

## Por qué este flip

En el modelo Codex original:
- El harness era el agent loop, consumía MCPs externos para obtener tools.
- El usuario veía: "esta tool viene de context7".

En nuestro modelo:
- El **CLI hijo** es el agente; **él** consume MCPs (incluido el nuestro).
- El usuario configura los MCPs externos directamente en `claude`/`codex`, no en el harness.
- El harness se enfoca en **proveer rails determinísticas** al CLI: tasks, skills, memory, repo.

## El servidor `harness-mcp-server`

Implementado en `backend/crates/harness-mcp-server/`. Es:
- **Una instancia por spawn** (cada CLI hijo tiene su propio MCP server hablándole).
- **stdio JSONL** (spec MCP).
- **Tools agrupadas por namespace** (`task.*`, `spec.*`, etc.).

Ver catálogo completo en [[agents/rust-rails]] y [[agents/capability-registry]] §"Tools del harness-bridge".

## Cómo el CLI hijo lo descubre

Al spawn, el `harness-session` genera un archivo de config MCP temporal:
```
/tmp/harness-mcp-<spawn-uuid>.json
```
con contenido tipo:
```jsonc
{
  "mcpServers": {
    "harness-bridge": {
      "command": "harness-mcp-server",
      "args": ["--spawn", "<spawn-uuid>"]
    }
  }
}
```
Y lanza el CLI con `--mcp-config /tmp/harness-mcp-<spawn-uuid>.json`. El CLI conecta vía stdio.

Al terminar el spawn, el config temporal se borra.

## MCPs externos (opt-in del usuario)

Si el usuario quiere `context7`, `playwright` u otros:
- Se configuran en `~/.harness/profiles/<active>/config.toml` bajo `[mcp]`.
- Al spawn, el `harness-session` los añade al MCP config del CLI hijo (uno por servidor).
- El CLI los conecta directamente; el harness solo facilita el descubrimiento.

Ver [[recipes/add-mcp-server]].

## Sandbox del MCP externo

Cuando el MCP externo es stdio local (no HTTP), corre bajo el sandbox del SO desde el container backend:
- Linux: seccomp + bind mounts.
- macOS: sandbox-exec profile.
- Windows: AppContainer (F6).

El `harness-mcp-server` propio **no necesita sandbox** porque es código nuestro confiable.

## Tools que NO exponemos al CLI

Por seguridad/scope, NO ofrecemos tools que:
- Permiten al CLI mutar profiles, configs globales o credenciales.
- Permiten escribir directo a `events.jsonl` (es append-only del backend).
- Permiten cambiar el budget del thread (eso es decisión humana).
- Llaman a APIs externas no whitelisted.

Si el CLI necesita una capacidad, se añade vía `capability.request` (ver [[agents/smart-loading]] §"Nivel 3").

## Anti-patrones

| Mal | Bien |
|---|---|
| `harness-server` llamando MCPs como cliente | Lo hace el CLI hijo; nosotros somos server |
| Una sola instancia de MCP server compartida entre todos los spawns | Una por spawn (aislamiento) |
| Tools que mutan estado sin validar contra schemas | Cada tool valida input contra JSON Schema |
| Auth complejo en stdio MCP local | Confianza implícita (es child del backend) |
| MCP externos sin sandbox | Sandbox del SO obligatorio para stdio locals |
