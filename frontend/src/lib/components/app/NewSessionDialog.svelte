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
  import { api, ApiError, type SessionKind } from '$lib/api/client';
  import { goto } from '$app/navigation';
  import { toast } from 'svelte-sonner';
  import { Loader2 } from '$lib/icons';

  interface Props {
    open: boolean;
    /** Optional existing thread to attach the session to. */
    threadId?: string | null;
  }

  let { open = $bindable(false), threadId = null }: Props = $props();

  let kind = $state<SessionKind>('claude');
  let cwd = $state<string>('');
  let submitting = $state<boolean>(false);
  let error = $state<string | null>(null);

  function reset() {
    kind = 'claude';
    cwd = '';
    error = null;
    submitting = false;
  }

  async function submit(ev: SubmitEvent) {
    ev.preventDefault();
    if (submitting) return;
    submitting = true;
    error = null;
    try {
      let tid = threadId;
      if (!tid) {
        const t = await api.threads.create();
        tid = t.data.id;
      }
      const res = await api.sessions.create(tid, {
        kind,
        cwd: cwd.trim() ? cwd.trim() : undefined
      });
      open = false;
      reset();
      await goto(`/threads/${tid}/sessions/${res.data.session_id}`);
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
      <DialogDescription>Spawn a claude or codex CLI in a managed PTY.</DialogDescription>
    </DialogHeader>
    <form class="mt-4 flex flex-col gap-4" onsubmit={submit}>
      <div class="flex flex-col gap-2">
        <Label for="kind">Agent</Label>
        <div class="flex gap-2" role="radiogroup" id="kind">
          {#each ['claude', 'codex'] as const as opt (opt)}
            <button
              type="button"
              role="radio"
              aria-checked={kind === opt}
              class="flex-1 rounded-md border px-3 py-2 text-sm transition-colors {kind === opt
                ? 'border-primary bg-primary text-primary-foreground'
                : 'border-border bg-background text-muted-foreground hover:bg-accent hover:text-foreground'}"
              onclick={() => (kind = opt)}
            >
              {opt}
            </button>
          {/each}
        </div>
      </div>
      <div class="flex flex-col gap-2">
        <Label for="cwd">Working directory (optional)</Label>
        <Input id="cwd" bind:value={cwd} placeholder="/path/to/project" autocomplete="off" />
        <p class="text-xs text-muted-foreground">Defaults to the backend process cwd when empty.</p>
      </div>
      {#if error}
        <p
          class="rounded-md border border-destructive/50 bg-destructive/10 px-3 py-2 text-xs text-destructive-foreground"
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
