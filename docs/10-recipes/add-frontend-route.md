---
id: recipes/add-frontend-route
title: Receta — añadir una vista SvelteKit
shard: 10-recipes
tags: [recipe, sveltekit, howto]
summary: Nueva ruta consumiendo un namespace JSON-RPC existente.
related: [frontend-shell/sveltekit-integration, frontend-shell/state-store, frontend-shell/routing-shell]
sources: []
---

# Añadir una vista SvelteKit

Ejemplo: vista `/threads` para listar y reanudar threads.

## 1. Definir tipos del RPC
Si el namespace ya existe, los tipos están auto-generados. Si añades un método nuevo: regenera con:
```bash
cargo run -p harness-app-server --bin gen-ts-types > apps/desktop/src/lib/rpc/types.ts
```

## 2. Crear la ruta

`apps/desktop/src/routes/threads/+page.svelte`:
```svelte
<script lang="ts">
  import { onMount } from "svelte";
  import { rpc } from "$lib/rpc/client";
  import type { ThreadMeta } from "$lib/rpc/types";

  let threads: ThreadMeta[] = [];
  let loading = true;

  onMount(async () => {
    threads = await rpc.call("thread.list", { archived: false });
    loading = false;
  });

  async function resume(id: string) {
    await rpc.call("thread.resume", { id });
    window.location.href = `/threads/${id}`;
  }
</script>

{#if loading}
  <Spinner />
{:else}
  <ul>
    {#each threads as t}
      <li>
        <strong>{t.title}</strong>
        <small>{t.model} · {t.updated_at}</small>
        <button on:click={() => resume(t.id)}>Resume</button>
      </li>
    {/each}
  </ul>
{/if}
```

## 3. Añadir al sidebar
Si el módulo lo requiere, añadir un capability flag en el server; el sidebar lo recoge automáticamente. Para rutas "core" (threads, settings), agregar manual en `+layout.svelte`.

## 4. Store reactivo (opcional)
Si la vista necesita updates en vivo:
```ts
// $lib/stores/threads.ts
export const threads = writable<ThreadMeta[]>([]);
rpc.on("thread.created", (m) => threads.update(t => [...t, m]));
rpc.on("thread.archived", (m) => threads.update(t => t.filter(x => x.id !== m.id)));
```

Y en la vista:
```svelte
<script>
  import { threads } from "$lib/stores/threads";
</script>

{#each $threads as t}
  ...
{/each}
```

## 5. Test
- Smoke en dev: `bun run dev`, abrir `/threads`.
- E2E (opcional v1.1): Playwright contra Tauri.

## Checklist
- [ ] Tipos TS sincronizados con server
- [ ] Ruta crea/lee/actualiza datos via RPC
- [ ] Store reactivo si hay updates en vivo
- [ ] Manejo de errores (toast + retry)
- [ ] Estados loading/empty/error
- [ ] Cumple atajos globales (`Cmd+K`, etc.)
