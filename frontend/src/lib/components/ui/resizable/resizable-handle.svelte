<script lang="ts">
  import { PaneResizer, type PaneResizerProps } from 'paneforge';
  import { GripVertical } from '$lib/icons';
  import { cn } from '$lib/utils';

  type Props = PaneResizerProps & {
    class?: string;
    withHandle?: boolean;
  };

  let { class: className, withHandle = false, children, ...restProps }: Props = $props();
</script>

<PaneResizer
  class={cn(
    'relative flex w-px items-center justify-center bg-[var(--border-subtle)] transition-colors hover:bg-[var(--accent)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent-ring)] data-[direction=vertical]:h-px data-[direction=vertical]:w-full',
    className
  )}
  {...restProps}
>
  {#if withHandle}
    <div
      class="z-10 flex h-8 w-4 items-center justify-center rounded-md border bg-[var(--surface-panel)]"
      style="border-color: var(--border-input);"
    >
      <GripVertical class="h-3.5 w-3.5" style="color: var(--fg-muted);" />
    </div>
  {/if}
  {@render children?.()}
</PaneResizer>
