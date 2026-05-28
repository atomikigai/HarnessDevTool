<!--
  WorkspaceSwitcher — top-of-app dropdown showing the active profile
  (workspace) and letting the user create / switch.

  Switching requires a backend restart in this slice (no hot-swap yet), so
  the dropdown surfaces a clear "needs restart" toast after activation.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import {
    api,
    ApiError,
    type ProfileSummary,
    type ActiveProfile
  } from '$lib/api/client';
  import { Button } from '$lib/components/ui/button';
  import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogDescription,
    DialogFooter
  } from '$lib/components/ui/dialog';
  import { Input } from '$lib/components/ui/input';
  import { Label } from '$lib/components/ui/label';
  import { toast } from 'svelte-sonner';

  let profiles = $state<ProfileSummary[]>([]);
  let active = $state<ActiveProfile | null>(null);
  let open = $state(false);

  let createOpen = $state(false);
  let newId = $state('');
  let newName = $state('');
  let newPath = $state('');
  let creating = $state(false);
  let createError = $state<string | null>(null);

  async function refresh() {
    try {
      const [list, current] = await Promise.all([api.profiles.list(), api.profiles.active()]);
      profiles = list.data;
      active = current.data;
    } catch (err) {
      console.warn('profiles refresh failed', err);
    }
  }

  onMount(refresh);

  async function onPick(id: string) {
    if (!active) return;
    if (id === active.active) {
      open = false;
      return;
    }
    try {
      const res = await api.profiles.activate(id);
      open = false;
      if (res.data.requires_restart) {
        toast.info(`Profile "${id}" queued for next backend restart`, {
          description: 'Restart harness-server to load this workspace.',
          duration: 8000
        });
        void refresh();
        return;
      }
      // Hot-swap fired. Backend is rebuilding right now — poll /health
      // until it answers again, then reload the page so every store
      // re-subscribes against the new profile's data.
      const swapping = toast.loading(`Switching to "${id}"…`, {
        description: 'Backend is rebuilding workspace state.'
      });
      try {
        await waitForBackend();
        toast.dismiss(swapping);
        toast.success(`Workspace "${id}" active`);
        // Hard reload: cheapest way to clear in-memory caches (sessions,
        // tasks, db connections) and re-subscribe SSE against new threads.
        if (typeof window !== 'undefined') window.location.reload();
      } catch (e) {
        toast.dismiss(swapping);
        toast.error(`Backend did not come back online: ${e}`);
      }
    } catch (err) {
      const msg = err instanceof ApiError ? err.message : String(err);
      toast.error(`Activate failed: ${msg}`);
    }
  }

  /**
   * Poll /health until the backend answers with the same protocol version.
   * The hot-swap is fast (~300ms) but we give it up to 10s before failing.
   */
  async function waitForBackend(): Promise<void> {
    const start = Date.now();
    const deadline = start + 10_000;
    // Brief initial pause — the activate request hasn't finished returning
    // to us until just now; the backend is about to shut down its axum
    // server in a few ms.
    await new Promise((r) => setTimeout(r, 250));
    while (Date.now() < deadline) {
      try {
        const res = await api.health();
        if (res.status === 200) return;
      } catch {
        // ignore — backend down mid-swap
      }
      await new Promise((r) => setTimeout(r, 250));
    }
    throw new Error('timeout');
  }

  async function onCreate(ev: SubmitEvent) {
    ev.preventDefault();
    if (creating) return;
    creating = true;
    createError = null;
    try {
      const body: { id: string; display_name: string; path?: string } = {
        id: newId.trim(),
        display_name: newName.trim()
      };
      if (newPath.trim()) body.path = newPath.trim();
      await api.profiles.create(body);
      toast.success(`Workspace "${body.display_name}" created`);
      newId = '';
      newName = '';
      newPath = '';
      createOpen = false;
      await refresh();
    } catch (err) {
      createError = err instanceof ApiError ? err.message : String(err);
    } finally {
      creating = false;
    }
  }

  const activeRow = $derived(profiles.find((p) => p.active) ?? null);
</script>

<div class="relative inline-flex items-center gap-2">
  <button
    type="button"
    onclick={() => (open = !open)}
    class="inline-flex items-center gap-1.5 rounded-md border px-2.5 py-1 text-[12px] font-medium transition-colors hover:bg-[var(--accent-soft)]"
    style="border-color: var(--border-subtle); color: var(--fg-default); background: var(--surface-titlebar);"
    title="Switch workspace"
  >
    <span class="font-mono text-[10px]" style="color: var(--fg-muted);">workspace:</span>
    <span class="truncate max-w-[160px]">
      {activeRow?.display_name ?? active?.active ?? 'default'}
    </span>
    <svg class="h-3 w-3 opacity-60" viewBox="0 0 12 12" fill="none">
      <path d="M3 4.5l3 3 3-3" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
    </svg>
  </button>

  {#if active?.pending && active.pending !== active.active}
    <span
      class="inline-flex items-center rounded border px-1.5 py-0.5 text-[10px]"
      style="color: rgb(251 191 36); border-color: rgba(251 191 36 / 0.4); background: rgba(251 191 36 / 0.1);"
      title={`Restart backend to load "${active.pending}"`}
    >
      restart pending → {active.pending}
    </span>
  {/if}

  {#if open}
    <button
      type="button"
      class="fixed inset-0 z-30 cursor-default bg-transparent"
      aria-label="Close workspace switcher"
      onclick={() => (open = false)}
    ></button>
    <div
      class="absolute left-0 top-full z-40 mt-1 w-[280px] overflow-hidden rounded-md border shadow-lg"
      style="background: var(--surface-panel); border-color: var(--border-subtle);"
    >
      <div
        class="border-b px-3 py-1.5 text-[10px] font-bold uppercase tracking-wider"
        style="border-color: var(--border-subtle); color: var(--fg-label);"
      >
        Workspaces
      </div>
      <div class="max-h-[260px] overflow-y-auto">
        {#each profiles as p (p.id)}
          <button
            type="button"
            onclick={() => onPick(p.id)}
            class="flex w-full flex-col items-start gap-0.5 border-b px-3 py-2 text-left transition-colors hover:bg-[var(--accent-soft)]"
            style="border-color: var(--border-subtle);"
          >
            <div class="flex w-full items-center gap-2">
              <span
                class="h-1.5 w-1.5 rounded-full"
                style={p.active
                  ? 'background: var(--accent);'
                  : 'background: var(--border-input);'}
              ></span>
              <span
                class="flex-1 truncate text-[13px]"
                style={p.active
                  ? 'color: var(--accent); font-weight: 600;'
                  : 'color: var(--fg-default);'}
              >
                {p.display_name}
              </span>
              <span class="font-mono text-[10px]" style="color: var(--fg-muted);">
                {p.id}
              </span>
            </div>
            {#if p.path}
              <span class="truncate font-mono text-[10px]" style="color: var(--fg-muted);">
                {p.path}
              </span>
            {/if}
          </button>
        {/each}
      </div>
      <button
        type="button"
        onclick={() => {
          open = false;
          createOpen = true;
        }}
        class="flex w-full items-center justify-center gap-1.5 border-t px-3 py-2 text-[12px] font-medium transition-colors hover:bg-[var(--accent-soft)]"
        style="border-color: var(--border-subtle); color: var(--accent);"
      >
        + Create workspace
      </button>
    </div>
  {/if}
</div>

<Dialog bind:open={createOpen}>
  <DialogContent class="sm:max-w-md">
    <DialogHeader>
      <DialogTitle>New workspace</DialogTitle>
      <DialogDescription>
        Isolated profile under <code class="font-mono">~/.harness/profiles/&lt;id&gt;/</code>.
        Sessions, threads, tasks and DB connections stay separate per workspace.
      </DialogDescription>
    </DialogHeader>
    <form class="mt-3 flex flex-col gap-3" onsubmit={onCreate}>
      <div class="flex flex-col gap-1">
        <Label for="ws-id">ID</Label>
        <Input
          id="ws-id"
          bind:value={newId}
          placeholder="aventi"
          autocomplete="off"
          required
        />
        <p class="text-[10px]" style="color: var(--fg-muted);">
          ascii alphanumeric + <code class="font-mono">-</code>/<code class="font-mono">_</code>
        </p>
      </div>
      <div class="flex flex-col gap-1">
        <Label for="ws-name">Display name</Label>
        <Input
          id="ws-name"
          bind:value={newName}
          placeholder="Aventi project"
          autocomplete="off"
          required
        />
      </div>
      <div class="flex flex-col gap-1">
        <Label for="ws-path">Project path (optional)</Label>
        <Input
          id="ws-path"
          bind:value={newPath}
          placeholder="/home/me/code/aventi"
          autocomplete="off"
        />
      </div>
      {#if createError}
        <p
          class="rounded-md border px-3 py-2 text-xs"
          style="border-color: color-mix(in srgb, var(--dot-danger) 35%, transparent); background: color-mix(in srgb, var(--dot-danger) 10%, transparent); color: var(--dot-danger);"
        >
          {createError}
        </p>
      {/if}
      <DialogFooter>
        <Button
          type="button"
          variant="ghost"
          onclick={() => (createOpen = false)}
          disabled={creating}
        >
          Cancel
        </Button>
        <Button type="submit" disabled={creating}>Create</Button>
      </DialogFooter>
    </form>
  </DialogContent>
</Dialog>
