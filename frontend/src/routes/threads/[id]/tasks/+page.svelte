<!--
  /threads/[id]/tasks — task board for a single thread.
  Left: filter bar + table (or graph). Right: optional drawer with TaskDetail.
  Wired to the tasksState rune store, which keeps an SSE connection live to
  /api/events?thread=:tid for incremental updates.
-->
<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { page } from '$app/stores';
  import { tasksState } from '$lib/stores/tasks.svelte';
  import TaskStatusBadge from '$lib/components/tasks/TaskStatusBadge.svelte';
  import TaskDetail from '$lib/components/tasks/TaskDetail.svelte';
  import TaskCreateForm from '$lib/components/tasks/TaskCreateForm.svelte';
  import TaskGraph from '$lib/components/tasks/TaskGraph.svelte';
  import BudgetMeter from '$lib/components/tasks/BudgetMeter.svelte';
  import AgentCostBreakdown from '$lib/components/tasks/AgentCostBreakdown.svelte';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { budgetStore } from '$lib/stores/budget.svelte';
  import {
    Plus,
    ChevronLeft,
    Loader2,
    CircleAlert,
    ListChecks,
    Network,
    RefreshCw,
    Settings,
    AlertTriangle
  } from '$lib/icons';
  import { formatDistanceToNow } from 'date-fns';
  import type { TaskStatus } from '$lib/api/models/task';

  const threadId = $derived($page.params.id as string);

  let selectedId = $state<string | null>(null);
  let createOpen = $state(false);
  let view = $state<'table' | 'graph'>('table');

  let statusFilter = $state<TaskStatus | ''>('');
  let labelFilter = $state('');

  const selected = $derived(selectedId ? tasksState.byId(selectedId) : null);
  const budgetEntry = $derived(budgetStore.get(threadId));
  const budgetView = $derived(budgetEntry.view);

  // Hard cap visible rows at 200 (see deuda about virtualization).
  const visible = $derived(tasksState.items.slice(0, 200));
  const truncated = $derived(tasksState.items.length > 200);

  onMount(() => {
    tasksState.start(threadId);
  });

  onDestroy(() => {
    tasksState.stop();
  });

  // When the thread id in the URL changes, re-bind the store.
  $effect(() => {
    if (threadId) tasksState.start(threadId);
  });

  function applyFilters() {
    tasksState.setFilters({
      status: statusFilter || undefined,
      label: labelFilter.trim() || undefined
    });
  }

  function clearFilters() {
    statusFilter = '';
    labelFilter = '';
    tasksState.setFilters({});
  }

  function selectRow(id: string) {
    selectedId = id;
  }

  function reasonTitle(task: (typeof tasksState.items)[number]): string {
    const notes = task.notes;
    return (
      notes.last_failure?.trim() ||
      notes.rejected_reason?.trim() ||
      notes.blocked_reason?.trim() ||
      notes.paused_reason?.trim() ||
      notes.why_paused?.trim() ||
      (notes.needs_human ? 'Human input required' : '')
    );
  }

  const statuses: TaskStatus[] = [
    'proposed',
    'queued',
    'in_progress',
    'pending_verify',
    'done',
    'paused',
    'blocked',
    'abandoned'
  ];
</script>

<div class="flex h-full w-full flex-col">
  <!-- Header / filters -->
  <header
    class="flex flex-wrap items-center gap-3 border-b px-4 py-2"
    style="background: var(--surface-panel); border-color: var(--border-subtle);"
  >
    <Button variant="ghost" size="sm" onclick={() => history.back()} aria-label="Back">
      <ChevronLeft class="h-4 w-4" />
    </Button>
    <nav class="flex items-center gap-1.5 text-xs" style="color: var(--fg-breadcrumb);">
      <a href="/" class="hover:underline">Agents</a>
      <span style="color: var(--fg-muted);">/</span>
      <span class="font-mono" style="color: var(--fg-muted);">{threadId.slice(0, 8)}</span>
      <span style="color: var(--fg-muted);">/</span>
      <span style="color: var(--fg-default);">tasks</span>
    </nav>

    <div
      class="ml-2 flex items-center gap-1 rounded-md border p-0.5"
      style="border-color: var(--border-subtle); background: var(--surface-window);"
    >
      <button
        class="flex items-center gap-1 rounded px-2 py-1 text-xs"
        style={view === 'table'
          ? 'background: var(--accent-soft); color: var(--accent);'
          : 'color: var(--fg-muted);'}
        onclick={() => (view = 'table')}
      >
        <ListChecks class="h-3 w-3" /> Table
      </button>
      <button
        class="flex items-center gap-1 rounded px-2 py-1 text-xs"
        style={view === 'graph'
          ? 'background: var(--accent-soft); color: var(--accent);'
          : 'color: var(--fg-muted);'}
        onclick={() => (view = 'graph')}
      >
        <Network class="h-3 w-3" /> Graph
      </button>
    </div>

    <div class="ml-auto flex items-center gap-2">
      <select
        bind:value={statusFilter}
        onchange={applyFilters}
        class="h-8 rounded-md border bg-[var(--surface-window)] px-2 text-xs"
        style="border-color: var(--border-input); color: var(--fg-default);"
      >
        <option value="">all status</option>
        {#each statuses as s (s)}
          <option value={s}>{s}</option>
        {/each}
      </select>
      <Input
        placeholder="filter by label"
        bind:value={labelFilter}
        class="h-8 w-36 text-xs"
        onkeydown={(e: KeyboardEvent) => e.key === 'Enter' && applyFilters()}
      />
      {#if statusFilter || labelFilter}
        <Button size="sm" variant="ghost" onclick={clearFilters}>clear</Button>
      {/if}
      <Button size="sm" variant="outline" onclick={() => tasksState.refresh()}>
        <RefreshCw class="h-3.5 w-3.5" />
      </Button>
      <Button size="sm" onclick={() => (createOpen = true)}>
        <Plus class="h-3.5 w-3.5" /> New task
      </Button>
      <a
        href="/agents"
        class="inline-flex h-8 w-8 items-center justify-center rounded-md"
        style="color: var(--fg-muted);"
        title="Agents registry"
      >
        <Settings class="h-3.5 w-3.5" />
      </a>
    </div>
  </header>

  <!-- Budget strip — shows USD/wallclock burn for this thread. -->
  <div
    class="border-b px-4 py-2"
    style="background: var(--surface-canvas); border-color: var(--border-subtle);"
  >
    <BudgetMeter {threadId} />
    <div class="mt-2">
      <AgentCostBreakdown view={budgetView} />
    </div>
  </div>

  <!-- Body — two panes -->
  <div class="flex min-h-0 flex-1">
    <section class="min-w-0 flex-1 overflow-hidden">
      {#if tasksState.loading && tasksState.items.length === 0}
        <div
          class="flex h-full items-center justify-center gap-2 text-sm"
          style="color: var(--fg-muted);"
        >
          <Loader2 class="h-4 w-4 animate-spin" /> Loading tasks…
        </div>
      {:else if tasksState.error}
        <div
          class="m-4 flex items-start gap-3 rounded-md border p-4 text-sm"
          style="border-color: color-mix(in srgb, var(--dot-danger) 35%, transparent); background: color-mix(in srgb, var(--dot-danger) 10%, transparent); color: var(--dot-danger);"
        >
          <CircleAlert class="mt-0.5 h-4 w-4" />
          <div>
            <p class="font-medium">Failed to load tasks</p>
            <p class="mt-0.5 text-xs" style="color: var(--fg-muted);">{tasksState.error}</p>
          </div>
        </div>
      {:else if tasksState.items.length === 0}
        <div class="flex h-full flex-col items-center justify-center gap-3 p-8 text-center">
          <p class="text-sm" style="color: var(--fg-muted);">No tasks in this thread yet.</p>
          <Button onclick={() => (createOpen = true)}
            ><Plus class="h-3.5 w-3.5" /> Create first task</Button
          >
        </div>
      {:else if view === 'graph'}
        <TaskGraph tasks={visible} onSelect={selectRow} />
      {:else}
        <div class="h-full overflow-auto">
          {#if truncated}
            <div
              class="flex items-center gap-2 border-b px-4 py-1.5 text-[11px]"
              style="border-color: var(--border-subtle); background: var(--surface-titlebar); color: var(--dot-warn);"
            >
              <AlertTriangle class="h-3 w-3" /> Showing first 200 of {tasksState.items.length} tasks.
              Virtualization not yet wired (F2 deuda).
            </div>
          {/if}
          <table class="w-full text-sm">
            <thead
              class="sticky top-0 text-left text-[10px] uppercase tracking-wider"
              style="background: var(--surface-titlebar); color: var(--fg-label);"
            >
              <tr>
                <th class="px-4 py-2">ID</th>
                <th class="px-4 py-2">Title</th>
                <th class="px-4 py-2">Status</th>
                <th class="px-4 py-2">Assignee</th>
                <th class="px-4 py-2">Updated</th>
                <th class="px-4 py-2">Blocked by</th>
              </tr>
            </thead>
            <tbody>
              {#each visible as t (t.id)}
                {@const isSel = t.id === selectedId}
                <tr
                  class="cursor-pointer border-b transition-colors hover:bg-[var(--accent-soft)]"
                  style="border-color: var(--row-divider); {isSel
                    ? 'background: var(--accent-soft);'
                    : ''}"
                  onclick={() => selectRow(t.id)}
                >
                  <td class="px-4 py-2 font-mono text-[12px]" style="color: var(--fg-muted);"
                    >{t.id}</td
                  >
                  <td class="max-w-md px-4 py-2">
                    <div class="flex min-w-0 items-center gap-2">
                      <span class="truncate">{t.title}</span>
                      {#if reasonTitle(t)}
                        <span
                          class="shrink-0 rounded-sm px-1.5 py-0.5 text-[10px]"
                          style="background: color-mix(in srgb, var(--dot-warn) 14%, transparent); color: var(--dot-warn);"
                          title={reasonTitle(t)}
                        >
                          !
                        </span>
                      {/if}
                    </div>
                  </td>
                  <td class="px-4 py-2"><TaskStatusBadge status={t.status} /></td>
                  <td class="px-4 py-2 font-mono text-[12px]" style="color: var(--fg-muted);">
                    {t.assignee ?? '—'}
                  </td>
                  <td class="px-4 py-2 text-[12px]" style="color: var(--fg-muted);">
                    {formatDistanceToNow(new Date(t.updated_at), { addSuffix: true })}
                  </td>
                  <td class="px-4 py-2 text-[12px]">
                    {#if t.blocked_by.length === 0}
                      <span style="color: var(--fg-muted);">—</span>
                    {:else}
                      <span
                        title={t.blocked_by.join(', ')}
                        class="inline-flex items-center gap-1 rounded-full px-1.5 py-0.5 text-[10px]"
                        style="background: color-mix(in srgb, var(--dot-warn) 14%, transparent); color: var(--dot-warn);"
                      >
                        {t.blocked_by.length} blocker{t.blocked_by.length === 1 ? '' : 's'}
                      </span>
                    {/if}
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </section>

    {#if selected}
      <section class="w-[420px] shrink-0">
        <TaskDetail
          {threadId}
          task={selected}
          onClose={() => (selectedId = null)}
          onSelect={selectRow}
          onChange={() => tasksState.refreshOne(selected!.id)}
        />
      </section>
    {/if}
  </div>
</div>

<TaskCreateForm
  bind:open={createOpen}
  {threadId}
  existingTasks={tasksState.items}
  onCreated={(t) => (selectedId = t.id)}
/>
