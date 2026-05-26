---
id: frontend-shell/state-store
title: Stores Svelte
shard: 05-frontend-shell
tags: [state, store, svelte]
summary: Stores reactivos alimentados por HTTP REST y SSE; reset al cambiar profile/thread.
related: [frontend-shell/event-stream-ui, frontend-shell/sveltekit-integration]
sources: []
---

# Stores

> Sin Redux ni Pinia. Stores Svelte nativos + `derived` cubren todo el state cliente.

## Stores top-level

```ts
// $lib/stores/session.ts
import { writable } from "svelte/store";
import type { Capabilities, ProfileInfo } from "$lib/api/types";

export const capabilities = writable<Capabilities | null>(null);
export const activeProfile = writable<ProfileInfo | null>(null);

// $lib/stores/threads.ts
export const threads = writable<Map<string, Thread>>(new Map());
export const activeThread = writable<Thread | null>(null);

// $lib/stores/thread.ts (state del thread activo en detalle)
export const items = writable<ItemEvent[]>([]);
export const tasks = writable<Map<string, Task>>(new Map());
export const sessions = writable<Map<string, Session>>(new Map());

// $lib/stores/approvals.ts
export const approvals = writable<ApprovalRequest[]>([]);

// $lib/stores/budget.ts
export const budget = writable<BudgetState | null>(null);

// $lib/stores/memory.ts
export const continuity = writable<Continuity | null>(null);
export const memoryInbox = writable<MemoryProposal[]>([]);  // proposed by agents
```

## Inicialización

```ts
// $lib/stores/init.ts
import { api } from "$lib/api/client";
import * as stores from "$lib/stores";

export async function initialize() {
  // 1. capabilities + profile
  const [caps, profile] = await Promise.all([
    api.capabilities.get(),
    api.profile.current(),
  ]);
  stores.capabilities.set(caps);
  stores.activeProfile.set(profile);

  // 2. continuity banner
  stores.continuity.set(await api.memory.continuity());

  // 3. threads list
  const list = await api.threads.list();
  stores.threads.update(m => { list.forEach(t => m.set(t.id, t)); return m; });

  // 4. (al abrir thread) suscribir SSE
}
```

## Suscribir SSE al abrir thread

```ts
// $lib/stores/thread-activate.ts
import { subscribe } from "$lib/api/sse";
import * as stores from "$lib/stores";

let currentSub: ReturnType<typeof subscribe> | null = null;

export function activateThread(threadId: string) {
  currentSub?.close();

  // reset stores
  stores.items.set([]);
  stores.tasks.set(new Map());
  stores.sessions.set(new Map());

  // load initial state
  api.tasks.list(threadId).then(ts => {
    stores.tasks.update(m => { ts.forEach(t => m.set(t.id, t)); return m; });
  });

  // subscribe SSE
  currentSub = subscribe(threadId);
  currentSub.on("item", (item) => {
    stores.items.update(arr => [...arr, item].slice(-500));
    routeItem(item);
  });
  currentSub.on("task", (e) => { /* update tasks */ });
  currentSub.on("approval", (e) => stores.approvals.update(a => [...a, e]));
}

function routeItem(item: ItemEvent) {
  if (item.kind === "budget.consumed") {
    stores.budget.update(b => ({ ...b, ...item.payload }));
  }
  // ... otros routes
}
```

## Derived stores

```ts
import { derived } from "svelte/store";

export const isStreaming = derived(
  [items, sessions],
  ([$items, $sessions]) => Array.from($sessions.values()).some(s => s.state === "running")
);

export const tasksByStatus = derived(tasks, $t => {
  const groups = { queued: [], in_progress: [], pending_verify: [], paused: [], blocked: [], done: [], abandoned: [] };
  for (const t of $t.values()) groups[t.status].push(t);
  return groups;
});

export const pendingApprovalsCount = derived(approvals, a => a.length);
```

Componentes consumen `$tasksByStatus`, `$pendingApprovalsCount`, etc.

## Reset al cambiar profile/thread

Cambio de profile:
```ts
async function switchProfile(name: string) {
  // backend hace el switch
  await api.profile.use(name);

  // reset todos los stores
  stores.threads.set(new Map());
  stores.activeThread.set(null);
  stores.items.set([]);
  stores.tasks.set(new Map());
  // ...

  // re-init
  await initialize();
}
```

## Persistencia local

Solo preferencias UI en `localStorage`:
- `theme: dark|light`
- `sidebar.collapsed: boolean`
- `lastActiveThread: <thread-uuid>` (para auto-resume al recargar)

**No** persistimos data del backend. Fuente de verdad es el server.

## Idempotencia de updates

Items SSE pueden llegar duplicados tras reconexión con `Last-Event-ID`. Stores idempotentes:

```ts
function applyItem(item: ItemEvent) {
  items.update(arr => {
    if (arr.some(i => i.id === item.id)) return arr;  // ya está
    return [...arr, item].slice(-500);
  });
}
```

## Anti-patrones

| Mal | Bien |
|---|---|
| Estado global mutado fuera de stores | Solo stores; componentes leen con `$store` |
| Persistir tasks en `localStorage` | Backend es la verdad |
| Stores no resetados al cambiar thread | Reset explícito en `activateThread` |
| Re-derive en cada render | Usar `derived(...)` para memoizar |
| Subscribirse a SSE sin desuscribirse al unmount | Close en `onDestroy` |
