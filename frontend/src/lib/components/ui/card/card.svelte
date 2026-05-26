<script lang="ts">
  import type { HTMLAttributes } from 'svelte/elements';
  import type { Snippet } from 'svelte';
  import { cn } from '$lib/utils';

  type Props = HTMLAttributes<HTMLDivElement> & {
    class?: string;
    children?: Snippet;
  };

  let { class: className, children, ...rest }: Props = $props();
</script>

<div
  class={cn(
    'rounded-lg border text-[var(--fg-default)]',
    'border-[var(--border-subtle)] bg-[var(--surface-panel)]',
    'shadow-[var(--shadow-card)]',
    className
  )}
  {...rest}
>
  {@render children?.()}
</div>
