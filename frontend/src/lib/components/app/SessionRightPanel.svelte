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
  import { apiRequest, type SessionMeta, type SessionKind } from '$lib/api/client';
  import { tasksState } from '$lib/stores/tasks.svelte';
  import { Bot } from '$lib/icons';
  import { taskProgress } from '$lib/sessionDisplay';
  import { onDestroy } from 'svelte';
  import { toast } from 'svelte-sonner';

  interface ChildSession {
    session_id: string;
    parent_session_id: string | null;
    root_session_id: string;
    kind: SessionKind;
    role: string | null;
    status: 'running' | 'exited' | 'killed';
    started_at: number;
    pid: number;
    detected_state?: 'working' | 'blocked' | 'idle' | 'unknown' | null;
  }

  interface Props {
    session: SessionMeta | null;
    /** Notified when the user picks a child from the Agents tab so the
     *  parent view can swap the active terminal. */
    onChildSelected?: (childSessionId: string) => void;
  }

  let { session, onChildSelected }: Props = $props();

  type Tab = 'tasks' | 'agents' | 'info';
  let tab = $state<Tab>('tasks');
  let children = $state<ChildSession[]>([]);
  let childrenError = $state<string | null>(null);
  let pollTimer: ReturnType<typeof setInterval> | null = null;
  // Track ids and statuses across polls so we can fire toasts when something
  // actually changes — a hard refresh of the list always renders, but the
  // visual blip belongs on transitions.
  let knownIds = $state<Set<string>>(new Set());
  let knownStatuses = $state<Map<string, string>>(new Map());
  let activeSessionId: string | null = null;

  // Convenience — keep counts reactive without re-fetching.
  const prog = $derived(taskProgress(tasksState.items));
  const runningChildren = $derived(children.filter((c) => c.status === 'running').length);

  // Done-set: F2 uses status===done. F3 may also collapse
  // pending_verify when checks pass.
  function isDone(status: string): boolean {
    return status === 'done';
  }

  async function loadChildren() {
    if (!session) {
      children = [];
      return;
    }
    try {
      const res = await apiRequest<ChildSession[]>(`/sessions/${session.id}/children`);
      const next = res.data ?? [];
      diffAndToast(next);
      children = next;
      childrenError = null;
    } catch (err) {
      childrenError = err instanceof Error ? err.message : String(err);
    }
  }

  /**
   * Diff the polled list against what we already showed: surface new spawns
   * and status transitions as toasts so the user gets a real "something
   * happened" signal even when they're not looking at the Agents tab.
   */
  function diffAndToast(next: ChildSession[]) {
    // Skip the first round after a session switch — everything would toast.
    if (activeSessionId !== session?.id) {
      activeSessionId = session?.id ?? null;
      knownIds = new Set(next.map((c) => c.session_id));
      knownStatuses = new Map(next.map((c) => [c.session_id, c.status]));
      return;
    }
    for (const c of next) {
      if (!knownIds.has(c.session_id)) {
        toast.success(`Child spawned: ${c.role ?? '(no role)'} · ${c.kind}`, {
          description: `session ${c.session_id.slice(0, 8)}…`
        });
        knownIds.add(c.session_id);
      }
      const prev = knownStatuses.get(c.session_id);
      if (prev && prev !== c.status) {
        const icon = c.status === 'running' ? 'info' : c.status === 'exited' ? 'success' : 'error';
        toast[icon === 'info' ? 'info' : icon === 'success' ? 'success' : 'error'](
          `Child ${c.role ?? c.kind} → ${c.status}`,
          { description: `session ${c.session_id.slice(0, 8)}…` }
        );
      }
      knownStatuses.set(c.session_id, c.status);
    }
  }

  // Defensive: every time the Tasks tab becomes visible, refetch the list.
  // SSE wiring should keep this in sync but if a `task.created` event was
  // dropped (timing race, MCP fallback to local FS, etc.) the user still
  // sees the latest state by clicking through. Cheap (~few KB).
  $effect(() => {
    if (tab === 'tasks' && session) {
      void tasksState.refresh();
    }
  });

  // Poll children whenever there's a session selected — not only when the
  // Agents tab is open. That way the badge count + toasts stay live even
  // while the user is reading tasks or info.
  $effect(() => {
    if (pollTimer) {
      clearInterval(pollTimer);
      pollTimer = null;
    }
    if (session) {
      void loadChildren();
      pollTimer = setInterval(() => void loadChildren(), 1500);
    } else {
      children = [];
      knownIds = new Set();
      knownStatuses = new Map();
      activeSessionId = null;
    }
  });

  onDestroy(() => {
    if (pollTimer) clearInterval(pollTimer);
  });

  function statusColor(s: string): string {
    if (s === 'running') return 'var(--dot-success)';
    if (s === 'exited') return 'var(--dot-warning, #f59e0b)';
    return 'var(--dot-danger)';
  }

  function startedLabel(ms: number): string {
    const diff = Math.max(0, Date.now() - ms) / 1000;
    if (diff < 60) return `${Math.round(diff)}s ago`;
    if (diff < 3600) return `${Math.round(diff / 60)}m ago`;
    return new Date(ms).toLocaleString();
  }
</script>

<style>
  /* Pulse animation for running children — subtle, single-color so it works
     against any theme. */
  @keyframes harness-dot-pulse {
    0%, 100% { transform: scale(1); opacity: 1; }
    50%      { transform: scale(1.45); opacity: 0.55; }
  }
  .dot-pulse {
    animation: harness-dot-pulse 1.8s ease-in-out infinite;
  }
</style>

<aside
  class="flex h-full w-[280px] shrink-0 flex-col overflow-hidden border-l"
  style="background: var(--surface-panel); border-color: var(--border-subtle);"
>
  <!-- Tab strip -->
  <div
    class="flex shrink-0 gap-1 border-b p-1.5"
    style="background: var(--surface-window); border-color: var(--border-subtle);"
  >
    {#each [{ id: 'tasks' as const, label: session ? `Tasks ${prog.done}/${prog.total}` : 'Tasks' }, { id: 'agents' as const, label: children.length > 0 ? `Agents · ${runningChildren}/${children.length}` : 'Agents' }, { id: 'info' as const, label: 'Info' }] as t (t.id)}
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
          Sub-agents · {children.length} spawned
        </div>

        {#if childrenError}
          <p
            class="rounded-md border px-2 py-1.5 text-[11px]"
            style="border-color: color-mix(in srgb, var(--dot-danger) 35%, transparent); background: color-mix(in srgb, var(--dot-danger) 10%, transparent); color: var(--dot-danger);"
          >
            {childrenError}
          </p>
        {/if}

        {#if children.length === 0}
          <div class="flex flex-col items-center gap-2 px-2 py-8 text-center">
            <Bot class="h-5 w-5 opacity-30" />
            <p class="text-xs leading-relaxed" style="color: var(--fg-muted);">
              No sub-agents spawned by this session. Zeus and other orchestrators can spawn
              workers via the harness MCP <code class="font-mono">session_spawn_child</code> tool.
            </p>
          </div>
        {:else}
          {#each children as ag (ag.session_id)}
            <button
              type="button"
              class="relative overflow-hidden rounded-lg border px-3 py-2.5 text-left transition-colors hover:bg-[var(--accent-soft)]"
              style="
                border-color: var(--border-subtle);
                background: var(--surface-window);
              "
              onclick={() => onChildSelected?.(ag.session_id)}
              title="Open child session terminal"
            >
              <div class="flex items-center gap-2">
                <span
                  class="h-2 w-2 rounded-full {ag.status === 'running' ? 'dot-pulse' : ''}"
                  style="background: {statusColor(ag.status)};"
                ></span>
                <span class="flex-1 truncate text-[12.5px] font-semibold">
                  {ag.role ?? '(no role)'} · <span class="text-[var(--fg-muted)]">{ag.kind}</span>
                </span>
                <span
                  class="rounded border px-1.5 py-0.5 text-[9.5px] font-bold uppercase"
                  style="color: {statusColor(ag.status)}; border-color: var(--accent-soft-border); background: var(--accent-soft);"
                >
                  {ag.status}
                </span>
              </div>
              <p class="mt-1 truncate font-mono text-[10px]" style="color: var(--fg-muted);">
                {ag.session_id.slice(0, 8)}… · pid {ag.pid} · started {startedLabel(ag.started_at)}
              </p>
              {#if ag.detected_state && ag.detected_state !== 'unknown' && ag.status === 'running'}
                <p
                  class="mt-1 inline-flex items-center gap-1 text-[10px] font-mono"
                  style="color: {ag.detected_state === 'working'
                    ? 'rgb(96 165 250)'
                    : ag.detected_state === 'blocked'
                      ? 'rgb(251 191 36)'
                      : 'rgb(148 163 184)'};"
                >
                  {ag.detected_state === 'working'
                    ? '⋯ thinking'
                    : ag.detected_state === 'blocked'
                      ? '⏸ waiting for input'
                      : '✓ idle'}
                </p>
              {/if}
            </button>
          {/each}
        {/if}
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
