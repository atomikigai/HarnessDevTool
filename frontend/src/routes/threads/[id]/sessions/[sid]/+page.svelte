<script lang="ts">
  import { onMount } from 'svelte';
  import { page } from '$app/stores';
  import TerminalView from '$lib/components/app/TerminalView.svelte';
  import { api, type SessionMeta } from '$lib/api/client';
  import { Loader2, CircleAlert, ChevronLeft } from '$lib/icons';
  import { Button } from '$lib/components/ui/button';

  const threadId = $derived($page.params.id as string);
  const sessionId = $derived($page.params.sid as string);

  let meta = $state<SessionMeta | null>(null);
  let error = $state<string | null>(null);
  let loading = $state<boolean>(true);

  async function loadMeta() {
    loading = true;
    try {
      const res = await api.sessions.get(sessionId);
      meta = res.data;
      error = null;
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
      meta = null;
    } finally {
      loading = false;
    }
  }

  onMount(loadMeta);
</script>

<div class="flex h-full w-full flex-col">
  <header
    class="flex items-center justify-between border-b px-4 py-2"
    style="background: var(--surface-panel); border-color: var(--border-subtle);"
  >
    <div class="flex items-center gap-3">
      <Button
        variant="ghost"
        size="sm"
        onclick={() => history.back()}
        title="Back"
        aria-label="Back"
      >
        <ChevronLeft class="h-4 w-4" />
      </Button>
      <!-- Breadcrumb: Threads / <thread> / <session> -->
      <nav class="flex items-center gap-1.5 text-xs" style="color: var(--fg-breadcrumb);">
        <a href="/" class="hover:underline">Threads</a>
        <span style="color: var(--fg-muted);">/</span>
        <span class="font-mono" style="color: var(--fg-muted);">{threadId.slice(0, 8)}</span>
        <span style="color: var(--fg-muted);">/</span>
        <span class="font-mono" style="color: var(--fg-default);">{sessionId.slice(0, 8)}</span>
      </nav>
      {#if loading}
        <span class="inline-flex items-center gap-2 text-sm" style="color: var(--fg-muted);">
          <Loader2 class="h-3.5 w-3.5 animate-spin" /> Loading session…
        </span>
      {:else if error}
        <span class="inline-flex items-center gap-2 text-sm" style="color: var(--dot-danger);">
          <CircleAlert class="h-3.5 w-3.5" />
          {error}
        </span>
      {:else if meta}
        <div class="flex items-center gap-2 text-sm">
          <span class="font-mono font-semibold" style="color: var(--accent);">{meta.kind}</span>
          <span style="color: var(--fg-muted);">·</span>
          <span style="color: var(--fg-muted);">pid {meta.pid}</span>
          <span style="color: var(--fg-muted);">·</span>
          <span
            class="inline-flex items-center gap-1.5"
            style={meta.status === 'running'
              ? 'color: var(--dot-success);'
              : 'color: var(--fg-muted);'}
          >
            <span
              class="h-dot"
              class:h-dot--ok={meta.status === 'running'}
              class:h-dot--err={meta.status === 'killed'}
            ></span>
            {meta.status}{meta.exit_code != null ? ` (${meta.exit_code})` : ''}
          </span>
          {#if meta.cwd}
            <span
              class="ml-2 truncate font-mono text-xs"
              style="color: var(--fg-muted);"
              title={meta.cwd}
            >
              {meta.cwd}
            </span>
          {/if}
        </div>
      {/if}
    </div>
  </header>
  <div class="min-h-0 flex-1">
    <TerminalView {threadId} {sessionId} />
  </div>
</div>
