<script lang="ts">
  import { Dialog as DialogPrimitive } from 'bits-ui';
  import { cn } from '$lib/utils';

  type Props = DialogPrimitive.TitleProps & { class?: string };

  let { class: className, ...rest }: Props = $props();
</script>

<DialogPrimitive.Title class={cn('text-lg font-semibold tracking-tight', className)} {...rest} />
