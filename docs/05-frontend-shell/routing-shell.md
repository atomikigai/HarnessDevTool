---
id: frontend-shell/routing-shell
title: Sidebar y routing
shard: 05-frontend-shell
tags: [routing, sidebar, navigation, profile]
summary: Layout principal con sidebar fija, badge de profile, atajos globales.
related: [frontend-shell/tech-stack, agents/overview, cross-cutting/profiles, memory/continuity]
sources: []
---

# Routing shell

## Layout

```
┌──────────────┬─────────────────────────────────────────────────────┐
│ sidebar      │ contenido (rutas)                                   │
│              │                                                     │
│ [profile ▼]  │ Inbox approvals: badge ?  (top-right)               │
│              │                                                     │
│ 🏠 Dashboard │                                                     │
│ 💬 Threads   │                                                     │
│ 🤖 Agents    │                                                     │
│ ─────────    │                                                     │
│ 🧠 Skills    │                                                     │
│ 📒 Memory    │                                                     │
│ ─────────    │                                                     │
│ 💾 DB        │   (rutas habilitadas según capabilities)            │
│ 🌐 SSH       │                                                     │
│ ─────────    │                                                     │
│ ⚙ Settings   │                                                     │
└──────────────┴─────────────────────────────────────────────────────┘
```

Profile selector en la parte superior:
```
[ personal ▼ ]    ← click → dropdown con todos los profiles + "Create new"
```

Al cambiar profile, modal de confirmación si hay spawns activos.

## Rutas

| Path | Vista |
|---|---|
| `/` | Dashboard con CONTINUITY banner + lista de threads recientes + budgets |
| `/threads` | Lista de threads (filtrable: active, archived) |
| `/threads/[id]` | Layout con tabs: chat / tasks / sessions / spec / budget |
| `/threads/[id]` (default) | "Activity feed" del thread + composer para enviar prompt |
| `/threads/[id]/tasks` | Vista de tasks con TaskGraph DAG + tabla |
| `/threads/[id]/sessions/[sid]` | TerminalView xterm.js + input |
| `/threads/[id]/spec` | Editor de spec.md (CodeMirror) |
| `/threads/[id]/budget` | BudgetMeter + log de gasto |
| `/agents` | Registry de agentes (overview + capabilities por agente) |
| `/skills` | Tabs: Active / Proposed / Stale / Archived / Bundled (F5+) |
| `/memory` | Tabs: Decisions / Pending / In-flight / Facts / Snapshots |
| `/memory/[id]` | Detalle/editor de entrada |
| `/db` | Conexiones DB (F4) |
| `/ssh` | Hosts SSH (F4) |
| `/settings` | Profile, MCPs, sandbox, telemetry, approvals rules |

## Sidebar dinámica

Items mostrados según `capabilities.features`:
```svelte
{#if $capabilities.features.includes("module.db")}
  <SidebarItem href="/db" icon={iconDatabase} label="DB" />
{/if}
```

Si una feature está deshabilitada en el build, no aparece (route devuelve 404).

## Atajos globales

| Atajo | Acción |
|---|---|
| `Cmd/Ctrl+K` | Command palette (resume thread, switch profile, jump to memory, ...) |
| `Cmd/Ctrl+B` | Toggle sidebar |
| `Cmd/Ctrl+1..9` | Saltar a las primeras rutas de la sidebar |
| `Cmd/Ctrl+L` | Limpiar / abrir thread nuevo |
| `Cmd/Ctrl+Shift+P` | Switch profile |
| `Cmd/Ctrl+Shift+.` | Kill-switch global (pause all) |
| `Esc` | Cerrar modales |

Implementación con un store global `keybinds.ts` + handler en `+layout.svelte`.

## Inbox de approvals (overlay)

Card flotante top-right que muestra:
- Badge contador.
- Click → expande lista de approvals pendientes.
- Cada uno con preview + botones (Allow / Edit & Allow / Deny / Remember).

Visible en cualquier ruta; no depende del thread activo (los approvals pueden venir de cualquier thread del profile).

## Banner de CONTINUITY (dashboard)

En `/` (dashboard), card "Continuidad" con resumen de:
- Threads activos (botones Resume).
- Pendientes editoriales (links a memory).
- Stats rápidas.

Datos vía `GET /api/memory/continuity`. Auto-refresh al recibir SSE events que afecten state.

## PWA install

En F1+, mostrar prompt "Install app" al detectar `beforeinstallprompt`. Manifest.json incluye:
- name: "HarnessDevTool"
- start_url: "/"
- display: "standalone"
- icons: 192/512 PNG

## Anti-patrones

| Mal | Bien |
|---|---|
| Atajos globales sin guard de input focus | Skip si el target es `<input>`/`<textarea>` |
| Sidebar fija que tapa contenido en mobile | Toggle automático en < 768px |
| Rutas hardcoded sin chequear capabilities | Sidebar dinámica + 404 elegante |
| Switch de profile sin confirm con spawns activos | Modal + opción "Kill all and switch" |
| Persistir profile activo en `localStorage` | Backend es la verdad; UI solo refleja |
