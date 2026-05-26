<!--
  SessionRightPanel — tabbed side panel on the far right of the Agents view.

  Tabs:
    • Tasks (live)   — reads from `tasksState` (the parent owns
                       start/stop on the current thread to avoid double
                       SSE subscriptions).
    • Agents (stub)  — sub-agents spawned by the orchestrator. F2 shows
                       the empty state since we don't spawn sub-agents
                       yet; F3 will wire the real data.
    • Info           — session metadata (id, kind, pid, cwd, …).
-->
<script lang="ts">
  import type { SessionMeta } from '$lib/api/client';
  import { tasksState } from '$lib/stores/tasks.svelte';
  import { Bot } from '$lib/icons';
  import { taskProgress } from '$lib/sessionDisplay';

  interface Props {
    session: SessionMeta | null;
  }

  let { session }: Props = $props();

  type Tab = 'tasks' | 'agents' | 'info';
  let tab = $state<Tab>('tasks');

  // Convenience — keep counts reactive without re-fetching.
  const prog = $derived(taskProgress(tasksState.items));

  // Done-set: F2 uses status===done. F3 may also collapse
  // pending_verify when checks pass.
  function isDone(status: string): boolean {
    return status === 'done';
  }

  // TODO(F3): replace with real SubAgent type once `harness-core::subagents`
  // lands. Expected shape (kept here to ease the wire-up):
  //   interface SubAgent {
  //     id: string;
  //     parent_session_id: string;
  //     role: string;            // "Code Analyst", "Patch Writer", …
  //     status: 'active' | 'idle' | 'stopped';
  //     current_action: string;  // 1-line summary
  //     started_at: string;
  //   }
  const subAgents: never[] = [];
</script>

<aside
  class="flex h-full w-[280px] shrink-0 flex-col overflow-hidden border-l"
  style="background: var(--surface-panel); border-color: var(--border-subtle);"
>
  <!-- Tab strip -->
  <div
    class="flex shrink-0 gap-1 border-b p-1.5"
    style="background: var(--surface-window); border-color: var(--border-subtle);"
  >
    {#each [{ id: 'tasks' as const, label: session ? `Tasks ${prog.done}/${prog.total}` : 'Tasks' }, { id: 'agents' as const, label: 'Agents' }, { id: 'info' as const, label: 'Info' }] as t (t.id)}
      <button
        type="button"
        onclick={() => (tab = t.id)}
        class="flex-1 rounded-md py-1.5 text-[11px] transition-colors"
        style="
          background: {tab === t.id ? 'var(--surface-panel)' : 'transparent'};
          color: {tab === t.id ? 'var(--accent)' : 'var(--fg-muted)'};
          font-weight: {tab === t.id ? 600 : 400};
          box-shadow: {tab === t.id ? 'var(--shadow-soft)' : 'none'};
          border-bottom: 2px solid {tab === t.id ? 'var(--accent)' : 'transparent'};
        "
      >
        {t.label}
      </button>
    {/each}
  </div>

  <!-- Body -->
  <div class="min-h-0 flex-1 overflow-y-auto">
    {#if !session}
      <div class="flex flex-col items-center gap-2 px-6 py-10 text-center">
        <div class="text-2xl opacity-30">⌨</div>
        <p class="text-xs" style="color: var(--fg-muted);">
          Select a session to see its tasks, sub-agents and metadata.
        </p>
      </div>
    {:else if tab === 'tasks'}
      <div class="flex flex-col gap-1.5 p-3">
        {#if tasksState.loading && tasksState.items.length === 0}
          <p class="px-1 text-xs" style="color: var(--fg-muted);">Loading tasks…</p>
        {:else if tasksState.items.length === 0}
          <div class="flex flex-col items-center gap-2 px-2 py-8 text-center">
            <p class="text-xs" style="color: var(--fg-muted);">No tasks yet.</p>
            <p class="text-[11px]" style="color: var(--fg-label);">
              Create one from the Tasks page, or let the agent plan its work.
            </p>
          </div>
        {:else}
          {#each tasksState.items as t (t.id)}
            {@const done = isDone(t.status)}
            <div
              class="flex items-start gap-2.5 rounded-md border px-2.5 py-2"
              style="
                border-color: {done ? 'var(--accent-soft-border)' : 'var(--border-subtle)'};
                background: {done ? 'var(--accent-soft)' : 'transparent'};
              "
            >
              <span
                class="mt-0.5 inline-flex h-4 w-4 shrink-0 items-center justify-center rounded border"
                style="
                  background: {done ? 'var(--accent)' : 'transparent'};
                  border-color: {done ? 'var(--accent)' : 'var(--border-input)'};
                "
              >
                {#if done}
                  <svg width="9" height="9" viewBox="0 0 16 16" fill="none" aria-hidden="true">
                    <path
                      d="M3 8.5L6.5 12 13 4"
                      stroke="#fff"
                      stroke-width="2.5"
                      stroke-linecap="round"
                      stroke-linejoin="round"
                    />
                  </svg>
                {/if}
              </span>
              <span
                class="text-xs leading-snug"
                style="
                  color: {done ? 'var(--fg-muted)' : 'var(--fg-default)'};
                  text-decoration: {done ? 'line-through' : 'none'};
                "
              >
                {t.title}
              </span>
            </div>
          {/each}
        {/if}
      </div>
    {:else if tab === 'agents'}
      <div class="flex flex-col gap-2.5 p-3">
        <div class="text-[10px] font-bold uppercase tracking-wider" style="color: var(--fg-label);">
          Sub-agents · {subAgents.length} spawned
        </div>

        {#if subAgents.length === 0}
          <div class="flex flex-col items-center gap-2 px-2 py-8 text-center">
            <Bot class="h-5 w-5 opacity-30" />
            <p class="text-xs leading-relaxed" style="color: var(--fg-muted);">
              Sub-agents will appear here when the orchestrator spawns workers (F3).
            </p>
          </div>
        {/if}

        <!-- TODO(F3): render real sub-agent cards here. The reference
             markup is preserved below as a comment so the wire-up is a
             matter of swapping `subAgents` with the live store. -->
        <!--
          {#each subAgents as ag (ag.id)}
            <article
              class="relative overflow-hidden rounded-lg border px-3 py-2.5"
              style="
                border-color: var(--border-subtle);
                background: var(--surface-window);
              "
            >
              <div class="flex items-center gap-2">
                <span class="h-2 w-2 rounded-full" style="background: var(--dot-success);"></span>
                <span class="flex-1 text-[12.5px] font-semibold">{ag.role}</span>
                <span
                  class="rounded border px-1.5 py-0.5 text-[9.5px] font-bold uppercase"
                  style="color: var(--dot-success); border-color: var(--accent-soft-border); background: var(--accent-soft);"
                >
                  {ag.status}
                </span>
              </div>
              <p class="mt-1.5 text-[11px] leading-snug" style="color: var(--fg-breadcrumb);">
                {ag.current_action}
              </p>
              <p class="mt-1 font-mono text-[10px]" style="color: var(--fg-label);">
                started {ag.started_at}
              </p>
            </article>
          {/each}
        -->
      </div>
    {:else if tab === 'info'}
      <div class="flex flex-col gap-0 p-3">
        {#each [['Session ID', session.id], ['Kind', session.kind], ['Status', session.status], ['PID', String(session.pid)], ['Started', new Date(session.started_at).toLocaleString()], ['Directory', session.cwd ?? '(default)'], ['Thread ID', session.thread_id]] as [label, val] (label)}
          <div
            class="grid grid-cols-[88px_1fr] gap-2 border-b py-2"
            style="border-color: var(--row-divider);"
          >
            <span class="font-mono text-[10.5px]" style="color: var(--fg-muted);">{label}</span>
            <span
              class="truncate font-mono text-[11px]"
              style="color: var(--fg-default);"
              title={val}
            >
              {val}
            </span>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</aside>
