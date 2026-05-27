<!--
  Read-only schema view for a single table — columns, indexes, foreign keys.
  Used inside the table tab's "Schema" sub-tab.
-->
<script lang="ts">
  import type { TableMeta } from '$lib/api/db';

  interface Props {
    table: TableMeta;
  }

  let { table }: Props = $props();
</script>

<div class="flex h-full min-h-0 flex-col overflow-y-auto">
  <div class="mx-auto w-full max-w-4xl px-5 py-5">
    <!-- Summary -->
    <div class="mb-5 flex items-baseline gap-3">
      <h2 class="font-mono text-base font-semibold" style="color: var(--fg-default);">
        {table.name}
      </h2>
      <span
        class="rounded px-1.5 py-0.5 text-[10px] font-bold uppercase tracking-wider"
        style="background: var(--accent-soft); color: var(--accent);"
      >
        {table.kind === 'materialized_view' ? 'matview' : table.kind}
      </span>
      <span class="text-[11px]" style="color: var(--fg-muted);">
        {table.columns.length} column{table.columns.length === 1 ? '' : 's'}
        {#if table.row_estimate != null} · ~{table.row_estimate.toLocaleString()} rows{/if}
      </span>
    </div>

    <!-- Columns -->
    <section class="mb-6">
      <h3
        class="mb-2 text-[10px] font-semibold uppercase tracking-wider"
        style="color: var(--fg-muted);"
      >
        Columns
      </h3>
      <div
        class="overflow-hidden rounded-md border"
        style="border-color: var(--border-subtle); background: var(--surface-panel);"
      >
        <table class="w-full text-xs">
          <thead>
            <tr
              class="text-left"
              style="background: var(--surface-titlebar); color: var(--fg-muted);"
            >
              <th class="px-3 py-2 font-medium">Name</th>
              <th class="px-3 py-2 font-medium">Type</th>
              <th class="px-3 py-2 font-medium">Null</th>
              <th class="px-3 py-2 font-medium">Default</th>
              <th class="px-3 py-2 font-medium">Notes</th>
            </tr>
          </thead>
          <tbody>
            {#each table.columns as col (col.name)}
              <tr style="border-top: 1px solid var(--border-subtle);">
                <td class="px-3 py-1.5">
                  <span class="inline-flex items-center gap-1.5">
                    {#if col.pk}
                      <span style="color: var(--accent);" title="primary key">🔑</span>
                    {/if}
                    <span class="font-mono" style="color: var(--fg-default);">{col.name}</span>
                  </span>
                </td>
                <td class="px-3 py-1.5 font-mono" style="color: var(--fg-muted);">
                  {col.data_type}
                </td>
                <td class="px-3 py-1.5" style="color: var(--fg-muted);">
                  {col.nullable ? 'YES' : 'NO'}
                </td>
                <td class="px-3 py-1.5 font-mono" style="color: var(--fg-muted);">
                  {col.default ?? '—'}
                </td>
                <td class="px-3 py-1.5" style="color: var(--fg-muted);">
                  {col.comment ?? ''}
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    </section>

    <!-- Indexes -->
    <section class="mb-6">
      <h3
        class="mb-2 text-[10px] font-semibold uppercase tracking-wider"
        style="color: var(--fg-muted);"
      >
        Indexes ({table.indexes.length})
      </h3>
      {#if table.indexes.length === 0}
        <p class="text-xs" style="color: var(--fg-muted);">No indexes.</p>
      {:else}
        <div
          class="overflow-hidden rounded-md border"
          style="border-color: var(--border-subtle); background: var(--surface-panel);"
        >
          <table class="w-full text-xs">
            <thead>
              <tr
                class="text-left"
                style="background: var(--surface-titlebar); color: var(--fg-muted);"
              >
                <th class="px-3 py-2 font-medium">Name</th>
                <th class="px-3 py-2 font-medium">Columns</th>
                <th class="px-3 py-2 font-medium">Unique</th>
              </tr>
            </thead>
            <tbody>
              {#each table.indexes as ix (ix.name)}
                <tr style="border-top: 1px solid var(--border-subtle);">
                  <td class="px-3 py-1.5 font-mono" style="color: var(--fg-default);">
                    {ix.name}
                  </td>
                  <td class="px-3 py-1.5 font-mono" style="color: var(--fg-muted);">
                    {ix.columns.join(', ')}
                  </td>
                  <td class="px-3 py-1.5" style="color: var(--fg-muted);">
                    {ix.unique ? 'YES' : 'no'}
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </section>

    <!-- Foreign keys / relations -->
    <section class="mb-6">
      <h3
        class="mb-2 text-[10px] font-semibold uppercase tracking-wider"
        style="color: var(--fg-muted);"
      >
        Relations ({table.foreign_keys.length})
      </h3>
      {#if table.foreign_keys.length === 0}
        <p class="text-xs" style="color: var(--fg-muted);">No foreign keys declared.</p>
      {:else}
        <div class="flex flex-col gap-2">
          {#each table.foreign_keys as fk (fk.name)}
            <div
              class="rounded-md border px-3 py-2"
              style="border-color: var(--border-subtle); background: var(--surface-panel);"
            >
              <div class="mb-1 font-mono text-[11px]" style="color: var(--fg-muted);">
                {fk.name}
              </div>
              <div class="flex flex-wrap items-center gap-2 font-mono text-xs">
                <span
                  class="rounded px-1.5 py-0.5"
                  style="background: var(--surface-titlebar); color: var(--fg-default);"
                >
                  {table.name}({fk.columns.join(', ')})
                </span>
                <span style="color: var(--fg-muted);">→</span>
                <span
                  class="rounded px-1.5 py-0.5"
                  style="background: var(--accent-soft); color: var(--accent);"
                >
                  {fk.ref_table}({fk.ref_columns.join(', ')})
                </span>
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </section>
  </div>
</div>
