<!--
  BudgetMeter — USD spend gauge for a single thread.
  Subscribes directly to `/events` (global, unfiltered) for
  `budget.warning`: that event is fanned out on the default channel,
  not on the per-thread topic that tasks use (`/events?thread=`).
  Thresholds come from the server view; never hardcode them here.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { budgetStore } from '$lib/stores/budget.svelte';
  import { toast } from 'svelte-sonner';
  import { subscribeSSE, type SSEHandle } from '$lib/api/sse';

  interface Props {
    threadId: string;
  }

  let { threadId }: Props = $props();

  const entry = $derived(budgetStore.get(threadId));
  const view = $derived(entry.view);

  // Form state. Initialized lazily from the server view so we don't
  // wipe a user's in-progress edit when SSE refreshes happen.
  let limitInput = $state<string>('');
  let maxWorkersInput = $state<string>('');
  let limitDirty = $state(false);
  let maxWorkersDirty = $state(false);

  $effect(() => {
    if (!limitDirty && view) {
      limitInput = String(view.limit_usd);
    }
    if (!maxWorkersDirty && view) {
      maxWorkersInput = view.max_concurrent_workers == null ? '' : String(view.max_concurrent_workers);
    }
  });

  onMount(() => {
    void budgetStore.loadBudget(threadId);

    // budget.warning is emitted on the default channel by the backend
    // (see harness-server::state::TickWarningSink). Tasks use a separate
    // named-event stream at /events?thread=&lt;id&gt;; budgets are global.
    const sse: SSEHandle = subscribeSSE<{ type?: string; thread_id?: string }>(
      '/events',
      (data) => {
        if (
          data &&
          typeof data === 'object' &&
          (data as { type?: string }).type === 'budget.warning'
        ) {
          handleWarning(data as { thread_id?: string; pct?: number });
        }
      },
      {
        reconnect: true,
        onResync: () => {
          void budgetStore.loadBudget(threadId);
        }
      }
    );
    return () => sse.close();
  });

  function handleWarning(payload: { thread_id?: string; pct?: number }) {
    if (!payload?.thread_id) return;
    // Filter: only act on warnings for the current thread.
    if (payload.thread_id !== threadId) return;
    void budgetStore.loadBudget(threadId);
    const pctStr = typeof payload.pct === 'number' ? `${payload.pct.toFixed(0)}%` : '';
    toast.warning(`Budget crossed ${pctStr}`.trim());
  }

  function stateFor(pct: number, softPct: number, hardPct: number): 'ok' | 'warn' | 'danger' {
    if (hardPct > 0 && pct >= hardPct) return 'danger';
    if (softPct > 0 && pct >= softPct) return 'warn';
    return 'ok';
  }

  function colorVar(s: 'ok' | 'warn' | 'danger'): string {
    if (s === 'danger') return 'var(--dot-danger)';
    if (s === 'warn') return 'var(--dot-warn)';
    return 'var(--dot-success)';
  }

  function fmtUsd(n: number): string {
    return `$${(Number.isFinite(n) ? n : 0).toFixed(2)}`;
  }

  const spent = $derived(view?.spent_usd ?? 0);
  const limit = $derived(view?.limit_usd ?? 0);
  const pct = $derived(view?.pct ?? 0);
  const softPct = $derived(view?.soft_pct ?? 80);
  const hardPct = $derived(view?.hard_pct ?? 100);
  const maxConcurrentWorkers = $derived(view?.max_concurrent_workers ?? null);

  const usdPct = $derived(Math.min(pct, 100));
  const usdState = $derived(stateFor(pct, softPct, hardPct));
  const usdColor = $derived(colorVar(usdState));

  async function submitLimit(e: SubmitEvent) {
    e.preventDefault();
    const parsed = Number(limitInput);
    if (!Number.isFinite(parsed) || parsed <= 0) {
      toast.error('Budget limit must be a positive number');
      return;
    }
    const trimmedMax = maxWorkersInput.trim();
    const maxWorkers = trimmedMax === '' ? null : Number(trimmedMax);
    if (maxWorkers !== null && (!Number.isInteger(maxWorkers) || maxWorkers < 1)) {
      toast.error('Max workers must be a positive integer');
      return;
    }
    await budgetStore.setLimit(threadId, parsed, maxWorkers);
    const next = budgetStore.get(threadId);
    if (next.error) {
      toast.error(`Set limit failed: ${next.error}`);
    } else {
      limitDirty = false;
      maxWorkersDirty = false;
      toast.success(`Budget saved`);
    }
  }
</script>

<div
  class="flex flex-col gap-2 rounded-md border p-3"
  style="border-color: var(--border-subtle); background: var(--surface-panel);"
  aria-label="Thread budget"
>
  <!-- USD row -->
  <div class="flex flex-col gap-1">
    <div class="flex items-baseline justify-between text-[11px]">
      <span class="uppercase tracking-wider" style="color: var(--fg-label);">Spend</span>
      <span class="font-mono" style="color: var(--fg-default);">
        {fmtUsd(spent)} / {fmtUsd(limit)}
        <span style="color: var(--fg-muted);">({pct.toFixed(0)}%)</span>
      </span>
    </div>
    <div
      class="h-1.5 w-full overflow-hidden rounded-full"
      style="background: color-mix(in srgb, {usdColor} 12%, transparent);"
      role="progressbar"
      aria-valuemin="0"
      aria-valuemax={limit || 1}
      aria-valuenow={spent}
      aria-label="USD spent vs limit"
    >
      <div
        class="h-full rounded-full transition-[width,background-color] duration-300"
        style="width: {usdPct}%; background: {usdColor};"
      ></div>
    </div>
  </div>

  <!-- Inline limit form -->
  <form class="flex flex-wrap items-center gap-2 text-[11px]" onsubmit={submitLimit}>
    <label class="uppercase tracking-wider" style="color: var(--fg-label);" for="budget-limit">
      Limit (USD)
    </label>
    <input
      id="budget-limit"
      type="number"
      step="0.01"
      min="0.01"
      class="h-7 w-24 rounded-md border bg-[var(--surface-window)] px-2 font-mono text-[11px]"
      style="border-color: var(--border-input); color: var(--fg-default);"
      bind:value={limitInput}
      oninput={() => (limitDirty = true)}
      disabled={entry.saving}
    />
    <label class="uppercase tracking-wider" style="color: var(--fg-label);" for="budget-workers">
      Workers
    </label>
    <input
      id="budget-workers"
      type="number"
      step="1"
      min="1"
      placeholder="3"
      title="Max concurrent workers for this thread. Empty uses default."
      class="h-7 w-16 rounded-md border bg-[var(--surface-window)] px-2 font-mono text-[11px]"
      style="border-color: var(--border-input); color: var(--fg-default);"
      bind:value={maxWorkersInput}
      oninput={() => (maxWorkersDirty = true)}
      disabled={entry.saving}
    />
    <button
      type="submit"
      class="h-7 rounded-md border px-2 text-[11px]"
      style="border-color: var(--border-input); color: var(--fg-default); background: var(--surface-window);"
      disabled={entry.saving || (!limitDirty && !maxWorkersDirty)}
    >
      {entry.saving ? 'Saving…' : 'Set'}
    </button>
    <span class="font-mono" style="color: var(--fg-muted);">
      max {maxConcurrentWorkers ?? 3}
    </span>
    {#if entry.error}
      <span style="color: var(--dot-danger);">{entry.error}</span>
    {/if}
  </form>
</div>
