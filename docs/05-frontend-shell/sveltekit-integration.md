---
id: frontend-shell/sveltekit-integration
title: SvelteKit ↔ harness-server (HTTP + SSE)
shard: 05-frontend-shell
tags: [sveltekit, http, sse, integration, ts-rs, valibot]
summary: Cliente tipado fetch + EventSource; tipos via ts-rs; validación valibot opcional.
related: [frontend-shell/state-store, frontend-shell/event-stream-ui, architecture/ipc-protocol]
sources: []
---

# Integración SvelteKit ↔ backend

## Capas

```
SvelteKit components
        │
        ▼
$lib/api/client.ts    (fetch wrapper, types tipados)
$lib/api/sse.ts       (EventSource wrapper, reconexión)
$lib/api/types/       (autogenerado por ts-rs desde Rust)
$lib/validators/      (valibot, opcional para inputs sensibles)
        │
        ▼
HTTP REST + SSE  ──►  harness-server (Axum)
```

## Cliente HTTP tipado

```ts
// $lib/api/client.ts
import type * as T from "./types";

const BASE = import.meta.env.PUBLIC_API_BASE_URL ?? "/api";

async function request<R>(method: string, path: string, body?: unknown): Promise<R> {
  const res = await fetch(`${BASE}${path}`, {
    method,
    headers: { "Content-Type": "application/json", "X-Protocol-Version": "1.0" },
    body: body ? JSON.stringify(body) : undefined,
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new ApiError(res.status, err);
  }
  return res.json();
}

export const api = {
  threads: {
    list: () => request<T.Thread[]>("GET", "/threads"),
    get: (id: string) => request<T.Thread>("GET", `/threads/${id}`),
    create: (body: T.ThreadCreate) => request<T.Thread>("POST", "/threads", body),
    archive: (id: string) => request<void>("DELETE", `/threads/${id}`),
  },
  tasks: {
    list: (threadId: string) => request<T.Task[]>("GET", `/threads/${threadId}/tasks`),
    create: (threadId: string, body: T.TaskCreate) => request<T.Task>("POST", `/threads/${threadId}/tasks`, body),
    update: (threadId: string, taskId: string, patch: T.TaskUpdate) =>
      request<T.Task>("PATCH", `/threads/${threadId}/tasks/${taskId}`, patch),
  },
  sessions: {
    spawn: (threadId: string, body: T.SessionSpawn) =>
      request<T.Session>("POST", `/threads/${threadId}/sessions`, body),
    input: (sid: string, data: string) =>
      request<void>("POST", `/sessions/${sid}/input`, { data }),
    resize: (sid: string, cols: number, rows: number) =>
      request<void>("POST", `/sessions/${sid}/resize`, { cols, rows }),
    kill: (sid: string) => request<void>("DELETE", `/sessions/${sid}`),
  },
  // ... memory, skills, profiles
};
```

## Cliente SSE

```ts
// $lib/api/sse.ts
export type ItemEvent = { id: string; kind: string; thread: string; payload: any };
export type TaskEvent = { task: string; from: string; to: string };

export function subscribe(threadId: string) {
  const url = `${BASE}/events?thread=${threadId}`;
  const es = new EventSource(url);

  const handlers = {
    item: new Set<(e: ItemEvent) => void>(),
    task: new Set<(e: TaskEvent) => void>(),
    approval: new Set<(e: any) => void>(),
  };

  es.addEventListener("item", (e) => {
    const data = JSON.parse((e as MessageEvent).data);
    handlers.item.forEach(h => h(data));
  });
  // ... task, approval, ping

  return {
    on<K extends keyof typeof handlers>(kind: K, h: (e: any) => void) {
      handlers[kind].add(h);
      return () => handlers[kind].delete(h);
    },
    close: () => es.close(),
  };
}
```

## Tipos generados (ts-rs)

`backend/crates/*/src/lib.rs` tiene structs con derive:
```rust
#[derive(serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../../bindings/")]
pub struct Task {
    pub id: String,
    pub title: String,
    pub status: TaskStatus,
    pub created_at: String,
    // ...
}
```

Pipeline:
```bash
just gen-types
# Ejecuta: cd backend && cargo test --features ts-export export_bindings
# Copia: backend/bindings/* → frontend/src/lib/api/types/
```

Frontend importa `import type * as T from "$lib/api/types"`. Cualquier mismatch → error de TS al build → CI bloquea.

## Validación con valibot

Para inputs sensibles (forms, parsing de TOML editado a mano, defensa en profundidad sobre respuestas SSE):

```ts
// $lib/validators/task.ts
import * as v from "valibot";
import type { Task } from "$lib/api/types";

export const TaskSchema = v.object({
  id: v.string(),
  title: v.pipe(v.string(), v.minLength(1), v.maxLength(120)),
  status: v.picklist(["queued","in_progress","pending_verify","paused","blocked","done","abandoned"]),
}) satisfies v.GenericSchema<Task>;

export const parseTask = (raw: unknown): Task => v.parse(TaskSchema, raw);
```

Cuándo usar:
- Formularios (mensajes de error claros).
- Cargar archivos TOML editados a mano por el usuario.
- Verificar que respuestas SSE matchean el tipo esperado (defensa profundidad).

Cuándo NO:
- Validar cada response del backend (lo cubre ts-rs en compile-time).

## Hooks útiles

```ts
// $lib/hooks/useApi.ts
import { onMount } from "svelte";
import { writable } from "svelte/store";

export function useApiList<T>(loader: () => Promise<T[]>) {
  const data = writable<T[]>([]);
  const loading = writable(true);
  const error = writable<Error | null>(null);

  onMount(async () => {
    try { data.set(await loader()); } catch (e) { error.set(e as Error); }
    finally { loading.set(false); }
  });

  return { data, loading, error };
}
```

## Auto-reconnect

`EventSource` reconnect-ea por default. Adicional:
- Si el browser detecta `error: 0` (red caída) → mostrar banner + retry exponencial.
- Al reconectar → `Last-Event-ID` permite catch-up.

## Dev vs Prod

| | Dev (host) | Prod (Docker) |
|---|---|---|
| Frontend URL | `http://localhost:5173` (vite) | `http://localhost:8080` |
| Backend URL | `http://localhost:7777` | `http://localhost:7777` |
| CORS | proxy via vite | CORS allowed in Axum |
| Hot reload | sí | no |

`vite.config.ts` proxy:
```ts
server: {
  proxy: {
    "/api": { target: "http://localhost:7777" },
  }
}
```

En prod, `PUBLIC_API_BASE_URL=http://localhost:7777` (env var del container frontend).

## Anti-patrones

| Mal | Bien |
|---|---|
| Editar `types/` a mano | Solo Rust → `just gen-types` |
| Sin reconexión SSE | EventSource reconnect + banner |
| Sin valibot en forms | valibot con mensajes localizados |
| Hardcode de `localhost:7777` en componentes | Centralizar en `$lib/api/client.ts` |
| Importar todo de `$lib/api/types` | `import type * as T from "$lib/api/types"` |
