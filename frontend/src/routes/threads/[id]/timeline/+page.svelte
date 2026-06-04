<script lang="ts">
  import { onMount } from 'svelte';
  import { page } from '$app/stores';
  import { apiRequest } from '$lib/api/client';
  import { Button } from '$lib/components/ui/button';
  import { ChevronLeft, CircleAlert, History, Loader2, RefreshCw } from '$lib/icons';
  import { formatDistanceToNow } from 'date-fns';
  import type { TimelineItem, TimelineReport } from '$lib/api/models/task';

  const threadId = $derived($page.params.id as string);

  let report = $state<TimelineReport | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let filter = $state<string>('all');

  const filters = $derived.by(() => {
    const kinds = new Set<string>();
    for (const item of report?.items ?? []) {
      if (item.entity?.kind) kinds.add(item.entity.kind);
    }
    return ['all', ...Array.from(kinds).sort()];
  });

  const visible = $derived.by(() => {
    const items = report?.items ?? [];
    if (filter === 'all') return items;
    return items.filter((item) => item.entity?.kind === filter);
  });

  onMount(() => {
    void loadTimeline();
  });

  $effect(() => {
    if (threadId) void loadTimeline();
  });

  async function loadTimeline() {
    loading = true;
    error = null;
    try {
      const res = await apiRequest<TimelineReport>(`/threads/${threadId}/timeline?limit=500`);
      report = res.data;
      if (!filters.includes(filter)) filter = 'all';
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  function eventTime(item: TimelineItem): string {
    return new Date(item.at).toLocaleTimeString([], {
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit'
    });
  }

  function rawPayload(item: TimelineItem): string {
    return JSON.stringify(item.payload ?? {}, null, 2);
  }
</script>

<div class="flex h-full w-full flex-col">
  <header
    class="flex flex-wrap items-center gap-3 border-b px-4 py-2"
    style="background: var(--surface-panel); border-color: var(--border-subtle);"
  >
    <Button variant="ghost" size="sm" onclick={() => history.back()} aria-label="Back">
      <ChevronLeft class="h-4 w-4" />
    </Button>
    <div class="flex items-center gap-2">
      <History class="h-4 w-4" style="color: var(--fg-muted);" />
      <div>
        <div class="text-sm font-medium" style="color: var(--fg-default);">Timeline</div>
        <div class="font-mono text-[11px]" style="color: var(--fg-muted);">
          {threadId.slice(0, 8)}
        </div>
      </div>
    </div>

    <div class="ml-auto flex items-center gap-2">
      <select
        bind:value={filter}
        class="h-8 rounded-md border bg-[var(--surface-window)] px-2 text-xs"
        style="border-color: var(--border-input); color: var(--fg-default);"
      >
        {#each filters as f (f)}
          <option value={f}>{f}</option>
        {/each}
      </select>
      <Button size="sm" variant="outline" onclick={loadTimeline} disabled={loading}>
        {#if loading}
          <Loader2 class="h-3.5 w-3.5 animate-spin" />
        {:else}
          <RefreshCw class="h-3.5 w-3.5" />
        {/if}
      </Button>
    </div>
  </header>

  {#if error}
    <div
      class="m-4 flex items-start gap-3 rounded-md border p-4 text-sm"
      style="border-color: color-mix(in srgb, var(--dot-danger) 35%, transparent); background: color-mix(in srgb, var(--dot-danger) 10%, transparent); color: var(--dot-danger);"
    >
      <CircleAlert class="mt-0.5 h-4 w-4" />
      <div>
        <p class="font-medium">Failed to load timeline</p>
        <p class="mt-0.5 text-xs" style="color: var(--fg-muted);">{error}</p>
      </div>
    </div>
  {:else if loading && !report}
    <div class="flex flex-1 items-center justify-center gap-2 text-sm" style="color: var(--fg-muted);">
      <Loader2 class="h-4 w-4 animate-spin" /> Loading timeline…
    </div>
  {:else}
    <main class="min-h-0 flex-1 overflow-auto">
      <div
        class="border-b px-4 py-2 text-xs"
        style="border-color: var(--border-subtle); color: var(--fg-muted);"
      >
        {visible.length} visible event{visible.length === 1 ? '' : 's'}
        {#if report}
          · generated {formatDistanceToNow(new Date(report.generated_at), { addSuffix: true })}
        {/if}
      </div>

      {#if visible.length === 0}
        <div class="p-8 text-center text-sm" style="color: var(--fg-muted);">
          No timeline events for this filter.
        </div>
      {:else}
        <ol class="divide-y" style="border-color: var(--border-subtle);">
          {#each visible as item (item.seq)}
            <li class="grid grid-cols-[96px_1fr] gap-3 px-4 py-3">
              <div class="font-mono text-[11px]" style="color: var(--fg-muted);">
                <div>{eventTime(item)}</div>
                <div>#{item.seq}</div>
              </div>
              <div class="min-w-0">
                <div class="flex min-w-0 flex-wrap items-center gap-2">
                  <span class="truncate text-sm font-medium" style="color: var(--fg-default);">
                    {item.summary}
                  </span>
                  <span
                    class="rounded-sm px-1.5 py-0.5 font-mono text-[10px]"
                    style="background: var(--surface-titlebar); color: var(--fg-muted);"
                  >
                    {item.type}
                  </span>
                  {#if item.entity}
                    <span class="font-mono text-[11px]" style="color: var(--accent);">
                      {item.entity.kind}:{item.entity.id}
                    </span>
                  {/if}
                </div>
                <div class="mt-1 text-[11px]" style="color: var(--fg-muted);">
                  actor {item.actor ?? 'unknown'}
                </div>
                <details class="mt-2">
                  <summary class="cursor-pointer text-[11px]" style="color: var(--fg-muted);">
                    payload
                  </summary>
                  <pre
                    class="mt-2 max-h-80 overflow-auto rounded border p-3 text-[11px]"
                    style="border-color: var(--border-subtle); background: var(--surface-window); color: var(--fg-default);"
                  >{rawPayload(item)}</pre>
                </details>
              </div>
            </li>
          {/each}
        </ol>
      {/if}
    </main>
  {/if}
</div>
