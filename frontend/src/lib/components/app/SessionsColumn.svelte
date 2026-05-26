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
  import { Plus, ChevronRight, ChevronLeft, Bot } from '$lib/icons';
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
    collapsed,
    onToggleCollapsed,
    progressByThread = {}
  }: Props = $props();

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
          {#each sessions as s (s.id)}
            {@const u = uiStatus(s)}
            {@const k = kindChip(s.kind)}
            {@const selected = s.id === selectedSessionId}
            {@const prog = progressFor(s)}
            <li>
              <button
                type="button"
                onclick={() => onSelect(s.id)}
                class="flex w-full flex-col gap-2 px-3.5 py-3 text-left transition-colors"
                style="
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
                <!-- Row 2: kind chip + stats -->
                <div class="flex items-center gap-2">
                  <span
                    class="inline-flex items-center rounded px-1.5 py-0.5 font-mono text-[10px] font-semibold"
                    style="color: {k.color}; background: {k.bg};"
                  >
                    {k.label}
                  </span>
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
          {/each}
        </ul>
      {/if}
    </div>

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
