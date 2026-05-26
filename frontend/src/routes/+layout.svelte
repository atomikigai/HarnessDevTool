<script lang="ts">
  import '../app.css';
  import { page } from '$app/stores';
  import IconRail from '$lib/components/app/IconRail.svelte';
  import TopBar from '$lib/components/app/TopBar.svelte';
  import { Toaster } from '$lib/components/ui/sonner';
  import type { Snippet } from 'svelte';

  let { children }: { children: Snippet } = $props();

  const currentPath = $derived($page.url.pathname);
</script>

<!--
  Shell — three zones (top bar, narrow rail, main).
  Background tokens come from `app.css`; theme switch toggles `.dark` on
  <html> via the theme store, so the whole tree restyles in one shot.
-->
<div
  class="flex h-screen w-screen flex-col overflow-hidden"
  style="background: var(--surface-window); color: var(--fg-default);"
>
  <TopBar />
  <div class="flex min-h-0 flex-1">
    <IconRail {currentPath} />
    <main class="min-w-0 flex-1 overflow-y-auto" style="background: var(--surface-canvas);">
      {@render children()}
    </main>
  </div>
</div>

<Toaster />
