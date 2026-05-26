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
  <header class="flex items-center justify-between border-b border-border bg-card px-4 py-2">
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
      {#if loading}
        <span class="inline-flex items-center gap-2 text-sm text-muted-foreground">
          <Loader2 class="h-3.5 w-3.5 animate-spin" /> Loading session…
        </span>
      {:else if error}
        <span class="inline-flex items-center gap-2 text-sm text-destructive">
          <CircleAlert class="h-3.5 w-3.5" />
          {error}
        </span>
      {:else if meta}
        <div class="flex items-center gap-2 text-sm">
          <span class="font-mono font-semibold">{meta.kind}</span>
          <span class="text-muted-foreground">·</span>
          <span class="text-muted-foreground">pid {meta.pid}</span>
          <span class="text-muted-foreground">·</span>
          <span
            class:text-emerald-400={meta.status === 'running'}
            class:text-zinc-400={meta.status !== 'running'}
          >
            {meta.status}{meta.exit_code != null ? ` (${meta.exit_code})` : ''}
          </span>
          {#if meta.cwd}
            <span class="ml-2 truncate font-mono text-xs text-muted-foreground" title={meta.cwd}>
              {meta.cwd}
            </span>
          {/if}
        </div>
      {/if}
    </div>
    <div class="text-xs text-muted-foreground">
      thread <span class="font-mono">{threadId.slice(0, 8)}</span>
    </div>
  </header>
  <div class="min-h-0 flex-1">
    <TerminalView {threadId} {sessionId} />
  </div>
</div>
