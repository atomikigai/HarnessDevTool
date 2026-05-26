---
id: frontend-shell/event-stream-ui
title: Render del event stream (PTY + items)
shard: 05-frontend-shell
tags: [streaming, ui, xterm, markdown, items]
summary: PTY → xterm.js en vivo; items estructurados → componentes Svelte tipados.
related: [frontend-shell/state-store, harness-core/streaming-events, agents/spawn-lifecycle]
sources: []
---

# Render del event stream

> Dos rendering paths: **PTY raw** (xterm.js) y **items estructurados** (componentes Svelte). Vienen del mismo SSE pero se procesan distinto.

## Path 1 — PTY output (xterm.js)

Items con `kind = "spawn.output"` llevan bytes base64 del PTY del CLI hijo. Se rendean en un `<TerminalView>` dedicado por session.

```svelte
<!-- $lib/components/app/TerminalView.svelte -->
<script lang="ts">
  import { Terminal } from "xterm";
  import { FitAddon } from "xterm-addon-fit";
  import { WebglAddon } from "xterm-addon-webgl";
  import { onMount, onDestroy } from "svelte";
  import { api } from "$lib/api/client";
  import { subscribe } from "$lib/api/sse";

  export let threadId: string;
  export let sessionId: string;

  let container: HTMLDivElement;
  let term: Terminal;
  let fitAddon: FitAddon;
  let unsub: () => void;

  onMount(() => {
    term = new Terminal({ fontFamily: "JetBrains Mono", fontSize: 14, scrollback: 10000 });
    fitAddon = new FitAddon();
    term.loadAddon(fitAddon);
    try { term.loadAddon(new WebglAddon()); } catch {}
    term.open(container);
    fitAddon.fit();

    // input → backend
    term.onData((data) => api.sessions.input(sessionId, data));

    // SSE → terminal
    const sse = subscribe(threadId);
    unsub = sse.on("item", (e) => {
      if (e.kind === "spawn.output" && e.spawn === sessionId) {
        const bytes = atob(e.payload.data_b64);
        term.write(bytes);
      }
    });

    // resize observer
    const ro = new ResizeObserver(debounce(() => {
      fitAddon.fit();
      api.sessions.resize(sessionId, term.cols, term.rows);
    }, 100));
    ro.observe(container);
  });

  onDestroy(() => { unsub?.(); term?.dispose(); });
</script>

<div bind:this={container} class="h-full w-full"></div>
```

Performance: WebGL2 renderer hace 60fps sostenido. Si tu hardware no soporta WebGL2, fallback canvas (también 30+ fps).

## Path 2 — Items estructurados

Items con kinds como `task.transitioned`, `approval.requested`, `budget.cap_warning` van a **stores Svelte** y desde ahí a componentes dedicados.

```ts
// $lib/stores/thread.ts
import { writable, derived } from "svelte/store";
import type { ItemEvent, Task, ApprovalRequest } from "$lib/api/types";

export const items = writable<ItemEvent[]>([]);
export const tasks = writable<Record<string, Task>>({});
export const approvals = writable<ApprovalRequest[]>([]);

// reducer
export function applyItem(item: ItemEvent) {
  items.update(arr => [...arr, item].slice(-500));

  if (item.kind.startsWith("task.")) {
    tasks.update(t => {
      const id = item.payload.task_id ?? item.payload.id;
      return { ...t, [id]: { ...t[id], ...item.payload } };
    });
  }

  if (item.kind === "approval.requested") {
    approvals.update(arr => [...arr, item.payload]);
  }
  // ...
}
```

Componentes que consumen:
- `<TaskList>` lee `$tasks` y pinta por status.
- `<ApprovalsInbox>` lee `$approvals` y muestra cards con Allow/Deny.
- `<BudgetMeter>` deriva de los items `budget.consumed`.
- `<ActivityFeed>` lee `$items` filtrado por relevancia.

## Componente para markdown incremental (asistant_message no aplica aquí)

> **Nota**: en nuestra arquitectura no recibimos `assistant_message` items (eso es del CLI hijo, va vía PTY). El path 1 cubre todo lo que el modelo "dice".

Si en el futuro añadimos respuestas estructuradas del agente (no por PTY), un `<MarkdownStream>` con `marked` + parser tolerante haría el render incremental. Por ahora, no necesario.

## Filtrado por session/spawn

La UI muestra una sesión a la vez (la activa). Items se filtran por `spawn === activeSessionId`:

```ts
const activeOutput = derived([items, activeSessionId], ([$items, $active]) =>
  $items.filter(i => i.kind === "spawn.output" && i.spawn === $active)
);
```

## Backpressure UI-side

Si llegan muchos items rápido (PTY output flood), el browser puede atrasarse pintando. Mitigación:
- xterm.js maneja su propio buffer internamente.
- Para items estructurados, batching con `requestAnimationFrame`:

```ts
let pending: ItemEvent[] = [];
let rafId: number | null = null;

function queueItem(item: ItemEvent) {
  pending.push(item);
  if (rafId == null) {
    rafId = requestAnimationFrame(() => {
      for (const i of pending) applyItem(i);
      pending = [];
      rafId = null;
    });
  }
}
```

## Persistencia en cliente

- Stores se resetean al cambiar de thread.
- No persistimos items en `localStorage` (fuente de verdad es el backend; resume via SSE).
- Únicamente preferencias UI (theme, sidebar collapsed) en `localStorage`.

## Anti-patrones

| Mal | Bien |
|---|---|
| Acumular todos los items sin slice | Cap a últimos 500; backend tiene full history |
| xterm.js sin WebGL renderer | Activar WebglAddon (fallback canvas automático) |
| Parsear PTY ANSI a mano | Dejarlo a xterm.js |
| Aplicar items sin throttling al state | Batching con rAF |
| Stores compartidos entre threads sin reset | Reset al cambiar thread activo |
