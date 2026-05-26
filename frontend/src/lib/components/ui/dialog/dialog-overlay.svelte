<script lang="ts">
  import { Dialog as DialogPrimitive } from 'bits-ui';
  import { cn } from '$lib/utils';

  type Props = DialogPrimitive.OverlayProps & { class?: string };

  let { class: className, ...rest }: Props = $props();
</script>

<DialogPrimitive.Overlay
  class={cn(
    'fixed inset-0 z-50 bg-black/70 backdrop-blur-sm data-[state=open]:animate-in data-[state=closed]:animate-out',
    className
  )}
  {...rest}
/>
