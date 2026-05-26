---
id: harness-core/approval-flow
title: Flujo de aprobación humana
shard: 03-harness-core
tags: [approval, human-in-the-loop, safety]
summary: Pausa antes de acciones riesgosas; espera allow/deny vía JSON-RPC.
related: [harness-core/tool-execution, architecture/ipc-protocol]
sources: [foundations/openai-codex-architecture]
---

# Approval flow

## Trigger
Una tool emite `approval.request` cuando:
- `tool.requires_approval(args)` devuelve true, **y**
- `cfg.approval_mode != "auto"`.

Modos:
- `auto` — todo se ejecuta sin preguntar (CI, headless).
- `risky-only` — solo acciones destructivas (default).
- `every-call` — pide para cada tool.

## Protocolo

```jsonc
// server → cliente (notification)
{ "method": "approval.request", "params": {
    "id": "req-123",
    "thread": "...", "turn": "...",
    "tool": "shell.exec",
    "args": { "cmd": "rm -rf node_modules" },
    "risk_explanation": "Recursive delete inside workspace",
    "expires_at": "2026-05-26T12:00:00Z"
}}

// cliente → server (request)
{ "method": "approval.respond", "params": {
    "id": "req-123",
    "decision": "allow",       // allow | deny | allow-and-remember
    "note": "OK, esperado"
}}
```

## Estados internos
```
pending → allowed → executing → completed
        ↘ denied  → cancelled
        ↘ expired → cancelled
```

## Timeout
- Default 5 min. Si expira → `cancelled (reason = approval-expired)`.
- El modelo recibe el resultado como tool error con causa.

## "Allow and remember"
- Persiste una regla en `~/.harness/policy.toml`: `<tool, args-pattern> → allow`.
- Útil para evitar prompts repetidos en flujos rutinarios.
- Auditable: cada regla guarda quién/cuándo.

## UI
- Modal no-bloqueante en SvelteKit: muestra cmd + diff esperado + botones.
- En CLI: prompt interactivo con `y`/`n`/`d` (details).

## Anti-patrón
- Pedir aprobación para CADA call por defecto → fatiga, el usuario aprueba todo a ciegas. Mantener `risky-only`.
- Aprobaciones "silenciosas" sin riesgo explicado → el usuario no aprende qué decir no.
