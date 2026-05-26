<!--
  TaskStatusBadge — small pill rendering a TaskStatus with theme-aware tones.
  Tones map to the design tokens already exposed in app.css (dot-* / accent).
-->
<script lang="ts">
  import { statusTone, type TaskStatus } from '$lib/api/models/task';

  let { status, size = 'sm' }: { status: TaskStatus; size?: 'xs' | 'sm' } = $props();

  const tone = $derived(statusTone(status));

  const palette = $derived.by(() => {
    switch (tone) {
      case 'success':
        return {
          bg: 'color-mix(in srgb, var(--dot-success) 14%, transparent)',
          fg: 'var(--dot-success)',
          border: 'color-mix(in srgb, var(--dot-success) 35%, transparent)'
        };
      case 'warn':
        return {
          bg: 'color-mix(in srgb, var(--dot-warn) 14%, transparent)',
          fg: 'var(--dot-warn)',
          border: 'color-mix(in srgb, var(--dot-warn) 35%, transparent)'
        };
      case 'danger':
        return {
          bg: 'color-mix(in srgb, var(--dot-danger) 12%, transparent)',
          fg: 'var(--dot-danger)',
          border: 'color-mix(in srgb, var(--dot-danger) 35%, transparent)'
        };
      case 'accent':
        return {
          bg: 'var(--accent-soft)',
          fg: 'var(--accent)',
          border: 'var(--accent-soft-border)'
        };
      default:
        return {
          bg: 'var(--surface-titlebar)',
          fg: 'var(--fg-muted)',
          border: 'var(--border-subtle)'
        };
    }
  });

  const sizeClass = $derived(
    size === 'xs' ? 'text-[10px] px-1.5 py-0.5' : 'text-[11px] px-2 py-0.5'
  );
</script>

<span
  class="inline-flex items-center gap-1 rounded-full border font-medium uppercase tracking-wider {sizeClass}"
  style="background: {palette.bg}; color: {palette.fg}; border-color: {palette.border};"
>
  {status.replace('_', ' ')}
</span>
