---
id: frontend-shell/sveltekit-integration
title: Integración SvelteKit ↔ App Server
shard: 05-frontend-shell
tags: [sveltekit, rpc, integration]
summary: Cliente JSON-RPC tipado, stores reactivos enlazados a notifications.
related: [frontend-shell/state-store, frontend-shell/event-stream-ui, architecture/ipc-protocol]
sources: []
---

# Integración

## Cliente JSON-RPC (tipado)

```ts
// src/lib/rpc/client.ts
import type { Methods, Notifications } from "./types"; // generados desde schema

export class RpcClient {
  call<M extends keyof Methods>(method: M, params: Methods[M]["params"]): Promise<Methods[M]["result"]>;
  on<N extends keyof Notifications>(method: N, h: (p: Notifications[N]) => void): Unsub;
  close(): void;
}
```

Los tipos se **generan** desde el JSON schema del protocolo (un archivo `protocol.schema.json` en `crates/harness-app-server`).

## Bridge Tauri

```ts
// dev / desktop: window.__TAURI__ está disponible
import { invoke } from "@tauri-apps/api/core";
import { Channel } from "@tauri-apps/api/core";

const ch = new Channel<unknown>();
ch.onmessage = (msg) => dispatcher.dispatch(msg);
await invoke("rpc_connect", { channel: ch });
await invoke("rpc_send", { line: JSON.stringify(request) });
```

El crate Tauri spawnea `harness-app-server` como sidecar, mantiene pipes y rutea líneas al Channel.

## Bridge web (futuro)
Misma `RpcClient` con `fetch('/rpc')` + `EventSource('/events')`. La capa de tipos es la misma; cambia solo el transport.

## Auto-reconnect
- Si el App Server muere (Tauri SIGCHLD): UI muestra banner, reintentos exponenciales.
- Al reconectar: enumerar threads activos y resumir el visible.

## Inicialización

```ts
const rpc = await RpcClient.connect();
const { capabilities } = await rpc.call("session.initialize", {
  protocolVersion: "1.0",
  clientFeatures: ["streaming", "approvals.allow-and-remember"],
});
modules.set(capabilities.modules);  // store global
```
