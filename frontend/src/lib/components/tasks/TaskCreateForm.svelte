<!--
  TaskCreateForm — modal dialog wrapping the create-task POST.
  Validation is local-first via valibot so we can surface field issues
  before the request goes out.
-->
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
  import { Loader2, Plus, X, AlertTriangle } from '$lib/icons';
  import { api, ApiError } from '$lib/api/client';
  import { toast } from 'svelte-sonner';
  import { safeParse, createTaskSchema } from '$lib/api/schemas/task';
  import type { Task, CreateTaskRequest } from '$lib/api/models/task';

  interface Props {
    open: boolean;
    threadId: string;
    existingTasks: Task[];
    onCreated?: (task: Task) => void;
  }

  let { open = $bindable(false), threadId, existingTasks, onCreated }: Props = $props();

  let title = $state('');
  let parent = $state<string>('');
  let dependsOn = $state<string[]>([]);
  let labelInput = $state('');
  let labels = $state<string[]>([]);
  let checks = $state<string[]>(['']);
  let submitting = $state(false);
  let errors = $state<string[]>([]);

  const overChecksLimit = $derived(checks.length > 6);

  function reset() {
    title = '';
    parent = '';
    dependsOn = [];
    labels = [];
    labelInput = '';
    checks = [''];
    errors = [];
    submitting = false;
  }

  function addCheck() {
    checks = [...checks, ''];
  }
  function removeCheck(i: number) {
    checks = checks.filter((_, idx) => idx !== i);
    if (checks.length === 0) checks = [''];
  }
  function updateCheck(i: number, v: string) {
    const next = [...checks];
    next[i] = v;
    checks = next;
  }

  function addLabel() {
    const v = labelInput.trim();
    if (v && !labels.includes(v)) labels = [...labels, v];
    labelInput = '';
  }
  function removeLabel(l: string) {
    labels = labels.filter((x) => x !== l);
  }

  function toggleDepends(id: string) {
    if (dependsOn.includes(id)) dependsOn = dependsOn.filter((x) => x !== id);
    else dependsOn = [...dependsOn, id];
  }

  async function submit(ev: SubmitEvent) {
    ev.preventDefault();
    if (submitting) return;
    errors = [];

    const cleanedChecks = checks.map((c) => c.trim()).filter((c) => c.length > 0);
    const payload: CreateTaskRequest = {
      title: title.trim(),
      parent: parent || undefined,
      depends_on: dependsOn.length > 0 ? dependsOn : undefined,
      labels: labels.length > 0 ? labels : undefined,
      acceptance:
        cleanedChecks.length > 0 ? { checks: cleanedChecks.map((text) => ({ text })) } : undefined,
      created_by: 'human'
    };

    const result = safeParse(createTaskSchema, payload);
    if (!result.ok) {
      errors = result.errors;
      return;
    }

    submitting = true;
    try {
      const res = await api.tasks.create(threadId, result.value as CreateTaskRequest);
      toast.success(`Task ${res.data.id} created`);
      onCreated?.(res.data);
      open = false;
      reset();
    } catch (err) {
      const msg =
        err instanceof ApiError
          ? ((err.body as { error?: string } | undefined)?.error ?? err.message)
          : err instanceof Error
            ? err.message
            : String(err);
      errors = [msg];
      toast.error(`Failed to create task: ${msg}`);
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
  <DialogContent class="sm:max-w-lg">
    <DialogHeader>
      <DialogTitle>New task</DialogTitle>
      <DialogDescription>Define title, dependencies and acceptance checks.</DialogDescription>
    </DialogHeader>

    <form class="mt-4 flex flex-col gap-4" onsubmit={submit}>
      <div class="flex flex-col gap-2">
        <Label for="title">Title</Label>
        <Input
          id="title"
          bind:value={title}
          placeholder="Wire up the foo widget"
          autocomplete="off"
        />
      </div>

      <div class="grid grid-cols-2 gap-3">
        <div class="flex flex-col gap-2">
          <Label for="parent">Parent (optional)</Label>
          <select
            id="parent"
            bind:value={parent}
            class="h-9 rounded-md border bg-[var(--surface-window)] px-2 text-sm"
            style="border-color: var(--border-input); color: var(--fg-default);"
          >
            <option value="">— none —</option>
            {#each existingTasks as t (t.id)}
              <option value={t.id}>{t.id} · {t.title}</option>
            {/each}
          </select>
        </div>
        <div class="flex flex-col gap-2">
          <Label>Depends on</Label>
          <div
            class="flex max-h-24 flex-wrap gap-1 overflow-auto rounded-md border p-1.5"
            style="border-color: var(--border-input); background: var(--surface-window);"
          >
            {#if existingTasks.length === 0}
              <span class="text-xs" style="color: var(--fg-muted);">No tasks yet</span>
            {/if}
            {#each existingTasks as t (t.id)}
              {@const on = dependsOn.includes(t.id)}
              <button
                type="button"
                onclick={() => toggleDepends(t.id)}
                class="rounded px-1.5 py-0.5 font-mono text-[11px] transition-colors"
                style={on
                  ? 'background: var(--accent-soft); color: var(--accent); border: 1px solid var(--accent-soft-border);'
                  : 'background: var(--surface-titlebar); color: var(--fg-muted); border: 1px solid var(--border-subtle);'}
                title={t.title}
              >
                {t.id}
              </button>
            {/each}
          </div>
        </div>
      </div>

      <div class="flex flex-col gap-2">
        <Label>Labels</Label>
        <div class="flex gap-2">
          <Input
            bind:value={labelInput}
            placeholder="add label and press +"
            onkeydown={(e: KeyboardEvent) => {
              if (e.key === 'Enter') {
                e.preventDefault();
                addLabel();
              }
            }}
          />
          <Button type="button" variant="outline" size="sm" onclick={addLabel}>
            <Plus class="h-3.5 w-3.5" />
          </Button>
        </div>
        {#if labels.length > 0}
          <div class="flex flex-wrap gap-1">
            {#each labels as l (l)}
              <span
                class="inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[11px]"
                style="background: var(--accent-soft); color: var(--accent); border: 1px solid var(--accent-soft-border);"
              >
                {l}
                <button type="button" onclick={() => removeLabel(l)} aria-label="Remove">
                  <X class="h-3 w-3" />
                </button>
              </span>
            {/each}
          </div>
        {/if}
      </div>

      <div class="flex flex-col gap-2">
        <div class="flex items-center justify-between">
          <Label>Acceptance checks</Label>
          {#if overChecksLimit}
            <span
              class="inline-flex items-center gap-1 text-[11px]"
              style="color: var(--dot-warn);"
            >
              <AlertTriangle class="h-3 w-3" /> over recommended max (6)
            </span>
          {/if}
        </div>
        <div class="flex flex-col gap-1.5">
          {#each checks as check, i (i)}
            <div class="flex gap-2">
              <Input
                value={check}
                placeholder="e.g. tests pass"
                oninput={(e: Event) => updateCheck(i, (e.target as HTMLInputElement).value)}
              />
              <Button type="button" variant="ghost" size="icon" onclick={() => removeCheck(i)}>
                <X class="h-3.5 w-3.5" />
              </Button>
            </div>
          {/each}
          <Button type="button" variant="outline" size="sm" onclick={addCheck}>
            <Plus class="h-3.5 w-3.5" /> Add check
          </Button>
        </div>
      </div>

      {#if errors.length > 0}
        <div
          class="rounded-md border px-3 py-2 text-xs"
          style="border-color: color-mix(in srgb, var(--dot-danger) 35%, transparent); background: color-mix(in srgb, var(--dot-danger) 10%, transparent); color: var(--dot-danger);"
        >
          <ul class="list-disc pl-4">
            {#each errors as e (e)}
              <li>{e}</li>
            {/each}
          </ul>
        </div>
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
          {#if submitting}<Loader2 class="h-4 w-4 animate-spin" />{/if}
          Create task
        </Button>
      </DialogFooter>
    </form>
  </DialogContent>
</Dialog>
