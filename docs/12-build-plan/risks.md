---
id: build-plan/risks
title: Riesgos del plan
shard: 12-build-plan
tags: [risks, mitigations]
summary: Riesgos catalogados con probabilidad/impacto/mitigación y fase donde más duelen.
related: [build-plan/overview, build-plan/open-questions]
sources: []
---

# Riesgos

## Matriz (P = probabilidad, I = impacto, 1–5)

| ID | Riesgo | P | I | Fase | Mitigación |
|---|---|---|---|---|---|
| R1 | `claude`/`codex` no responde a `--mcp-config` como esperamos | 3 | 5 | F2 | spike temprano en F1 con un MCP "hello world"; documentar versión mínima del CLI |
| R2 | Bind-mount del binario del host no funciona dentro de distroless | 3 | 4 | F1 | fallback `debian:slim` ya planificado; validar con hello-world Axum |
| R3 | PTY en Windows con ConPTY tiene bugs | 4 | 3 | F6 | postponer Windows a F6; documentar "Unix-first" |
| R4 | TOML round-trip pierde comentarios al re-serializar | 3 | 3 | F2 | usar `toml_edit` (no `toml`); test de round-trip |
| R5 | Prompt cache miss invisible por re-serialización no determinista | 2 | 4 | F0+ | hashear el prefix y loggear; tests determinismo |
| R6 | Lease + `flock` no portable a Windows | 4 | 2 | F2 | crate `fs2` o lock-file convention; tests cross-platform |
| R7 | SSE buffering en proxies cuando el usuario despliega tras nginx propio | 2 | 3 | F0+ | docs explícitas: `proxy_buffering off` + `X-Accel-Buffering: no` header |
| R8 | Costos descontrolados en F3 sin budget cap robusto | 4 | 5 | F3 | budget hard cap obligatorio; tracking persistido a cada turn (no diferido) |
| R9 | Roles con prompts mal afinados → equipo no converge | 5 | 4 | F3 | tasks-target reproducibles + iteración; budget bajo en dev evita sangría |
| R10 | Sandbox cross-OS frágil (especialmente macOS deprecated `sandbox-exec`) | 4 | 3 | F3+ | level `none` admitido con warning fuerte; documentar limitaciones |
| R11 | Learner genera drafts de skills basura ("ruido") | 4 | 2 | F5 | siempre `proposed/`, nunca auto; humano filtra |
| R12 | Curator LLM review patchea skills agresivamente | 3 | 3 | F6 | snapshots tar.zst + `harness curator rollback`; pinning |
| R13 | GEPA emite PRs malos por tasks-target frágiles | 3 | 3 | F6 | set curado; eval contra baseline obligatoria antes de emitir PR |
| R14 | Drift TS ↔ Rust si `ts-rs` regen no se corre | 2 | 3 | F0+ | hook git pre-commit obligatorio + check en CI |
| R15 | `claude`/`codex` updates rompen contrato (versión nueva del CLI cambia output) | 3 | 4 | F1+ | pin versión soportada en docs; tests E2E contra versión actual |
| R16 | Persistencia: `events.jsonl` corrompido por crash mid-write | 2 | 4 | F0+ | escritura atómica (`tmp + rename`) por línea no es viable → fsync periódico + recovery que truncar línea inválida final |
| R17 | Permisos de bind-mount de `~/.claude/` en Linux (user ID mismatch) | 4 | 3 | F1 | docs explícitas; switchear container a UID del usuario host |
| R18 | shadcn-svelte versión cambia API rompiendo nuestros components | 2 | 2 | F0+ | pin version exacto; revisar en cada upgrade manual |
| R19 | Compilación distroless static-musl para sqlx (`libsqlite3`) | 3 | 3 | F4 | feature `sqlx/sqlite-bundled` para embebido C |
| R20 | TLS cert chain para Postgres dentro del container | 2 | 2 | F4 | montar CA del host como volumen; docs |

## Riesgos top-3 a vigilar

### 🔴 R8 — Costos descontrolados (F3)
**Por qué**: un equipo de 3 generators + 1 evaluator iterando puede quemar $50 USD/hora si los prompts son malos. **Sin budget cap pasa**.
**Acción**: budget cap no es opcional. Test específico en F3 dispara el cap manualmente para verificar pause.

### 🔴 R9 — Prompts de rol mal afinados (F3)
**Por qué**: el "TODO app challenge" depende de que planner descomponga bien, generator implemente correctamente, evaluator distinga done de no-done. Si los prompts no convergen, F3 nunca termina.
**Acción**: invertir días específicos en `roles/*.toml` antes de declarar F3 done; mantener un set de 5 tasks-target que se ejecuten al cambiar prompts.

### 🔴 R1 — MCP config con `claude`/`codex` (F2)
**Por qué**: si los CLIs no aceptan nuestra config como esperamos, todo F2+ se rompe. El plan asume que sí.
**Acción**: spike de 1 día en F1 antes de declarar F1 done — probar que un `claude` con `--mcp-config` apuntando a un MCP "hello world" llama a una tool.

## Riesgos de cronograma (meta)

- **Sub-estimación de F3**: es la fase más larga e incierta. Reservar 2× el tiempo estimado.
- **Bug-fix sin reservar**: cada fase debe reservar 20% de tiempo a fixes de regresión sobre fases anteriores.
- **Distrabilidad por features atractivas**: GEPA y Curator LLM son seductoras pero **no** entregan valor sin F2–F4 sólidos. Disciplina de orden.

## Cómo se cierra/escala un riesgo

- **Detected**: añadir al log de incidentes (`docs/incidents/` cuando ocurra).
- **Mitigated**: marcar P o I más bajo con razón.
- **Materialized**: incident report + post-mortem corto.
