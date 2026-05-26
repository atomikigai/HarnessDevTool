<!--
  /agents — minimal registry of registered agents.
  Lists rows from GET /api/agents and lets the user create new ones with a
  small `{kind, label}` form. Not surfaced in the IconRail yet (its "Agents"
  entry still routes to the dashboard).
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { Label } from '$lib/components/ui/label';
  import { api, ApiError } from '$lib/api/client';
  import type { Agent } from '$lib/api/models/task';
  import { Loader2, CircleAlert, Plus, RefreshCw, Bot } from '$lib/icons';
  import { toast } from 'svelte-sonner';
  import { safeParse, createAgentSchema } from '$lib/api/schemas/task';
  import { formatDistanceToNow } from 'date-fns';

  let agents = $state<Agent[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);

  let kind = $state('claude');
  let label = $state('');
  let submitting = $state(false);
  let formErrors = $state<string[]>([]);

  async function refresh() {
    loading = true;
    error = null;
    try {
      const res = await api.agents.list();
      agents = res.data ?? [];
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
  }

  async function createAgent(ev: SubmitEvent) {
    ev.preventDefault();
    formErrors = [];
    const validation = safeParse(createAgentSchema, { kind: kind.trim(), label: label.trim() });
    if (!validation.ok) {
      formErrors = validation.errors;
      return;
    }
    submitting = true;
    try {
      const res = await api.agents.create(validation.value);
      agents = [...agents, res.data];
      toast.success(`Agent ${res.data.id} created`);
      label = '';
    } catch (err) {
      const msg =
        err instanceof ApiError
          ? ((err.body as { error?: string } | undefined)?.error ?? err.message)
          : err instanceof Error
            ? err.message
            : String(err);
      formErrors = [msg];
      toast.error(msg);
    } finally {
      submitting = false;
    }
  }

  onMount(refresh);
</script>

<div class="h-full overflow-y-auto">
  <div class="mx-auto flex max-w-4xl flex-col gap-6 p-8">
    <header class="flex items-start justify-between gap-4">
      <div>
        <h1 class="text-3xl font-medium tracking-tight">Agents</h1>
        <p class="mt-1 text-sm" style="color: var(--fg-muted);">
          Registry of agents that can claim tasks. F2 lets you list and create entries.
        </p>
      </div>
      <Button variant="outline" size="sm" onclick={refresh} disabled={loading}>
        {#if loading}<Loader2 class="h-4 w-4 animate-spin" />{:else}<RefreshCw
            class="h-4 w-4"
          />{/if}
        Refresh
      </Button>
    </header>

    <section
      class="rounded-md border p-4"
      style="border-color: var(--border-subtle); background: var(--surface-panel);"
    >
      <h2 class="h-eyebrow mb-3">Create agent</h2>
      <form class="flex flex-wrap items-end gap-3" onsubmit={createAgent}>
        <div class="flex w-32 flex-col gap-1.5">
          <Label for="kind">Kind</Label>
          <Input id="kind" bind:value={kind} autocomplete="off" />
        </div>
        <div class="flex flex-1 flex-col gap-1.5">
          <Label for="label">Label</Label>
          <Input id="label" bind:value={label} placeholder="planner-1" autocomplete="off" />
        </div>
        <Button type="submit" disabled={submitting}>
          {#if submitting}<Loader2 class="h-4 w-4 animate-spin" />{/if}
          <Plus class="h-3.5 w-3.5" /> Create
        </Button>
      </form>
      {#if formErrors.length > 0}
        <ul
          class="mt-3 list-disc rounded-md border px-4 py-2 pl-6 text-xs"
          style="border-color: color-mix(in srgb, var(--dot-danger) 35%, transparent); background: color-mix(in srgb, var(--dot-danger) 10%, transparent); color: var(--dot-danger);"
        >
          {#each formErrors as e (e)}
            <li>{e}</li>
          {/each}
        </ul>
      {/if}
    </section>

    <section>
      <h2 class="h-eyebrow mb-2">Registered agents</h2>
      {#if loading && agents.length === 0}
        <div class="flex items-center gap-2 text-sm" style="color: var(--fg-muted);">
          <Loader2 class="h-4 w-4 animate-spin" /> Loading…
        </div>
      {:else if error}
        <div
          class="flex items-start gap-3 rounded-md border p-4 text-sm"
          style="border-color: color-mix(in srgb, var(--dot-danger) 35%, transparent); background: color-mix(in srgb, var(--dot-danger) 10%, transparent); color: var(--dot-danger);"
        >
          <CircleAlert class="mt-0.5 h-4 w-4" />
          <div>
            <p class="font-medium">Failed to load agents</p>
            <p class="mt-0.5 text-xs" style="color: var(--fg-muted);">{error}</p>
          </div>
        </div>
      {:else if agents.length === 0}
        <p class="text-sm" style="color: var(--fg-muted);">No agents registered yet.</p>
      {:else}
        <div class="overflow-hidden rounded-md border" style="border-color: var(--border-subtle);">
          <table class="w-full text-sm">
            <thead
              class="text-left text-[10px] uppercase tracking-wider"
              style="background: var(--surface-titlebar); color: var(--fg-label);"
            >
              <tr>
                <th class="px-4 py-2">ID</th>
                <th class="px-4 py-2">Kind</th>
                <th class="px-4 py-2">Label</th>
                <th class="px-4 py-2">Created</th>
              </tr>
            </thead>
            <tbody>
              {#each agents as a (a.id)}
                <tr class="border-t" style="border-color: var(--row-divider);">
                  <td class="px-4 py-2 font-mono text-[12px]" style="color: var(--fg-muted);"
                    >{a.id}</td
                  >
                  <td class="px-4 py-2">
                    <span class="inline-flex items-center gap-1.5 font-mono text-[12px]">
                      <Bot class="h-3 w-3" style="color: var(--fg-muted);" />
                      {a.kind}
                    </span>
                  </td>
                  <td class="px-4 py-2">{a.label}</td>
                  <td class="px-4 py-2 text-[12px]" style="color: var(--fg-muted);">
                    {formatDistanceToNow(new Date(a.created_at), { addSuffix: true })}
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </section>
  </div>
</div>
