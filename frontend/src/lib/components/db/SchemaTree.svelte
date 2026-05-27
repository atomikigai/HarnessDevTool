<!--
  Collapsible schema → table tree.
  Click a table to open it as a tab via the supplied callback.
  Style ported from `harness-table-v2.jsx` TreeItem.
-->
<script lang="ts" module>
  import type { TableMeta as _TableMeta } from '$lib/api/db';
  /**
   * Public target shape for the export menu — keeps the consumer free
   * to mount whatever dialog it wants without depending on this file.
   */
  export type SchemaTreeExportTarget =
    | { kind: 'table'; schema: string; table: _TableMeta }
    | { kind: 'schema'; name: string; tables: _TableMeta[] };
</script>

<script lang="ts">
  import { ChevronRight, ChevronLeft, Download } from '$lib/icons';
  import type { SchemaTree, TableMeta } from '$lib/api/db';
  import { ContextMenu, type ContextMenuItem } from '$lib/components/ui/context-menu';

  interface Props {
    tree: SchemaTree | null;
    loading?: boolean;
    error?: string | null;
    onOpenTable?: (schema: string, table: TableMeta) => void;
    activeTable?: { schema: string; name: string } | null;
    /** Called when the user picks Export… on a table or schema row. */
    onExport?: (target: SchemaTreeExportTarget) => void;
  }

  let {
    tree,
    loading = false,
    error = null,
    onOpenTable,
    activeTable,
    onExport
  }: Props = $props();

  // ── Context menu state ────────────────────────────────────────────────────
  let menuOpen = $state(false);
  let menuX = $state(0);
  let menuY = $state(0);
  let menuItems = $state<ContextMenuItem[]>([]);

  function openSchemaMenu(e: MouseEvent, schemaName: string, tables: TableMeta[]) {
    e.preventDefault();
    menuX = e.clientX;
    menuY = e.clientY;
    menuItems = [
      {
        label: 'Export schema…',
        icon: Download,
        onSelect: () => onExport?.({ kind: 'schema', name: schemaName, tables })
      }
    ];
    menuOpen = true;
  }

  function openTableMenu(e: MouseEvent, schemaName: string, table: TableMeta) {
    e.preventDefault();
    menuX = e.clientX;
    menuY = e.clientY;
    menuItems = [
      {
        label: 'Export table…',
        icon: Download,
        onSelect: () => onExport?.({ kind: 'table', schema: schemaName, table })
      }
    ];
    menuOpen = true;
  }

  let expanded = $state<Record<string, boolean>>({});
  let filter = $state('');
  let lastSchemaKey = $state<string>('');

  $effect(() => {
    if (!tree) return;
    // Re-sync expansion state when the tree's schema set changes
    // (e.g., after switching database). Auto-expand all when there
    // are few schemas — typical case for sqlite/mysql (1) and most
    // postgres setups (public + a handful). Above 8, only expand the
    // first to avoid an avalanche.
    const key = tree.schemas.map((s) => s.name).join('|');
    if (key === lastSchemaKey) return;
    lastSchemaKey = key;
    const next: Record<string, boolean> = {};
    if (tree.schemas.length === 0) {
      expanded = next;
      return;
    }
    if (tree.schemas.length <= 8) {
      for (const s of tree.schemas) next[s.name] = true;
    } else {
      next[tree.schemas[0].name] = true;
    }
    expanded = next;
  });

  function toggle(name: string) {
    expanded = { ...expanded, [name]: !expanded[name] };
  }

  function visibleTables(tables: TableMeta[]): TableMeta[] {
    const q = filter.trim().toLowerCase();
    if (!q) return tables;
    return tables.filter((t) => t.name.toLowerCase().includes(q));
  }
</script>

<div class="flex h-full min-h-0 flex-col">
  <!-- Search filter -->
  <div class="px-3 pt-3 pb-2">
    <div
      class="flex items-center gap-2 rounded-md border px-2.5 py-1.5 text-xs"
      style="border-color: var(--border-input); background: var(--surface-titlebar);"
    >
      <input
        bind:value={filter}
        placeholder="Filter tables…"
        class="flex-1 bg-transparent outline-none"
        style="color: var(--fg-default);"
      />
    </div>
  </div>

  <div class="px-4 pt-1 pb-1">
    <span class="h-eyebrow">Schemas</span>
  </div>

  <div class="flex-1 overflow-y-auto pb-3">
    {#if loading}
      <p class="px-4 py-3 text-xs" style="color: var(--fg-muted);">Loading schema…</p>
    {:else if error}
      <p class="px-4 py-3 text-xs" style="color: var(--dot-danger);">{error}</p>
    {:else if !tree || tree.schemas.length === 0}
      <p class="px-4 py-3 text-xs" style="color: var(--fg-muted);">No schemas found.</p>
    {:else}
      {#each tree.schemas as schema (schema.name)}
        {@const matches = visibleTables(schema.tables)}
        {@const open = expanded[schema.name] || (filter.trim() !== '' && matches.length > 0)}
        <button
          type="button"
          class="flex w-full items-center gap-2 px-3 py-1.5 text-left text-[13px]"
          style="color: var(--fg-default);"
          onclick={() => toggle(schema.name)}
          oncontextmenu={(e) => openSchemaMenu(e, schema.name, schema.tables)}
        >
          <span class="inline-flex w-3" style="color: var(--fg-muted);">
            {#if open}
              <ChevronLeft class="h-3 w-3 -rotate-90" />
            {:else}
              <ChevronRight class="h-3 w-3" />
            {/if}
          </span>
          <span class="font-medium">{schema.name}</span>
          <span class="ml-auto font-mono text-[10px]" style="color: var(--fg-muted);">
            {schema.tables.length}
          </span>
        </button>
        {#if open}
          <div class="pl-2">
            <div
              class="px-4 pt-2 pb-1 text-[9px] font-bold uppercase tracking-wider"
              style="color: var(--fg-label);"
            >
              Tables · {matches.length}
            </div>
            {#if matches.length === 0}
              <p class="px-4 py-1 text-[11px]" style="color: var(--fg-muted);">
                {filter.trim() ? 'No tables match the filter.' : 'No tables in this schema.'}
              </p>
            {/if}
            {#each matches as t (t.name)}
              {@const active =
                activeTable && activeTable.schema === schema.name && activeTable.name === t.name}
              <button
                type="button"
                class="flex w-full items-center gap-2 py-1 pr-3 pl-7 text-left text-[12.5px] transition-colors"
                style={active
                  ? 'background: var(--accent-soft); color: var(--accent); font-weight: 600; border-left: 2px solid var(--accent); padding-left: calc(1.75rem - 2px);'
                  : 'color: var(--fg-default);'}
                onclick={() => onOpenTable?.(schema.name, t)}
                oncontextmenu={(e) => openTableMenu(e, schema.name, t)}
              >
                <span style="color: {active ? 'var(--accent)' : 'var(--fg-muted)'};">
                  {t.kind === 'view' ? 'ʌ' : '⊞'}
                </span>
                <span class="truncate">{t.name}</span>
                {#if t.row_estimate != null}
                  <span class="ml-auto font-mono text-[10px]" style="color: var(--fg-muted);">
                    {t.row_estimate}
                  </span>
                {/if}
              </button>
            {/each}
          </div>
        {/if}
      {/each}
    {/if}
  </div>
</div>

<ContextMenu
  x={menuX}
  y={menuY}
  open={menuOpen}
  items={menuItems}
  onClose={() => (menuOpen = false)}
/>
