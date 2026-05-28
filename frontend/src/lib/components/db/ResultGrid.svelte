<!--
  ResultGrid — paginated, virtualized result table.

  Styling cribbed from `harness-table-v2.jsx`: sticky monospace header
  with type chips, alternating row stripes, hover-revealed row actions
  on the right (kebab → Edit / Duplicate / Delete). Uses @tanstack/
  virtual-core for row virtualization whenever row count exceeds a
  threshold; otherwise renders directly for simplicity.

  F4 inline-edit slice: when `editable` is true (table-browser tabs with
  a known PK), double-click on a non-PK cell swaps it for an <input>.
  ESC cancels, Enter commits to the parent pending buffer, Tab commits
  and advances to the next editable cell in the row. Cells with a
  pending change get a tinted left-border treatment.
-->
<script lang="ts">
  import { onDestroy, tick, untrack } from 'svelte';
  import type { QueryResult } from '$lib/api/db';
  import {
    Virtualizer,
    observeElementOffset,
    observeElementRect,
    elementScroll
  } from '@tanstack/virtual-core';
  import { Edit3, Trash2, Copy, X } from '$lib/icons';
  import type { Column } from '$lib/api/db';
  import JsonCellEditor from './JsonCellEditor.svelte';

  type PendingRowMap = Record<
    number,
    { changes: Record<string, unknown>; original: Record<string, unknown> }
  >;

  interface PendingInsert {
    tempId: string;
    values: Record<string, unknown>;
    errors?: Record<string, string>;
  }

  interface Props {
    result: QueryResult | null;
    pkColumns?: string[];
    /** When true, double-click on a non-PK cell enters inline edit mode. */
    editable?: boolean;
    /** Pending edits for this grid's tab, keyed by row index. */
    pendingByRow?: PendingRowMap;
    /** Pending inline-insert rows, rendered as a band above the data. */
    pendingInserts?: PendingInsert[];
    /** Full Column[] metadata (nullability, default, pk) for insert validation. */
    columnsMeta?: Column[];
    /** Caller-supplied auto-increment heuristic (read-only insert cell). */
    isAutoIncrement?: (col: Column) => boolean;
    onEdit?: (row: unknown[], idx: number) => void;
    onDuplicate?: (row: unknown[], idx: number) => void;
    onDelete?: (row: unknown[], idx: number) => void;
    /** Commit a cell edit into the parent's pending buffer. */
    onCellCommit?: (rowIndex: number, columnName: string, newValue: unknown) => void;
    /** Commit an inline-insert cell edit (per tempId, per column). */
    onInsertCellCommit?: (tempId: string, columnName: string, newValue: unknown) => void;
    /** Drop an inline-insert row entirely. */
    onInsertDiscardRow?: (tempId: string) => void;
  }

  let {
    result,
    pkColumns = [],
    editable = false,
    pendingByRow = {},
    pendingInserts = [],
    columnsMeta,
    isAutoIncrement,
    onEdit,
    onDuplicate,
    onDelete,
    onCellCommit,
    onInsertCellCommit,
    onInsertDiscardRow
  }: Props = $props();

  const ROW_HEIGHT = 36;
  const VIRT_THRESHOLD = 200;

  let scrollEl = $state<HTMLDivElement | null>(null);
  /** The inline-insert band tbody (above the virtualized rows). We measure its
   *  height so the virtualizer can offset its visible-window math by it —
   *  otherwise virtualizer.scrollOffset (relative to scrollEl) and the rows'
   *  absolute-positioned coordinate space (relative to the data tbody, which
   *  sits BELOW the band) get out of sync and the wrong rows are shown. */
  let insertBandEl = $state<HTMLTableSectionElement | null>(null);
  let scrollMargin = $state(0);
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let virtualizer: Virtualizer<HTMLDivElement, any> | null = null;
  let virtCleanup: (() => void) | null = null;
  type VItem = { index: number; start: number; size: number; key: number | string | bigint };
  let virtItems = $state<VItem[]>([]);
  let totalSize = $state(0);

  const rows = $derived(result?.rows ?? []);
  const cols = $derived(result?.columns ?? []);
  const useVirtual = $derived(rows.length > VIRT_THRESHOLD);

  // ── inline-edit state ──────────────────────────────────────────────────
  /**
   * Active edit target. `kind: 'row'` edits a real result row; `kind: 'insert'`
   * edits a pending insert row keyed by tempId. `kind: 'none'` means idle.
   */
  type EditTarget =
    | { kind: 'none' }
    | { kind: 'row'; rowIndex: number; colIndex: number }
    | { kind: 'insert'; tempId: string; colIndex: number };
  let editing = $state<EditTarget>({ kind: 'none' });
  let editingValue = $state('');
  let editingInputEl = $state<HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement | null>(
    null
  );
  let editingError = $state<string | null>(null);

  // Kept as $derived bridges so existing helpers below need only minimal change.
  const editingRow = $derived(editing.kind === 'row' ? editing.rowIndex : -1);
  const editingCol = $derived(
    editing.kind === 'row' || editing.kind === 'insert' ? editing.colIndex : -1
  );

  function isPkCell(colIndex: number): boolean {
    const col = cols[colIndex];
    return !!col && pkColumns.includes(col.name);
  }

  function isCellEditable(colIndex: number): boolean {
    return editable && pkColumns.length > 0 && !isPkCell(colIndex);
  }

  function cellDisplay(rowIndex: number, colIndex: number, fallback: unknown): unknown {
    const colName = cols[colIndex]?.name;
    if (!colName) return fallback;
    const pend = pendingByRow[rowIndex];
    if (pend && colName in pend.changes) return pend.changes[colName];
    return fallback;
  }

  function isCellPending(rowIndex: number, colIndex: number): boolean {
    const colName = cols[colIndex]?.name;
    if (!colName) return false;
    const pend = pendingByRow[rowIndex];
    return !!pend && colName in pend.changes;
  }

  function originalFor(rowIndex: number, colIndex: number): unknown {
    const colName = cols[colIndex]?.name;
    if (!colName) return undefined;
    return pendingByRow[rowIndex]?.original?.[colName];
  }

  async function startEdit(rowIndex: number, colIndex: number) {
    if (!isCellEditable(colIndex)) return;
    const current = cellDisplay(rowIndex, colIndex, rows[rowIndex]?.[colIndex]);
    editingValue =
      current === null || current === undefined
        ? ''
        : typeof current === 'object'
          ? JSON.stringify(current, null, 2)
          : String(current);
    editingError = null;
    editing = { kind: 'row', rowIndex, colIndex };
    await tick();
    editingInputEl?.focus();
    if (editingInputEl && 'select' in editingInputEl) editingInputEl.select();
  }

  function cancelEdit() {
    editing = { kind: 'none' };
    editingValue = '';
    editingInputEl = null;
    editingError = null;
  }

  /** Heuristic — auto-increment columns are read-only on inserts. */
  function isInsertCellEditable(col: Column): boolean {
    if (isAutoIncrement?.(col)) return false;
    return true;
  }

  async function startInsertEdit(tempId: string, colIndex: number) {
    const col = columnsMeta?.[colIndex];
    if (!col || !isInsertCellEditable(col)) return;
    const ins = pendingInserts.find((p) => p.tempId === tempId);
    const current = ins?.values?.[col.name];
    editingValue =
      current === null || current === undefined
        ? ''
        : typeof current === 'object'
          ? JSON.stringify(current, null, 2)
          : String(current);
    editingError = null;
    editing = { kind: 'insert', tempId, colIndex };
    await tick();
    editingInputEl?.focus();
    if (editingInputEl && 'select' in editingInputEl) editingInputEl.select();
  }

  /** Best-effort coercion using the column's declared type. */
  function coerceForColumn(raw: string, col: Column): unknown {
    if (raw === '') return null;
    const t = col.data_type.toLowerCase();
    if (t.includes('bool')) {
      if (raw === 'true' || raw === '1') return true;
      if (raw === 'false' || raw === '0') return false;
      return raw;
    }
    if (
      t.includes('int') ||
      t.includes('float') ||
      t.includes('numeric') ||
      t.includes('decimal') ||
      t.includes('double') ||
      t.includes('real') ||
      t.includes('serial')
    ) {
      const n = Number(raw);
      return Number.isFinite(n) ? n : raw;
    }
    return raw;
  }

  /**
   * Coerce a string from the input back to a sensible JS value:
   *   • empty string when the original was null → null
   *   • numeric original → Number(...) if parseable
   *   • boolean original → "true"/"false" → boolean
   *   • everything else → string
   */
  function coerce(raw: string, rowIndex: number, colIndex: number): unknown {
    const colName = cols[colIndex]?.name;
    if (!colName) return raw;
    const original = pendingByRow[rowIndex]?.original?.[colName] ?? rows[rowIndex]?.[colIndex];
    if (original === null || original === undefined) {
      return raw === '' ? null : raw;
    }
    if (typeof original === 'number') {
      if (raw === '') return null;
      const n = Number(raw);
      return Number.isFinite(n) ? n : raw;
    }
    if (typeof original === 'boolean') {
      if (raw === 'true') return true;
      if (raw === 'false') return false;
      return raw;
    }
    return raw;
  }

  function columnMetaFor(colIndex: number): Column | null {
    const name = cols[colIndex]?.name;
    if (!name) return null;
    return columnsMeta?.find((c) => c.name === name) ?? null;
  }

  function isJsonColumn(colIndex: number, meta?: Column | null): boolean {
    const dt = meta?.data_type ?? cols[colIndex]?.data_type ?? '';
    return dt.toLowerCase().includes('json');
  }

  function isEnumColumn(
    meta?: Column | null
  ): meta is Column & { kind: { kind: 'Enum'; variants: string[] } } {
    return meta?.kind?.kind === 'Enum';
  }

  function commitEnumValue(raw: string) {
    if (editing.kind === 'none') return;
    const colIndex = editing.colIndex;
    const meta = columnMetaFor(colIndex) ?? columnsMeta?.[colIndex];
    if (!meta) return;
    const value = coerceForColumn(raw, meta);
    if (editing.kind === 'row') {
      const colName = cols[colIndex]?.name;
      if (colName) onCellCommit?.(editing.rowIndex, colName, value);
    } else {
      onInsertCellCommit?.(editing.tempId, meta.name, value);
    }
    cancelEdit();
  }

  function commitJsonValue(parsed: unknown, raw: string) {
    if (editing.kind === 'none') return;
    const value = parsed === null && raw.trim() === '' ? null : raw;
    const colIndex = editing.colIndex;
    if (editing.kind === 'row') {
      const colName = cols[colIndex]?.name;
      if (colName) onCellCommit?.(editing.rowIndex, colName, value);
    } else {
      const col = columnsMeta?.[colIndex];
      if (col) onInsertCellCommit?.(editing.tempId, col.name, value);
    }
    cancelEdit();
  }

  function commitEdit(advance = false) {
    if (editing.kind === 'none') return;
    const colIndex = editing.colIndex;
    if (editing.kind === 'row') {
      const colName = cols[colIndex]?.name;
      if (colName && onCellCommit) {
        const value = coerce(editingValue, editing.rowIndex, colIndex);
        onCellCommit(editing.rowIndex, colName, value);
      }
      const r = editing.rowIndex;
      cancelEdit();
      if (advance) {
        for (let next = colIndex + 1; next < cols.length; next++) {
          if (isCellEditable(next)) {
            void startEdit(r, next);
            return;
          }
        }
      }
    } else {
      // insert
      const col = columnsMeta?.[colIndex];
      if (col && onInsertCellCommit) {
        const value = coerceForColumn(editingValue, col);
        onInsertCellCommit(editing.tempId, col.name, value);
      }
      const tempId = editing.tempId;
      cancelEdit();
      if (advance && columnsMeta) {
        for (let next = colIndex + 1; next < columnsMeta.length; next++) {
          const ncol = columnsMeta[next];
          if (ncol && isInsertCellEditable(ncol)) {
            void startInsertEdit(tempId, next);
            return;
          }
        }
      }
    }
  }

  function onCellKey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      cancelEdit();
    } else if (e.key === 'Enter') {
      e.preventDefault();
      commitEdit(false);
    } else if (e.key === 'Tab') {
      e.preventDefault();
      commitEdit(true);
    }
  }

  function teardownVirtual() {
    try {
      virtCleanup?.();
    } catch {
      /* no-op */
    }
    virtCleanup = null;
    virtualizer = null;
    virtItems = [];
    totalSize = 0;
  }

  function setupVirtual() {
    teardownVirtual();
    if (!scrollEl || !useVirtual) return;
    virtualizer = new Virtualizer({
      count: rows.length,
      getScrollElement: () => scrollEl,
      estimateSize: () => ROW_HEIGHT,
      overscan: 8,
      scrollMargin,
      observeElementRect,
      observeElementOffset,
      scrollToFn: elementScroll,
      onChange: (instance) => {
        virtItems = instance.getVirtualItems();
        totalSize = instance.getTotalSize();
      }
    });
    virtCleanup = virtualizer._didMount() ?? null;
    virtualizer._willUpdate();
    virtItems = virtualizer.getVirtualItems();
    totalSize = virtualizer.getTotalSize();
  }

  /** Re-init only when the underlying data shape changes (NEW result, NEW
   *  virtualization mode, NEW scrollEl). Critically does NOT depend on
   *  `pendingByRow` / `pendingInserts` — those only retint cells, they
   *  must not blow away the virtualizer (which would reset scrollTop). */
  $effect(() => {
    void result; // identity change → new query result
    void useVirtual;
    void scrollEl;
    untrack(() => {
      // Editing state is bound to a row index from the previous result; drop
      // it whenever the data changes underneath us.
      if (editingRow >= 0) cancelEdit();
      setupVirtual();
    });
    return () => teardownVirtual();
  });

  /** Keep the virtualizer's scrollMargin in sync with the insert-band height
   *  WITHOUT recreating it (preserves scrollOffset). */
  $effect(() => {
    void scrollMargin;
    untrack(() => {
      if (!virtualizer) return;
      virtualizer.setOptions({
        ...virtualizer.options,
        scrollMargin
      });
      virtualizer._willUpdate();
      virtItems = virtualizer.getVirtualItems();
      totalSize = virtualizer.getTotalSize();
    });
  });

  /** Observe the insert-band's actual rendered height. Falls back to 0 when
   *  there's no band. */
  $effect(() => {
    void pendingInserts.length;
    void columnsMeta;
    const el = insertBandEl;
    if (!el) {
      scrollMargin = 0;
      return;
    }
    const update = () => {
      scrollMargin = el.getBoundingClientRect().height;
    };
    update();
    const ro = new ResizeObserver(update);
    ro.observe(el);
    return () => ro.disconnect();
  });

  onDestroy(() => {
    teardownVirtual();
  });

  function fmt(v: unknown): string {
    if (v === null || v === undefined) return 'NULL';
    if (typeof v === 'object') return JSON.stringify(v);
    return String(v);
  }

  function isNull(v: unknown): boolean {
    return v === null || v === undefined;
  }

  const TYPE_COLOR = 'var(--fg-muted)';

  const hasPk = $derived(pkColumns.length > 0);
  const PENDING_BG = 'color-mix(in srgb, var(--dot-warn) 18%, transparent)';
  const PENDING_BORDER = 'var(--dot-warn)';
  const INSERT_BG = 'color-mix(in srgb, var(--dot-success) 10%, transparent)';
  const INSERT_BORDER = 'var(--dot-success)';
  const ERROR_BORDER = 'var(--dot-danger)';
  const ERROR_BG = 'color-mix(in srgb, var(--dot-danger) 10%, transparent)';

  /** True when there's a table to render (rows OR pending inserts). */
  const hasAnything = $derived(rows.length > 0 || pendingInserts.length > 0);
</script>

<div class="flex h-full min-h-0 flex-col" style="background: var(--surface-canvas);">
  {#if !result}
    <div class="flex flex-1 items-center justify-center text-sm" style="color: var(--fg-muted);">
      Run a query to see results.
    </div>
  {:else if !hasAnything}
    <div class="flex flex-1 items-center justify-center text-sm" style="color: var(--fg-muted);">
      Query returned no rows ({result.elapsed_ms}ms).
    </div>
  {:else}
    <div bind:this={scrollEl} class="flex-1 overflow-auto">
      <table
        class="w-full border-separate text-[13px]"
        style="border-spacing: 0; font-family: var(--font-mono);"
      >
        <thead>
          <tr>
            <th
              class="sticky top-0 z-10 px-3 py-2.5 text-right text-[11px] font-semibold"
              style="background: var(--surface-titlebar); color: var(--fg-label); border-bottom: 1px solid var(--border-subtle); width: 48px;"
            >
              #
            </th>
            {#each cols as col (col.name)}
              <th
                class="sticky top-0 z-10 px-3 py-2.5 text-left text-[11px] font-semibold whitespace-nowrap"
                style="background: var(--surface-titlebar); color: var(--fg-default); border-bottom: 1px solid var(--border-subtle);"
              >
                <div class="flex items-center gap-1.5">
                  {#if pkColumns.includes(col.name)}
                    <span title="primary key" style="color: var(--accent);">🔑</span>
                  {/if}
                  <span>{col.name}</span>
                  <span
                    class="rounded px-1.5 text-[9px] font-semibold tracking-wide"
                    style="background: var(--surface-panel); color: {TYPE_COLOR};"
                  >
                    {col.data_type}
                  </span>
                </div>
              </th>
            {/each}
            <th
              class="sticky top-0 z-10"
              style="background: var(--surface-titlebar); border-bottom: 1px solid var(--border-subtle); width: 100px;"
            ></th>
          </tr>
        </thead>
        {#snippet rowCells(row: unknown[], ri: number)}
          {#each row as cell, ci (ci)}
            {@const pending = isCellPending(ri, ci)}
            {@const displayed = cellDisplay(ri, ci, cell)}
            {@const editingThis = editingRow === ri && editingCol === ci}
            {@const cellEditable = isCellEditable(ci)}
            {@const meta = columnMetaFor(ci)}
            {@const jsonEditing = editingThis && isJsonColumn(ci, meta)}
            <td
              class="px-3 py-2 whitespace-nowrap {cellEditable && !editingThis
                ? 'cell-editable'
                : ''}"
              style="border-bottom: 1px solid var(--row-divider); {pending
                ? `background: ${PENDING_BG}; box-shadow: inset 2px 0 0 0 ${PENDING_BORDER};`
                : jsonEditing && editingError
                  ? `background: ${ERROR_BG}; box-shadow: inset 2px 0 0 0 ${ERROR_BORDER};`
                  : ''} {cellEditable && !editingThis ? 'cursor: text;' : ''}"
              title={pending
                ? `was: ${fmt(originalFor(ri, ci))}`
                : jsonEditing && editingError
                  ? editingError
                  : cellEditable && !editingThis
                    ? 'Click to edit'
                    : ''}
              onclick={() => cellEditable && !editingThis && startEdit(ri, ci)}
            >
              {#if editingThis}
                {#if isEnumColumn(meta)}
                  <select
                    bind:this={editingInputEl}
                    bind:value={editingValue}
                    onkeydown={onCellKey}
                    onchange={(e) => commitEnumValue((e.currentTarget as HTMLSelectElement).value)}
                    class="w-full rounded-sm border px-1 py-0.5 text-[13px] outline-none"
                    style="background: var(--surface-titlebar); border-color: var(--accent); color: var(--fg-default); font-family: var(--font-mono);"
                  >
                    {#if meta.nullable}<option value=""></option>{/if}
                    {#each meta.kind.variants as variant (variant)}
                      <option value={variant}>{variant}</option>
                    {/each}
                  </select>
                {:else if isJsonColumn(ci, meta)}
                  <JsonCellEditor
                    value={editingValue}
                    nullable={meta?.nullable ?? true}
                    onCommit={commitJsonValue}
                    onCancel={cancelEdit}
                    onParseError={(err) => (editingError = err)}
                  />
                {:else}
                  <input
                    bind:this={editingInputEl}
                    bind:value={editingValue}
                    onkeydown={onCellKey}
                    onblur={() => commitEdit(false)}
                    class="w-full rounded-sm border px-1 py-0.5 text-[13px] outline-none"
                    style="background: var(--surface-titlebar); border-color: var(--accent); color: var(--fg-default); font-family: var(--font-mono);"
                  />
                {/if}
              {:else if isNull(displayed)}
                <span style="color: var(--fg-muted); font-style: italic; opacity: 0.6;">NULL</span>
              {:else}
                <span style="color: var(--fg-default);">{fmt(displayed)}</span>
              {/if}
            </td>
          {/each}
        {/snippet}
        {#snippet insertCells(ins: PendingInsert)}
          {#each columnsMeta ?? [] as col, ci (col.name)}
            {@const editingThis =
              editing.kind === 'insert' && editing.tempId === ins.tempId && editing.colIndex === ci}
            {@const auto = isAutoIncrement?.(col) ?? false}
            {@const val = ins.values?.[col.name]}
            {@const hasError = !!ins.errors?.[col.name]}
            {@const cellEditable = !auto}
            {@const jsonEditing = editingThis && isJsonColumn(ci, col)}
            <td
              class="px-3 py-2 whitespace-nowrap {cellEditable && !editingThis
                ? 'cell-editable'
                : ''}"
              style="border-bottom: 1px solid var(--row-divider); {hasError
                ? `background: ${ERROR_BG}; box-shadow: inset 2px 0 0 0 ${ERROR_BORDER};`
                : jsonEditing && editingError
                  ? `background: ${ERROR_BG}; box-shadow: inset 2px 0 0 0 ${ERROR_BORDER};`
                  : ''} {cellEditable && !editingThis ? 'cursor: text;' : 'cursor: default;'}"
              title={hasError
                ? ins.errors?.[col.name]
                : jsonEditing && editingError
                  ? editingError
                  : auto
                    ? 'auto-generated by the database'
                    : 'Click to edit'}
              onclick={() => cellEditable && !editingThis && startInsertEdit(ins.tempId, ci)}
            >
              {#if editingThis}
                {#if isEnumColumn(col)}
                  <select
                    bind:this={editingInputEl}
                    bind:value={editingValue}
                    onkeydown={onCellKey}
                    onchange={(e) => commitEnumValue((e.currentTarget as HTMLSelectElement).value)}
                    class="w-full rounded-sm border px-1 py-0.5 text-[13px] outline-none"
                    style="background: var(--surface-titlebar); border-color: var(--accent); color: var(--fg-default); font-family: var(--font-mono);"
                  >
                    {#if col.nullable}<option value=""></option>{/if}
                    {#each col.kind.variants as variant (variant)}
                      <option value={variant}>{variant}</option>
                    {/each}
                  </select>
                {:else if isJsonColumn(ci, col)}
                  <JsonCellEditor
                    value={editingValue}
                    nullable={col.nullable}
                    onCommit={commitJsonValue}
                    onCancel={cancelEdit}
                    onParseError={(err) => (editingError = err)}
                  />
                {:else}
                  <input
                    bind:this={editingInputEl}
                    bind:value={editingValue}
                    onkeydown={onCellKey}
                    onblur={() => commitEdit(false)}
                    class="w-full rounded-sm border px-1 py-0.5 text-[13px] outline-none"
                    style="background: var(--surface-titlebar); border-color: var(--accent); color: var(--fg-default); font-family: var(--font-mono);"
                  />
                {/if}
              {:else if auto}
                <span style="color: var(--fg-muted); font-style: italic; opacity: 0.55;">auto</span>
              {:else if val === undefined || val === null || val === ''}
                <span style="color: var(--fg-muted); font-style: italic; opacity: 0.5;">
                  {col.nullable || (col.default != null && col.default !== '')
                    ? '(empty)'
                    : '(required)'}
                </span>
              {:else}
                <span style="color: var(--fg-default);">{fmt(val)}</span>
              {/if}
            </td>
          {/each}
        {/snippet}
        {#snippet rowActions(row: unknown[], ri: number)}
          {#if hasPk}
            <div class="inline-flex gap-0.5 opacity-0 transition-opacity group-hover:opacity-100">
              <button
                title="Edit"
                class="rounded p-1 hover:bg-[var(--accent-soft)]"
                onclick={() => onEdit?.(row, ri)}
                style="color: var(--fg-muted);"
              >
                <Edit3 class="h-3 w-3" />
              </button>
              <button
                title="Duplicate"
                class="rounded p-1 hover:bg-[var(--accent-soft)]"
                onclick={() => onDuplicate?.(row, ri)}
                style="color: var(--fg-muted);"
              >
                <Copy class="h-3 w-3" />
              </button>
              <button
                title="Delete"
                class="rounded p-1 hover:bg-[color-mix(in_srgb,var(--dot-danger)_10%,transparent)]"
                onclick={() => onDelete?.(row, ri)}
                style="color: var(--dot-danger);"
              >
                <Trash2 class="h-3 w-3" />
              </button>
            </div>
          {/if}
        {/snippet}

        {#if pendingInserts.length > 0 && columnsMeta && columnsMeta.length > 0}
          <tbody bind:this={insertBandEl}>
            {#each pendingInserts as ins (ins.tempId)}
              <tr
                class="group"
                style="background: {INSERT_BG}; box-shadow: inset 3px 0 0 0 {INSERT_BORDER};"
              >
                <td
                  class="px-3 py-2 text-right font-mono text-[10px]"
                  style="color: {INSERT_BORDER}; border-bottom: 1px solid var(--row-divider); width: 48px;"
                  title="new row (unsaved)"
                >
                  +
                </td>
                {@render insertCells(ins)}
                <td
                  class="px-2 py-2 text-right"
                  style="border-bottom: 1px solid var(--row-divider); width: 100px;"
                >
                  <button
                    type="button"
                    title="Discard this new row"
                    class="rounded p-1 hover:bg-[color-mix(in_srgb,var(--dot-danger)_10%,transparent)]"
                    onclick={() => onInsertDiscardRow?.(ins.tempId)}
                    style="color: var(--dot-danger);"
                  >
                    <X class="h-3 w-3" />
                  </button>
                </td>
              </tr>
            {/each}
          </tbody>
        {/if}
        {#if useVirtual}
          <tbody style="position: relative; height: {Math.max(0, totalSize - scrollMargin)}px;">
            {#each virtItems as v (v.key)}
              {@const row = rows[v.index]}
              <tr
                class="group"
                style="position: absolute; top: 0; left: 0; right: 0; transform: translateY({v.start -
                  scrollMargin}px); height: {v.size}px; background: {v.index % 2 === 1
                  ? 'var(--row-stripe)'
                  : 'transparent'};"
              >
                <td
                  class="px-3 py-2 text-right font-mono text-[11px]"
                  style="color: var(--fg-label); border-bottom: 1px solid var(--row-divider); width: 48px;"
                >
                  {v.index + 1}
                </td>
                {@render rowCells(row, v.index)}
                <td
                  class="px-2 py-2 text-right"
                  style="border-bottom: 1px solid var(--row-divider); width: 100px;"
                >
                  {@render rowActions(row, v.index)}
                </td>
              </tr>
            {/each}
          </tbody>
        {:else}
          <tbody>
            {#each rows as row, ri (ri)}
              <tr
                class="group"
                style="background: {ri % 2 === 1 ? 'var(--row-stripe)' : 'transparent'};"
              >
                <td
                  class="px-3 py-2 text-right font-mono text-[11px]"
                  style="color: var(--fg-label); border-bottom: 1px solid var(--row-divider); width: 48px;"
                >
                  {ri + 1}
                </td>
                {@render rowCells(row, ri)}
                <td
                  class="px-2 py-2 text-right"
                  style="border-bottom: 1px solid var(--row-divider); width: 100px;"
                >
                  {@render rowActions(row, ri)}
                </td>
              </tr>
            {/each}
          </tbody>
        {/if}
      </table>
    </div>
    <div
      class="flex h-7 shrink-0 items-center justify-between border-t px-4 font-mono text-[11px]"
      style="border-color: var(--border-subtle); background: var(--surface-statusbar); color: var(--fg-muted);"
    >
      <span>
        {rows.length} row{rows.length === 1 ? '' : 's'}{result.truncated ? ' (truncated)' : ''}
      </span>
      <span>queried in {result.elapsed_ms}ms</span>
    </div>
  {/if}
</div>

<style>
  /* Subtle hover hint on editable cells — paired with `cursor: text`. */
  .cell-editable:hover {
    background: color-mix(in srgb, var(--accent) 6%, transparent);
  }
</style>
