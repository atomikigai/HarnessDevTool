<script lang="ts">
  import {
    Card,
    CardHeader,
    CardTitle,
    CardDescription,
    CardContent
  } from '$lib/components/ui/card';
  import { Button } from '$lib/components/ui/button';
  import {
    Activity,
    CircleAlert,
    CircleCheck,
    RefreshCw,
    Loader2,
    Plus,
    Terminal
  } from '$lib/icons';
  import NewSessionDialog from '$lib/components/app/NewSessionDialog.svelte';
  import { health } from '$lib/stores/health.svelte';
  import { sessionsState } from '$lib/stores/session.svelte';

  let newSessionOpen = $state(false);

  // The TopBar owns the polling cadence; this page just reads the store
  // and offers a manual refresh button.
</script>

<div class="mx-auto flex max-w-5xl flex-col gap-6 p-8">
  <header class="flex items-start justify-between gap-4">
    <div>
      <h1 class="text-3xl font-medium tracking-tight">Dashboard</h1>
      <p class="mt-1 text-sm" style="color: var(--fg-muted);">
        Backend health, active sessions, and shell wiring.
      </p>
    </div>
    <div class="flex items-center gap-2">
      {#if health.protocolVersion}
        <span
          class="inline-flex items-center gap-1.5 rounded-md border px-2.5 py-1 text-xs font-medium"
          style="border-color: var(--accent-soft-border); background: var(--accent-soft); color: var(--accent);"
          title="X-Protocol-Version header from backend"
        >
          <CircleCheck class="h-3.5 w-3.5" />
          Protocol v{health.protocolVersion}
        </span>
      {:else}
        <span
          class="inline-flex items-center gap-1.5 rounded-md border px-2.5 py-1 text-xs font-medium"
          style="border-color: var(--border-subtle); background: var(--surface-panel); color: var(--fg-muted);"
        >
          <CircleAlert class="h-3.5 w-3.5" />
          Protocol unknown
        </span>
      {/if}
      <Button
        variant="outline"
        size="sm"
        onclick={() => health.refresh()}
        disabled={health.state === 'connecting'}
      >
        {#if health.state === 'connecting'}
          <Loader2 class="h-4 w-4 animate-spin" />
        {:else}
          <RefreshCw class="h-4 w-4" />
        {/if}
        Refresh
      </Button>
    </div>
  </header>

  <Card>
    <CardHeader>
      <CardTitle class="flex items-center gap-2 font-sans text-base font-semibold">
        <Terminal class="h-4 w-4" style="color: var(--fg-muted);" />
        Sessions
        {#if sessionsState.active.length > 0}
          <span
            class="ml-1 rounded-full px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wider"
            style="background: var(--accent-soft); color: var(--accent);"
          >
            {sessionsState.active.length} running
          </span>
        {/if}
      </CardTitle>
      <CardDescription>Launch a claude or codex CLI inside a managed PTY.</CardDescription>
    </CardHeader>
    <CardContent>
      <Button onclick={() => (newSessionOpen = true)}>
        <Plus class="h-4 w-4" />
        New session
      </Button>
    </CardContent>
  </Card>

  <Card>
    <CardHeader>
      <CardTitle class="flex items-center gap-2 font-sans text-base font-semibold">
        <Activity class="h-4 w-4" style="color: var(--fg-muted);" />
        Backend
      </CardTitle>
      <CardDescription>GET /api/health, refreshed every 10s.</CardDescription>
    </CardHeader>
    <CardContent>
      {#if health.error && !health.data}
        <div
          class="flex items-start gap-3 rounded-md border px-4 py-3 text-sm"
          style="border-color: color-mix(in srgb, var(--dot-danger) 35%, transparent); background: color-mix(in srgb, var(--dot-danger) 10%, transparent); color: var(--dot-danger);"
        >
          <CircleAlert class="mt-0.5 h-4 w-4" />
          <div>
            <p class="font-medium">Backend unreachable</p>
            <p class="mt-0.5 text-xs" style="color: var(--fg-muted);">{health.error}</p>
          </div>
        </div>
      {:else if !health.data}
        <div class="flex items-center gap-2 text-sm" style="color: var(--fg-muted);">
          <Loader2 class="h-4 w-4 animate-spin" />
          Loading…
        </div>
      {:else}
        <dl class="grid gap-4 sm:grid-cols-2">
          <div>
            <dt class="h-eyebrow">Backend version</dt>
            <dd class="mt-1 font-mono text-lg">{health.data.version}</dd>
          </div>
          <div>
            <dt class="h-eyebrow">Uptime</dt>
            <dd class="mt-1 font-mono text-lg">{health.data.uptime_s}s</dd>
          </div>
        </dl>
        {#if health.error}
          <p class="mt-3 text-xs" style="color: var(--dot-warn);">
            Last refresh failed: {health.error}
          </p>
        {/if}
        {#if health.lastUpdated}
          <p class="mt-3 text-xs" style="color: var(--fg-muted);">
            Updated {health.lastUpdated.toLocaleTimeString()}
          </p>
        {/if}
      {/if}
    </CardContent>
  </Card>
</div>

<NewSessionDialog bind:open={newSessionOpen} />
