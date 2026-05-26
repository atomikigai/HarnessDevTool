<!-- AbandonDialog — confirm modal asking for a `why` reason before DELETE. -->
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
  import { Label } from '$lib/components/ui/label';
  import { api, ApiError } from '$lib/api/client';
  import { toast } from 'svelte-sonner';
  import { Loader2 } from '$lib/icons';

  interface Props {
    open: boolean;
    threadId: string;
    taskId: string;
    onAbandoned?: () => void;
  }

  let { open = $bindable(false), threadId, taskId, onAbandoned }: Props = $props();

  let why = $state('');
  let busy = $state(false);
  let error = $state<string | null>(null);

  async function confirm() {
    if (busy) return;
    if (!why.trim()) {
      error = 'Provide a short reason.';
      return;
    }
    busy = true;
    error = null;
    try {
      await api.tasks.remove(threadId, taskId, { why: why.trim(), by: 'human' });
      toast.success(`${taskId} abandoned`);
      onAbandoned?.();
      open = false;
      why = '';
    } catch (err) {
      const msg =
        err instanceof ApiError
          ? ((err.body as { error?: string } | undefined)?.error ?? err.message)
          : err instanceof Error
            ? err.message
            : String(err);
      error = msg;
      toast.error(msg);
    } finally {
      busy = false;
    }
  }
</script>

<Dialog bind:open>
  <DialogContent class="sm:max-w-md">
    <DialogHeader>
      <DialogTitle>Abandon task {taskId}?</DialogTitle>
      <DialogDescription>
        Provide a short reason. This is recorded in the task history.
      </DialogDescription>
    </DialogHeader>
    <div class="mt-2 flex flex-col gap-2">
      <Label for="why">Reason</Label>
      <textarea
        id="why"
        bind:value={why}
        rows="3"
        class="rounded-md border px-3 py-2 text-sm outline-none"
        style="border-color: var(--border-input); background: var(--surface-window); color: var(--fg-default);"
        placeholder="Why are you abandoning this?"
      ></textarea>
      {#if error}
        <p class="text-xs" style="color: var(--dot-danger);">{error}</p>
      {/if}
    </div>
    <DialogFooter>
      <Button variant="outline" onclick={() => (open = false)} disabled={busy}>Cancel</Button>
      <Button variant="destructive" onclick={confirm} disabled={busy}>
        {#if busy}<Loader2 class="h-4 w-4 animate-spin" />{/if}
        Abandon
      </Button>
    </DialogFooter>
  </DialogContent>
</Dialog>
