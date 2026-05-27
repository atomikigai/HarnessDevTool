<!--
  Connection workspace.
  Layout (left → right):
    [ Sidebar: db selector + schema tree ]
    [ Main:    tab bar + active tab body  ]
  Tab kinds:
    sql   — CodeMirror editor + run button + results
    table — paginated SELECT * grid + Insert button
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { page } from '$app/stores';
  import { Button } from '$lib/components/ui/button';
  import { dbStore, type DbTab } from '$lib/stores/db.svelte';
  import { engineLabel, type Column, type TableMeta } from '$lib/api/db';
  import SchemaTree from '$lib/components/db/SchemaTree.svelte';
  import SqlEditor from '$lib/components/db/SqlEditor.svelte';
  import ResultGrid from '$lib/components/db/ResultGrid.svelte';
  import RowEditorDialog from '$lib/components/db/RowEditorDialog.svelte';
  import { Play, Plus, X, RefreshCw, Loader2, ChevronLeft, ChevronRight } from '$lib/icons';
  import { dbApi } from '$lib/api/db';
  import { toast } from 'svelte-sonner';

  const connId = $derived(($page.params.id ?? '') as string);
  const conn = $derived(dbStore.connections.find((c) => c.id === connId) ?? null);
  const ws = $derived(dbStore.workspace(connId));
  const activeTab = $derived<DbTab | null>(ws.tabs.find((t) => t.id === ws.activeTabId) ?? null);
  const activeMeta = $derived<TableMeta | null>(
    activeTab?.kind === 'table' ? (activeTab.tableMeta ?? null) : null
  );
  const activePkCols = $derived<string[]>(
    activeMeta ? activeMeta.columns.filter((c) => c.pk).map((c) => c.name) : []
  );

  // Row editor state
  let editorOpen = $state(false);
  let editorMode = $state<'insert' | 'update' | 'duplicate'>('insert');
  let editorInitial = $state<Record<string, unknown> | undefined>(undefined);

  onMount(async () => {
    if (dbStore.connections.length === 0) await dbStore.refresh();
    await dbStore.loadDatabases(connId);
    await dbStore.loadSchema(connId, dbStore.workspace(connId).database ?? undefined);
  });

  function changeDatabase(db: string) {
    dbStore.setDatabase(connId, db);
  }

  function onOpenTable(schema: string, t: TableMeta) {
    const id = dbStore.openTableTab(connId, schema, t);
    void dbStore.runTab(connId, id);
  }

  function onNewSqlTab() {
    dbStore.openSqlTab(connId);
  }

  function closeTab(id: string) {
    dbStore.closeTab(connId, id);
  }

  async function runActive() {
    if (!activeTab) return;
    await dbStore.runTab(connId, activeTab.id);
  }

  function setPageSize(n: number) {
    if (!activeTab) return;
    dbStore.patchTab(connId, activeTab.id, { pageSize: n, page: 0 });
    void dbStore.runTab(connId, activeTab.id);
  }

  function gotoPage(delta: number) {
    if (!activeTab) return;
    const next = Math.max(0, activeTab.page + delta);
    dbStore.patchTab(connId, activeTab.id, { page: next });
    void dbStore.runTab(connId, activeTab.id);
  }

  function rowToRecord(cols: Column[] | undefined, row: unknown[]): Record<string, unknown> {
    const out: Record<string, unknown> = {};
    if (!cols) return out;
    cols.forEach((c, i) => (out[c.name] = row[i]));
    return out;
  }

  function onEditRow(row: unknown[]) {
    if (!activeMeta) return;
    editorMode = 'update';
    editorInitial = rowToRecord(activeMeta.columns, row);
    editorOpen = true;
  }

  function onDuplicateRow(row: unknown[]) {
    if (!activeMeta) return;
    editorMode = 'duplicate';
    editorInitial = rowToRecord(activeMeta.columns, row);
    editorOpen = true;
  }

  async function onDeleteRow(row: unknown[]) {
    if (!activeMeta || !activeTab) return;
    const rec = rowToRecord(activeMeta.columns, row);
    const pk: Record<string, unknown> = {};
    for (const c of activeMeta.columns) if (c.pk) pk[c.name] = rec[c.name];
    const pkStr = Object.entries(pk)
      .map(([k, v]) => `${k}=${v}`)
      .join(', ');
    if (!confirm(`Delete row where ${pkStr}?`)) return;
    try {
      await dbApi.rows.remove(connId, activeMeta.name, {
        schema: activeTab.schema,
        database: activeTab.database,
        pk
      });
      toast.success('Row deleted');
      await dbStore.runTab(connId, activeTab.id);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Delete failed');
    }
  }

  function onInsertRow() {
    if (!activeMeta) return;
    editorMode = 'insert';
    editorInitial = undefined;
    editorOpen = true;
  }
</script>

<div class="flex h-full min-h-0 flex-col">
  <!-- Subheader -->
  <header
    class="flex h-12 shrink-0 items-center justify-between gap-4 border-b px-5"
    style="background: var(--surface-window); border-color: var(--border-subtle);"
  >
    <div class="flex items-center gap-3">
      <a
        href="/db"
        class="text-xs hover:underline"
        style="color: var(--fg-muted);"
        title="Back to connections"
      >
        ← Connections
      </a>
      <span style="color: var(--fg-muted);">/</span>
      <span class="font-semibold" style="color: var(--fg-default);">
        {conn?.name ?? connId}
      </span>
      {#if conn}
        <span
          class="rounded px-1.5 py-0.5 text-[9px] font-bold uppercase tracking-wider text-white"
          style="background: var(--accent);"
        >
          {engineLabel(conn.engine)}
        </span>
      {/if}
    </div>
    <div class="flex items-center gap-2">
      <Button
        variant="outline"
        size="sm"
        onclick={() => dbStore.loadSchema(connId, ws.database ?? undefined)}
        disabled={ws.schemaLoading}
      >
        {#if ws.schemaLoading}
          <Loader2 class="h-3.5 w-3.5 animate-spin" />
        {:else}
          <RefreshCw class="h-3.5 w-3.5" />
        {/if}
        Refresh schema
      </Button>
    </div>
  </header>

  <div class="flex min-h-0 flex-1">
    <!-- Sidebar -->
    <aside
      class="flex w-72 shrink-0 flex-col border-r"
      style="background: var(--surface-panel); border-color: var(--border-subtle);"
    >
      <!-- Database selector -->
      <div class="px-3 pt-3">
        <div class="h-eyebrow mb-1.5">Database</div>
        <select
          value={ws.database ?? ''}
          onchange={(e) => changeDatabase((e.currentTarget as HTMLSelectElement).value)}
          disabled={ws.databases.length === 0 ||
            (conn?.engine === 'sqlite' && ws.databases.length <= 1)}
          class="h-8 w-full rounded-md border px-2 text-xs"
          style="border-color: var(--border-input); background: var(--surface-titlebar); color: var(--fg-default);"
        >
          {#if ws.databases.length === 0}
            <option value="">{conn?.database ?? '(default)'}</option>
          {:else}
            {#each ws.databases as d (d)}
              <option value={d}>{d}</option>
            {/each}
          {/if}
        </select>
      </div>

      <SchemaTree
        tree={ws.schema}
        loading={ws.schemaLoading}
        error={ws.schemaError}
        {onOpenTable}
        activeTable={activeTab?.kind === 'table'
          ? { schema: activeTab.schema ?? '', name: activeTab.table ?? '' }
          : null}
      />
    </aside>

    <!-- Main -->
    <section class="flex min-w-0 flex-1 flex-col" style="background: var(--surface-canvas);">
      <!-- Tab bar -->
      <div
        class="flex h-10 shrink-0 items-center gap-1 border-b px-2"
        style="border-color: var(--border-subtle); background: var(--surface-titlebar);"
      >
        {#each ws.tabs as t (t.id)}
          {@const active = t.id === ws.activeTabId}
          <div
            class="flex h-8 items-center gap-1.5 rounded-md border px-3 text-[12px]"
            style={active
              ? 'background: var(--surface-canvas); border-color: var(--border-subtle); color: var(--accent); font-weight: 600;'
              : 'background: transparent; border-color: transparent; color: var(--fg-muted);'}
          >
            <button type="button" onclick={() => dbStore.setActiveTab(connId, t.id)}>
              <span class="font-mono text-[11px]">{t.kind === 'sql' ? '⌥' : '⊞'}</span>
              <span class="ml-1.5">{t.title}</span>
            </button>
            <button
              type="button"
              onclick={() => closeTab(t.id)}
              class="rounded p-0.5 hover:bg-[var(--accent-soft)]"
              title="Close tab"
            >
              <X class="h-3 w-3" />
            </button>
          </div>
        {/each}
        <button
          type="button"
          onclick={onNewSqlTab}
          class="ml-1 inline-flex h-7 items-center gap-1 rounded-md border border-dashed px-2 text-[11px]"
          style="border-color: var(--border-input); color: var(--fg-muted);"
          title="New SQL tab"
        >
          <Plus class="h-3 w-3" /> New SQL
        </button>
      </div>

      <!-- Tab body -->
      {#if !activeTab}
        <div
          class="flex flex-1 flex-col items-center justify-center gap-3 text-sm"
          style="color: var(--fg-muted);"
        >
          <p>No tab open. Pick a table in the sidebar or open a new SQL tab.</p>
          <Button size="sm" onclick={onNewSqlTab}>
            <Plus class="h-3.5 w-3.5" /> New SQL tab
          </Button>
        </div>
      {:else}
        <div class="flex min-h-0 flex-1 flex-col">
          <!-- Toolbar -->
          <div
            class="flex h-11 shrink-0 items-center justify-between gap-3 border-b px-4"
            style="border-color: var(--border-subtle); background: var(--surface-window);"
          >
            <div class="flex items-center gap-2">
              <Button size="sm" onclick={runActive} disabled={activeTab.loading}>
                {#if activeTab.loading}
                  <Loader2 class="h-3.5 w-3.5 animate-spin" />
                {:else}
                  <Play class="h-3.5 w-3.5" />
                {/if}
                Run
              </Button>
              {#if activeTab.kind === 'table'}
                <Button size="sm" variant="outline" onclick={onInsertRow}>
                  <Plus class="h-3.5 w-3.5" /> Insert row
                </Button>
              {/if}
              <label
                class="ml-2 inline-flex items-center gap-1.5 text-[11px]"
                style="color: var(--fg-muted);"
              >
                Page size
                <select
                  value={activeTab.pageSize}
                  onchange={(e) =>
                    setPageSize(Number((e.currentTarget as HTMLSelectElement).value))}
                  class="h-7 rounded border px-1.5 text-[11px]"
                  style="border-color: var(--border-input); background: var(--surface-titlebar);"
                >
                  {#each [50, 100, 200, 500, 1000] as n (n)}
                    <option value={n}>{n}</option>
                  {/each}
                </select>
              </label>
            </div>

            <div
              class="flex items-center gap-2 font-mono text-[11px]"
              style="color: var(--fg-muted);"
            >
              <button
                type="button"
                class="rounded p-1 hover:bg-[var(--accent-soft)]"
                onclick={() => gotoPage(-1)}
                disabled={activeTab.page === 0}
              >
                <ChevronLeft class="h-3.5 w-3.5" />
              </button>
              <span>page {activeTab.page + 1}</span>
              <button
                type="button"
                class="rounded p-1 hover:bg-[var(--accent-soft)]"
                onclick={() => gotoPage(1)}
              >
                <ChevronRight class="h-3.5 w-3.5" />
              </button>
            </div>
          </div>

          <!-- Editor / table -->
          {#if activeTab.kind === 'sql'}
            <div class="flex min-h-0 flex-1 flex-col">
              <div class="h-2/5 min-h-[140px] border-b" style="border-color: var(--border-subtle);">
                <SqlEditor
                  value={activeTab.sql ?? ''}
                  onChange={(v) => dbStore.patchTab(connId, activeTab.id, { sql: v })}
                  onRun={runActive}
                />
              </div>
              <div class="min-h-0 flex-1">
                {#if activeTab.error}
                  <div
                    class="m-4 rounded-md border px-3 py-2 text-xs"
                    style="border-color: color-mix(in srgb, var(--dot-danger) 35%, transparent); background: color-mix(in srgb, var(--dot-danger) 10%, transparent); color: var(--dot-danger);"
                  >
                    {activeTab.error}
                  </div>
                {:else}
                  <ResultGrid result={activeTab.result} />
                {/if}
              </div>
            </div>
          {:else}
            <div class="min-h-0 flex-1">
              {#if activeTab.error}
                <div
                  class="m-4 rounded-md border px-3 py-2 text-xs"
                  style="border-color: color-mix(in srgb, var(--dot-danger) 35%, transparent); background: color-mix(in srgb, var(--dot-danger) 10%, transparent); color: var(--dot-danger);"
                >
                  {activeTab.error}
                </div>
              {:else}
                <ResultGrid
                  result={activeTab.result}
                  pkColumns={activePkCols}
                  onEdit={(row) => onEditRow(row)}
                  onDuplicate={(row) => onDuplicateRow(row)}
                  onDelete={(row) => onDeleteRow(row)}
                />
              {/if}
            </div>
          {/if}
        </div>
      {/if}
    </section>
  </div>
</div>

{#if activeMeta}
  <RowEditorDialog
    bind:open={editorOpen}
    mode={editorMode}
    connectionId={connId}
    schema={activeTab?.schema}
    database={activeTab?.database}
    table={activeMeta}
    initialValues={editorInitial}
    onSaved={() => activeTab && dbStore.runTab(connId, activeTab.id)}
  />
{/if}
