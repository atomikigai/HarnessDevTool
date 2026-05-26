<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { MessageSquare, Bot, Settings, Plus } from '$lib/icons';
  import { cn } from '$lib/utils';
  import { Button } from '$lib/components/ui/button';
  import NewSessionDialog from './NewSessionDialog.svelte';
  import { sessionsState } from '$lib/stores/session.svelte';

  type Entry = { label: string; href: string; icon: typeof MessageSquare; badge?: () => number };

  const entries: Entry[] = [
    {
      label: 'Threads',
      href: '/threads',
      icon: MessageSquare,
      badge: () => sessionsState.active.length
    },
    { label: 'Agents', href: '/agents', icon: Bot },
    { label: 'Settings', href: '/settings', icon: Settings }
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
</script>

<aside class="flex h-full w-60 shrink-0 flex-col border-r border-border bg-card">
  <div class="px-6 py-5">
    <a href="/" class="text-base font-semibold tracking-tight">Harness</a>
    <p class="mt-0.5 text-xs text-muted-foreground">Dev Tool</p>
  </div>

  <div class="px-3 pb-3">
    <Button class="w-full" size="sm" onclick={() => (dialogOpen = true)}>
      <Plus class="h-4 w-4" />
      New session
    </Button>
  </div>

  <nav class="flex flex-1 flex-col gap-1 px-3">
    {#each entries as entry (entry.href)}
      {@const Icon = entry.icon}
      {@const active = currentPath === entry.href}
      {@const count = entry.badge?.() ?? 0}
      <a
        href={entry.href}
        class={cn(
          'flex items-center gap-3 rounded-md px-3 py-2 text-sm transition-colors',
          active
            ? 'bg-accent text-accent-foreground'
            : 'text-muted-foreground hover:bg-accent/50 hover:text-foreground'
        )}
      >
        <Icon class="h-4 w-4" />
        <span class="flex-1">{entry.label}</span>
        {#if count > 0}
          <span
            class="inline-flex h-5 min-w-[20px] items-center justify-center rounded-full bg-emerald-500/20 px-1.5 text-[10px] font-semibold text-emerald-300"
            title="{count} running session{count === 1 ? '' : 's'}"
          >
            {count}
          </span>
        {/if}
      </a>
    {/each}
  </nav>
  <div class="px-6 py-4 text-xs text-muted-foreground">F1 terminal</div>
</aside>

<NewSessionDialog bind:open={dialogOpen} />
