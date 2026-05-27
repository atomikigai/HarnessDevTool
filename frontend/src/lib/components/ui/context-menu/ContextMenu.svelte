<!--
  Lightweight floating context menu.

  Consumer pattern:
    <ContextMenu
      x={menuX}
      y={menuY}
      open={menuOpen}
      items={menuItems}
      onClose={() => (menuOpen = false)}
    />

  Items are `{ label, icon?, onSelect, destructive?, disabled? }`.
  The panel is rendered at `(x, y)` and clamped to the viewport.
  Click outside or ESC closes it. No deps beyond what's already in use.
-->
<script lang="ts" module>
  // Icons in this app come from `lucide-svelte`, whose component type
  // doesn't satisfy Svelte 5's strict `Component<...>` shape. Keep the
  // slot loose so callers can pass any svelte component with a `class` prop.
  /* eslint-disable @typescript-eslint/no-explicit-any */
  export interface ContextMenuItem {
    label: string;
    icon?: any;
    onSelect: () => void;
    destructive?: boolean;
    disabled?: boolean;
  }
</script>

<script lang="ts">
  interface Props {
    x: number;
    y: number;
    open: boolean;
    items: ContextMenuItem[];
    onClose: () => void;
  }

  let { x, y, open, items, onClose }: Props = $props();

  let panel = $state<HTMLDivElement | null>(null);
  let pos = $state<{ left: number; top: number }>({ left: 0, top: 0 });

  // Clamp to viewport after the panel mounts/resizes.
  $effect(() => {
    if (!open || !panel) return;
    const r = panel.getBoundingClientRect();
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    const pad = 6;
    let left = x;
    let top = y;
    if (left + r.width + pad > vw) left = Math.max(pad, vw - r.width - pad);
    if (top + r.height + pad > vh) top = Math.max(pad, vh - r.height - pad);
    pos = { left, top };
  });

  function onWindowDown(e: MouseEvent) {
    if (!open) return;
    if (panel && e.target instanceof Node && panel.contains(e.target)) return;
    onClose();
  }
  function onKey(e: KeyboardEvent) {
    if (!open) return;
    if (e.key === 'Escape') {
      e.preventDefault();
      onClose();
    }
  }

  function pick(item: ContextMenuItem) {
    if (item.disabled) return;
    onClose();
    // Defer so any focus/close side-effects settle before the action runs.
    queueMicrotask(() => item.onSelect());
  }
</script>

<svelte:window onmousedown={onWindowDown} oncontextmenu={onWindowDown} onkeydown={onKey} />

{#if open}
  <div
    bind:this={panel}
    role="menu"
    tabindex="-1"
    class="fixed z-[100] min-w-[180px] rounded-md border py-1 text-[12.5px] shadow-[var(--shadow-pop)]"
    style="left: {pos.left}px; top: {pos.top}px; border-color: var(--border-subtle); background: var(--surface-window); color: var(--fg-default);"
  >
    {#each items as item, i (i)}
      <button
        type="button"
        role="menuitem"
        disabled={item.disabled}
        onclick={() => pick(item)}
        class="flex w-full items-center gap-2 px-3 py-1.5 text-left transition-colors disabled:opacity-50"
        style={item.destructive
          ? 'color: var(--dot-danger);'
          : 'color: var(--fg-default);'}
        onmouseenter={(e) => {
          if (!item.disabled) (e.currentTarget as HTMLElement).style.background = 'var(--accent-soft)';
        }}
        onmouseleave={(e) => {
          (e.currentTarget as HTMLElement).style.background = 'transparent';
        }}
      >
        {#if item.icon}
          {@const Icon = item.icon}
          <Icon class="h-3.5 w-3.5" />
        {/if}
        <span>{item.label}</span>
      </button>
    {/each}
  </div>
{/if}
