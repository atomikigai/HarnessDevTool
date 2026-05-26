---
id: frontend-shell/event-stream-ui
title: Render incremental de items
shard: 05-frontend-shell
tags: [streaming, ui, items]
summary: Componente que pinta items a medida que llegan deltas.
related: [harness-core/turn-and-item-primitives, frontend-shell/state-store]
sources: []
---

# Render incremental

## Modelo de datos en el cliente

```ts
type Item = {
  id: string;
  kind: ItemKind;
  status: "open" | "completed";
  text?: string;            // se concatena con cada delta
  toolCall?: { name: string; argsJson: string };
  toolResult?: any;
};

type TurnState = {
  id: string;
  items: Item[];          // orden por started_at
  startedAt: number;
  completedAt?: number;
};
```

## Reducer

```ts
function applyItemEvent(state: ThreadState, ev: ItemEvent): ThreadState {
  const turn = ensureTurn(state, ev.turn);
  switch (ev.type) {
    case "item.started":   return upsert(turn, { id: ev.id, kind: ev.kind, status: "open" });
    case "item.delta":     return appendText(turn, ev.id, ev.text);
    case "item.completed": return finalize(turn, ev.id, ev.payload);
  }
}
```

## Componente Svelte

```svelte
<script lang="ts">
  import { thread } from "$lib/stores/thread";
  $: items = $thread.activeTurn?.items ?? [];
</script>

{#each items as item (item.id)}
  {#if item.kind === "assistant_message"}
    <MarkdownStream text={item.text} done={item.status === "completed"} />
  {:else if item.kind === "tool_call"}
    <ToolCallView call={item.toolCall} result={item.toolResult} />
  {:else if item.kind === "approval_request"}
    <ApprovalCard request={item} />
  {/if}
{/each}
```

`MarkdownStream` renderiza markdown parcial: usa un parser tolerante (p.ej. `marked` con `gfm: true`) y re-renderiza cuando llega delta. Evita parpadeo manteniendo identidad por `item.id`.

## Performance
- Coalesce de deltas dentro de un frame (requestAnimationFrame).
- Listas largas (>500 items) → virtual list.
- Memoizar el markdown rendered en cada item completado.
