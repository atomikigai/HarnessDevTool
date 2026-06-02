---
id: harness-core/approval-flow
title: Approval flow (humano in the loop)
shard: 03-harness-core
tags: [approval, human-in-the-loop, safety]
summary: Pausa antes de acciones sensibles; espera allow/deny del humano vía SSE/UI.
related: [agents/overview, agents/autonomy-protocol, harness-core/tool-execution, memory/lifecycle, architecture/ipc-protocol]
sources: []
---

# Approval flow

> Aplica a **tools del harness-bridge** que mutan estado sensible. Las tools internas del CLI hijo (shell.exec, edits) tienen su propio approval (del propio `claude`/`codex`); el harness no intercepta esos.

## Cuándo dispara

Una tool del harness emite `approval.requested` cuando:
- `tool.requires_approval(args) == true`.
- Y la `approval_mode` del thread no es `auto`.

Modos por thread:
- `auto` — todo se ejecuta sin preguntar. CI / batch.
- `risky-only` — solo tools que el catálogo marca sensibles (default).
- `every-call` — pide para cada tool del harness-bridge. Solo para debugging.

El `approval_mode` efectivo puede derivarse del `autonomy_profile` del thread
cuando no se define manualmente. Ver [[agents/autonomy-protocol]]:
- `manual` → `every-call` para tools sensibles.
- `assisted` → `risky-only`.
- `autonomous` → `auto` para allowlists del proyecto y `risky-only` fuera de ellas.
- `ci` → `auto`; si falta aprobacion o recurso, falla con error estructurado.

Tools típicamente con `requires_approval`:
- `memory.note`, `memory.update` (siempre, salvo orchestrator)
- `skills.manage(action="create"|"edit")` antes de F5
- `tasks.create` cuando creada por agente no-orchestrator
- `contracts.elevate_declared`
- `budget.set_cap`

## Protocolo

```jsonc
// SSE event (server → browser)
event: approval
data: {
  "id": "req-01HX...",
  "thread": "...",
  "spawn": "...",
  "tool": "memory.note",
  "args_preview": { "kind": "decision", "title": "...", ... },
  "preview_body": "...",          // markdown render del cuerpo propuesto
  "risk_explanation": "Va a crear una entrada de memoria nueva",
  "expires_at": "2026-05-26T19:35:00Z"
}

// HTTP POST (browser → server)
POST /api/approvals/req-01HX.../respond
{
  "decision": "allow" | "deny" | "allow-and-remember" | "edit-and-allow",
  "edited_args": { ... },            // si edit-and-allow
  "note": "OK, esperado"
}
```

El CLI hijo, mientras tanto, está bloqueado en la llamada MCP. El backend:
- Mantiene la llamada pendiente con timeout (5 min default).
- Al recibir respuesta del humano, completa la llamada.
- Si timeout → tool falla con `approval_timeout`; el CLI puede re-intentar o reportar.

## Estados internos

```
pending → allowed → executing → completed
        ↘ denied  → cancelled
        ↘ expired → cancelled
        ↘ edited  → allowed (con args modificados)
```

## "Allow and remember"

Crea regla en `~/.harness/profiles/<active>/policy.toml`:
```toml
[[policies]]
tool = "memory.note"
args_match = { kind = "fact", source = "agent:learner-1" }
action = "allow"
created_at = "..."
created_by = "user"
```

Próxima vez que un call matchee, se aprueba automáticamente. Reglas listables/editables en UI bajo `/settings/approvals`.

## UI

- **Inbox lateral** muestra aprobaciones pendientes (badge contador en sidebar).
- Card por aprobación: tool, args preview, risk_explanation, botones (Allow / Edit & Allow / Deny / Allow-and-remember).
- Si hay > 5 pendientes simultáneas → modal centralizado para batch decision.
- Notification toast cuando llega nueva approval mientras el usuario está en otra ruta.

## Tools de aplicación del CLI hijo

`claude`/`codex` tienen su **propio** approval flow para `shell.exec`, `edit`, etc. Eso pasa **dentro del PTY** — el usuario lo ve en el terminal de su sesión y responde ahí. **Nosotros no interferimos**.

Distinguir:
- Approval del **CLI** → en xterm.js, respuesta del usuario tecleando en el terminal.
- Approval del **harness** → en UI lateral, click de botón, vía HTTP.

Ambos son legítimos; cubren scopes distintos.

## Anti-patrones

| Mal | Bien |
|---|---|
| Approval para CADA call (fatiga, usuario aprueba todo a ciegas) | `risky-only` por default |
| Aprobación silenciosa sin explicar riesgo | `risk_explanation` obligatorio |
| Reglas allow-and-remember sin args matching | Match específico para no auto-aprobar lo equivocado |
| Timeout muy corto (< 1 min) | 5 min default; configurable por tool |
| Interceptar approval del CLI hijo | Es problema del CLI; respeta su flujo |
