<!-- PauseDialog — captures the `why_paused` note and hands it off to the caller. -->
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

  interface Props {
    open: boolean;
    onSubmit: (why: string) => void;
  }

  let { open = $bindable(false), onSubmit }: Props = $props();
  let why = $state('');

  function submit() {
    onSubmit(why.trim());
    open = false;
    why = '';
  }
</script>

<Dialog bind:open>
  <DialogContent class="sm:max-w-md">
    <DialogHeader>
      <DialogTitle>Pause task</DialogTitle>
      <DialogDescription>Optional note to capture why this is being paused.</DialogDescription>
    </DialogHeader>
    <div class="mt-2 flex flex-col gap-2">
      <Label for="why-paused">Why pause</Label>
      <textarea
        id="why-paused"
        bind:value={why}
        rows="3"
        class="rounded-md border px-3 py-2 text-sm outline-none"
        style="border-color: var(--border-input); background: var(--surface-window); color: var(--fg-default);"
        placeholder="Waiting on..."
      ></textarea>
    </div>
    <DialogFooter>
      <Button variant="outline" onclick={() => (open = false)}>Cancel</Button>
      <Button onclick={submit}>Pause</Button>
    </DialogFooter>
  </DialogContent>
</Dialog>
