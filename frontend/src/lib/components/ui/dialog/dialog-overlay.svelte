<script lang="ts">
  import { Dialog as DialogPrimitive } from 'bits-ui';
  import { cn } from '$lib/utils';

  type Props = DialogPrimitive.OverlayProps & { class?: string };

  let { class: className, ...rest }: Props = $props();
</script>

<!--
  Warm overlay — softer than pitch black, matches the paper "haze" feel.
  Uses a tint of the canvas color so it works in both themes.
-->
<DialogPrimitive.Overlay
  class={cn(
    'fixed inset-0 z-50 backdrop-blur-[2px] data-[state=open]:animate-in data-[state=closed]:animate-out',
    className
  )}
  style="background: color-mix(in srgb, var(--fg-default) 35%, transparent);"
  {...rest}
/>
