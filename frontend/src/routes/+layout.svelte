<script lang="ts">
  import '../app.css';
  import { page } from '$app/stores';
  import Sidebar from '$lib/components/app/Sidebar.svelte';
  import { Toaster } from '$lib/components/ui/sonner';
  import type { Snippet } from 'svelte';

  let { children }: { children: Snippet } = $props();

  const currentPath = $derived($page.url.pathname);
</script>

<div class="flex h-screen w-screen overflow-hidden bg-background text-foreground">
  <Sidebar {currentPath} />
  <main class="flex-1 overflow-y-auto">
    {@render children()}
  </main>
</div>

<Toaster />
