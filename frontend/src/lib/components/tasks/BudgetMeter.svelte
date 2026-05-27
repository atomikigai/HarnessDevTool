<!--
  BudgetMeter — compact two-row gauge showing USD spend and wallclock burn
  against soft/hard caps. Purely presentational; the parent owns the data.
  Color states: green (<80% of hard cap), amber (>=80% / soft cap),
  red (>=100% / hard cap).
-->
<script lang="ts">
  interface Props {
    spent_usd?: number;
    soft_cap?: number;
    hard_cap?: number;
    wall_s?: number;
    wall_max_s?: number;
  }

  let {
    spent_usd = 0,
    soft_cap = 8,
    hard_cap = 10,
    wall_s = 0,
    wall_max_s = 3600
  }: Props = $props();

  function clampPct(value: number, max: number): number {
    if (max <= 0) return 0;
    const pct = (value / max) * 100;
    if (!Number.isFinite(pct) || pct < 0) return 0;
    return Math.min(pct, 100);
  }

  function stateFor(value: number, soft: number, hard: number): 'ok' | 'warn' | 'danger' {
    if (hard > 0 && value >= hard) return 'danger';
    if (soft > 0 && value >= soft) return 'warn';
    // also flag warn at >=80% of hard cap even if no soft cap supplied
    if (hard > 0 && value >= 0.8 * hard) return 'warn';
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
    const mm = String(m).padStart(2, '0');
    const ss = String(r).padStart(2, '0');
    return `${mm}:${ss}`;
  }

  const usdPct = $derived(clampPct(spent_usd, hard_cap));
  const usdState = $derived(stateFor(spent_usd, soft_cap, hard_cap));
  const usdColor = $derived(colorVar(usdState));

  const wallPct = $derived(clampPct(wall_s, wall_max_s));
  // Treat 80% of wall_max as the soft threshold for the wallclock bar.
  const wallState = $derived(stateFor(wall_s, 0.8 * wall_max_s, wall_max_s));
  const wallColor = $derived(colorVar(wallState));
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
        {fmtUsd(spent_usd)} / {fmtUsd(hard_cap)}
      </span>
    </div>
    <div
      class="h-1.5 w-full overflow-hidden rounded-full"
      style="background: color-mix(in srgb, {usdColor} 12%, transparent);"
      role="progressbar"
      aria-valuemin="0"
      aria-valuemax={hard_cap}
      aria-valuenow={spent_usd}
      aria-label="USD spent vs hard cap"
    >
      <div
        class="h-full rounded-full transition-[width,background-color] duration-300"
        style="width: {usdPct}%; background: {usdColor};"
      ></div>
    </div>
  </div>

  <!-- Wallclock row -->
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
