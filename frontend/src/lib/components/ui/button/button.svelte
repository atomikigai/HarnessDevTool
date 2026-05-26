<script lang="ts" module>
  import { type VariantProps, tv } from 'tailwind-variants';

  // Paper-leaning button variants: primary keeps a subtle accent shadow;
  // outline/ghost lean on token colors so theme switch restyles cleanly.
  export const buttonVariants = tv({
    base: 'inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-md text-sm font-medium ring-offset-[var(--surface-window)] transition-[background-color,color,box-shadow,transform] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--accent-ring)] focus-visible:ring-offset-2 disabled:pointer-events-none disabled:opacity-50 active:scale-[0.98]',
    variants: {
      variant: {
        default:
          'bg-primary text-primary-foreground shadow-[var(--shadow-primary)] hover:brightness-110',
        destructive: 'bg-destructive text-destructive-foreground hover:brightness-110',
        outline:
          'border border-[var(--border-input)] bg-[var(--surface-window)] text-[var(--fg-default)] hover:bg-[var(--accent-soft)] hover:text-[var(--accent)]',
        secondary:
          'bg-[var(--surface-panel)] text-[var(--fg-default)] hover:bg-[var(--surface-titlebar)]',
        ghost: 'text-[var(--fg-muted)] hover:bg-[var(--accent-soft)] hover:text-[var(--accent)]',
        link: 'text-[var(--accent)] underline-offset-4 hover:underline'
      },
      size: {
        default: 'h-9 px-3.5 py-2',
        sm: 'h-8 rounded-md px-2.5 text-[13px]',
        lg: 'h-10 rounded-md px-5',
        icon: 'h-9 w-9'
      }
    },
    defaultVariants: {
      variant: 'default',
      size: 'default'
    }
  });

  export type ButtonVariant = VariantProps<typeof buttonVariants>['variant'];
  export type ButtonSize = VariantProps<typeof buttonVariants>['size'];
</script>

<script lang="ts">
  import type { HTMLButtonAttributes } from 'svelte/elements';
  import type { Snippet } from 'svelte';
  import { cn } from '$lib/utils';

  type Props = HTMLButtonAttributes & {
    variant?: ButtonVariant;
    size?: ButtonSize;
    class?: string;
    children?: Snippet;
  };

  let {
    variant = 'default',
    size = 'default',
    class: className,
    children,
    ...rest
  }: Props = $props();
</script>

<button class={cn(buttonVariants({ variant, size }), className)} {...rest}>
  {@render children?.()}
</button>
