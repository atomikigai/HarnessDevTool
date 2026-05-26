<!--
  TopBar — global header for the shell.
  Layout: [ logo ] · [ flexible center search ] · [ theme toggle + conn pill ]
  The search input is visual-only in F1 (no command palette wired). The
  connection pill subscribes to the shared `health` store — polling is
  centralized there so the dashboard and the bar agree on status.
-->
<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import HarnessIcons from './HarnessIcons.svelte';
  import { health } from '$lib/stores/health.svelte';
  import { theme } from '$lib/stores/theme.svelte';

  const REFRESH_MS = 10_000;
  let timer: ReturnType<typeof setInterval> | null = null;

  onMount(() => {
    health.refresh();
    timer = setInterval(() => health.refresh(), REFRESH_MS);
  });

  onDestroy(() => {
    if (timer) clearInterval(timer);
  });

  const dotClass = $derived.by(() => {
    if (health.state === 'ok') return 'h-dot h-dot--ok';
    if (health.state === 'down') return 'h-dot h-dot--err';
    return 'h-dot h-dot--warn';
  });

  const connLabel = $derived.by(() => {
    if (health.state === 'ok') return 'localhost · backend';
    if (health.state === 'down') return 'backend down';
    return 'connecting…';
  });
</script>

<header
  class="flex h-12 shrink-0 items-center gap-3 border-b px-3"
  style="background: var(--surface-titlebar); border-color: var(--border-subtle);"
>
  <!-- Logo -->
  <a href="/" class="flex items-center gap-2 px-1">
    <span
      class="inline-flex h-6 w-6 items-center justify-center rounded-md text-[var(--fg-on-accent)]"
      style="background: var(--accent);"
      aria-hidden="true"
    >
      <HarnessIcons name="agents" size={12} />
    </span>
    <span
      class="font-serif text-[15px] font-medium tracking-tight"
      style="color: var(--fg-default);">Harness</span
    >
    <span
      class="hidden text-[10px] uppercase tracking-widest sm:inline"
      style="color: var(--fg-label);">dev tool</span
    >
  </a>

  <!-- Center search -->
  <div class="mx-auto flex w-full max-w-md items-center">
    <div
      class="flex w-full items-center gap-2 rounded-md px-3 py-1.5 text-sm"
      style="background: var(--surface-window); border: 1px solid var(--border-input);"
    >
      <HarnessIcons name="search" size={13} class="shrink-0" />
      <input
        type="search"
        placeholder="Search threads, sessions, settings…"
        class="flex-1 bg-transparent text-sm outline-none placeholder:text-[var(--fg-muted)]"
        style="color: var(--fg-default);"
        aria-label="Global search (preview)"
      />
      <span class="hidden items-center gap-1 sm:inline-flex">
        <span class="h-kbd">⌘</span>
        <span class="h-kbd">K</span>
      </span>
    </div>
  </div>

  <!-- Theme toggle + connection pill -->
  <div class="flex items-center gap-2">
    <button
      type="button"
      onclick={() => theme.toggle()}
      class="flex h-7 w-7 items-center justify-center rounded-md transition-colors"
      style="color: var(--fg-muted);"
      title={theme.current === 'paper' ? 'Switch to warmth (dark)' : 'Switch to paper (light)'}
      aria-label="Toggle theme"
    >
      {#if theme.current === 'paper'}
        <HarnessIcons name="moon" size={14} />
      {:else}
        <HarnessIcons name="sun" size={14} />
      {/if}
    </button>
    <div
      class="flex items-center gap-2 rounded-md px-2.5 py-1 text-[11px]"
      style="background: var(--accent-soft); border: 1px solid var(--accent-soft-border); color: var(--fg-breadcrumb);"
      title={health.error ?? `Backend ${health.state}`}
    >
      <span class={dotClass}></span>
      <span class="font-mono">{connLabel}</span>
    </div>
  </div>
</header>
