---
id: meta/glossary
title: Glosario
shard: 00-meta
tags: [meta, glossary]
summary: Términos clave del harness y sus definiciones canónicas.
related: [foundations/harness-concept, harness-core/turn-and-item-primitives]
sources: [references/sources]
---

# Glosario

| Término | Definición |
|---|---|
| **Harness** | Capa de software entre el modelo y el mundo: orquesta prompt, tools, sandbox, persistencia y UI. No es el modelo; lo envuelve. |
| **Surface** | Cliente que consume el harness: CLI, IDE, Web, Desktop. |
| **Core** | Librería compartida con la lógica del harness. En este proyecto: `harness-core` (Rust). |
| **harness-server** | Único binario backend (Axum). Sirve HTTP+SSE a la UI y aloja threads/tasks/sessions. Anteriormente "App Server" en el modelo Codex. Ver [[app-server/overview]]. |
| **harness-bridge** | MCP server local (uno por spawn) que expone rails Rust al CLI hijo. Ver [[agents/rust-rails]]. |
| **Spawn** | Proceso `claude`/`codex` lanzado para resolver una task. Efímero. Ver [[agents/spawn-lifecycle]]. |
| **Profile** | Namespace aislado con threads, memoria, skills, auth propios. Ver [[cross-cutting/profiles]]. |
| **CONTINUITY.md** | Archivo auto-generado con estado vivo del profile. Ver [[memory/continuity]]. |
| **Rail (Rust rail)** | Función determinística expuesta como tool MCP al CLI hijo. Ver [[agents/rust-rails]]. |
| **Thread** | Sesión durable de conversación. Contiene N turns. Estados: active, archived. |
| **Turn** | Unidad de trabajo iniciada por un mensaje de usuario. Termina cuando el agente emite respuesta final. |
| **Item** | Unidad atómica de I/O dentro de un turn. Ciclo: `started → delta* → completed`. |
| **Tool** | Función invocable por el modelo. Puede ser nativa del core (sandboxed) o de un MCP externo. |
| **MCP** | Model Context Protocol. Servidores externos que exponen tools. Se auto-sandboxean. |
| **Sandbox** | Restricciones de FS/red impuestas al ejecutar tools. Ver [[harness-core/sandbox]]. |
| **Approval** | Pausa solicitando `allow`/`deny` al cliente antes de ejecutar una acción riesgosa. |
| **Compaction** | Sustituir historial por un blob `encrypted_content` que conserva semántica latente. |
| **Context reset** | Limpiar la ventana y pasar estado vía handoff estructurado (alternativa a compaction). |
| **Sprint contract** | Acuerdo Generator↔Evaluator sobre criterios "done" antes de implementar. (Patrón Anthropic). |
| **Load-bearing** | Componente del harness que compensa una limitación del modelo. Si el modelo mejora, puede sobrar. |
| **Skill** | Procedimiento reusable, persistido como Markdown + YAML frontmatter. Memoria procedimental del agente. Hermes-style. Ver [[foundations/lessons-learned]] §H1. |
| **Learner** | Policy que dispara `skill_manage` automáticamente tras turns "aprendibles" (≥5 tool calls, recovery, corrección). |
| **Curator** | Agente de fondo que mantiene el corpus de skills: marca stale/archived, propone consolidaciones. Nunca borra. |
| **GEPA** | Genetic-Pareto Prompt Evolution. Proceso offline que lee traces, propone variantes de prompt/skill y emite PR. |
| **SOUL.md / USER.md / MEMORY.md** | Tres tiers de memoria (Hermes): personalidad, modelo del usuario, episódica. |
| **Profile** | Aislamiento por perfil: cada uno con su HARNESS_HOME, sessions, skills, gateway PID. |
| **Trajectory** | Sesión exportada en formato ShareGPT, base para fine-tuning o eval. |
