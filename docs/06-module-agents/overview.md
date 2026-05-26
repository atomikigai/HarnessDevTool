---
id: module-agents/overview
title: Módulo Agentes — overview
shard: 06-module-agents
tags: [module, agents, claude-cli]
summary: Lanza y gestiona instancias de `claude` CLI con PTY desde Rust.
related: [module-agents/claude-cli-bootstrap, module-agents/session-pty, module-agents/multi-agent]
sources: []
---

# Módulo Agentes

## Alcance v1
- Lanzar `claude` CLI como child con PTY.
- Una o más sesiones concurrentes.
- Render del PTY en xterm.js dentro de SvelteKit.
- Inyectar input desde la UI; cancelar; reiniciar.
- (Stretch) integrar otros CLIs agénticos (codex, aider) bajo misma API.

## Por qué CLI y no la API del modelo directamente
- Reutilizar configuración, hooks y MCP servers ya instalados por el usuario en su `claude`.
- Mantener compatibilidad con sesiones que el usuario corre fuera del harness.
- El harness actúa como **shell de orquestación**, no reemplazo del agente.

## API JSON-RPC
```
module.agents.session.spawn { profile?, cwd?, args[] } → { session_id, pid }
module.agents.session.input { session_id, data }
module.agents.session.resize { session_id, cols, rows }
module.agents.session.kill  { session_id, signal? }
module.agents.session.list  → [{ session_id, agent, cwd, started_at, pid }]

# notifications
module.agents.session.output { session_id, data }   # bytes del PTY (utf-8 mejor effort)
module.agents.session.exited { session_id, code, signal }
```

## Auditoría
Cada sesión persiste su buffer de PTY en `~/.harness/modules/agents/<session>/output.log` (con rotación). Útil para revisar qué hizo Claude horas después.

## Interacción con el harness-core
Las sesiones de agentes son **opacas** al core (no roban tools). Pero el módulo registra una tool `agents.run` para que un thread del harness pueda **delegar** una sub-tarea a un agente:

```jsonc
{ "tool": "agents.run", "args": { "profile": "claude", "prompt": "Refactor X", "cwd": "..." } }
→ output completo cuando termine
```

Esto habilita patrones tipo Anthropic GAN tri-agente: el thread principal coordina sub-agentes.
