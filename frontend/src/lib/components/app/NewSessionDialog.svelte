<script lang="ts">
  import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogDescription,
    DialogFooter
  } from '$lib/components/ui/dialog';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { Label } from '$lib/components/ui/label';
  import {
    api,
    ApiError,
    type AutonomyProfile,
    type CapabilityProfile,
    type CurrentRepoReport,
    type SessionKind
  } from '$lib/api/client';
  import { goto } from '$app/navigation';
  import { toast } from 'svelte-sonner';
  import { Loader2 } from '$lib/icons';

  interface Props {
    open: boolean;
    /** Optional existing thread to attach the session to. */
    threadId?: string | null;
    /**
     * Called after the session is created with the new ids. When provided,
     * the dialog does NOT navigate away — the caller is expected to update
     * its own selection (e.g. the redesigned Agents view). When omitted,
     * the dialog falls back to navigating to the dedicated session route
     * so existing callers (e.g. the rail's "+" button outside this view)
     * keep their behaviour.
     */
    onCreated?: (info: { threadId: string; sessionId: string }) => void;
  }

  let { open = $bindable(false), threadId = null, onCreated }: Props = $props();

  let kind = $state<SessionKind>('claude');
  let autonomy = $state<AutonomyProfile>('assisted');
  let cwd = $state<string>('');
  let submitting = $state<boolean>(false);
  let error = $state<string | null>(null);
  let repoReport = $state<CurrentRepoReport | null>(null);
  let repoLookup = $state<'idle' | 'loading' | 'error'>('idle');
  let repoMode = $state<'resume' | 'context' | 'none'>('context');
  let capabilityProfile = $state<Extract<CapabilityProfile, 'auto' | 'none'>>('auto');

  function reset() {
    kind = 'claude';
    autonomy = 'assisted';
    cwd = '';
    error = null;
    repoReport = null;
    repoLookup = 'idle';
    repoMode = 'context';
    capabilityProfile = 'auto';
    submitting = false;
  }

  async function lookupRepo() {
    const requestedCwd = cwd.trim();
    repoReport = null;
    repoMode = 'context';
    if (!requestedCwd) {
      repoLookup = 'idle';
      return;
    }
    repoLookup = 'loading';
    try {
      const res = await api.repos.current(requestedCwd);
      repoReport = res.data.detected ? res.data : null;
      repoLookup = 'idle';
      if (repoReport?.repo?.last_thread_id) repoMode = 'resume';
      else if (repoReport) repoMode = 'context';
    } catch {
      repoLookup = 'error';
    }
  }

  /**
   * Estimate the (cols, rows) the freshly-mounted terminal will end up with
   * so the backend can open the PTY at the right size from the start.
   *
   * The terminal sits in the middle column of the Agents view (between the
   * left sessions list and the right panel) inside a "macOS window" frame
   * with a fake titlebar and a footer prompt. We subtract those chrome
   * widths/heights from `window.inner*` and divide by approximate character
   * metrics for the 13px JetBrains Mono / Fira Code stack TerminalView uses.
   *
   * The estimate is intentionally rough — TerminalView calls `fit()` on
   * mount and POSTs the exact size to `/resize` a moment later, so any
   * slop here is corrected on the very next frame. The goal is just to be
   * close enough that the first frame the TUI renders is not mangled.
   */
  function estimateInitialSize(): { cols: number; rows: number } {
    const COL_PX = 7.7; // ~width of a monospace char at 13px
    const ROW_PX = 17; // ~line height at 13px
    const SIDEBAR_PX = 280; // left sessions list
    const RIGHT_PANEL_PX = 360; // right panel (tasks/agents)
    const HORIZONTAL_CHROME = 64; // frame border + paddings
    const VERTICAL_CHROME = 180; // outer header + titlebar + footer + paddings
    const w = typeof window !== 'undefined' ? window.innerWidth : 1280;
    const h = typeof window !== 'undefined' ? window.innerHeight : 800;
    const cols = Math.max(
      40,
      Math.min(300, Math.floor((w - SIDEBAR_PX - RIGHT_PANEL_PX - HORIZONTAL_CHROME) / COL_PX))
    );
    const rows = Math.max(10, Math.min(120, Math.floor((h - VERTICAL_CHROME) / ROW_PX)));
    return { cols, rows };
  }

  async function submit(ev: SubmitEvent) {
    ev.preventDefault();
    if (submitting) return;
    submitting = true;
    error = null;
    try {
      let tid = threadId;
      const requestedCwd = cwd.trim() ? cwd.trim() : undefined;
      if (!tid && repoMode === 'resume') {
        tid =
          repoReport?.continuity?.recommended_thread_id ??
          repoReport?.repo?.last_thread_id ??
          null;
      }
      if (!tid) {
        const t = await api.threads.create(undefined, autonomy, requestedCwd);
        tid = t.data.id;
        if (requestedCwd) {
          try {
            await api.threads.recalculateReadiness(tid, requestedCwd);
          } catch {
            // Session creation is still allowed; the banner will show the
            // previous readiness state and the user can recalculate later.
          }
        }
      }
      const { cols, rows } = estimateInitialSize();
      const res = await api.sessions.create(tid, {
        kind,
        cwd: requestedCwd,
        include_project_context: repoMode !== 'none',
        capability_profile: capabilityProfile,
        cols,
        rows
      });
      open = false;
      reset();
      if (onCreated) {
        onCreated({ threadId: tid, sessionId: res.data.session_id });
      } else {
        await goto(`/threads/${tid}/sessions/${res.data.session_id}`);
      }
    } catch (err) {
      if (err instanceof ApiError) {
        const body = err.body as { error?: string; install_hint?: string } | undefined;
        if (err.status === 400 && body?.install_hint) {
          error = body.install_hint;
          toast.error(`${body.error ?? 'Binary not found'}`, {
            description: body.install_hint
          });
        } else {
          error = body?.error ?? err.message;
          toast.error(error ?? 'Failed to create session');
        }
      } else {
        error = err instanceof Error ? err.message : String(err);
        toast.error(error);
      }
    } finally {
      submitting = false;
    }
  }

  function onOpenChange(v: boolean) {
    open = v;
    if (!v) reset();
  }
</script>

<Dialog bind:open {onOpenChange}>
  <DialogContent class="sm:max-w-md">
    <DialogHeader>
      <DialogTitle>New session</DialogTitle>
      <DialogDescription>
        Pick a CLI (or <strong>Zeus</strong> to let the harness orchestrate multiple).
      </DialogDescription>
    </DialogHeader>
    <form class="mt-4 flex flex-col gap-4" onsubmit={submit}>
      <div class="flex flex-col gap-2">
        <Label for="kind">Agent</Label>
        <div class="grid grid-cols-3 gap-2" role="radiogroup" id="kind">
          {#each ['claude', 'codex', 'cursor', 'antigravity', 'zeus'] as const as opt (opt)}
            <button
              type="button"
              role="radio"
              aria-checked={kind === opt}
              class="rounded-md border px-3 py-2 text-sm transition-colors {kind === opt
                ? opt === 'zeus'
                  ? 'border-emerald-500/70 bg-emerald-500/10 text-emerald-400 font-medium'
                  : 'border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)] font-medium'
                : 'border-[var(--border-input)] bg-[var(--surface-titlebar)] text-[var(--fg-muted)] hover:text-[var(--fg-default)]'}"
              onclick={() => (kind = opt)}
              title={opt === 'zeus'
                ? 'Zeus orchestrates the other CLIs by role (Codex=primary/implementation/tests, Claude=architecture fallback, Cursor=IDE, Antigravity=cloud). Falls back to Claude on quota/error. Today it runs a Codex PTY with the Zeus briefing; real multi-CLI delegation lands with F3.'
                : opt}
            >
              {opt === 'zeus' ? 'Zeus' : opt}
            </button>
          {/each}
        </div>
        {#if kind === 'zeus'}
          <p class="text-[11px] leading-relaxed text-emerald-400/90">
            Zeus is an orchestrator session — it plans and delegates work across the CLIs by role,
            falling back to Claude on quota/error. Under the hood it runs a Codex PTY with the Zeus
            orchestrator briefing; real multi-CLI worker spawning lands with F3.
          </p>
        {/if}
      </div>
      <div class="flex flex-col gap-2">
        <Label for="cwd">Working directory (optional)</Label>
        <Input
          id="cwd"
          bind:value={cwd}
          placeholder="/path/to/project"
          autocomplete="off"
          onblur={lookupRepo}
        />
        <p class="text-xs text-[var(--fg-muted)]">
          Defaults to the backend process cwd when empty.
        </p>
        {#if repoLookup === 'loading'}
          <p class="text-xs text-[var(--fg-muted)]">Checking project history…</p>
        {:else if repoReport?.identity}
          <div
            class="rounded-md border px-3 py-2 text-xs"
            style="border-color: var(--border-subtle); background: var(--surface-titlebar); color: var(--fg-muted);"
          >
            <div class="font-medium" style="color: var(--fg-default);">
              {repoReport.repo ? 'Known project' : 'Git project detected'}
            </div>
            <div class="mt-1 truncate">{repoReport.identity.canonical_path}</div>
            {#if repoReport.repo?.last_thread_id && !threadId}
              <div class="mt-2 grid gap-2">
                <button
                  type="button"
                  class="rounded border px-2 py-1 text-left transition-colors {repoMode === 'resume'
                    ? 'border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)]'
                    : 'border-[var(--border-input)] text-[var(--fg-muted)] hover:text-[var(--fg-default)]'}"
                  onclick={() => (repoMode = 'resume')}
                >
                  Resume last thread
                </button>
                <button
                  type="button"
                  class="rounded border px-2 py-1 text-left transition-colors {repoMode === 'context'
                    ? 'border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)]'
                    : 'border-[var(--border-input)] text-[var(--fg-muted)] hover:text-[var(--fg-default)]'}"
                  onclick={() => (repoMode = 'context')}
                >
                  Start fresh with project context
                </button>
                <button
                  type="button"
                  class="rounded border px-2 py-1 text-left transition-colors {repoMode === 'none'
                    ? 'border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)]'
                    : 'border-[var(--border-input)] text-[var(--fg-muted)] hover:text-[var(--fg-default)]'}"
                  onclick={() => (repoMode = 'none')}
                >
                  Start completely fresh
                </button>
              </div>
              {#if repoReport.continuity?.last_goal}
                <div class="mt-2 line-clamp-2">
                  Last goal: {repoReport.continuity.last_goal}
                </div>
              {/if}
              {#if repoReport.continuity?.blockers?.length}
                <div class="mt-1 line-clamp-2">
                  Blocked: {repoReport.continuity.blockers[0]}
                </div>
              {/if}
            {:else if !threadId}
              <div class="mt-2 grid gap-2">
                <button
                  type="button"
                  class="rounded border px-2 py-1 text-left transition-colors {repoMode === 'context'
                    ? 'border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)]'
                    : 'border-[var(--border-input)] text-[var(--fg-muted)] hover:text-[var(--fg-default)]'}"
                  onclick={() => (repoMode = 'context')}
                >
                  Start with project context
                </button>
                <button
                  type="button"
                  class="rounded border px-2 py-1 text-left transition-colors {repoMode === 'none'
                    ? 'border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)]'
                    : 'border-[var(--border-input)] text-[var(--fg-muted)] hover:text-[var(--fg-default)]'}"
                  onclick={() => (repoMode = 'none')}
                >
                  Start completely fresh
                </button>
              </div>
            {/if}
          </div>
        {/if}
      </div>
      <div class="flex flex-col gap-2">
        <Label for="autonomy">Autonomy</Label>
        <div class="grid grid-cols-4 gap-2" role="radiogroup" id="autonomy">
          {#each ['manual', 'assisted', 'autonomous', 'ci'] as const as opt (opt)}
            <button
              type="button"
              role="radio"
              aria-checked={autonomy === opt}
              class="rounded-md border px-2 py-2 text-xs capitalize transition-colors {autonomy ===
              opt
                ? 'border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)] font-medium'
                : 'border-[var(--border-input)] bg-[var(--surface-titlebar)] text-[var(--fg-muted)] hover:text-[var(--fg-default)]'}"
              onclick={() => (autonomy = opt)}
              title={opt === 'manual'
                ? 'Ask before sensitive actions.'
                : opt === 'assisted'
                  ? 'Default: assume reversible choices, ask for real blockers.'
                  : opt === 'autonomous'
                    ? 'Run without interruption inside policy and budget.'
                    : 'Headless: do not ask; fail with a structured blocker.'}
            >
              {opt}
            </button>
          {/each}
        </div>
      </div>
      <div class="flex flex-col gap-2">
        <Label for="capability-profile">Capabilities</Label>
        <div class="grid grid-cols-2 gap-2" role="radiogroup" id="capability-profile">
          {#each ['auto', 'none'] as const as opt (opt)}
            <button
              type="button"
              role="radio"
              aria-checked={capabilityProfile === opt}
              class="rounded-md border px-2 py-2 text-xs transition-colors {capabilityProfile ===
              opt
                ? 'border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)] font-medium'
                : 'border-[var(--border-input)] bg-[var(--surface-titlebar)] text-[var(--fg-muted)] hover:text-[var(--fg-default)]'}"
              onclick={() => (capabilityProfile = opt)}
              title={opt === 'auto'
                ? 'Default smart loading: Harness MCP when useful, Crawl4AI only by heuristic.'
                : 'Lightweight local mode: skip Harness MCP injection and use only built-in agent tools.'}
            >
              {opt === 'auto' ? 'Auto' : 'Light'}
            </button>
          {/each}
        </div>
      </div>
      {#if error}
        <p
          class="rounded-md border px-3 py-2 text-xs"
          style="border-color: color-mix(in srgb, var(--dot-danger) 35%, transparent); background: color-mix(in srgb, var(--dot-danger) 10%, transparent); color: var(--dot-danger);"
        >
          {error}
        </p>
      {/if}
      <DialogFooter>
        <Button
          type="button"
          variant="outline"
          onclick={() => (open = false)}
          disabled={submitting}
        >
          Cancel
        </Button>
        <Button type="submit" disabled={submitting}>
          {#if submitting}
            <Loader2 class="h-4 w-4 animate-spin" />
          {/if}
          Create
        </Button>
      </DialogFooter>
    </form>
  </DialogContent>
</Dialog>
