<script lang="ts">
  import { onMount } from 'svelte';
  import {
    Card,
    CardHeader,
    CardTitle,
    CardDescription,
    CardContent
  } from '$lib/components/ui/card';
  import { Button } from '$lib/components/ui/button';
  import { api, type HealthResponse } from '$lib/api/client';
  import { Activity, CircleAlert, CircleCheck, RefreshCw, Loader2 } from '$lib/icons';

  const REFRESH_MS = 10_000;

  let health = $state<HealthResponse | null>(null);
  let protocolVersion = $state<string | null>(null);
  let error = $state<string | null>(null);
  let loading = $state<boolean>(false);
  let lastUpdated = $state<Date | null>(null);

  let controller: AbortController | null = null;
  let timer: ReturnType<typeof setInterval> | null = null;

  async function fetchHealth() {
    controller?.abort();
    controller = new AbortController();
    loading = true;
    try {
      const res = await api.health(controller.signal);
      health = res.data;
      protocolVersion = res.protocolVersion;
      error = null;
      lastUpdated = new Date();
    } catch (e) {
      if ((e as { name?: string }).name === 'AbortError') return;
      error = e instanceof Error ? e.message : String(e);
      health = null;
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    fetchHealth();
    timer = setInterval(fetchHealth, REFRESH_MS);
    return () => {
      if (timer) clearInterval(timer);
      controller?.abort();
    };
  });
</script>

<div class="mx-auto flex max-w-5xl flex-col gap-6 p-8">
  <header class="flex items-start justify-between gap-4">
    <div>
      <h1 class="text-2xl font-semibold tracking-tight">Dashboard</h1>
      <p class="mt-1 text-sm text-muted-foreground">Backend health and protocol status.</p>
    </div>
    <div class="flex items-center gap-2">
      {#if protocolVersion}
        <span
          class="inline-flex items-center gap-1.5 rounded-md border border-border bg-secondary px-2.5 py-1 text-xs font-medium text-secondary-foreground"
          title="X-Protocol-Version header from backend"
        >
          <CircleCheck class="h-3.5 w-3.5 text-emerald-400" />
          Protocol v{protocolVersion}
        </span>
      {:else}
        <span
          class="inline-flex items-center gap-1.5 rounded-md border border-border bg-secondary px-2.5 py-1 text-xs font-medium text-muted-foreground"
        >
          <CircleAlert class="h-3.5 w-3.5 text-amber-400" />
          Protocol unknown
        </span>
      {/if}
      <Button variant="outline" size="sm" onclick={fetchHealth} disabled={loading}>
        {#if loading}
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
      <CardTitle class="flex items-center gap-2">
        <Activity class="h-4 w-4 text-muted-foreground" />
        Backend
      </CardTitle>
      <CardDescription>GET /api/health, refreshed every 10s.</CardDescription>
    </CardHeader>
    <CardContent>
      {#if error && !health}
        <div
          class="flex items-start gap-3 rounded-md border border-destructive/50 bg-destructive/10 px-4 py-3 text-sm text-destructive-foreground"
        >
          <CircleAlert class="mt-0.5 h-4 w-4 text-destructive" />
          <div>
            <p class="font-medium">Backend unreachable</p>
            <p class="mt-0.5 text-xs text-muted-foreground">{error}</p>
          </div>
        </div>
      {:else if !health}
        <div class="flex items-center gap-2 text-sm text-muted-foreground">
          <Loader2 class="h-4 w-4 animate-spin" />
          Loading…
        </div>
      {:else}
        <dl class="grid gap-4 sm:grid-cols-2">
          <div>
            <dt class="text-xs uppercase tracking-wide text-muted-foreground">Backend version</dt>
            <dd class="mt-1 font-mono text-lg">{health.version}</dd>
          </div>
          <div>
            <dt class="text-xs uppercase tracking-wide text-muted-foreground">Uptime</dt>
            <dd class="mt-1 font-mono text-lg">{health.uptime_s}s</dd>
          </div>
        </dl>
        {#if error}
          <p class="mt-3 text-xs text-amber-400">Last refresh failed: {error}</p>
        {/if}
        {#if lastUpdated}
          <p class="mt-3 text-xs text-muted-foreground">
            Updated {lastUpdated.toLocaleTimeString()}
          </p>
        {/if}
      {/if}
    </CardContent>
  </Card>
</div>
