---
id: foundations/harness-concept
title: Qué es un harness
shard: 01-foundations
tags: [concept, foundation, motivation]
summary: Definición operativa de "harness" y por qué Rust es buena elección de núcleo.
related: [foundations/anthropic-principles, foundations/openai-codex-architecture, foundations/design-tradeoffs]
sources: [references/sources]
---

# Qué es un harness

Un **harness** es la capa de software entre un modelo de lenguaje y el mundo real (FS, red, procesos, UI). El modelo no ejecuta nada por sí mismo: produce tokens. El harness convierte esos tokens en acciones útiles y devuelve resultados al modelo, repitiendo el ciclo hasta una respuesta final.

## Responsabilidades canónicas
1. **Construir el prompt** desde estado: system, config, tools, historial, input.
2. **Llamar al modelo** vía API (streaming, retry, backoff).
3. **Parsear tool calls** y ejecutarlas (sandbox, paralelismo, paginación).
4. **Acumular historial** sin invalidar el prefix cache.
5. **Gestionar contexto**: compactar o resetear cuando se llena la ventana.
6. **Persistir** threads y eventos para resume/fork.
7. **Exponer eventos** a surfaces (CLI/IDE/Web) vía un protocolo estable.
8. **Aplicar políticas**: aprobación humana, límites de gasto, ZDR.

## Por qué un núcleo en Rust
- **Un solo binario** para varias surfaces (CLI, daemon, embed en desktop).
- **Latencia y memoria** bajo control: ideal para streaming SSE y procesos largos.
- **Sandbox**: control fino sobre `seccomp` (Linux), `sandbox-exec` (macOS), AppContainer (Windows).
- **Drivers maduros**: `tokio`, `tracing`, `sqlx`, `russh`, `serde_json`, `tower`.
- **FFI**: `napi-rs` / `pyo3` / `wasm` si una surface necesita embed in-process.
- **Una librería, N surfaces**: misma lógica detrás de CLI, App Server y Tauri. Patrón Codex.

## Lo que **no** es el harness
- No es el modelo. No "razona".
- No es la UI. La UI consume eventos.
- No es una librería de prompts: aloja prompts, no los inventa por dominio.

## Tesis central
> A medida que los modelos mejoran, el espacio de combinaciones interesantes de harness **no se reduce — se mueve**. El harness debe re-evaluarse en cada release del modelo. (Anthropic)

Esto justifica: módulos pequeños, contratos estables, fácil retirar componentes que se vuelven sobra. Ver [[foundations/design-tradeoffs]].
