<!--
  Agents view — root page of the harness UI.

  Layout (left to right):
    [ IconRail (rendered by +layout.svelte) ]
    [ SessionsColumn        — middle column, list of sessions      ]
    [ SessionMainView       — terminal + chrome + footer prompt    ]
    [ SessionRightPanel     — Tasks / Agents / Info tabs           ]

  The page also carries the "Dashboard" subheader above the three columns
  (per the redesign brief). The connection chip lives in the top bar; the
  Protocol/Refresh controls live here because they belong to this view.
-->
<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { Button } from '$lib/components/ui/button';
  import { CircleCheck, CircleAlert, Loader2, RefreshCw } from '$lib/icons';
  import SessionsColumn from '$lib/components/app/SessionsColumn.svelte';
  import SessionMainView from '$lib/components/app/SessionMainView.svelte';
  import SessionRightPanel from '$lib/components/app/SessionRightPanel.svelte';
  import NewSessionDialog from '$lib/components/app/NewSessionDialog.svelte';
  import WorkspaceSwitcher from '$lib/components/app/WorkspaceSwitcher.svelte';
  import { health } from '$lib/stores/health.svelte';
  import { sessionsState } from '$lib/stores/session.svelte';
  import { tasksState } from '$lib/stores/tasks.svelte';
  import { taskProgress } from '$lib/sessionDisplay';
  import { api, ApiError, type SessionMeta } from '$lib/api/client';
  import { toast } from 'svelte-sonner';

  // ── Local UI state ────────────────────────────────────────────────────────
  // Persisted in localStorage so reloading the browser keeps the user on the
  // session they were inspecting. Scoped per active backend profile so each
  // workspace remembers its own last session — switching workspaces and
  // coming back leaves you where you were.
  const SELECTED_SESSION_KEY = 'harness.selectedSessionId';
  let activeProfile = $state<string>('default');

  function sessionKey(profile: string): string {
    return `${SELECTED_SESSION_KEY}.${profile}`;
  }
  function readPersistedSession(profile: string): string | null {
    if (typeof window === 'undefined') return null;
    // Fall back to legacy global key (pre-profile) so existing users don't
    // see their selection vanish after the upgrade.
    return localStorage.getItem(sessionKey(profile)) ?? localStorage.getItem(SELECTED_SESSION_KEY);
  }
  function writePersistedSession(profile: string, id: string | null) {
    if (typeof window === 'undefined') return;
    if (id) localStorage.setItem(sessionKey(profile), id);
    else localStorage.removeItem(sessionKey(profile));
  }

  let selectedSessionId = $state<string | null>(null);

  // Resolve active profile on mount (without blocking) then load its scoped
  // selection. Falls back to the legacy global key while the request races.
  onMount(async () => {
    selectedSessionId = readPersistedSession('default');
    try {
      const res = await api.profiles.active();
      activeProfile = res.data.active;
      const scoped = readPersistedSession(activeProfile);
      if (scoped) selectedSessionId = scoped;
    } catch {
      // backend not up; keep legacy fallback.
    }
  });
  let collapsed = $state(false);
  let newDialogOpen = $state(false);
  let lastActiveIds = new Set<string>();

  // Mirror selection into localStorage so reloads land back on the same
  // session card. Scoped by active profile so each workspace remembers its
  // own last session. Cleared when the session disappears (e.g. user killed).
  $effect(() => {
    writePersistedSession(activeProfile, selectedSessionId);
  });

  // ── Derived ───────────────────────────────────────────────────────────────
  // Build a flat session list from the live `sessionsState.threads`. We show
  // both running and exited sessions so the user can review history; the UI
  // status helper colors them appropriately.
  const allSessions = $derived.by<SessionMeta[]>(() => {
    const out: SessionMeta[] = [];
    for (const t of sessionsState.threads) {
      if (Array.isArray(t.sessions)) {
        for (const s of t.sessions) out.push(s);
      }
    }
    // Newest first — `started_at` is ISO so lexical sort works.
    out.sort((a, b) => (a.started_at < b.started_at ? 1 : -1));
    return out;
  });

  const selectedSession = $derived<SessionMeta | null>(
    selectedSessionId ? (allSessions.find((s) => s.id === selectedSessionId) ?? null) : null
  );

  const selectedThread = $derived(
    selectedSession
      ? (sessionsState.threads.find((t) => t.id === selectedSession.thread_id) ?? null)
      : null
  );

  const readiness = $derived(selectedThread?.readiness ?? null);

  // Per-thread task progress — only computed for the currently-selected
  // thread (the tasks SSE only subscribes to one thread at a time). All
  // other threads show "—/—" until F3 wires a multi-thread progress store.
  const progressByThread = $derived.by<Record<string, ReturnType<typeof taskProgress>>>(() => {
    const map: Record<string, ReturnType<typeof taskProgress>> = {};
    if (tasksState.threadId) {
      map[tasksState.threadId] = taskProgress(tasksState.items);
    }
    return map;
  });

  // ── Effects ───────────────────────────────────────────────────────────────
  // Auto-select the first session once data lands. Also auto-select any
  // newly-created session (so the New-session modal "feels" connected even
  // though it navigates away today).
  $effect(() => {
    if (!sessionsState.loaded) return;
    if (allSessions.length === 0) {
      selectedSessionId = null;
      return;
    }
    // Preserve the persisted/current selection if it still exists in the
    // session list — only auto-pick the newest when the previous selection
    // is gone (killed) or nothing was selected yet.
    if (!selectedSessionId || !allSessions.some((s) => s.id === selectedSessionId)) {
      selectedSessionId = allSessions[0].id;
    }
  });

  // Detect freshly-spawned sessions and auto-select them.
  $effect(() => {
    const ids = new Set(allSessions.map((s) => s.id));
    for (const id of ids) {
      if (!lastActiveIds.has(id) && lastActiveIds.size > 0) {
        selectedSessionId = id;
        break;
      }
    }
    lastActiveIds = ids;
  });

  // Drive the tasks store from the selected session's thread.
  $effect(() => {
    const tid = selectedSession?.thread_id ?? null;
    if (tid) {
      tasksState.start(tid);
    } else {
      tasksState.stop();
    }
  });

  // ── Polling ───────────────────────────────────────────────────────────────
  // The IconRail also polls sessions, but it may be unmounted later (when the
  // user navigates to a different route). Keeping a dedicated poller here
  // makes the home page self-sufficient.
  const POLL_MS = 5_000;
  let timer: ReturnType<typeof setInterval> | null = null;
  let controller: AbortController | null = null;

  function refreshSessions() {
    controller?.abort();
    controller = new AbortController();
    sessionsState.refresh(controller.signal);
  }

  onMount(() => {
    refreshSessions();
    timer = setInterval(refreshSessions, POLL_MS);
  });

  onDestroy(() => {
    if (timer) clearInterval(timer);
    controller?.abort();
    tasksState.stop();
  });

  function onSelect(id: string) {
    selectedSessionId = id;
  }

  function onNew() {
    newDialogOpen = true;
  }

  function onToggleCollapsed() {
    collapsed = !collapsed;
  }

  async function onSessionKilled() {
    // Refresh the list so the killed session updates its status badge.
    refreshSessions();
  }

  /// Hard-delete a session from the Agents panel (kebab → Delete).
  /// Calls DELETE /api/sessions/:id which kills the PTY and forgets the
  /// session in the Manager so subsequent polls see it gone. We optimistically
  /// drop the local selection so the right panel doesn't flash stale meta
  /// while waiting for the next poll.
  async function onSessionDelete(sid: string) {
    try {
      await api.sessions.kill(sid);
      if (selectedSessionId === sid) {
        selectedSessionId = null;
      }
      refreshSessions();
    } catch (err) {
      const msg =
        err instanceof ApiError
          ? ((err.body as { error?: string } | undefined)?.error ?? err.message)
          : err instanceof Error
            ? err.message
            : String(err);
      toast.error(`Delete failed: ${msg}`);
    }
  }

  async function onSessionReplaced(newId: string) {
    refreshSessions();
    selectedSessionId = newId;
  }

  async function refreshReadiness() {
    if (!selectedThread) return;
    try {
      await api.threads.recalculateReadiness(selectedThread.id, selectedSession?.cwd ?? undefined);
      refreshSessions();
      toast.success('Readiness refreshed');
    } catch (err) {
      const msg =
        err instanceof ApiError
          ? ((err.body as { error?: string } | undefined)?.error ?? err.message)
          : err instanceof Error
            ? err.message
            : String(err);
      toast.error(`Readiness failed: ${msg}`);
    }
  }
</script>

<div class="flex h-full min-h-0 flex-col">
  <!-- Dashboard subheader -->
  <header
    class="flex h-14 shrink-0 items-center justify-between gap-4 border-b px-5"
    style="background: var(--surface-window); border-color: var(--border-subtle);"
  >
    <div>
      <h1 class="font-serif text-xl font-semibold tracking-tight" style="color: var(--fg-default);">
        Dashboard
      </h1>
      <p class="text-xs" style="color: var(--fg-muted);">
        Backend health, active sessions, and shell wiring.
      </p>
    </div>
    <div class="flex items-center gap-2">
      <WorkspaceSwitcher />
      {#if health.protocolVersion}
        <span
          class="inline-flex items-center gap-1.5 rounded-md border px-2.5 py-1 text-[11px] font-medium"
          style="
            border-color: var(--accent-soft-border);
            background: var(--accent-soft);
            color: var(--accent);
          "
          title="X-Protocol-Version header from backend"
        >
          <CircleCheck class="h-3 w-3" />
          Protocol v{health.protocolVersion}
        </span>
      {:else}
        <span
          class="inline-flex items-center gap-1.5 rounded-md border px-2.5 py-1 text-[11px] font-medium"
          style="
            border-color: var(--border-subtle);
            background: var(--surface-panel);
            color: var(--fg-muted);
          "
        >
          <CircleAlert class="h-3 w-3" />
          Protocol unknown
        </span>
      {/if}
      <Button
        variant="outline"
        size="sm"
        onclick={() => health.refresh()}
        disabled={health.state === 'connecting'}
      >
        {#if health.state === 'connecting'}
          <Loader2 class="h-3.5 w-3.5 animate-spin" />
        {:else}
          <RefreshCw class="h-3.5 w-3.5" />
        {/if}
        Refresh
      </Button>
    </div>
  </header>

  {#if selectedThread && readiness}
    <div
      class="flex shrink-0 items-center justify-between gap-4 border-b px-5 py-2"
      style="
        border-color: var(--border-subtle);
        background: {readiness.status === 'blocked'
        ? 'color-mix(in srgb, var(--dot-danger) 10%, var(--surface-panel))'
        : readiness.status === 'ready_with_warnings'
          ? 'color-mix(in srgb, var(--dot-warn) 10%, var(--surface-panel))'
          : 'var(--surface-panel)'};
      "
    >
      <div class="flex min-w-0 items-center gap-2 text-xs">
        {#if readiness.status === 'ready'}
          <CircleCheck class="h-4 w-4 shrink-0" style="color: var(--dot-success);" />
        {:else}
          <CircleAlert
            class="h-4 w-4 shrink-0"
            style="color: {readiness.status === 'blocked'
              ? 'var(--dot-danger)'
              : 'var(--dot-warn)'};"
          />
        {/if}
        <span class="font-medium" style="color: var(--fg-default);">
          {readiness.status === 'blocked'
            ? 'Blocked'
            : readiness.status === 'ready_with_warnings'
              ? 'Ready with warnings'
              : 'Ready'}
        </span>
        <span class="truncate" style="color: var(--fg-muted);">
          Mode {readiness.suggested_execution_mode} · Autonomy {selectedThread.autonomy_profile ??
            'assisted'}
          {#if readiness.blocking.length > 0}
            · {readiness.blocking[0].message}
          {:else if readiness.warnings.length > 0}
            · {readiness.warnings.length} warning{readiness.warnings.length === 1 ? '' : 's'}
          {:else}
            · {readiness.cwd}
          {/if}
        </span>
      </div>
      <Button variant="outline" size="sm" onclick={refreshReadiness}>
        <RefreshCw class="h-3.5 w-3.5" />
        Recheck
      </Button>
    </div>
  {/if}

  <!-- Three-column body -->
  <div class="flex min-h-0 flex-1">
    <SessionsColumn
      sessions={allSessions}
      {selectedSessionId}
      {onSelect}
      {onNew}
      onDelete={onSessionDelete}
      {collapsed}
      {onToggleCollapsed}
      {progressByThread}
    />
    <SessionMainView session={selectedSession} {onSessionReplaced} {onSessionKilled} />
    <SessionRightPanel
      session={selectedSession}
      onChildSelected={(cid) => (selectedSessionId = cid)}
    />
  </div>
</div>

<NewSessionDialog
  bind:open={newDialogOpen}
  onCreated={({ sessionId }) => {
    refreshSessions();
    selectedSessionId = sessionId;
  }}
/>
