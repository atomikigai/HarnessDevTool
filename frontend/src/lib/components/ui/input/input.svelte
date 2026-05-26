<script lang="ts">
  import type { HTMLInputAttributes } from 'svelte/elements';
  import { cn } from '$lib/utils';

  type Props = HTMLInputAttributes & {
    class?: string;
    value?: string | number | null;
  };

  let { class: className, value = $bindable(), ...rest }: Props = $props();
</script>

<input
  bind:value
  class={cn(
    'flex h-9 w-full rounded-md border px-3 py-2 text-sm transition-colors',
    'border-[var(--border-input)] bg-[var(--surface-titlebar)] text-[var(--fg-default)]',
    'placeholder:text-[var(--fg-muted)]',
    'focus-visible:outline-none focus-visible:border-[var(--accent)] focus-visible:ring-2 focus-visible:ring-[var(--accent-ring)]',
    'disabled:cursor-not-allowed disabled:opacity-50',
    className
  )}
  {...rest}
/>
