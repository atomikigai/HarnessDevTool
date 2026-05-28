<!--
  SessionsColumn — vertical list of sessions sitting between the icon rail
  and the main session view. Mirrors the reference design's middle column.

  Two modes:
    • expanded (~280px): header + status pills + per-session cards
    • collapsed (~52px): "+" button + vertical status dots (click selects)

  Data:
    • `sessions`           — flat list of all sessions (from `sessionsState`)
    • `selectedSessionId`  — currently selected session id (bindable string|null)
    • `threadOf(sid)`      — caller-provided lookup for the thread of a session
    • `progressFor(tid)`   — optional per-thread task progress (caller passes
                             the selected thread's progress so we don't have
                             to subscribe to every thread's task SSE).
-->
<script lang="ts">
  import type { SessionMeta } from '$lib/api/client';
  import HarnessIcons from './HarnessIcons.svelte';
  import { Plus, ChevronRight, ChevronLeft, ChevronDown, Bot, Trash2 } from '$lib/icons';
  import { confirmDialog } from '$lib/components/ui/confirm-dialog';
  import {
    kindChip,
    relTime,
    statusColor,
    statusLabel,
    tokensLabel,
    uiStatus,
    uptime,
    type TaskProgress
  } from '$lib/sessionDisplay';

  interface Props {
    sessions: SessionMeta[];
    selectedSessionId: string | null;
    onSelect: (sessionId: string) => void;
    onNew: () => void;
    /** Caller deletes the session (kill PTY + drop from manager). Awaited so
     *  the column can clear local hover/menu state synchronously. */
    onDelete: (sessionId: string) => Promise<void> | void;
    collapsed: boolean;
    onToggleCollapsed: () => void;
    /** Optional per-thread task progress. F2: caller supplies only for the
     *  selected thread; everything else falls back to 0/0. F3 will index
     *  this by thread id once we wire a multi-thread progress store. */
    progressByThread?: Record<string, TaskProgress>;
  }

  let {
    sessions,
    selectedSessionId,
    onSelect,
    onNew,
    onDelete,
    collapsed,
    onToggleCollapsed,
    progressByThread = {}
  }: Props = $props();

  /// Per-card "deleting" guard so a slow DELETE can't be re-issued by an
  /// impatient user.
  let deleting = $state<string | null>(null);

  // ── Session tree grouping ────────────────────────────────────────────────
  // Sessions form a tree via `parent_session_id` + `root_session_id`. Group
  // children under their root so Zeus orchestrators visually own the worker
  // sessions they spawned. Roots without children render as a single card.
  interface SessionGroup {
    root: SessionMeta;
    children: SessionMeta[];
  }
  const groups = $derived.by<SessionGroup[]>(() => {
    const childrenByRoot = new Map<string, SessionMeta[]>();
    const roots: SessionMeta[] = [];
    for (const s of sessions) {
      const isRoot = !s.parent_session_id || s.parent_session_id === s.id;
      if (isRoot) {
        roots.push(s);
      } else {
        // Anchor on `root_session_id` if present (carries the topmost
        // ancestor); fall back to direct parent so legacy sessions still
        // appear under something sensible.
        const anchor = s.root_session_id ?? s.parent_session_id ?? s.id;
        const arr = childrenByRoot.get(anchor) ?? [];
        arr.push(s);
        childrenByRoot.set(anchor, arr);
      }
    }
    return roots.map((root) => ({
      root,
      children: (childrenByRoot.get(root.id) ?? []).sort((a, b) =>
        a.started_at < b.started_at ? -1 : 1
      )
    }));
  });

  // Expand/collapse state per root, persisted across reloads. Default: all
  // expanded so newly-spawned children are visible without an extra click.
  const EXPAND_KEY = 'harness.expandedRoots';
  function readExpanded(): Set<string> {
    if (typeof window === 'undefined') return new Set();
    try {
      const raw = localStorage.getItem(EXPAND_KEY);
      if (!raw) return new Set();
      const arr = JSON.parse(raw) as string[];
      return new Set(arr);
    } catch {
      return new Set();
    }
  }
  function writeExpanded(set: Set<string>) {
    if (typeof window === 'undefined') return;
    localStorage.setItem(EXPAND_KEY, JSON.stringify([...set]));
  }
  // `null` means "not yet persisted, default expanded". Anything in the set
  // is explicitly collapsed (negation logic is cheaper for the common case).
  let collapsedRoots = $state<Set<string>>(readExpanded());
  function isExpanded(rootId: string): boolean {
    return !collapsedRoots.has(rootId);
  }
  function toggleRoot(rootId: string) {
    const next = new Set(collapsedRoots);
    if (next.has(rootId)) next.delete(rootId);
    else next.add(rootId);
    collapsedRoots = next;
    writeExpanded(next);
  }

  function runningChildren(g: SessionGroup): number {
    return g.children.filter((c) => uiStatus(c) === 'active').length;
  }

  async function handleDelete(ev: MouseEvent, s: SessionMeta) {
    ev.stopPropagation();
    ev.preventDefault();
    if (deleting === s.id) return;
    const label = s.kind + ' · ' + s.id.slice(0, 8);
    const ok = await confirmDialog({
      title: `Delete session ${label}?`,
      description: 'The PTY is killed and the card removed.',
      confirmLabel: 'Delete',
      destructive: true
    });
    if (!ok) return;
    deleting = s.id;
    try {
      await onDelete(s.id);
    } finally {
      deleting = null;
    }
  }

  const activeCount = $derived(sessions.filter((s) => uiStatus(s) === 'active').length);
  const idleCount = $derived(
    sessions.filter((s) => {
      const u = uiStatus(s);
      return u === 'idle' || u === 'untitled';
    }).length
  );

  function progressFor(s: SessionMeta): TaskProgress {
    return progressByThread[s.thread_id] ?? { done: 0, total: 0, pct: 0 };
  }
</script>

<aside
  class="flex h-full shrink-0 flex-col overflow-hidden border-r transition-[width]"
  style="
    width: {collapsed ? '52px' : '280px'};
    background: var(--surface-panel);
    border-color: var(--border-subtle);
    transition-duration: 180ms;
  "
>
  {#if collapsed}
    <!-- COLLAPSED — chevron expand, + new, then one status dot per session. -->
    <div class="flex flex-col items-center gap-1.5 pt-3">
      <button
        type="button"
        onclick={onToggleCollapsed}
        title="Expand sessions"
        aria-label="Expand sessions"
        class="flex h-7 w-7 items-center justify-center rounded-md border transition-colors hover:bg-[var(--accent-soft)]"
        style="border-color: var(--border-subtle); color: var(--fg-muted); background: var(--surface-window);"
      >
        <ChevronRight class="h-3.5 w-3.5" />
      </button>
      <button
        type="button"
        onclick={onNew}
        title="New session"
        aria-label="New session"
        class="flex h-8 w-8 items-center justify-center rounded-md text-[var(--fg-on-accent)] transition-transform hover:scale-[1.04]"
        style="background: var(--accent); box-shadow: var(--shadow-primary);"
      >
        <Plus class="h-3.5 w-3.5" />
      </button>
      <div class="my-1 h-px w-6" style="background: var(--border-subtle);"></div>
      <div class="flex flex-col items-center gap-1 overflow-y-auto pb-2">
        {#each sessions as s (s.id)}
          {@const u = uiStatus(s)}
          {@const selected = s.id === selectedSessionId}
          <button
            type="button"
            onclick={() => onSelect(s.id)}
            title={s.kind + ' · ' + s.id.slice(0, 8)}
            aria-label={'Select session ' + s.id.slice(0, 8)}
            class="flex h-7 w-7 items-center justify-center rounded-md border transition-colors"
            style="
              border-color: {selected ? 'var(--accent)' : 'transparent'};
              background: {selected ? 'var(--accent-soft)' : 'transparent'};
            "
          >
            <span class="h-2 w-2 rounded-full" style="background: {statusColor(u)};"></span>
          </button>
        {/each}
      </div>
    </div>
  {:else}
    <!-- EXPANDED -->
    <div class="flex flex-col gap-2 border-b px-3 py-3" style="border-color: var(--border-subtle);">
      <div class="flex items-center justify-between gap-2">
        <span class="font-sans text-sm font-semibold" style="color: var(--fg-default);">
          Sessions
        </span>
        <div class="flex items-center gap-1.5">
          <button
            type="button"
            onclick={onNew}
            class="inline-flex items-center gap-1 rounded-md px-2.5 py-1 text-[11px] font-semibold text-[var(--fg-on-accent)] transition-transform hover:scale-[1.02]"
            style="background: var(--accent); box-shadow: var(--shadow-primary);"
          >
            <Plus class="h-3 w-3" />
            New
          </button>
          <button
            type="button"
            onclick={onToggleCollapsed}
            title="Collapse sessions"
            aria-label="Collapse sessions"
            class="flex h-7 w-7 items-center justify-center rounded-md border transition-colors hover:bg-[var(--accent-soft)]"
            style="border-color: var(--border-subtle); color: var(--fg-muted);"
          >
            <ChevronLeft class="h-3 w-3" />
          </button>
        </div>
      </div>
      <div class="flex flex-wrap gap-1.5">
        <span
          class="inline-flex items-center gap-1.5 rounded-full border px-2 py-0.5 text-[10px] font-semibold"
          style="
            color: var(--dot-success);
            background: color-mix(in srgb, var(--dot-success) 10%, transparent);
            border-color: color-mix(in srgb, var(--dot-success) 28%, transparent);
          "
        >
          <span class="h-1.5 w-1.5 rounded-full" style="background: var(--dot-success);"></span>
          {activeCount} active
        </span>
        <span
          class="inline-flex items-center gap-1.5 rounded-full border px-2 py-0.5 text-[10px] font-semibold"
          style="
            color: var(--dot-warn);
            background: color-mix(in srgb, var(--dot-warn) 10%, transparent);
            border-color: color-mix(in srgb, var(--dot-warn) 28%, transparent);
          "
        >
          <span class="h-1.5 w-1.5 rounded-full" style="background: var(--dot-warn);"></span>
          {idleCount} idle
        </span>
      </div>
    </div>

    <!-- Session list -->
    <div class="min-h-0 flex-1 overflow-y-auto">
      {#if sessions.length === 0}
        <div class="flex flex-col items-center gap-2 px-4 py-8 text-center">
          <HarnessIcons name="agents" size={22} class="opacity-30" />
          <p class="text-xs" style="color: var(--fg-muted);">No sessions yet.</p>
          <button
            type="button"
            onclick={onNew}
            class="inline-flex items-center gap-1.5 rounded-md px-3 py-1.5 text-xs font-medium text-[var(--fg-on-accent)]"
            style="background: var(--accent);"
          >
            <Plus class="h-3 w-3" /> Create one
          </button>
        </div>
      {:else}
        <ul class="flex flex-col">
          {#each groups as g (g.root.id)}
            {@const expanded = isExpanded(g.root.id)}
            {@const runningKids = runningChildren(g)}
            {@render sessionCard(g.root, false, g.children.length, expanded, runningKids)}
            {#if expanded && g.children.length > 0}
              {#each g.children as c (c.id)}
                {@render sessionCard(c, true, 0, false, 0)}
              {/each}
            {/if}
          {/each}
        </ul>
      {/if}
    </div>

{#snippet sessionCard(
  s: SessionMeta,
  isChild: boolean,
  childCount: number,
  rootExpanded: boolean,
  runningKids: number
)}
            {@const u = uiStatus(s)}
            {@const k = kindChip(s.kind)}
            {@const selected = s.id === selectedSessionId}
            {@const prog = progressFor(s)}
            <li
              class="group relative"
              style={isChild ? 'padding-left: 18px;' : ''}
            >
              {#if isChild}
                <!-- Tree spine: a vertical line + horizontal arm drawn over
                     the padded-left area so children visually hang from their
                     root. Cheap absolute-positioned divs — no extra DOM per row. -->
                <div
                  class="pointer-events-none absolute left-[10px] top-0 h-full w-px"
                  style="background: var(--border-subtle);"
                ></div>
                <div
                  class="pointer-events-none absolute left-[10px] top-[18px] h-px w-[10px]"
                  style="background: var(--border-subtle);"
                ></div>
              {/if}
              <!-- Destructive affordance — hidden until row hover/selection so
                   it doesn't compete with the primary "select session" tap
                   target. Stops propagation so clicking it doesn't also
                   re-select the card we're about to delete. -->
              <button
                type="button"
                onclick={(e) => handleDelete(e, s)}
                disabled={deleting === s.id}
                aria-label={'Delete session ' + s.id.slice(0, 8)}
                title="Delete session"
                class="absolute right-2 top-2 z-10 flex h-6 w-6 items-center justify-center rounded-md border opacity-0 transition-opacity hover:bg-[color-mix(in_srgb,var(--dot-danger)_15%,transparent)] focus-visible:opacity-100 group-hover:opacity-100 disabled:opacity-50"
                style="border-color: var(--border-subtle); color: var(--dot-danger); background: var(--surface-window);"
              >
                <Trash2 class="h-3 w-3" />
              </button>
              {#if !isChild && childCount > 0}
                <!-- Expand / collapse caret. Lives outside the card button so
                     clicking it doesn't also select the root. -->
                <button
                  type="button"
                  onclick={(e) => {
                    e.stopPropagation();
                    toggleRoot(s.id);
                  }}
                  aria-label={rootExpanded ? 'Collapse children' : 'Expand children'}
                  title={rootExpanded
                    ? `Collapse ${childCount} child${childCount === 1 ? '' : 'ren'}`
                    : `Expand ${childCount} child${childCount === 1 ? '' : 'ren'}`}
                  class="absolute left-1 top-3 z-10 flex h-5 w-5 items-center justify-center rounded transition-colors hover:bg-[var(--accent-soft)]"
                  style="color: var(--fg-muted);"
                >
                  {#if rootExpanded}
                    <ChevronDown class="h-3 w-3" />
                  {:else}
                    <ChevronRight class="h-3 w-3" />
                  {/if}
                </button>
              {/if}
              <button
                type="button"
                onclick={() => onSelect(s.id)}
                class="flex w-full flex-col gap-2 py-3 pr-3.5 text-left transition-colors"
                style="
                  padding-left: {!isChild && childCount > 0 ? '26px' : '14px'};
                  background: {selected ? 'var(--accent-soft)' : 'transparent'};
                  border-left: 2px solid {selected ? 'var(--accent)' : 'transparent'};
                "
                onmouseenter={(e) => {
                  if (!selected)
                    (e.currentTarget as HTMLElement).style.background = 'var(--row-stripe)';
                }}
                onmouseleave={(e) => {
                  if (!selected) (e.currentTarget as HTMLElement).style.background = 'transparent';
                }}
              >
                <!-- Row 1: status dot, title, time -->
                <div class="flex items-center gap-2">
                  <span
                    class="h-2 w-2 shrink-0 rounded-full"
                    style="
                      background: {statusColor(u)};
                      box-shadow: {u === 'active'
                      ? '0 0 0 3px color-mix(in srgb, var(--dot-success) 18%, transparent)'
                      : 'none'};
                    "
                    title={statusLabel(u)}
                  ></span>
                  <span
                    class="flex-1 truncate text-[13px]"
                    style="
                      color: {selected ? 'var(--accent)' : 'var(--fg-default)'};
                      font-weight: {selected ? 600 : 500};
                    "
                  >
                    {s.id ? s.kind + ' · ' + s.id.slice(0, 8) : '(untitled)'}
                  </span>
                  <span class="shrink-0 text-[10px] font-mono" style="color: var(--fg-muted);">
                    {relTime(s.started_at)}
                  </span>
                </div>
                <!-- Row 2: kind chip + role + stats -->
                <div class="flex items-center gap-2">
                  <span
                    class="inline-flex items-center rounded px-1.5 py-0.5 font-mono text-[10px] font-semibold"
                    style="color: {k.color}; background: {k.bg};"
                  >
                    {k.label}
                  </span>
                  {#if s.role === 'zeus-orchestrator'}
                    <span
                      class="inline-flex items-center rounded border px-1.5 py-0.5 font-mono text-[10px] font-bold uppercase"
                      style="color: rgb(74 222 128); border-color: rgba(74 222 128 / 0.5); background: rgba(74 222 128 / 0.1);"
                      title="Zeus orchestrator"
                    >
                      Zeus
                    </span>
                  {:else if s.parent_session_id}
                    <span
                      class="inline-flex items-center rounded border px-1.5 py-0.5 font-mono text-[10px] font-semibold"
                      style="color: var(--fg-default); border-color: var(--border-subtle); background: var(--surface-titlebar);"
                      title={`Child of ${s.parent_session_id}`}
                    >
                      ↳ {s.role ?? 'child'}
                    </span>
                  {:else if s.role}
                    <span
                      class="inline-flex items-center rounded border px-1.5 py-0.5 font-mono text-[10px]"
                      style="color: var(--fg-muted); border-color: var(--border-subtle);"
                    >
                      {s.role}
                    </span>
                  {/if}
                  {#if !isChild && childCount > 0}
                    <span
                      class="inline-flex items-center gap-1 rounded border px-1.5 py-0.5 font-mono text-[10px] font-semibold"
                      style="color: var(--accent); border-color: var(--accent-soft-border); background: var(--accent-soft);"
                      title={`${childCount} child session${childCount === 1 ? '' : 's'} · ${runningKids} running`}
                    >
                      ▾ {runningKids}/{childCount}
                    </span>
                  {/if}
                  {#if s.detected_state && s.detected_state !== 'unknown' && uiStatus(s) === 'active'}
                    {@const ds = s.detected_state}
                    <span
                      class="inline-flex items-center gap-1 rounded border px-1.5 py-0.5 font-mono text-[10px] font-semibold"
                      style="
                        color: {ds === 'working'
                        ? 'rgb(96 165 250)'
                        : ds === 'blocked'
                          ? 'rgb(251 191 36)'
                          : 'rgb(148 163 184)'};
                        border-color: {ds === 'working'
                        ? 'rgba(96 165 250 / 0.4)'
                        : ds === 'blocked'
                          ? 'rgba(251 191 36 / 0.4)'
                          : 'rgba(148 163 184 / 0.3)'};
                        background: {ds === 'working'
                        ? 'rgba(96 165 250 / 0.08)'
                        : ds === 'blocked'
                          ? 'rgba(251 191 36 / 0.1)'
                          : 'transparent'};
                      "
                      title={ds === 'working'
                        ? 'Agent is thinking / running a tool'
                        : ds === 'blocked'
                          ? 'Agent is waiting for input (approval / prompt)'
                          : 'Agent is idle, ready for the next message'}
                    >
                      {ds === 'working' ? '⋯' : ds === 'blocked' ? '⏸' : '✓'}
                      {ds}
                    </span>
                  {/if}
                  <span class="font-mono text-[10px]" style="color: var(--fg-muted);">
                    {uptime(s.started_at)} · {tokensLabel(null)}
                  </span>
                </div>
                <!-- Row 3: progress bar + N/M -->
                <div class="flex items-center gap-2">
                  <div
                    class="h-[3px] flex-1 overflow-hidden rounded-full"
                    style="background: var(--border-input);"
                  >
                    {#if prog.total > 0}
                      <div
                        class="h-full rounded-full transition-[width]"
                        style="
                          width: {prog.pct}%;
                          background: {prog.pct === 100 ? 'var(--dot-success)' : 'var(--accent)'};
                          transition-duration: 300ms;
                        "
                      ></div>
                    {/if}
                  </div>
                  <span class="shrink-0 font-mono text-[10px]" style="color: var(--fg-muted);">
                    {prog.total > 0 ? `${prog.done}/${prog.total}` : '—/—'}
                  </span>
                </div>
              </button>
              <div class="mx-3.5 h-px" style="background: var(--row-divider);"></div>
            </li>
{/snippet}

    <!-- Footer link to the registry -->
    <a
      href="/agents"
      class="flex items-center gap-1.5 border-t px-3 py-2.5 text-[11px] transition-colors hover:bg-[var(--accent-soft)]"
      style="
        border-color: var(--border-subtle);
        color: var(--accent);
      "
    >
      <Bot class="h-3 w-3" />
      Agents registry →
    </a>
  {/if}
</aside>
