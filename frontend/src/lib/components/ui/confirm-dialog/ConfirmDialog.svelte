<!--
  Imperative-friendly confirm dialog built on shadcn `Dialog`.
  Replaces native window.confirm() so the look matches the app shell.

  Usage:
    import { confirmDialog } from '$lib/components/ui/confirm-dialog';
    const ok = await confirmDialog({
      title: 'Delete row?',
      description: 'This action cannot be undone.',
      confirmLabel: 'Delete',
      destructive: true,
    });
    if (!ok) return;

  The promise resolves true on confirm, false on cancel/ESC/backdrop.

  This file is the *component* — the helper lives in `index.ts`.
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

  interface Props {
    open: boolean;
    title: string;
    description?: string;
    confirmLabel?: string;
    cancelLabel?: string;
    destructive?: boolean;
    onResult?: (ok: boolean) => void;
  }

  let {
    open = $bindable(false),
    title,
    description,
    confirmLabel = 'Confirm',
    cancelLabel = 'Cancel',
    destructive = false,
    onResult
  }: Props = $props();

  function resolve(ok: boolean) {
    open = false;
    onResult?.(ok);
  }
</script>

<Dialog bind:open onOpenChange={(v) => !v && onResult?.(false)}>
  <DialogContent class="sm:max-w-md">
    <DialogHeader>
      <DialogTitle>{title}</DialogTitle>
      {#if description}
        <DialogDescription>{description}</DialogDescription>
      {/if}
    </DialogHeader>
    <DialogFooter>
      <Button variant="outline" onclick={() => resolve(false)}>{cancelLabel}</Button>
      <Button variant={destructive ? 'destructive' : 'default'} onclick={() => resolve(true)}>
        {confirmLabel}
      </Button>
    </DialogFooter>
  </DialogContent>
</Dialog>
