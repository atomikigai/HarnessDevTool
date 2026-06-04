<!--
  TaskDetail — right-hand drawer rendering one Task as structured sections.
  Subcards: Acceptance, Artifacts, Dependencies, History. Transition buttons
  are visible per current status. The raw-edit panel is a monospace textarea
  with a soft TOML-like shape preview; the real CodeMirror editor was skipped
  for F2 to keep the bundle lean — see deuda in the PR.
-->
<script lang="ts">
  import { Button } from '$lib/components/ui/button';
  import TaskStatusBadge from './TaskStatusBadge.svelte';
  import AbandonDialog from './AbandonDialog.svelte';
  import PauseDialog from './PauseDialog.svelte';
  import {
    X,
    Pause,
    Play,
    CheckCircle2,
    XCircle,
    Edit3,
    Clock,
    Tag,
    User,
    GitBranch,
    Link2,
    Loader2
  } from '$lib/icons';
  import type { Task, Artifact, AcceptanceCheck, PatchTaskRequest } from '$lib/api/models/task';
  import { api, ApiError, type Handoff } from '$lib/api/client';
  import { toast } from 'svelte-sonner';
  import { formatDistanceToNow } from 'date-fns';
  import { patchTaskSchema, safeParse } from '$lib/api/schemas/task';

  interface Props {
    threadId: string;
    task: Task;
    onClose: () => void;
    onSelect: (taskId: string) => void;
    onChange?: () => void;
  }

  let { threadId, task, onClose, onSelect, onChange }: Props = $props();

  let busy = $state(false);
  let editingRaw = $state(false);
  let rawDraft = $state('');
  let rawError = $state<string | null>(null);
  let abandonOpen = $state(false);
  let pauseOpen = $state(false);
  let handoffs = $state<Handoff[]>([]);
  let handoffsLoading = $state(false);

  const allVerified = $derived(
    task.acceptance.checks.length > 0 && task.acceptance.checks.every((c) => c.verified)
  );
  const hasBrief = $derived(
    !!task.brief &&
      !!(
        task.brief.objective.trim() ||
        task.brief.context.trim() ||
        task.brief.tasks.length ||
        task.brief.rules.length ||
        task.brief.expected_result.trim()
      )
  );
  const artifactMetadata = $derived<Artifact[]>(task.artifacts.metadata ?? []);
  const hasLegacyArtifacts = $derived(
    task.artifacts.files.length > 0 || task.artifacts.turns.length > 0 || !!task.artifacts.diff
  );
  function schedulerDecisionLabel(decision: string): string {
    return decision
      .split('_')
      .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
      .join(' ');
  }

  const reasonItems = $derived.by(() => {
    const notes = task.notes;
    const items: { label: string; value: string; urgent?: boolean }[] = [];
    if (notes.blocked_reason?.trim()) {
      items.push({ label: 'Blocked', value: notes.blocked_reason });
    }
    const pauseReason = notes.paused_reason?.trim() || notes.why_paused?.trim();
    if (pauseReason) {
      items.push({ label: 'Paused', value: pauseReason });
    }
    if (notes.rejected_reason?.trim()) {
      items.push({ label: 'Rejected', value: notes.rejected_reason });
    }
    if (notes.last_failure?.trim()) {
      items.push({ label: 'Last failure', value: notes.last_failure, urgent: true });
    }
    if (notes.why_abandoned?.trim()) {
      items.push({ label: 'Abandoned', value: notes.why_abandoned });
    }
    if (notes.needs_human) {
      items.push({ label: 'Needs human', value: 'Human input required', urgent: true });
    }
    const scheduler = task.scheduler_explanation;
    if (scheduler?.reason?.trim()) {
      items.push({
        label: `Scheduler ${schedulerDecisionLabel(scheduler.decision)}`,
        value: scheduler.reason,
        urgent: ['assignment_skipped', 'cooldown_skipped', 'evaluator_skipped', 'claim_busy'].includes(
          scheduler.decision
        )
      });
    }
    return items;
  });

  async function patch(body: PatchTaskRequest, optimisticMsg?: string) {
    const validation = safeParse(patchTaskSchema, body);
    if (!validation.ok) {
      toast.error(`Validation failed: ${validation.errors.join(', ')}`);
      return;
    }
    busy = true;
    try {
      await api.tasks.patch(threadId, task.id, body);
      if (optimisticMsg) toast.success(optimisticMsg);
      onChange?.();
    } catch (err) {
      const msg =
        err instanceof ApiError
          ? ((err.body as { error?: string } | undefined)?.error ?? err.message)
          : err instanceof Error
            ? err.message
            : String(err);
      toast.error(`Update failed: ${msg}`);
    } finally {
      busy = false;
    }
  }

  async function loadHandoffs() {
    handoffsLoading = true;
    try {
      const res = await api.tasks.handoffs(threadId, task.id);
      handoffs = res.data ?? [];
    } catch {
      handoffs = [];
    } finally {
      handoffsLoading = false;
    }
  }

  $effect(() => {
    void task.id;
    void loadHandoffs();
  });

  function toggleCheck(check: AcceptanceCheck) {
    // Only allow toggling if the status logically permits it (in_progress / pending_verify).
    if (!(task.status === 'in_progress' || task.status === 'pending_verify')) {
      toast.error('Acceptance checks can only be toggled while in_progress or pending_verify');
      return;
    }
    const nextChecks = task.acceptance.checks.map((c) =>
      c.id === check.id
        ? { ...c, verified: !c.verified, verified_by: !c.verified ? 'human' : undefined }
        : c
    );
    void patch({ acceptance: { checks: nextChecks }, by: 'human' });
  }

  function submitToVerify() {
    void patch({ status: 'pending_verify', by: 'human' }, `Submitted ${task.id} for verification`);
  }

  function markDone() {
    if (!allVerified) {
      toast.error('All acceptance checks must be verified first');
      return;
    }
    void patch({ status: 'done', by: 'human' }, `${task.id} marked done`);
  }

  function resume() {
    void patch({ status: 'queued', by: 'human' }, `${task.id} resumed`);
  }

  function promote() {
    const status = task.blocked_by.length > 0 ? 'blocked' : 'queued';
    const blocked_reason =
      status === 'blocked' ? `Waiting on dependencies: ${task.blocked_by.join(', ')}` : undefined;
    void patch({ status, blocked_reason, by: 'human' }, `${task.id} ${status}`);
  }

  function artifactKindLabel(kind: string): string {
    return kind.replaceAll('_', ' ');
  }

  function artifactDate(value: string): string {
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return value;
    return formatDistanceToNow(date, { addSuffix: true });
  }

  function openRaw() {
    rawDraft = serializeForEdit(task);
    editingRaw = true;
    rawError = null;
  }

  function serializeForEdit(t: Task): string {
    // Pseudo-TOML preview limited to mutable fields. The user can edit these.
    return [
      `title = ${JSON.stringify(t.title)}`,
      `status = ${JSON.stringify(t.status)}`,
      `assignee = ${JSON.stringify(t.assignee ?? '')}`,
      `labels = ${JSON.stringify(t.labels)}`
    ].join('\n');
  }

  function parseRaw(src: string): PatchTaskRequest | { error: string } {
    const out: Record<string, unknown> = { by: 'human' };
    const lines = src.split('\n');
    for (const line of lines) {
      const trimmed = line.trim();
      if (!trimmed || trimmed.startsWith('#')) continue;
      const eq = trimmed.indexOf('=');
      if (eq < 0) return { error: `Bad line (missing '='): ${trimmed}` };
      const k = trimmed.slice(0, eq).trim();
      const v = trimmed.slice(eq + 1).trim();
      try {
        const parsed = JSON.parse(v);
        if (k === 'assignee' && parsed === '') out.assignee = null;
        else out[k] = parsed;
      } catch {
        return { error: `Bad JSON value for ${k}: ${v}` };
      }
    }
    return out as unknown as PatchTaskRequest;
  }

  async function saveRaw() {
    rawError = null;
    const parsed = parseRaw(rawDraft);
    if ('error' in parsed) {
      rawError = parsed.error;
      return;
    }
    await patch(parsed, `Updated ${task.id}`);
    editingRaw = false;
  }
</script>

<aside
  class="flex h-full w-full flex-col border-l"
  style="background: var(--surface-panel); border-color: var(--border-subtle);"
>
  <header
    class="flex items-start justify-between border-b px-4 py-3"
    style="border-color: var(--border-subtle);"
  >
    <div class="min-w-0">
      <div class="flex items-center gap-2">
        <span class="font-mono text-xs" style="color: var(--fg-muted);">{task.id}</span>
        <TaskStatusBadge status={task.status} />
      </div>
      <h2 class="mt-1 truncate text-base font-medium" style="color: var(--fg-default);">
        {task.title}
      </h2>
    </div>
    <Button variant="ghost" size="icon" onclick={onClose} aria-label="Close detail">
      <X class="h-4 w-4" />
    </Button>
  </header>

  <div class="min-h-0 flex-1 overflow-y-auto px-4 py-4">
    <!-- Transitions row -->
    <div class="mb-4 flex flex-wrap gap-2">
      {#if task.status === 'proposed'}
        <Button size="sm" onclick={promote} disabled={busy}>
          <Play class="h-3.5 w-3.5" /> Promote
        </Button>
      {:else if task.status === 'queued'}
        <Button variant="outline" size="sm" disabled title="Agent-only in F2">
          Claim (agent only)
        </Button>
        <Button variant="outline" size="sm" onclick={() => (pauseOpen = true)} disabled={busy}>
          <Pause class="h-3.5 w-3.5" /> Pause
        </Button>
      {:else if task.status === 'in_progress'}
        <Button size="sm" onclick={submitToVerify} disabled={busy}>
          <CheckCircle2 class="h-3.5 w-3.5" /> Submit
        </Button>
        <Button variant="outline" size="sm" onclick={() => (pauseOpen = true)} disabled={busy}>
          <Pause class="h-3.5 w-3.5" /> Pause
        </Button>
      {:else if task.status === 'pending_verify'}
        <Button size="sm" onclick={markDone} disabled={busy || !allVerified}>
          <CheckCircle2 class="h-3.5 w-3.5" /> Mark done
        </Button>
      {:else if task.status === 'paused'}
        <Button size="sm" onclick={resume} disabled={busy}>
          <Play class="h-3.5 w-3.5" /> Resume
        </Button>
      {/if}

      {#if task.status !== 'abandoned' && task.status !== 'done'}
        <Button
          variant="outline"
          size="sm"
          onclick={() => (abandonOpen = true)}
          disabled={busy}
          class="ml-auto"
        >
          <XCircle class="h-3.5 w-3.5" /> Abandon
        </Button>
      {/if}

      {#if busy}
        <Loader2 class="ml-2 h-4 w-4 animate-spin" />
      {/if}
    </div>

    <!-- Meta row -->
    <dl class="mb-4 grid grid-cols-2 gap-2 text-xs">
      <div class="flex items-center gap-1.5" style="color: var(--fg-muted);">
        <User class="h-3 w-3" />
        <span class="font-mono">{task.assignee ?? '—'}</span>
      </div>
      <div class="flex items-center gap-1.5" style="color: var(--fg-muted);">
        <Clock class="h-3 w-3" />
        <span>Updated {formatDistanceToNow(new Date(task.updated_at), { addSuffix: true })}</span>
      </div>
      {#if task.parent}
        <div class="flex items-center gap-1.5" style="color: var(--fg-muted);">
          <GitBranch class="h-3 w-3" />
          <button
            class="font-mono hover:underline"
            style="color: var(--accent);"
            onclick={() => onSelect(task.parent!)}
          >
            {task.parent}
          </button>
        </div>
      {/if}
      {#if task.labels.length > 0}
        <div class="flex items-center gap-1.5">
          <Tag class="h-3 w-3" style="color: var(--fg-muted);" />
          <div class="flex flex-wrap gap-1">
            {#each task.labels as l (l)}
              <span
                class="rounded-full px-1.5 py-0.5 text-[10px]"
                style="background: var(--accent-soft); color: var(--accent);"
              >
                {l}
              </span>
            {/each}
          </div>
        </div>
      {/if}
    </dl>

    {#if reasonItems.length > 0}
      <section class="mb-4 flex flex-col gap-1.5">
        {#each reasonItems as item (`${item.label}:${item.value}`)}
          <div
            class="rounded-md border px-2.5 py-2 text-xs"
            style="
              border-color: {item.urgent
                ? 'color-mix(in srgb, var(--dot-warn) 45%, transparent)'
                : 'var(--border-subtle)'};
              background: {item.urgent
                ? 'color-mix(in srgb, var(--dot-warn) 10%, transparent)'
                : 'var(--surface-window)'};
            "
            title={item.value}
          >
            <span class="mr-2 text-[10px] uppercase" style="color: var(--fg-label);">
              {item.label}
            </span>
            <span class="line-clamp-2" style="color: var(--fg-default);">{item.value}</span>
          </div>
        {/each}
      </section>
    {/if}

    <!-- Brief -->
    {#if hasBrief && task.brief}
      <section class="mb-4">
        <h3 class="h-eyebrow mb-2">Brief</h3>
        <div
          class="rounded-md border px-3 py-2.5 text-sm"
          style="border-color: var(--border-subtle); background: var(--surface-window);"
        >
          {#if task.brief.objective.trim()}
            <div class="mb-2">
              <p class="text-[10px] uppercase" style="color: var(--fg-muted);">Objective</p>
              <p style="color: var(--fg-default);">{task.brief.objective}</p>
            </div>
          {/if}
          {#if task.brief.context.trim()}
            <div class="mb-2">
              <p class="text-[10px] uppercase" style="color: var(--fg-muted);">Context</p>
              <p class="whitespace-pre-wrap" style="color: var(--fg-default);">
                {task.brief.context}
              </p>
            </div>
          {/if}
          {#if task.brief.tasks.length > 0}
            <div class="mb-2">
              <p class="text-[10px] uppercase" style="color: var(--fg-muted);">Work</p>
              <ol class="ml-4 list-decimal space-y-1" style="color: var(--fg-default);">
                {#each task.brief.tasks as item, idx (`${idx}-${item}`)}
                  <li>{item}</li>
                {/each}
              </ol>
            </div>
          {/if}
          {#if task.brief.rules.length > 0}
            <div class="mb-2">
              <p class="text-[10px] uppercase" style="color: var(--fg-muted);">Rules</p>
              <ul class="ml-4 list-disc space-y-1" style="color: var(--fg-default);">
                {#each task.brief.rules as item, idx (`${idx}-${item}`)}
                  <li>{item}</li>
                {/each}
              </ul>
            </div>
          {/if}
          {#if task.brief.expected_result.trim()}
            <div>
              <p class="text-[10px] uppercase" style="color: var(--fg-muted);">Expected result</p>
              <p style="color: var(--fg-default);">{task.brief.expected_result}</p>
            </div>
          {/if}
        </div>
      </section>
    {/if}

    <!-- Handoffs -->
    <section class="mb-4 rounded-md border p-3" style="border-color: var(--border-subtle);">
      <div class="mb-2 flex items-center justify-between">
        <h3 class="text-sm font-medium" style="color: var(--fg-default);">Handoffs</h3>
        {#if handoffsLoading}
          <Loader2 class="h-3.5 w-3.5 animate-spin" style="color: var(--fg-muted);" />
        {/if}
      </div>
      {#if handoffs.length === 0}
        <p class="text-xs" style="color: var(--fg-muted);">No handoff recorded yet.</p>
      {:else}
        <div class="space-y-2">
          {#each handoffs as h, i (`${h.at}-${i}`)}
            <article
              class="rounded border px-2.5 py-2 text-xs"
              style="border-color: var(--border-subtle); background: var(--surface-titlebar);"
            >
              <div class="mb-1 flex flex-wrap items-center gap-1.5">
                <span class="font-mono" style="color: var(--fg-muted);">{h.from}</span>
                <span style="color: var(--fg-muted);">→</span>
                <span class="font-mono" style="color: var(--accent);">{h.to_role}</span>
                <span
                  class="rounded px-1.5 py-0.5"
                  style="background: var(--accent-soft); color: var(--accent);"
                >
                  {h.status}
                </span>
              </div>
              <p class="font-medium" style="color: var(--fg-default);">{h.goal}</p>
              <p class="mt-1" style="color: var(--fg-muted);">{h.next_agent_action}</p>
              {#if h.commands_run.length > 0}
                <div class="mt-2 font-mono text-[11px]" style="color: var(--fg-muted);">
                  {h.commands_run.join(' · ')}
                </div>
              {/if}
            </article>
          {/each}
        </div>
      {/if}
    </section>

    <!-- Acceptance -->
    <section class="mb-4">
      <h3 class="h-eyebrow mb-2">Acceptance</h3>
      {#if task.acceptance.checks.length === 0}
        <p class="text-xs" style="color: var(--fg-muted);">No checks defined.</p>
      {:else}
        <ul class="flex flex-col gap-1.5">
          {#each task.acceptance.checks as check (check.id)}
            <li
              class="flex items-start gap-2 rounded-md border px-2.5 py-2 text-sm"
              style="border-color: var(--border-subtle); background: var(--surface-window);"
            >
              <button
                type="button"
                onclick={() => toggleCheck(check)}
                class="mt-0.5 inline-flex h-4 w-4 shrink-0 items-center justify-center rounded border"
                style={check.verified
                  ? 'background: var(--dot-success); border-color: var(--dot-success); color: white;'
                  : 'border-color: var(--border-input); background: var(--surface-canvas);'}
                aria-label={check.verified ? 'Unverify' : 'Verify'}
              >
                {#if check.verified}<CheckCircle2 class="h-3 w-3" />{/if}
              </button>
              <div class="flex-1">
                <span
                  style={check.verified
                    ? 'color: var(--fg-muted); text-decoration: line-through;'
                    : ''}
                >
                  {check.text}
                </span>
                {#if check.verified_by}
                  <p class="text-[10px]" style="color: var(--fg-muted);">by {check.verified_by}</p>
                {/if}
              </div>
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    <!-- Dependencies -->
    <section class="mb-4">
      <h3 class="h-eyebrow mb-2 flex items-center gap-1">
        <Link2 class="h-3 w-3" /> Dependencies
      </h3>
      {#if task.blocked_by.length === 0 && task.unblocks.length === 0}
        <p class="text-xs" style="color: var(--fg-muted);">None.</p>
      {:else}
        {#if task.blocked_by.length > 0}
          <div class="mb-1.5">
            <span class="text-[10px] uppercase tracking-wider" style="color: var(--fg-muted);"
              >Blocked by</span
            >
            <div class="mt-1 flex flex-wrap gap-1">
              {#each task.blocked_by as id (id)}
                <button
                  class="rounded font-mono text-[11px] hover:underline"
                  style="color: var(--accent); background: var(--accent-soft); padding: 1px 6px;"
                  onclick={() => onSelect(id)}
                >
                  {id}
                </button>
              {/each}
            </div>
          </div>
        {/if}
        {#if task.unblocks.length > 0}
          <div>
            <span class="text-[10px] uppercase tracking-wider" style="color: var(--fg-muted);"
              >Unblocks</span
            >
            <div class="mt-1 flex flex-wrap gap-1">
              {#each task.unblocks as id (id)}
                <button
                  class="rounded font-mono text-[11px] hover:underline"
                  style="color: var(--accent); background: var(--accent-soft); padding: 1px 6px;"
                  onclick={() => onSelect(id)}
                >
                  {id}
                </button>
              {/each}
            </div>
          </div>
        {/if}
      {/if}
    </section>

    <!-- Artifacts -->
    <section class="mb-4">
      <h3 class="h-eyebrow mb-2">Artifacts</h3>
      {#if artifactMetadata.length === 0 && !hasLegacyArtifacts}
        <p class="text-xs" style="color: var(--fg-muted);">No artifacts yet.</p>
      {:else}
        {#if artifactMetadata.length > 0}
          <ul class="flex flex-col gap-2">
            {#each artifactMetadata as artifact (artifact.artifact_id)}
              <li
                class="rounded-md border px-2.5 py-2"
                style="border-color: var(--border-subtle); background: var(--surface-panel);"
              >
                <div class="flex min-w-0 items-center gap-2">
                  <span
                    class="shrink-0 rounded-sm px-1.5 py-0.5 text-[10px] uppercase"
                    style="background: var(--surface-titlebar); color: var(--fg-label);"
                  >
                    {artifactKindLabel(artifact.kind)}
                  </span>
                  <span class="min-w-0 flex-1 truncate font-mono text-[11px]" title={artifact.path}>
                    {artifact.path}
                  </span>
                </div>
                <div class="mt-1 flex min-w-0 items-center gap-2 text-[10px]">
                  <span class="truncate" style="color: var(--fg-muted);">
                    {artifact.produced_by}
                  </span>
                  <span style="color: var(--fg-muted);">·</span>
                  <span style="color: var(--fg-muted);">{artifactDate(artifact.created_at)}</span>
                </div>
                {#if artifact.summary}
                  <p class="mt-1 line-clamp-2 text-[11px]" style="color: var(--fg-muted);">
                    {artifact.summary}
                  </p>
                {/if}
              </li>
            {/each}
          </ul>
        {:else}
          <ul class="flex flex-col gap-1">
            {#each task.artifacts.files as f (f)}
              <li class="font-mono text-[11px]" style="color: var(--fg-breadcrumb);">{f}</li>
            {/each}
            {#each task.artifacts.turns as t (t)}
              <li class="font-mono text-[11px]" style="color: var(--fg-muted);">turn: {t}</li>
            {/each}
            {#if task.artifacts.diff}
              <li class="font-mono text-[11px]" style="color: var(--fg-muted);">diff</li>
            {/if}
          </ul>
        {/if}
      {/if}
    </section>

    <!-- History timeline -->
    <section class="mb-4">
      <h3 class="h-eyebrow mb-2">History</h3>
      {#if task.history.events.length === 0}
        <p class="text-xs" style="color: var(--fg-muted);">No history yet.</p>
      {:else}
        <ol class="flex flex-col gap-1.5">
          {#each task.history.events as ev (ev.at + ev.from + ev.to)}
            <li class="flex items-baseline gap-2 text-xs">
              <span class="font-mono" style="color: var(--fg-muted);">
                {formatDistanceToNow(new Date(ev.at), { addSuffix: true })}
              </span>
              <span style="color: var(--fg-default);">
                <span class="font-mono">{ev.by}</span>
                {ev.from} → <strong>{ev.to}</strong>
              </span>
            </li>
          {/each}
        </ol>
      {/if}
    </section>

    <!-- Raw edit -->
    <section class="mt-6 border-t pt-3" style="border-color: var(--border-subtle);">
      <button
        class="flex w-full items-center justify-between text-left"
        onclick={() => (editingRaw ? (editingRaw = false) : openRaw())}
      >
        <span class="h-eyebrow inline-flex items-center gap-1">
          <Edit3 class="h-3 w-3" /> Edit raw (TOML-ish)
        </span>
        <span class="text-xs" style="color: var(--fg-muted);">
          {editingRaw ? 'collapse' : 'expand'}
        </span>
      </button>
      {#if editingRaw}
        <div class="mt-2 flex flex-col gap-2">
          <textarea
            bind:value={rawDraft}
            spellcheck="false"
            class="min-h-[140px] resize-y rounded-md border px-3 py-2 font-mono text-[12px] outline-none"
            style="border-color: var(--border-input); background: var(--surface-window); color: var(--fg-default);"
          ></textarea>
          {#if rawError}
            <p class="text-xs" style="color: var(--dot-danger);">{rawError}</p>
          {/if}
          <div class="flex gap-2">
            <Button size="sm" onclick={saveRaw} disabled={busy}>Save</Button>
            <Button size="sm" variant="outline" onclick={() => (editingRaw = false)}>Cancel</Button>
          </div>
          <p class="text-[10px]" style="color: var(--fg-muted);">
            Edits only the simple scalar fields (title, status, assignee, labels). Validation runs
            through valibot before PATCH.
          </p>
        </div>
      {/if}
    </section>
  </div>
</aside>

<AbandonDialog
  bind:open={abandonOpen}
  {threadId}
  taskId={task.id}
  onAbandoned={() => onChange?.()}
/>
<PauseDialog
  bind:open={pauseOpen}
  onSubmit={(why) =>
    patch(
      { status: 'paused', notes: { why_paused: why, paused_reason: why }, by: 'human' },
      `Paused ${task.id}`
    )}
/>
