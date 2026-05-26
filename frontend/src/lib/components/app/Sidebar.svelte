<script lang="ts">
  import { MessageSquare, Bot, Settings } from '$lib/icons';
  import { cn } from '$lib/utils';

  type Entry = { label: string; href: string; icon: typeof MessageSquare };

  const entries: Entry[] = [
    { label: 'Threads', href: '/threads', icon: MessageSquare },
    { label: 'Agents', href: '/agents', icon: Bot },
    { label: 'Settings', href: '/settings', icon: Settings }
  ];

  let { currentPath = '/' }: { currentPath?: string } = $props();
</script>

<aside class="flex h-full w-60 shrink-0 flex-col border-r border-border bg-card">
  <div class="px-6 py-5">
    <a href="/" class="text-base font-semibold tracking-tight">Harness</a>
    <p class="mt-0.5 text-xs text-muted-foreground">Dev Tool</p>
  </div>
  <nav class="flex flex-1 flex-col gap-1 px-3">
    {#each entries as entry (entry.href)}
      {@const Icon = entry.icon}
      {@const active = currentPath === entry.href}
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
        <span>{entry.label}</span>
      </a>
    {/each}
  </nav>
  <div class="px-6 py-4 text-xs text-muted-foreground">F0 skeleton</div>
</aside>
