---
id: frontend-shell/routing-shell
title: Sidebar y routing
shard: 05-frontend-shell
tags: [routing, sidebar, navigation]
summary: Layout principal con sidebar fija y áreas por módulo.
related: [frontend-shell/tech-stack, module-agents/overview, module-db-manager/overview, module-ssh-manager/overview]
sources: []
---

# Routing shell

## Layout

```
┌──────────┬──────────────────────────────────────────────┐
│ sidebar  │ contenido (rutas)                            │
│          │                                              │
│ Agentes  │                                              │
│ DB       │                                              │
│ SSH      │                                              │
│ ───────  │                                              │
│ Threads  │                                              │
│ Settings │                                              │
└──────────┴──────────────────────────────────────────────┘
```

## Rutas

| Path | Vista |
|---|---|
| `/` | redirige a `/agents` |
| `/agents` | lista de sesiones; `+ New` abre una nueva |
| `/agents/[session]` | terminal + chat stream |
| `/db` | lista de conexiones; `+ Add` |
| `/db/[conn]` | árbol de schema + tabs (query editor, browser) |
| `/db/[conn]/tables/[table]` | tabla virtualizada |
| `/ssh` | lista de hosts |
| `/ssh/[host]` | dos paneles (local / remote) + cola de transferencias |
| `/threads` | índice de threads (resume / fork / archive) |
| `/threads/[id]` | thread embebido en cualquier módulo (overlay) |
| `/settings` | profiles, providers, MCP servers, sandbox |

## Sidebar dinámica
Items mostrados según `capabilities.modules`:

```svelte
{#each $session.capabilities.modules as mod}
  <SidebarItem href="/{mod.slug}" icon={mod.icon} label={mod.label} />
{/each}
```

Si un módulo no está habilitado (build feature off), la ruta no aparece y devuelve 404.

## Atajos
- `Cmd/Ctrl+K` — palette de comandos (resume thread, switch profile, ...).
- `Cmd/Ctrl+1..3` — saltar a Agentes / DB / SSH.
- `Cmd/Ctrl+L` — limpiar/abrir thread nuevo (en módulo Agentes).
