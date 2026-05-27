<!--
  BudgetMeter — USD spend gauge for a single thread.
  Pulls from `budgetStore`, refreshing on mount and on `budget.warning`
  SSE events fanned in by the page. The soft/hard thresholds are read
  from the server view (never hardcoded) so band tuning is a backend
  concern. Includes an inline form to update `limit_usd`.

  Wallclock row remains for visual continuity but is currently always
  zeroed — F3 ships USD only; wallclock surfacing is queued for a
  later slice.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { budgetStore } from '$lib/stores/budget.svelte';
  import { toast } from 'svelte-sonner';
  import { subscribeSSE, type SSEHandle } from '$lib/api/sse';

  interface Props {
    threadId: string;
    wall_s?: number;
    wall_max_s?: number;
  }

  let { threadId, wall_s = 0, wall_max_s = 3600 }: Props = $props();

  const entry = $derived(budgetStore.get(threadId));
  const view = $derived(entry.view);

  // Form state. Initialized lazily from the server view so we don't
  // wipe a user's in-progress edit when SSE refreshes happen.
  let limitInput = $state<string>('');
  let limitDirty = $state(false);

  $effect(() => {
    if (!limitDirty && view) {
      limitInput = String(view.limit_usd);
    }
  });

  onMount(() => {
    void budgetStore.loadBudget(threadId);

    // Listen for the global budget.warning event. The backend emits it on
    // the default channel as `{type: "budget.warning", ...}` per the F3
    // contract, but we also register a named handler in case future
    // payloads switch to event-typed framing.
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
        events: {
          'budget.warning': (data) => handleWarning(data as { thread_id?: string; pct?: number })
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

  function clampPct(value: number, max: number): number {
    if (max <= 0) return 0;
    const pct = (value / max) * 100;
    if (!Number.isFinite(pct) || pct < 0) return 0;
    return Math.min(pct, 100);
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

  function fmtClock(seconds: number): string {
    const s = Math.max(0, Math.floor(Number.isFinite(seconds) ? seconds : 0));
    const m = Math.floor(s / 60);
    const r = s % 60;
    return `${String(m).padStart(2, '0')}:${String(r).padStart(2, '0')}`;
  }

  const spent = $derived(view?.spent_usd ?? 0);
  const limit = $derived(view?.limit_usd ?? 0);
  const pct = $derived(view?.pct ?? 0);
  const softPct = $derived(view?.soft_pct ?? 80);
  const hardPct = $derived(view?.hard_pct ?? 100);

  const usdPct = $derived(Math.min(pct, 100));
  const usdState = $derived(stateFor(pct, softPct, hardPct));
  const usdColor = $derived(colorVar(usdState));

  const wallPct = $derived(clampPct(wall_s, wall_max_s));
  const wallState = $derived(stateFor(wallPct, 80, 100));
  const wallColor = $derived(colorVar(wallState));

  async function submitLimit(e: SubmitEvent) {
    e.preventDefault();
    const parsed = Number(limitInput);
    if (!Number.isFinite(parsed) || parsed <= 0) {
      toast.error('Budget limit must be a positive number');
      return;
    }
    await budgetStore.setLimit(threadId, parsed);
    const next = budgetStore.get(threadId);
    if (next.error) {
      toast.error(`Set limit failed: ${next.error}`);
    } else {
      limitDirty = false;
      toast.success(`Limit set to ${fmtUsd(parsed)}`);
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
  <form class="flex items-center gap-2 text-[11px]" onsubmit={submitLimit}>
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
    <button
      type="submit"
      class="h-7 rounded-md border px-2 text-[11px]"
      style="border-color: var(--border-input); color: var(--fg-default); background: var(--surface-window);"
      disabled={entry.saving || !limitDirty}
    >
      {entry.saving ? 'Saving…' : 'Set'}
    </button>
    {#if entry.error}
      <span style="color: var(--dot-danger);">{entry.error}</span>
    {/if}
  </form>

  <!-- Wallclock row (placeholder; not yet wired to server) -->
  <div class="flex flex-col gap-1">
    <div class="flex items-baseline justify-between text-[11px]">
      <span class="uppercase tracking-wider" style="color: var(--fg-label);">Wall</span>
      <span class="font-mono" style="color: var(--fg-default);">
        {fmtClock(wall_s)} / {fmtClock(wall_max_s)}
      </span>
    </div>
    <div
      class="h-1.5 w-full overflow-hidden rounded-full"
      style="background: color-mix(in srgb, {wallColor} 12%, transparent);"
      role="progressbar"
      aria-valuemin="0"
      aria-valuemax={wall_max_s}
      aria-valuenow={wall_s}
      aria-label="Wallclock vs max"
    >
      <div
        class="h-full rounded-full transition-[width,background-color] duration-300"
        style="width: {wallPct}%; background: {wallColor};"
      ></div>
    </div>
  </div>
</div>
