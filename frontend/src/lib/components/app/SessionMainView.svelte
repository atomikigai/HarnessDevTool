<!--
  SessionMainView — center panel of the Agents view.

  Layout:
    • Outer header (status dot + title + chip + stats + Stop/Restart)
    • "macOS window" framed body with:
        ▸ inner title line (kind · cwd · tokens)
        ▸ TerminalView (embedded mode — no built-in header)
    • Footer with attach (visual) + free-text input that forwards to PTY

  Notes:
    • The terminal owns its own SSE / xterm lifecycle; we just embed it.
    • The footer input is an alternative to clicking into the xterm — on
      Enter we POST the same bytes to /sessions/<sid>/input. The xterm
      will echo them back through SSE just like any other key press.
    • "Restart" is a best-effort: kill the current session, then create
      a new one with the same kind+cwd, and notify the parent so it can
      reselect.
-->
<script lang="ts">
  import { api, ApiError, type SessionMeta } from '$lib/api/client';
  import { Paperclip, RotateCcw, Send } from '$lib/icons';
  import { toast } from 'svelte-sonner';
  import TerminalView from './TerminalView.svelte';
  import HarnessIcons from './HarnessIcons.svelte';
  import {
    kindChip,
    statusColor,
    tokensLabel,
    uiStatus,
    uptime,
    isTerminal
  } from '$lib/sessionDisplay';

  interface Props {
    session: SessionMeta | null;
    /** Notified after a Restart with the new session id so the parent
     *  can update its selection. */
    onSessionReplaced?: (newSessionId: string) => void;
    /** Notified after a Kill so the parent can refresh its list. */
    onSessionKilled?: (sessionId: string) => void;
  }

  let { session, onSessionReplaced, onSessionKilled }: Props = $props();

  let input = $state('');
  let sending = $state(false);
  let stopping = $state(false);
  let restarting = $state(false);

  const encoder = new TextEncoder();

  const k = $derived(session ? kindChip(session.kind) : null);
  const u = $derived(uiStatus(session));
  const stopped = $derived(session ? isTerminal(session.status) : true);

  async function sendInput() {
    if (!session || !input || sending || stopped) return;
    sending = true;
    try {
      await api.sessions.input(session.id, encoder.encode(input + '\r'));
      input = '';
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      toast.error(`Send failed: ${msg}`);
    } finally {
      sending = false;
    }
  }

  function onInputKeydown(ev: KeyboardEvent) {
    if (ev.key === 'Enter' && !ev.shiftKey) {
      ev.preventDefault();
      void sendInput();
    }
  }

  async function onStop() {
    if (!session || stopping) return;
    stopping = true;
    try {
      await api.sessions.kill(session.id);
      toast.success('Session stopped');
      onSessionKilled?.(session.id);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      toast.error(`Stop failed: ${msg}`);
    } finally {
      stopping = false;
    }
  }

  async function onRestart() {
    if (!session || restarting) return;
    restarting = true;
    const { thread_id, kind, cwd, id: oldId } = session;
    try {
      // Best-effort kill first — ignore errors (session may already be dead).
      try {
        await api.sessions.kill(oldId);
      } catch (err) {
        if (!(err instanceof ApiError) || err.status !== 404) {
          console.warn('restart: kill failed', err);
        }
      }
      const res = await api.sessions.create(thread_id, {
        kind,
        cwd: cwd ?? undefined
      });
      toast.success('Session restarted');
      onSessionReplaced?.(res.data.session_id);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      toast.error(`Restart failed: ${msg}`);
    } finally {
      restarting = false;
    }
  }
</script>

<section
  class="flex h-full min-w-0 flex-1 flex-col overflow-hidden"
  style="background: var(--surface-canvas);"
>
  {#if !session}
    <!-- Empty state -->
    <div class="flex h-full flex-col items-center justify-center gap-3 px-6 text-center">
      <HarnessIcons name="agents" size={28} class="opacity-30" />
      <p class="text-sm font-medium" style="color: var(--fg-default); opacity: 0.5;">
        Select a session or create a new one
      </p>
      <p class="text-xs" style="color: var(--fg-muted);">The terminal output will appear here.</p>
    </div>
  {:else}
    <!-- Outer header -->
    <header
      class="flex h-12 shrink-0 items-center gap-3 border-b px-4"
      style="background: var(--surface-window); border-color: var(--border-subtle);"
    >
      <span
        class="h-2 w-2 shrink-0 rounded-full"
        style="
          background: {statusColor(u)};
          box-shadow: {u === 'active'
          ? '0 0 0 3px color-mix(in srgb, var(--dot-success) 18%, transparent)'
          : 'none'};
        "
        title={session.status}
      ></span>
      <span
        class="truncate text-sm font-semibold"
        style="color: var(--fg-default);"
        title={session.id}
      >
        {session.kind} · {session.id.slice(0, 8)}
      </span>
      {#if k}
        <span
          class="inline-flex items-center rounded px-1.5 py-0.5 font-mono text-[10px] font-semibold"
          style="color: {k.color}; background: {k.bg};"
        >
          {k.label}
        </span>
      {/if}
      <div class="ml-auto flex shrink-0 items-center gap-3">
        <span class="font-mono text-[11px]" style="color: var(--fg-muted);">
          {uptime(session.started_at)} · {tokensLabel(null)}
        </span>
        <div class="flex gap-1.5">
          <button
            type="button"
            onclick={onStop}
            disabled={stopping || stopped}
            class="rounded-md border px-3 py-1 text-[11px] font-semibold transition-colors disabled:opacity-50"
            style="
              border-color: color-mix(in srgb, var(--dot-danger) 40%, transparent);
              color: var(--dot-danger);
              background: color-mix(in srgb, var(--dot-danger) 8%, transparent);
            "
          >
            Stop
          </button>
          <button
            type="button"
            onclick={onRestart}
            disabled={restarting}
            class="inline-flex items-center gap-1 rounded-md border px-3 py-1 text-[11px] font-semibold transition-colors disabled:opacity-50"
            style="
              border-color: var(--border-subtle);
              color: var(--fg-breadcrumb);
              background: var(--surface-panel);
            "
          >
            <RotateCcw class="h-3 w-3" />
            Restart
          </button>
        </div>
      </div>
    </header>

    <!-- Window frame (macOS dots + cwd line + terminal) -->
    <div class="flex min-h-0 flex-1 flex-col p-3">
      <div
        class="flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border"
        style="
          background: #0b1220;
          border-color: var(--border-subtle);
          box-shadow: var(--shadow-card);
        "
      >
        <!-- "Title bar" — purely decorative -->
        <div
          class="flex h-8 shrink-0 items-center gap-2 border-b px-3"
          style="
            background: rgba(0, 0, 0, 0.3);
            border-color: rgba(255, 255, 255, 0.06);
          "
        >
          <div class="flex gap-1.5">
            <span class="h-2.5 w-2.5 rounded-full" style="background: #ed6a5e;"></span>
            <span class="h-2.5 w-2.5 rounded-full" style="background: #f4bf4f;"></span>
            <span class="h-2.5 w-2.5 rounded-full" style="background: #61c554;"></span>
          </div>
          <span
            class="ml-2 truncate font-mono text-[10.5px]"
            style="color: #94a3b8;"
            title={session.cwd ?? ''}
          >
            {k?.label ?? session.kind} · {session.cwd ?? '(default cwd)'}
          </span>
          <span class="ml-auto shrink-0 font-mono text-[10px]" style="color: #4a5568;">
            {tokensLabel(null)}
          </span>
        </div>

        <!-- xterm — header-less variant -->
        <div class="min-h-0 flex-1">
          <TerminalView threadId={session.thread_id} sessionId={session.id} embedded />
        </div>

        <!-- Footer prompt -->
        <div
          class="flex shrink-0 items-center gap-2 border-t px-3 py-2"
          style="
            background: rgba(0, 0, 0, 0.25);
            border-color: rgba(255, 255, 255, 0.06);
          "
        >
          <button
            type="button"
            class="flex h-7 w-7 shrink-0 items-center justify-center rounded-md border opacity-60"
            style="
              border-color: rgba(255, 255, 255, 0.1);
              background: rgba(255, 255, 255, 0.05);
              color: #94a3b8;
            "
            title="Attach files (coming soon)"
            aria-label="Attach files"
            disabled
          >
            <Paperclip class="h-3.5 w-3.5" />
          </button>
          <div
            class="flex flex-1 items-center gap-2 rounded-md border px-3 py-1"
            style="
              border-color: rgba(255, 255, 255, 0.08);
              background: rgba(255, 255, 255, 0.04);
            "
          >
            <span style="color: var(--accent); font-weight: 700;">❯</span>
            <input
              type="text"
              bind:value={input}
              onkeydown={onInputKeydown}
              placeholder="Message or command…"
              disabled={stopped}
              class="flex-1 bg-transparent text-sm outline-none placeholder:text-[#4a5568]"
              style="color: #e2e8f0; font-family: var(--font-mono); font-size: 13px;"
            />
            {#if input.trim().length > 0}
              <button
                type="button"
                onclick={sendInput}
                disabled={sending || stopped}
                class="inline-flex items-center gap-1 rounded px-2.5 py-0.5 text-[11px] font-semibold text-white disabled:opacity-50"
                style="background: var(--accent);"
              >
                <Send class="h-3 w-3" /> Send
              </button>
            {/if}
          </div>
        </div>
      </div>
    </div>
  {/if}
</section>
