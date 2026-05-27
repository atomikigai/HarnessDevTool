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
  import RowEditorPanel from '$lib/components/db/RowEditorPanel.svelte';
  import TableSchemaView from '$lib/components/db/TableSchemaView.svelte';
  import {
    Play,
    Plus,
    X,
    RefreshCw,
    Loader2,
    ChevronLeft,
    ChevronRight,
    MoreHorizontal,
    AlertTriangle
  } from '$lib/icons';
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

  // ── Inline cell-edit + insert pending buffer (derived from store) ───────
  /** This tab's cell-edit map (rowIndex → entry). */
  const pendingForActiveTab = $derived(
    activeTab ? (ws.pendingEdits[activeTab.id]?.edits ?? {}) : {}
  );
  /** This tab's inline-insert list (rendered as a band above the grid). */
  const pendingInsertsForActiveTab = $derived(
    activeTab ? (ws.pendingEdits[activeTab.id]?.inserts ?? []) : []
  );
  /** Cross-tab counters for the pending-changes sub-header bar. */
  const pendingTotals = $derived.by(() => {
    let cells = 0;
    let rowCount = 0;
    let inserts = 0;
    let errors = 0;
    for (const tabId in ws.pendingEdits) {
      const bucket = ws.pendingEdits[tabId];
      for (const ri in bucket.edits) {
        rowCount += 1;
        cells += Object.keys(bucket.edits[ri].changes).length;
      }
      for (const ins of bucket.inserts) {
        inserts += 1;
        if (ins.errors) errors += Object.keys(ins.errors).length;
      }
    }
    return { cells, rows: rowCount, inserts, errors };
  });
  let applyingPending = $state(false);

  /** Heuristic — auto-increment columns are read-only on inserts. */
  function isAutoIncrement(col: Column): boolean {
    if (!col.pk) return false;
    if (col.default != null && col.default !== '') return false;
    const t = col.data_type.toLowerCase();
    return t.includes('int') || t.includes('serial');
  }

  /** Per-insert validation: NOT-NULL required + simple type sanity. */
  function validateInsert(
    meta: TableMeta,
    values: Record<string, unknown>
  ): Record<string, string> {
    const errors: Record<string, string> = {};
    for (const col of meta.columns) {
      const raw = values[col.name];
      const empty = raw === undefined || raw === null || raw === '';
      if (empty) {
        const hasDefault = col.default != null && col.default !== '';
        if (!col.nullable && !hasDefault && !isAutoIncrement(col)) {
          errors[col.name] = 'Required';
        }
        continue;
      }
      const t = col.data_type.toLowerCase();
      const numeric =
        t.includes('int') ||
        t.includes('float') ||
        t.includes('numeric') ||
        t.includes('decimal') ||
        t.includes('double') ||
        t.includes('real') ||
        t.includes('serial');
      if (numeric) {
        const n = typeof raw === 'number' ? raw : Number(raw);
        if (!Number.isFinite(n)) errors[col.name] = 'Invalid number';
        continue;
      }
      if (t === 'date') {
        if (!/^\d{4}-\d{2}-\d{2}$/.test(String(raw))) errors[col.name] = 'Use YYYY-MM-DD';
        continue;
      }
      if (t.includes('timestamp') || t.includes('datetime')) {
        if (Number.isNaN(Date.parse(String(raw)))) errors[col.name] = 'Invalid date/time';
        continue;
      }
      if (t.includes('bool')) {
        if (
          typeof raw !== 'boolean' &&
          raw !== 'true' &&
          raw !== 'false' &&
          raw !== 0 &&
          raw !== 1
        )
          errors[col.name] = 'Use true/false';
      }
    }
    return errors;
  }

  function pkFromRow(meta: TableMeta, row: unknown[]): Record<string, unknown> {
    const pk: Record<string, unknown> = {};
    meta.columns.forEach((c, i) => {
      if (c.pk) pk[c.name] = row[i];
    });
    return pk;
  }

  function onCellCommit(rowIndex: number, columnName: string, newValue: unknown) {
    if (!activeTab || activeTab.kind !== 'table' || !activeMeta) return;
    const row = activeTab.result?.rows?.[rowIndex];
    if (!row) return;
    const original = rowToRecord(activeMeta.columns, row);
    const pk = pkFromRow(activeMeta, row);
    dbStore.stageCellEdit(connId, activeTab.id, rowIndex, columnName, newValue, original, pk);
  }

  function onInsertCellCommit(tempId: string, columnName: string, newValue: unknown) {
    if (!activeTab || activeTab.kind !== 'table') return;
    dbStore.updateInsertCell(connId, activeTab.id, tempId, columnName, newValue);
  }

  function onInsertDiscardRow(tempId: string) {
    if (!activeTab) return;
    dbStore.removeInsert(connId, activeTab.id, tempId);
  }

  function discardPending() {
    if (!confirm('Discard all pending edits and new rows?')) return;
    dbStore.clearPendingAll(connId);
    toast.success('Discarded pending changes');
  }

  /**
   * Apply pending changes: inserts FIRST (so freshly-typed rows don't fight
   * ordering with edits), then per-row PUT for cell edits. Validates inserts
   * up front and aborts cleanly if any have errors.
   */
  async function applyPending() {
    if (applyingPending) return;

    // ── Validate inserts up front; abort cleanly if any have errors.
    let firstErrorTabId: string | null = null;
    let anyErrors = false;
    for (const tabId in ws.pendingEdits) {
      const tab = ws.tabs.find((t) => t.id === tabId);
      if (!tab || tab.kind !== 'table' || !tab.tableMeta) continue;
      const bucket = ws.pendingEdits[tabId];
      for (const ins of bucket.inserts) {
        const errs = validateInsert(tab.tableMeta, ins.values);
        dbStore.setInsertErrors(connId, tabId, ins.tempId, errs);
        if (Object.keys(errs).length > 0) {
          anyErrors = true;
          if (!firstErrorTabId) firstErrorTabId = tabId;
        }
      }
    }
    if (anyErrors) {
      if (firstErrorTabId) dbStore.setActiveTab(connId, firstErrorTabId);
      toast.error('Some new rows need attention before apply');
      return;
    }

    applyingPending = true;
    let inserted = 0;
    let updated = 0;
    let failed = 0;
    const tabsTouched = new Set<string>();

    // Snapshot work units BEFORE mutating the buffer.
    const insertWork: Array<{
      tabId: string;
      tempId: string;
      table: string;
      schema?: string;
      database?: string;
      values: Record<string, unknown>;
    }> = [];
    const editWork: Array<{
      tabId: string;
      rowIndex: number;
      table: string;
      schema?: string;
      database?: string;
      pk: Record<string, unknown>;
      changes: Record<string, unknown>;
      original: Record<string, unknown>;
    }> = [];
    for (const tabId in ws.pendingEdits) {
      const tab = ws.tabs.find((t) => t.id === tabId);
      if (!tab || tab.kind !== 'table' || !tab.tableMeta) continue;
      const bucket = ws.pendingEdits[tabId];
      for (const ins of bucket.inserts) {
        // Strip undefined/'' so the backend can apply defaults / auto-inc PKs.
        const cleaned: Record<string, unknown> = {};
        for (const col of tab.tableMeta.columns) {
          const v = ins.values[col.name];
          if (v === undefined || v === '') continue;
          if (isAutoIncrement(col)) continue;
          cleaned[col.name] = v;
        }
        insertWork.push({
          tabId,
          tempId: ins.tempId,
          table: tab.tableMeta.name,
          schema: tab.schema,
          database: tab.database,
          values: cleaned
        });
      }
      for (const ri in bucket.edits) {
        editWork.push({
          tabId,
          rowIndex: Number(ri),
          table: tab.tableMeta.name,
          schema: tab.schema,
          database: tab.database,
          pk: bucket.edits[ri].pk,
          changes: bucket.edits[ri].changes,
          original: bucket.edits[ri].original
        });
      }
    }

    // Inserts first.
    for (const ins of insertWork) {
      try {
        await dbApi.rows.insert(connId, ins.table, {
          schema: ins.schema,
          database: ins.database,
          values: ins.values
        });
        dbStore.removeInsert(connId, ins.tabId, ins.tempId);
        inserted += 1;
        tabsTouched.add(ins.tabId);
      } catch (err) {
        failed += 1;
        // eslint-disable-next-line no-console
        console.error('apply insert failed', err);
      }
    }

    // Then edits.
    for (const ed of editWork) {
      try {
        await dbApi.rows.update(connId, ed.table, {
          schema: ed.schema,
          database: ed.database,
          pk: ed.pk,
          values: ed.changes
        });
        for (const col of Object.keys(ed.changes)) {
          dbStore.stageCellEdit(
            connId,
            ed.tabId,
            ed.rowIndex,
            col,
            ed.original[col],
            ed.original,
            ed.pk
          );
        }
        updated += 1;
        tabsTouched.add(ed.tabId);
      } catch (err) {
        failed += 1;
        // eslint-disable-next-line no-console
        console.error('apply edit failed', err);
      }
    }

    applyingPending = false;
    if (failed === 0) toast.success(`Inserted ${inserted}, updated ${updated}`);
    else toast.error(`Inserted ${inserted}, updated ${updated}, ${failed} failed`);
    for (const tabId of tabsTouched) await dbStore.runTab(connId, tabId);
  }

  function closeTabSafe(tabId: string) {
    const bucket = ws.pendingEdits[tabId];
    let cellCount = 0;
    let insertCount = 0;
    if (bucket) {
      for (const ri in bucket.edits) cellCount += Object.keys(bucket.edits[ri].changes).length;
      insertCount = bucket.inserts.length;
    }
    if (cellCount + insertCount > 0) {
      const parts: string[] = [];
      if (cellCount > 0) parts.push(`${cellCount} pending edit${cellCount === 1 ? '' : 's'}`);
      if (insertCount > 0) parts.push(`${insertCount} new row${insertCount === 1 ? '' : 's'}`);
      if (!confirm(`Discard ${parts.join(' and ')} in this tab?`)) return;
    }
    dbStore.closeTab(connId, tabId);
  }

  // Inner sub-tab per table tab (Data / Schema). Keyed by tab id.
  let tableSubTab = $state<Record<string, 'data' | 'schema'>>({});
  const activeSubTab = $derived<'data' | 'schema'>(
    activeTab?.kind === 'table' ? (tableSubTab[activeTab.id] ?? 'data') : 'data'
  );
  function setSubTab(kind: 'data' | 'schema') {
    if (!activeTab || activeTab.kind !== 'table') return;
    tableSubTab = { ...tableSubTab, [activeTab.id]: kind };
  }

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
    closeTabSafe(id);
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

  /**
   * Default action: inline insert. Creates a new tempId-backed insert row at
   * the top of the grid, immediately ready for the user to type into.
   */
  function onInsertRow() {
    if (!activeMeta || !activeTab) return;
    dbStore.startInsert(connId, activeTab.id, {});
  }

  /** Secondary affordance — opens the full slide-out form (legacy path). */
  function onInsertFullForm() {
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

  <!-- Pending changes bar (amber stripe). Visible whenever there's anything pending. -->
  {#if pendingTotals.cells > 0 || pendingTotals.inserts > 0}
    <div
      class="flex h-9 shrink-0 items-center justify-between gap-3 border-b px-4 text-[11px]"
      style="background: color-mix(in srgb, var(--dot-warn) 14%, transparent); border-color: color-mix(in srgb, var(--dot-warn) 45%, transparent); color: var(--fg-default);"
    >
      <span class="inline-flex items-center gap-2">
        <span
          class="inline-block h-2 w-2 rounded-full"
          style="background: var(--dot-warn);"
          aria-hidden="true"
        ></span>
        <strong class="font-semibold">{pendingTotals.cells}</strong> pending change{pendingTotals.cells ===
        1
          ? ''
          : 's'} across
        <strong class="font-semibold">{pendingTotals.rows}</strong> row{pendingTotals.rows === 1
          ? ''
          : 's'} ·
        <strong class="font-semibold">{pendingTotals.inserts}</strong> new row{pendingTotals.inserts ===
        1
          ? ''
          : 's'}
        {#if pendingTotals.errors > 0}
          <span
            class="ml-2 inline-flex items-center gap-1 rounded px-1.5 py-0.5"
            style="background: color-mix(in srgb, var(--dot-danger) 18%, transparent); color: var(--dot-danger);"
          >
            <AlertTriangle class="h-3 w-3" />
            {pendingTotals.errors} field{pendingTotals.errors === 1 ? '' : 's'} need attention
          </span>
        {/if}
      </span>
      <span class="inline-flex items-center gap-2">
        <Button size="sm" variant="outline" onclick={discardPending} disabled={applyingPending}>
          Discard
        </Button>
        <Button
          size="sm"
          onclick={applyPending}
          disabled={applyingPending || pendingTotals.errors > 0}
        >
          {#if applyingPending}
            <Loader2 class="h-3.5 w-3.5 animate-spin" />
          {/if}
          Apply
        </Button>
      </span>
    </div>
  {/if}

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
          <!-- Toolbar (hidden on table tabs while viewing Schema sub-tab) -->
          {#if !(activeTab.kind === 'table' && activeSubTab === 'schema')}
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
                  <div class="inline-flex items-stretch">
                    <Button
                      size="sm"
                      variant="outline"
                      onclick={onInsertRow}
                      class="rounded-r-none border-r-0"
                      title="Insert a new row inline (typeable at the top of the grid)"
                    >
                      <Plus class="h-3.5 w-3.5" /> Insert row
                    </Button>
                    <Button
                      size="sm"
                      variant="outline"
                      onclick={onInsertFullForm}
                      class="rounded-l-none px-2"
                      title="Use full form (slide-out panel)"
                      aria-label="Use full form"
                    >
                      <MoreHorizontal class="h-3.5 w-3.5" />
                    </Button>
                  </div>
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
          {/if}

          <!-- Data / Schema sub-tabs (table tabs only) -->
          {#if activeTab.kind === 'table'}
            <div
              class="flex h-9 shrink-0 items-center gap-1 border-b px-3"
              style="border-color: var(--border-subtle); background: var(--surface-titlebar);"
              role="tablist"
              aria-label="Table view"
            >
              {#each [{ k: 'data', label: 'Data' }, { k: 'schema', label: 'Schema' }] as t (t.k)}
                {@const sel = activeSubTab === t.k}
                <button
                  type="button"
                  role="tab"
                  aria-selected={sel}
                  onclick={() => setSubTab(t.k as 'data' | 'schema')}
                  class="h-7 rounded-md px-3 text-[11px] font-medium transition-colors"
                  style={sel
                    ? 'background: var(--surface-canvas); color: var(--accent); border: 1px solid var(--border-subtle);'
                    : 'background: transparent; color: var(--fg-muted); border: 1px solid transparent;'}
                >
                  {t.label}
                </button>
              {/each}
            </div>
          {/if}

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
          {:else if activeSubTab === 'schema' && activeMeta}
            <div class="min-h-0 flex-1">
              <TableSchemaView table={activeMeta} />
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
                  editable={true}
                  pendingByRow={pendingForActiveTab}
                  pendingInserts={pendingInsertsForActiveTab}
                  columnsMeta={activeMeta?.columns}
                  isAutoIncrement={isAutoIncrement}
                  onEdit={(row) => onEditRow(row)}
                  onDuplicate={(row) => onDuplicateRow(row)}
                  onDelete={(row) => onDeleteRow(row)}
                  onCellCommit={onCellCommit}
                  onInsertCellCommit={onInsertCellCommit}
                  onInsertDiscardRow={onInsertDiscardRow}
                />
              {/if}
            </div>
          {/if}
        </div>
      {/if}
    </section>

    {#if activeMeta}
      <RowEditorPanel
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
  </div>
</div>
