<!--
  ResultGrid — paginated, virtualized result table.

  Styling cribbed from `harness-table-v2.jsx`: sticky monospace header
  with type chips, alternating row stripes, hover-revealed row actions
  on the right (kebab → Edit / Duplicate / Delete). Uses @tanstack/
  virtual-core for row virtualization whenever row count exceeds a
  threshold; otherwise renders directly for simplicity.
-->
<script lang="ts">
  import { onDestroy, untrack } from 'svelte';
  import type { QueryResult } from '$lib/api/db';
  import {
    Virtualizer,
    observeElementOffset,
    observeElementRect,
    elementScroll
  } from '@tanstack/virtual-core';
  import { Edit3, Trash2, Copy } from '$lib/icons';

  interface Props {
    result: QueryResult | null;
    pkColumns?: string[];
    onEdit?: (row: unknown[], idx: number) => void;
    onDuplicate?: (row: unknown[], idx: number) => void;
    onDelete?: (row: unknown[], idx: number) => void;
  }

  let { result, pkColumns = [], onEdit, onDuplicate, onDelete }: Props = $props();

  const ROW_HEIGHT = 36;
  const VIRT_THRESHOLD = 200;

  let scrollEl = $state<HTMLDivElement | null>(null);
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let virtualizer: Virtualizer<HTMLDivElement, any> | null = null;
  type VItem = { index: number; start: number; size: number; key: number | string | bigint };
  let virtItems = $state<VItem[]>([]);
  let totalSize = $state(0);

  const rows = $derived(result?.rows ?? []);
  const cols = $derived(result?.columns ?? []);
  const useVirtual = $derived(rows.length > VIRT_THRESHOLD);

  function setupVirtual() {
    virtualizer?._didMount?.()?.();
    if (!scrollEl || !useVirtual) {
      virtualizer = null;
      virtItems = [];
      return;
    }
    virtualizer = new Virtualizer({
      count: rows.length,
      getScrollElement: () => scrollEl,
      estimateSize: () => ROW_HEIGHT,
      overscan: 8,
      observeElementRect,
      observeElementOffset,
      scrollToFn: elementScroll,
      onChange: (instance) => {
        virtItems = instance.getVirtualItems();
        totalSize = instance.getTotalSize();
      }
    });
    const cleanup = virtualizer._didMount();
    virtualizer._willUpdate();
    virtItems = virtualizer.getVirtualItems();
    totalSize = virtualizer.getTotalSize();
    return cleanup;
  }

  $effect(() => {
    // Re-init when underlying data shape changes.
    void rows.length;
    void useVirtual;
    untrack(() => {
      setupVirtual();
    });
  });

  onDestroy(() => {
    virtualizer = null;
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
</script>

<div class="flex h-full min-h-0 flex-col" style="background: var(--surface-canvas);">
  {#if !result}
    <div class="flex flex-1 items-center justify-center text-sm" style="color: var(--fg-muted);">
      Run a query to see results.
    </div>
  {:else if rows.length === 0}
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
        {#if useVirtual}
          <tbody style="position: relative; height: {totalSize}px;">
            {#each virtItems as v (v.key)}
              {@const row = rows[v.index]}
              <tr
                class="group"
                style="position: absolute; top: 0; left: 0; right: 0; transform: translateY({v.start}px); height: {v.size}px; background: {v.index %
                  2 ===
                1
                  ? 'var(--row-stripe)'
                  : 'transparent'};"
              >
                <td
                  class="px-3 py-2 text-right font-mono text-[11px]"
                  style="color: var(--fg-label); border-bottom: 1px solid var(--row-divider); width: 48px;"
                >
                  {v.index + 1}
                </td>
                {#each row as cell, ci (ci)}
                  <td
                    class="px-3 py-2 whitespace-nowrap"
                    style="border-bottom: 1px solid var(--row-divider);"
                  >
                    {#if isNull(cell)}
                      <span style="color: var(--fg-muted); font-style: italic; opacity: 0.6;"
                        >NULL</span
                      >
                    {:else}
                      <span style="color: var(--fg-default);">{fmt(cell)}</span>
                    {/if}
                  </td>
                {/each}
                <td
                  class="px-2 py-2 text-right"
                  style="border-bottom: 1px solid var(--row-divider); width: 100px;"
                >
                  {#if hasPk}
                    <div
                      class="inline-flex gap-0.5 opacity-0 transition-opacity group-hover:opacity-100"
                    >
                      <button
                        title="Edit"
                        class="rounded p-1 hover:bg-[var(--accent-soft)]"
                        onclick={() => onEdit?.(row, v.index)}
                        style="color: var(--fg-muted);"
                      >
                        <Edit3 class="h-3 w-3" />
                      </button>
                      <button
                        title="Duplicate"
                        class="rounded p-1 hover:bg-[var(--accent-soft)]"
                        onclick={() => onDuplicate?.(row, v.index)}
                        style="color: var(--fg-muted);"
                      >
                        <Copy class="h-3 w-3" />
                      </button>
                      <button
                        title="Delete"
                        class="rounded p-1 hover:bg-[color-mix(in_srgb,var(--dot-danger)_10%,transparent)]"
                        onclick={() => onDelete?.(row, v.index)}
                        style="color: var(--dot-danger);"
                      >
                        <Trash2 class="h-3 w-3" />
                      </button>
                    </div>
                  {/if}
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
                {#each row as cell, ci (ci)}
                  <td
                    class="px-3 py-2 whitespace-nowrap"
                    style="border-bottom: 1px solid var(--row-divider);"
                  >
                    {#if isNull(cell)}
                      <span style="color: var(--fg-muted); font-style: italic; opacity: 0.6;"
                        >NULL</span
                      >
                    {:else}
                      <span style="color: var(--fg-default);">{fmt(cell)}</span>
                    {/if}
                  </td>
                {/each}
                <td
                  class="px-2 py-2 text-right"
                  style="border-bottom: 1px solid var(--row-divider); width: 100px;"
                >
                  {#if hasPk}
                    <div
                      class="inline-flex gap-0.5 opacity-0 transition-opacity group-hover:opacity-100"
                    >
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
