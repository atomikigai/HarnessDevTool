---
id: agents/smart-loading
title: Smart/lazy loading de capacidades
shard: 13-agents
tags: [smart-loading, capabilities, mcp, skills]
summary: Tres niveles de decisión sobre qué MCPs/skills/tools cargar al spawn.
related: [agents/overview, agents/spawn-lifecycle, agents/capability-registry, agents/rust-rails]
sources: []
---

# Smart loading

> Idea central: un agente **declara** lo que *puede* cargar, el sistema **carga** solo lo *necesario para esta task*. Cada MCP o skill cargado infla el prompt, consume tokens y desconcentra. Cargar menos = más barato y más preciso.

## Los tres niveles de decisión

### Nivel 1 — Declaración estática (en el shard del agente)
El shard de cada agente declara `capabilities` que constituyen el **límite duro**. Nada fuera de esto puede cargarse, nunca.

```toml
# tomado del shard del agente
[capabilities]
mcp_available    = ["harness-bridge", "context7", "playwright"]
skill_tags       = ["svelte", "tailwind", "shadcn", "a11y", "frontend-design"]
tools_allowed    = ["task.*", "spec.read", "shell.exec", "browser.*"]
```

### Nivel 2 — Recomendación (orchestrator al crear la task)
Cuando el orchestrator descompone, declara su sugerencia en la task TOML:

```toml
# en T-0042.toml
[spawn_hint]
mcp     = ["harness-bridge"]
skills  = ["svelte"]
tools   = ["task.*", "spec.read", "shell.exec"]
reason  = "Paginación SvelteKit; patrón estándar; sin docs externas ni E2E"
```

El orchestrator usó rails Rust para conocer las capacidades disponibles (ver [[agents/rust-rails]] §`agents.describe`).

### Nivel 3 — Solicitud en runtime (el agente)
El agente puede pedir más durante la ejecución vía tool MCP:

```jsonc
{ "tool": "capability.request",
  "args": { "mcp": ["context7"], "reason": "Necesito docs de @sveltejs/kit v3" }
}
```

Resultado:
- Si está en `capabilities.mcp_available` del agente → `granted` + se carga en caliente (el MCP child se conecta).
- Si no → `denied: not in your declared capabilities`. Si insiste 3 veces → el harness lo loggea y emite warning al humano.

## Algoritmo al spawn (Rust)

```rust
fn resolve_load(agent: &AgentSpec, task: &Task) -> LoadedCapabilities {
    // 1. Empezar con spawn_hint de la task
    let mut load = task.spawn_hint.clone();

    // 2. Si no hay spawn_hint, aplicar heurística fallback
    if load.is_empty() {
        load = infer_hint_from_task(task);
    }

    // 3. Validar contra declared del agente — todo lo extra se rechaza
    load.mcp.retain(|m| agent.capabilities.mcp_available.contains(m));
    load.skills.retain(|s| agent.capabilities.skill_tags.contains(s));
    load.tools.retain(|t| matches_glob_any(t, &agent.capabilities.tools_allowed));

    // 4. Asegurar mínimos (harness-bridge siempre)
    load.mcp.push_if_missing("harness-bridge");
    load.tools.push_if_missing("task.list");
    load.tools.push_if_missing("task.update");

    load
}
```

## Heurística fallback (sin orchestrator)

Cuando la task viene sin `spawn_hint` (humano la creó manual, o vino de un import):

```rust
fn infer_hint_from_task(task: &Task) -> SpawnHint {
    let mut h = SpawnHint::minimal();

    // Por file patterns
    if task.touches.iter().any(|p| p.ends_with(".svelte")) { 
        h.skills.push("svelte"); 
    }
    if task.touches.iter().any(|p| p.starts_with("crates/") || p.ends_with(".rs")) {
        h.skills.push("rust-patterns");
    }
    if task.touches.iter().any(|p| p.contains("migrations/") || p.ends_with(".sql")) {
        h.skills.push("sql");
    }

    // Por labels
    for lbl in &task.labels {
        match lbl.as_str() {
            "a11y" | "accessibility" => h.skills.push("a11y"),
            "docs" | "research"      => h.mcp.push("context7"),
            "e2e" | "browser-test"   => h.mcp.push("playwright"),
            _ => {}
        }
    }

    // Por keywords del título (defensa última)
    let title = task.title.to_lowercase();
    if title.contains("docs") || title.contains("how to") {
        h.mcp.push_if_missing("context7");
    }

    h
}
```

Esta heurística es **conservadora**: si duda, no carga. El agente puede pedir más en runtime.

## Auditoría

Cada spawn registra en `meta.toml`:
- `spawn_hint` original (lo que se decidió antes).
- `loaded_capabilities` final (tras filtrado y mínimos).
- `runtime_requests` (lista de solicitudes nivel 3 + decisión).

Esto permite:
- Diagnóstico: "¿por qué este agente no tenía X?" → mirar el log.
- Aprendizaje: el learner mira spawn_hints vs runtime_requests; si muchos agentes piden lo mismo en runtime → propone añadirlo al default del spawn_hint del orchestrator.

## Cost saving estimado

| Escenario | Sin smart load | Con smart load |
|---|---|---|
| Task trivial (remove a print) | ~10K tokens iniciales | ~2K tokens iniciales |
| Task media (refactor componente) | ~15K | ~5K |
| Task compleja (build feature) | ~25K | ~10K |

Multiplicado por N spawns × N turns, ahorro material.

## Anti-patrones

- Cargar todas las capabilities por defecto "por si acaso".
- Permitir `capability.request` de cosas no declaradas (el agente se vuelve cualquier cosa).
- Olvidar `harness-bridge` en los mínimos (el agente queda sin task.*).
- Confiar que la heurística fallback es suficiente; el orchestrator debe seguir poniendo `spawn_hint` siempre que pueda.
