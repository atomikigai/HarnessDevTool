<!--
  TaskGraph — placeholder for the DAG view.
  The full layered/curved-edge DAG (planned with @xyflow/svelte) was skipped
  for F2 to keep the dependency surface small. This stub renders the same
  data as a grouped list so the toggle in the page still reads usefully.
  TODO(F3): wire @xyflow/svelte with nodes positioned by topological layer.
-->
<script lang="ts">
  import type { Task } from '$lib/api/models/task';
  import TaskStatusBadge from './TaskStatusBadge.svelte';
  import { Network } from '$lib/icons';

  let { tasks, onSelect }: { tasks: Task[]; onSelect: (id: string) => void } = $props();

  // Group by simple layer: roots (no blocked_by) first, then everything else
  // ordered by number of blockers. Real layered layout is F3 work.
  const layers = $derived.by(() => {
    const map = new Map<number, Task[]>();
    for (const t of tasks) {
      const depth = t.blocked_by.length;
      if (!map.has(depth)) map.set(depth, []);
      map.get(depth)!.push(t);
    }
    return [...map.entries()].sort((a, b) => a[0] - b[0]);
  });
</script>

<div class="flex h-full flex-col overflow-y-auto p-6">
  <div
    class="mb-4 flex items-start gap-3 rounded-md border px-3 py-2 text-xs"
    style="border-color: var(--border-subtle); background: var(--surface-titlebar); color: var(--fg-muted);"
  >
    <Network class="mt-0.5 h-3.5 w-3.5" />
    <div>
      <p>
        Graph view is a layered preview for F2. A full DAG with edges (xyflow) is queued for F3.
      </p>
    </div>
  </div>

  {#if tasks.length === 0}
    <p class="text-sm" style="color: var(--fg-muted);">No tasks to graph.</p>
  {:else}
    <div class="flex flex-col gap-6">
      {#each layers as [depth, items] (depth)}
        <section>
          <h3 class="h-eyebrow mb-2">Layer {depth} ({items.length})</h3>
          <div class="flex flex-wrap gap-2">
            {#each items as t (t.id)}
              <button
                class="flex flex-col gap-1 rounded-md border px-3 py-2 text-left text-xs transition-colors hover:border-[var(--accent)]"
                style="border-color: var(--border-subtle); background: var(--surface-panel); min-width: 180px; max-width: 240px;"
                onclick={() => onSelect(t.id)}
              >
                <div class="flex items-center gap-2">
                  <span class="font-mono text-[11px]" style="color: var(--fg-muted);">{t.id}</span>
                  <TaskStatusBadge status={t.status} size="xs" />
                </div>
                <span class="truncate" style="color: var(--fg-default);">{t.title}</span>
                {#if t.blocked_by.length > 0}
                  <span class="text-[10px]" style="color: var(--fg-muted);">
                    ← {t.blocked_by.join(', ')}
                  </span>
                {/if}
              </button>
            {/each}
          </div>
        </section>
      {/each}
    </div>
  {/if}
</div>
