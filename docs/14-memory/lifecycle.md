---
id: memory/lifecycle
title: Memoria — lifecycle y transiciones
shard: 14-memory
tags: [memory, lifecycle, transitions, approval]
summary: Cómo se crean, mueven y archivan las entradas; quién puede hacer qué.
related: [memory/overview, memory/entry-format, memory/continuity]
sources: []
---

# Lifecycle de las entradas

## Máquina de transiciones por kind

```
                 user / orchestrator (con approval del user)
                              │
                              ▼
              ┌──────────────────────────┐
              │       in_flight (active) │
              │  (tema en discusión)     │
              └─┬──────────────┬─────────┘
   resuelve     │              │   postpone
                ▼              ▼
   ┌────────────────────┐  ┌─────────────────────┐
   │ decision (settled) │  │  pending (open)     │
   │  inmutable*        │  │  retomable          │
   └────────┬───────────┘  └──────┬──────────────┘
            │                     │ retomar
            │                     ▼
            │              ┌──────────────────────┐
            │              │  in_flight (active)  │
            │              └──────────────────────┘
            │
            │ obsoleta
            ▼
   ┌─────────────────────┐
   │ decision (obsolete) │  no se borra; queda para auditoría
   └─────────────────────┘

fact (active) ─────► fact (obsolete)   cuando el patrón cambia

snapshot (active) ─► snapshot (obsolete)  tras 30d → archive tras 90d
```

\* "Inmutable" semánticamente: el contenido puede patcharse para clarificar, pero la decisión no cambia. Si cambia → la marca obsolete y crea una nueva.

## Quién puede hacer cada transición

| Transición | Humano | Orchestrator | Otro agente | Aprobación |
|---|---|---|---|---|
| Crear `in_flight` | ✅ | ✅ | ⚠️ con approval | humano para agentes |
| Crear `decision` | ✅ | ✅ | ⚠️ con approval | humano para agentes |
| Crear `pending` | ✅ | ✅ | ⚠️ con approval | humano para agentes |
| Crear `fact` | ✅ | ⚠️ con approval | ⚠️ con approval | humano siempre |
| Promover `in_flight → decision` | ✅ | ✅ | ❌ | humano si lo hizo orchestrator |
| Demote `in_flight → pending` | ✅ | ✅ | ❌ | humano si lo hizo orchestrator |
| Retomar `pending → in_flight` | ✅ | ✅ | ❌ | no |
| Marcar `obsolete` | ✅ | ⚠️ con approval | ❌ | humano siempre |
| Editar cuerpo | ✅ | ⚠️ con approval | ⚠️ con approval | humano para agentes |
| Borrar | ❌ | ❌ | ❌ | nunca; solo `obsolete` |

**Regla maestra**: cuando un agente quiere escribir memoria, **siempre** dispara `approval.request`. El humano ve la entrada propuesta antes de que se persista. (Decisión bloqueada).

## Approval flow para `memory.note` de un agente

```
1. Agente llama tool: memory.note(kind, title, body, related_*)
2. Harness valida schema → ok
3. Harness genera draft en RAM (no escribe disco aún)
4. Harness emite approval.request al humano:
   {
     "preview": "<draft completo de la entry>",
     "diff": "<si es update; ya hay versión previa>",
     "agent": "frontend-1",
     "task": "T-0042"
   }
5. Humano en UI: Allow / Edit & Allow / Deny
6. Si Allow → harness escribe + commit git
7. Si Edit & Allow → humano edita en modal, harness escribe versión editada
8. Si Deny → response al agente: { ok: false, reason: "user denied" }
```

UI: una "Inbox" lateral muestra las propuestas pendientes. El humano puede dejarlas para después; el agente recibe `pending_user` (no error) y sigue trabajando sin bloquearse.

## Eventos de lifecycle

Cada transición emite un evento al `events.jsonl` del thread relacionado:

```jsonc
{ "kind": "memory.created", "id": "memory/decisions/2026-05-26-tauri-out", "by": "user", "at": "..." }
{ "kind": "memory.transitioned", "id": "...", "from": "in_flight", "to": "decision", "by": "orchestrator", "approved_by": "user", "at": "..." }
{ "kind": "memory.updated", "id": "...", "patch_summary": "...", "by": "user", "at": "..." }
{ "kind": "memory.obsoleted", "id": "...", "reason": "...", "by": "user", "at": "..." }
```

Esto permite reconstruir el historial sin leer todo el git log.

## Snapshots

Generados automáticamente por el scheduler del harness:
- **Cada 6h** mientras hay actividad.
- **Al cierre de un thread** importante (manual o por kill-switch global).
- **On demand** vía `harness memory snapshot now`.

Contenido:
- Threads activos + status.
- Decisiones de las últimas 24h.
- Pendientes vivos.
- Aprendizajes destacados (skills recientes).

Rotación:
- Activos: últimos 7 días.
- 7–30 días: status `active` pero comprimidos.
- 30–90 días: status `obsolete`, accesibles vía búsqueda.
- > 90 días: archivados a `memory/snapshots/.archive/<año>.tar.zst`.

## Promoción de in-flight a decision (caso común)

Cuando una discusión se cierra:

```
Humano (en UI): "Marcar resolved" sobre una entrada in-flight
   │
   ▼
Harness pregunta: "¿Promover a decision?"
   │
   ▼
Si sí:
   - copia frontmatter, cambia kind: decision, status: settled
   - mantiene id pero mueve archivo de in-flight/ a decisions/
   - el archivo viejo en in-flight/ se elimina (no es destructivo: el git mantiene la historia)
   - related_memory backlinks se actualizan
   - commit: "memory: promote <id> in_flight → decision"
```

Si se postpone:
```
   - kind: pending, status: open
   - movido a pending/
```

## Limpiar y archivar (Curator)

El Curator (F6) puede actuar sobre memoria también, con reglas estrictas:
- **Nunca borra** una `decision`, ni siquiera obsolete.
- Snapshots viejos (>90d) pueden moverse a `.archive/`.
- `fact` obsolete y sin uso 60+ días puede archivarse (con backup).
- `pending` muy viejos (>180d sin retomar) → propone marcar como `abandoned` (kind nuevo: `abandoned`? o status `obsolete`?). Por ahora: usa `status: obsolete` con razón en cuerpo.

El Curator opera **solo con approval** sobre memoria, a diferencia de skills donde tiene más libertad.
