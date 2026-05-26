---
id: frontend-shell/state-store
title: State store (Svelte)
shard: 05-frontend-shell
tags: [state, store, svelte]
summary: Stores reactivos alimentados por notifications JSON-RPC.
related: [frontend-shell/event-stream-ui, frontend-shell/sveltekit-integration]
sources: []
---

# Stores

## Stores top-level

```ts
// $lib/stores/session.ts
export const session = writable<{ capabilities: Capabilities, profile: string }>();

// $lib/stores/threads.ts — índice de threads
export const threads = writable<Map<string, ThreadMeta>>(new Map());

// $lib/stores/thread.ts — thread activo
export const thread = writable<ThreadState | null>(null);

// $lib/stores/modules.ts
export const dbConnections = writable<Connection[]>([]);
export const sshHosts = writable<Host[]>([]);
export const agentSessions = writable<AgentSession[]>([]);
```

## Wiring de notifications → stores

```ts
rpc.on("item.started", (p) => thread.update(s => reduceItem(s, p)));
rpc.on("item.delta", ...);
rpc.on("item.completed", ...);
rpc.on("turn.completed", ...);
rpc.on("approval.request", (p) => approvals.update(a => [...a, p]));
rpc.on("module.db.connection.added", (p) => dbConnections.update(c => [...c, p]));
```

## Derived stores
```ts
export const isStreaming = derived(thread, $t => $t?.activeTurn?.items.some(i => i.status === "open"));
export const lastAssistant = derived(thread, $t => $t?.activeTurn?.items.findLast(i => i.kind === "assistant_message"));
```

## Persistencia local (UI)
- Preferencias de UI (theme, sidebar collapsed): `localStorage`.
- **No** persistir threads en el cliente — la fuente de verdad es el App Server.

## Concurrencia de updates
Las notifications llegan en orden por (thread, turn). Si llegan de varios threads concurrentes, el reducer las separa por `thread_id` antes de aplicar.
