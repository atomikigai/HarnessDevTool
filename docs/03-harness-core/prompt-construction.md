---
id: harness-core/prompt-construction
title: Construcción del prompt
shard: 03-harness-core
tags: [prompt, construction, ordering]
summary: Orden canónico de segmentos para maximizar prefix caching.
related: [harness-core/prompt-caching, harness-core/agent-loop]
sources: [foundations/openai-codex-architecture]
---

# Construcción del prompt

## Orden canónico (estricto)

```
1. developer_message       — sandbox, approval mode, paths del harness
2. config (opcional)       — ~/.harness/config.toml relevante a la sesión
3. AGENTS.md               — agregado desde git root del proyecto
4. env_context             — cwd, shell, OS, model id
5. tool_definitions        — orden determinista (lex por name)
6. history items           — append-only desde turn 1
7. user_input              — del turn actual
```

## Por qué este orden
- 1–5 son **estáticos por sesión** → prefix cache golpea.
- 6 crece append-only → cache extiende.
- 7 invalida solo el último segmento.

## Reglas duras
- **Tool defs**: ordenar lexicográficamente por `name`. Hashes con orden no determinista han causado misses sutiles.
- **AGENTS.md**: cargar UNA vez al inicio del thread; cambios posteriores → re-snapshot en un nuevo developer_message **al final**, no en su lugar.
- **Cambio de cwd**: nuevo developer_message append, no edición.
- **Cambio de sandbox**: idem.

## Builder en Rust

```rust
pub struct PromptBuilder { /* ... */ }

impl PromptBuilder {
    pub fn build(&self, history: &[Item], cfg: &SessionCfg) -> Prompt {
        let mut segs = Vec::new();
        segs.push(self.developer_message(cfg));
        if let Some(c) = self.config_segment(cfg) { segs.push(c); }
        if let Some(a) = self.agents_md(cfg) { segs.push(a); }
        segs.push(self.env_context(cfg));
        segs.extend(self.tool_definitions_sorted(cfg));
        segs.extend(history.iter().filter(|i| i.kind.is_prompt_relevant()).cloned());
        Prompt::from(segs)
    }
}
```

## Diagnóstico de cache miss
El builder loggea `prompt_prefix_hash` por request. Si dos requests adyacentes en el mismo thread no comparten prefix → emite warning con diff de segmentos. Ver [[cross-cutting/logging-tracing]].
