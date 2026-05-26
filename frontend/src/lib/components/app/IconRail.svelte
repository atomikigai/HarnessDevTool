<!--
  IconRail — narrow ~68px vertical rail with icon + label below.
  Replaces the previous wide Sidebar. Layout mirrors the reference
  `harness-interactive.jsx` (rail width 68, icon column gap 5, active
  pill rounded 8). Module entries that are not yet built render with
  a "soon" badge and are non-interactive.
-->
<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import HarnessIcons from './HarnessIcons.svelte';
  import NewSessionDialog from './NewSessionDialog.svelte';
  import { sessionsState } from '$lib/stores/session.svelte';
  import { cn } from '$lib/utils';

  type IconName = 'agents' | 'sql' | 'ssh' | 'memory' | 'settings';

  type Entry = {
    label: string;
    href: string;
    icon: IconName;
    soon?: boolean;
    /** Optional badge resolver — returns count to render as a small pill. */
    badge?: () => number;
  };

  const entries: Entry[] = [
    {
      label: 'Agents',
      href: '/',
      icon: 'agents',
      badge: () => sessionsState.active.length
    },
    { label: 'SQL', href: '/sql', icon: 'sql', soon: true },
    { label: 'SSH', href: '/ssh', icon: 'ssh', soon: true },
    { label: 'Memory', href: '/memory', icon: 'memory', soon: true },
    { label: 'Settings', href: '/settings', icon: 'settings', soon: true }
  ];

  let { currentPath = '/' }: { currentPath?: string } = $props();
  let dialogOpen = $state(false);

  const POLL_MS = 5_000;
  let timer: ReturnType<typeof setInterval> | null = null;
  let controller: AbortController | null = null;

  function refresh() {
    controller?.abort();
    controller = new AbortController();
    sessionsState.refresh(controller.signal);
  }

  onMount(() => {
    refresh();
    timer = setInterval(refresh, POLL_MS);
  });

  onDestroy(() => {
    if (timer) clearInterval(timer);
    controller?.abort();
  });

  function isActive(href: string) {
    if (href === '/') return currentPath === '/' || currentPath.startsWith('/threads');
    return currentPath === href || currentPath.startsWith(href + '/');
  }
</script>

<aside
  class="flex h-full w-[68px] shrink-0 flex-col items-center gap-1 border-r py-3"
  style="background: var(--surface-rail); border-color: var(--border-subtle);"
>
  <!-- New session button — compact "+" tile at the top of the rail. -->
  <button
    type="button"
    onclick={() => (dialogOpen = true)}
    class="relative mb-2 flex h-9 w-9 items-center justify-center rounded-md text-[var(--fg-on-accent)] transition-transform hover:scale-[1.04] active:scale-[0.97]"
    style="background: var(--accent); box-shadow: var(--shadow-primary);"
    title="New session"
    aria-label="New session"
  >
    <HarnessIcons name="plus" size={14} />
    {#if sessionsState.active.length > 0}
      <span
        class="absolute -right-1 -top-1 inline-flex h-4 min-w-[16px] items-center justify-center rounded-full px-1 text-[10px] font-semibold"
        style="background: var(--dot-success); color: white;"
        title="{sessionsState.active.length} running"
      >
        {sessionsState.active.length}
      </span>
    {/if}
  </button>

  {#each entries as entry (entry.href)}
    {@const active = isActive(entry.href)}
    {@const disabled = !!entry.soon}
    {@const count = entry.badge?.() ?? 0}
    {#if disabled}
      <!--
        Disabled rail entry — non-link div. We keep it focusable but
        explicitly aria-disabled so AT users get the same "soon" signal.
      -->
      <div
        aria-disabled="true"
        title="{entry.label} — coming soon"
        class="relative flex w-[54px] cursor-not-allowed flex-col items-center gap-1 rounded-lg px-2 py-2 opacity-60"
        style="color: var(--fg-muted);"
      >
        <HarnessIcons name={entry.icon} size={16} />
        <span class="text-[10px] leading-none">{entry.label}</span>
        <span
          class="absolute -right-1 top-0.5 rounded-full px-1 text-[8px] font-semibold uppercase tracking-wider"
          style="background: var(--surface-titlebar); color: var(--fg-label); border: 1px solid var(--border-subtle);"
        >
          soon
        </span>
      </div>
    {:else}
      <a
        href={entry.href}
        class={cn(
          'relative flex w-[54px] flex-col items-center gap-1 rounded-lg px-2 py-2 transition-colors'
        )}
        style={active
          ? 'background: var(--accent-soft); color: var(--accent);'
          : 'color: var(--fg-muted);'}
        title={entry.label}
      >
        {#if active}
          <span
            class="absolute left-[-10px] top-1/2 h-5 w-[3px] -translate-y-1/2 rounded"
            style="background: var(--accent);"
          ></span>
        {/if}
        <HarnessIcons name={entry.icon} size={16} />
        <span class="text-[10px] leading-none {active ? 'font-semibold' : ''}">{entry.label}</span>
        {#if count > 0}
          <span
            class="absolute right-0 top-0.5 inline-flex h-4 min-w-[16px] items-center justify-center rounded-full px-1 text-[9px] font-semibold"
            style="background: var(--dot-success); color: white;"
            title="{count} running session{count === 1 ? '' : 's'}"
          >
            {count}
          </span>
        {/if}
      </a>
    {/if}
  {/each}
</aside>

<NewSessionDialog bind:open={dialogOpen} />
