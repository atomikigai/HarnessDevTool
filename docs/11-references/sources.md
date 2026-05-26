---
id: references/sources
title: Fuentes
shard: 11-references
tags: [references, sources]
summary: URLs y notas de las fuentes que informaron la arquitectura.
related: [foundations/anthropic-principles, foundations/openai-codex-architecture, foundations/lessons-learned]
sources: []
---

# Fuentes

## Primarias

### Anthropic — Harness Design for Long-Running Application Development
- URL: https://www.anthropic.com/engineering/harness-design-long-running-apps
- Aporta: tri-agente (planner/generator/evaluator), context anxiety, sprint contracts, file-based communication, load-bearing components.
- Procesado en: [[foundations/anthropic-principles]].

### OpenAI — Harness engineering: leveraging Codex in an agent-first world
- URL: https://openai.com/index/harness-engineering/
- Nota: el fetch directo está bloqueado; el contenido se obtuvo de fuentes secundarias y de OpenAI "Unlocking the Codex harness".

### OpenAI — Unlocking the Codex harness (App Server)
- URL: https://openai.com/index/unlocking-the-codex-harness/
- Aporta: App Server, JSON-RPC stdio, thread lifecycle, items/turns/threads como primitivas, backward compat, web deploy.

### OpenAI — Unrolling the Codex agent loop
- URL: https://openai.com/index/unrolling-the-codex-agent-loop/
- Aporta: detalles del agent loop, FuturesOrdered, stateless requests, compaction, prefix caching.

### Nous Research — Hermes Agent (self-improving)
- Landing: https://hermes-ai.net/
- Docs (arquitectura): https://hermes-agent.nousresearch.com/docs/developer-guide/architecture
- Docs (Curator): https://hermes-agent.nousresearch.com/docs/user-guide/features/curator
- Aporta: closed-loop learning (Learner + Curator + GEPA), Skills como memoria procedimental (Markdown + YAML), `skill_manage` tool, telemetría `~/.hermes/skills/.usage.json`, profile isolation, tres tiers de memoria (SOUL/USER/MEMORY), tool registry como raíz del grafo de deps, design principles (prompt stability, observable execution, interruptible, platform-agnostic core, loose coupling).
- Procesado en: [[foundations/lessons-learned]] §H.

### Hermes Agent Advanced (Roan Brasil Monteiro)
- URL: https://medium.com/@roanmonteiro/hermes-agent-advanced-self-evolving-skills-mcp-subagents-and-production-8c827c79ce7e
- Aporta: principio crítico de auditoría — "Un agente que se auto-aprende sin auditoría es peligroso no por autónomo, sino porque su comportamiento cambia de modos que no ves, no rastreas y por lo tanto no puedes corregir". Citado en [[foundations/lessons-learned]] §H6.

## Secundarias (resúmenes técnicos públicos)

### SWE Quiz — How OpenAI built Codex
- URL: https://www.swequiz.com/articles/openai-codex-architecture
- Aporta: detalle técnico del Codex core (Rust), JSON-RPC App Server, prompt caching gotchas, primitivas Item/Turn/Thread.

### ZenML LLMOps DB — Codex CLI Architecture and Agent Loop Design
- URL: https://www.zenml.io/llmops-database/building-production-ready-ai-agents-openai-codex-cli-architecture-and-agent-loop-design
- Aporta: stateless model, endpoints por auth, integración con MCP, gotchas de caché.

## Citas relevantes
> "As models improve, the space of interesting harness combinations doesn't shrink. Instead, it moves." — Anthropic.

> "Codex core is a shared Rust library that contains the agent loop, thread lifecycle, config and auth management, and sandboxed tool execution." — OpenAI / análisis técnico.

> "File-based communication maintains fidelity to spec without overspecifying implementation." — Anthropic.
