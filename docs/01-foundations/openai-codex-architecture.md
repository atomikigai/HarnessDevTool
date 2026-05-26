---
id: foundations/openai-codex-architecture
title: Arquitectura Codex de OpenAI (Rust core + App Server)
shard: 01-foundations
tags: [openai, codex, rust, architecture, jsonrpc]
summary: Codex core como librería Rust compartida por CLI/IDE/Web/macOS, expuesta vía App Server JSON-RPC.
related: [foundations/harness-concept, app-server/overview, harness-core/agent-loop, harness-core/prompt-caching]
sources: [references/sources]
---

# Arquitectura Codex (OpenAI) — lecciones

Fuente: OpenAI "Harness engineering", "Unlocking the Codex harness", análisis técnicos públicos.

## Tesis: una librería, N surfaces

**Codex core** es una librería **Rust** compartida que contiene:
- Agent loop.
- Lifecycle de threads: create / resume / fork / archive.
- Gestión de config y auth.
- Ejecución de tools en sandbox.

Todas las surfaces (CLI, IDE VSCode, app macOS, Codex Web) usan la **misma librería**. Esto evita reimplementar lógica y garantiza paridad de comportamiento.

## App Server: el broker

El **App Server** es un proceso largo que aloja threads del core y los expone como JSON-RPC bidireccional sobre **stdio** (líneas JSON, JSONL). Nació porque MCP no bastaba para representar estado de sesión (diffs, progreso streaming).

Capas: `stdio transport → message processor → thread manager → core threads`.

**Backward compat**: clientes viejos pueden hablar con servidores nuevos sin romper.

## Agent loop (resumen)

1. Construir prompt JSON desde estado.
2. Stream a Responses API.
3. Parsear tool calls.
4. Ejecutar tools (en paralelo, `FuturesOrdered` para preservar orden).
5. Apendizar resultados al prompt.
6. Re-consultar.
7. Termina cuando el modelo emite mensaje de asistente final.

Cada request lleva historial completo → transmisión O(n²) en bytes a lo largo de la conversación. Mitigado por **prefix caching**.

## Prompt caching: orden estricto

El prompt se construye **append-only**, estático antes que dinámico:

```
[developer msg: sandbox + config path]
[config opcional ~/.codex/config.toml]
[AGENTS.md agregado desde git root]
[env context: cwd, shell]
[user input]
```

**Invalidan caché**:
- Reordenar tool defs (deben ser deterministas).
- Cambiar sandbox/approval mid-conversación.
- Cambiar modelo, cwd, herramientas disponibles.

Cambios "tardíos" se hacen apendizando nuevos developer messages, no insertando antes.

## Compaction

Al exceder `auto_compact_limit`, llamada a `/responses/compact` que devuelve input list más chica, incluyendo un item `type=compaction` con `encrypted_content` (blob opaco que codifica entendimiento semántico).
- Más rico que un resumen en texto.
- Privacy-preserving (clientes ZDR sólo guardan claves de descifrado).

## Primitivas del protocolo

- **Item**: I/O atómico. `item/started → item/*/delta → item/completed`. Permite UI incremental.
- **Turn**: una unidad de trabajo iniciada por un mensaje. Cubre N ciclos modelo↔tool.
- **Thread**: contenedor durable con event history en disco. Soporta resume / fork / archive.

## Ejecución de tools

Tres fuentes:
- **Codex-provided** — sandboxed por el core.
- **API-provided** — server-side de OpenAI.
- **MCP** — externos, **se auto-sandboxean** (responsabilidad del server).

Mecanismo de aprobación: el server emite `approval_request`, pausa, espera `allow`/`deny`.

## Deploy

- **Local** (CLI/IDE): binario App Server bundleado, stdio bidireccional.
- **Web**: App Server en contenedor; cliente habla HTTP+SSE. El agente sobrevive al cierre del tab; al reconectar, el cliente recupera del histórico.

## Stateless request model

Codex **no** usa `previous_response_id`. Cada request es autónomo. Justificación:
- Compliance ZDR.
- Multi-cloud / multi-provider.
- Implementación más simple para forks open-source.

## Por qué replicamos este patrón
- Encapsular la lógica en `harness-core` Rust.
- Exponerla vía `harness-app-server` (JSON-RPC stdio).
- SvelteKit + Tauri consumen el mismo App Server que un futuro `harness-cli`.

Ver [[architecture/system-overview]] y [[app-server/overview]].
