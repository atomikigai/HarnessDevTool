<!--
  /db — connections browser.
  Card grid (paper feel), inspired by `harness-ssh.jsx` ConnCard.
  Hover reveals kebab actions. Empty state and error banner included.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { Button } from '$lib/components/ui/button';
  import { dbApi, engineLabel, type Connection } from '$lib/api/db';
  import { dbStore } from '$lib/stores/db.svelte';
  import { Plus, Loader2, RefreshCw, Edit3, Trash2, Activity } from '$lib/icons';
  import ConnectionFormDialog from '$lib/components/db/ConnectionFormDialog.svelte';
  import { confirmDialog } from '$lib/components/ui/confirm-dialog';
  import { toast } from 'svelte-sonner';
  import { ApiError } from '$lib/api/client';

  let dialogOpen = $state(false);
  let editing = $state<Connection | null>(null);
  let filter = $state('');
  let testing = $state<Record<string, boolean>>({});
  let testResults = $state<Record<string, { ok: boolean; text: string }>>({});

  onMount(() => {
    void (async () => {
      await dbStore.refresh();
      if (
        dbStore.activeConnectionId &&
        dbStore.connections.some((c) => c.id === dbStore.activeConnectionId)
      ) {
        goto(`/db/${dbStore.activeConnectionId}`, { replaceState: true });
      }
    })();
  });

  const filtered = $derived.by(() => {
    const q = filter.trim().toLowerCase();
    if (!q) return dbStore.connections;
    return dbStore.connections.filter(
      (c) =>
        c.name.toLowerCase().includes(q) ||
        (c.host ?? '').toLowerCase().includes(q) ||
        c.database.toLowerCase().includes(q)
    );
  });

  function openCreate() {
    editing = null;
    dialogOpen = true;
  }

  function openEdit(c: Connection) {
    editing = c;
    dialogOpen = true;
  }

  async function onTest(c: Connection) {
    testing = { ...testing, [c.id]: true };
    try {
      const res = await dbApi.connections.test(c.id);
      testResults = {
        ...testResults,
        [c.id]: {
          ok: res.data.ok,
          text: res.data.ok
            ? `${res.data.latency_ms ?? '?'}ms${res.data.server_version ? ` · ${res.data.server_version}` : ''}`
            : (res.data.error ?? 'Failed')
        }
      };
    } catch (err) {
      testResults = {
        ...testResults,
        [c.id]: {
          ok: false,
          text:
            err instanceof ApiError
              ? `${err.status}`
              : err instanceof Error
                ? err.message
                : 'Failed'
        }
      };
    } finally {
      testing = { ...testing, [c.id]: false };
    }
  }

  async function onDelete(c: Connection) {
    const ok = await confirmDialog({
      title: `Delete connection "${c.name}"?`,
      description:
        'This removes the saved connection from the harness. It does not touch the database itself.',
      confirmLabel: 'Delete',
      destructive: true
    });
    if (!ok) return;
    try {
      await dbApi.connections.remove(c.id);
      await dbStore.refresh();
      toast.success('Connection deleted');
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Delete failed');
    }
  }

  function onConnect(c: Connection) {
    dbStore.setActiveConnection(c.id);
    goto(`/db/${c.id}`);
  }

  function engineBadgeColor(eng: string): string {
    if (eng === 'postgres') return '#336791';
    if (eng === 'mysql') return '#00758F';
    return '#7a5c3c';
  }
</script>

<div class="flex h-full min-h-0 flex-col">
  <!-- Subheader -->
  <header
    class="flex h-14 shrink-0 items-center justify-between gap-4 border-b px-6"
    style="background: var(--surface-window); border-color: var(--border-subtle);"
  >
    <div>
      <h1 class="font-serif text-xl font-semibold tracking-tight" style="color: var(--fg-default);">
        Databases
      </h1>
      <p class="text-xs" style="color: var(--fg-muted);">
        Manage SQL connections — SQLite, PostgreSQL, MySQL.
      </p>
    </div>
    <div class="flex items-center gap-2">
      <Button
        variant="outline"
        size="sm"
        onclick={() => dbStore.refresh()}
        disabled={dbStore.listLoading}
      >
        {#if dbStore.listLoading}
          <Loader2 class="h-3.5 w-3.5 animate-spin" />
        {:else}
          <RefreshCw class="h-3.5 w-3.5" />
        {/if}
        Refresh
      </Button>
      <Button size="sm" onclick={openCreate}>
        <Plus class="h-3.5 w-3.5" /> Add connection
      </Button>
    </div>
  </header>

  <!-- Body -->
  <div class="flex-1 overflow-auto px-8 py-8">
    <div class="mx-auto flex w-full max-w-5xl flex-col gap-6">
      <!-- Search -->
      <div
        class="flex items-center gap-2 rounded-md border px-3 py-2"
        style="border-color: var(--border-input); background: var(--surface-titlebar);"
      >
        <input
          bind:value={filter}
          placeholder="Search connections…"
          class="flex-1 bg-transparent text-sm outline-none"
          style="color: var(--fg-default);"
        />
      </div>

      {#if dbStore.listError}
        <div
          class="rounded-md border px-4 py-3 text-sm"
          style="border-color: color-mix(in srgb, var(--dot-danger) 35%, transparent); background: color-mix(in srgb, var(--dot-danger) 10%, transparent); color: var(--dot-danger);"
        >
          Backend unavailable — {dbStore.listError}
        </div>
      {/if}

      {#if dbStore.loaded && filtered.length === 0}
        <div
          class="flex flex-col items-center gap-3 rounded-lg border border-dashed px-6 py-16 text-center"
          style="border-color: var(--border-subtle);"
        >
          <p class="text-sm" style="color: var(--fg-muted);">
            {filter ? 'No connections match your filter.' : 'No saved connections yet.'}
          </p>
          {#if !filter}
            <Button onclick={openCreate} size="sm">
              <Plus class="h-3.5 w-3.5" /> Add your first connection
            </Button>
          {/if}
        </div>
      {:else}
        <div class="grid grid-cols-1 gap-3 md:grid-cols-2">
          {#each filtered as conn (conn.id)}
            {@const tr = testResults[conn.id]}
            <div
              class="group relative flex flex-col gap-2 rounded-lg border p-4 transition-all hover:shadow-[var(--shadow-card)]"
              style="border-color: var(--border-subtle); background: var(--surface-window);"
            >
              <button
                type="button"
                class="absolute inset-0 cursor-pointer rounded-lg"
                aria-label="Open {conn.name}"
                onclick={() => onConnect(conn)}
              ></button>

              <div class="relative flex items-start justify-between gap-2">
                <div class="min-w-0 flex-1">
                  <div class="flex items-center gap-2">
                    <h3 class="truncate text-sm font-semibold" style="color: var(--fg-default);">
                      {conn.name}
                    </h3>
                    <span
                      class="rounded px-1.5 py-0.5 text-[9px] font-bold uppercase tracking-wider text-white"
                      style="background: {engineBadgeColor(conn.engine)};"
                    >
                      {engineLabel(conn.engine)}
                    </span>
                  </div>
                  <p class="mt-1 truncate font-mono text-xs" style="color: var(--accent);">
                    {conn.engine === 'sqlite'
                      ? conn.database
                      : `${conn.username ? conn.username + '@' : ''}${conn.host}:${conn.port}/${conn.database}`}
                  </p>
                </div>
              </div>

              {#if tr}
                <div
                  class="relative inline-flex items-center gap-1 text-[11px]"
                  style="color: {tr.ok ? 'var(--dot-success)' : 'var(--dot-danger)'};"
                >
                  <Activity class="h-3 w-3" />
                  {tr.text}
                </div>
              {/if}

              <div class="relative mt-1 flex items-center gap-2">
                <Button
                  variant="outline"
                  size="sm"
                  onclick={(e) => {
                    e.stopPropagation();
                    onTest(conn);
                  }}
                  disabled={testing[conn.id]}
                >
                  {#if testing[conn.id]}
                    <Loader2 class="h-3 w-3 animate-spin" />
                  {/if}
                  Test
                </Button>
                <Button
                  size="sm"
                  onclick={(e) => {
                    e.stopPropagation();
                    onConnect(conn);
                  }}
                >
                  Connect
                </Button>
                <span class="flex-1"></span>
                <button
                  type="button"
                  title="Edit"
                  class="rounded p-1.5 text-[var(--fg-muted)] hover:bg-[var(--accent-soft)] hover:text-[var(--accent)]"
                  onclick={(e) => {
                    e.stopPropagation();
                    openEdit(conn);
                  }}
                >
                  <Edit3 class="h-3.5 w-3.5" />
                </button>
                <button
                  type="button"
                  title="Delete"
                  class="rounded p-1.5 text-[var(--fg-muted)] hover:bg-[color-mix(in_srgb,var(--dot-danger)_10%,transparent)] hover:text-[var(--dot-danger)]"
                  onclick={(e) => {
                    e.stopPropagation();
                    onDelete(conn);
                  }}
                >
                  <Trash2 class="h-3.5 w-3.5" />
                </button>
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  </div>
</div>

<ConnectionFormDialog bind:open={dialogOpen} existing={editing} onSaved={() => dbStore.refresh()} />
