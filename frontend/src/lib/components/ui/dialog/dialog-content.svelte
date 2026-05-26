<script lang="ts">
  import { Dialog as DialogPrimitive } from 'bits-ui';
  import type { Snippet } from 'svelte';
  import { cn } from '$lib/utils';
  import { X } from 'lucide-svelte';
  import DialogOverlay from './dialog-overlay.svelte';

  type Props = DialogPrimitive.ContentProps & {
    class?: string;
    children?: Snippet;
    showClose?: boolean;
  };

  let { class: className, children, showClose = true, ...rest }: Props = $props();
</script>

<DialogPrimitive.Portal>
  <DialogOverlay />
  <DialogPrimitive.Content
    class={cn(
      'fixed left-1/2 top-1/2 z-50 w-full max-w-lg -translate-x-1/2 -translate-y-1/2',
      'rounded-lg border border-border bg-background p-6 shadow-lg',
      'data-[state=open]:animate-in data-[state=closed]:animate-out',
      className
    )}
    {...rest}
  >
    {@render children?.()}
    {#if showClose}
      <DialogPrimitive.Close
        class="absolute right-4 top-4 rounded-sm text-muted-foreground opacity-70 transition-opacity hover:opacity-100 focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2"
      >
        <X class="h-4 w-4" />
        <span class="sr-only">Close</span>
      </DialogPrimitive.Close>
    {/if}
  </DialogPrimitive.Content>
</DialogPrimitive.Portal>
