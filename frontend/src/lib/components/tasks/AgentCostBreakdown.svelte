<script lang="ts">
  import type { BudgetView } from '$lib/api/client';

  interface Props {
    view: BudgetView | null;
  }

  let { view }: Props = $props();

  const money = new Intl.NumberFormat('en-US', {
    style: 'currency',
    currency: 'USD'
  });

  const agents = $derived(
    view ? [...view.agents].sort((a, b) => b.spent_usd - a.spent_usd) : []
  );
  const taskCount = $derived(view ? view.tasks.filter((t) => t.task_id).length : 0);
  const sessionCount = $derived(view ? view.sessions.length : 0);

  function displayRole(role: string): 'planner' | 'generator' | 'evaluator' | 'other' {
    switch (role) {
      case 'planner':
      case 'generator':
      case 'evaluator':
        return role;
      default:
        return 'other';
    }
  }

  function shortId(agentId: string): string {
    return agentId.length > 12 ? `${agentId.slice(0, 12)}...` : agentId;
  }

  function pct(spentUsd: number): number {
    return view && view.spent_usd > 0 ? Math.round((spentUsd / view.spent_usd) * 100) : 0;
  }
</script>

<div
  class="flex flex-col gap-2 rounded-md border p-3"
  style="border-color: var(--border-subtle); background: var(--surface-panel);"
  aria-label="Agent cost breakdown"
>
  {#if !view || agents.length === 0}
    <p class="text-[11px]" style="color: var(--fg-muted);">No agent activity yet</p>
  {:else}
    <div class="flex items-center justify-between text-[11px]">
      <span class="uppercase tracking-wider" style="color: var(--fg-label);">Agents</span>
      <span class="font-mono" style="color: var(--fg-muted);">
        {agents.length} · {taskCount} tasks · {sessionCount} sessions
      </span>
    </div>
    <div class="flex flex-col gap-1.5">
      {#each agents as agent (agent.agent_id)}
        {@const role = displayRole(agent.role)}
        <div class="grid grid-cols-[auto_minmax(0,1fr)_auto_auto] items-center gap-2 text-[11px]">
          <span
            class="role-badge role-{role} inline-flex items-center rounded-full border px-2 py-0.5 text-[10px] font-semibold"
          >
            {role}
          </span>
          <span
            class="truncate font-mono"
            style="color: var(--fg-default);"
            title={agent.agent_id}
          >
            {shortId(agent.agent_id)}
          </span>
          <span class="font-mono" style="color: var(--fg-muted);">{agent.sessions} ses</span>
          <span class="font-mono" style="color: var(--fg-default);">
            {money.format(agent.spent_usd)}
            <span style="color: var(--fg-muted);">({pct(agent.spent_usd)}%)</span>
          </span>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .role-badge {
    background: var(--surface-soft, var(--surface-titlebar));
    color: var(--fg-muted);
    border-color: var(--border-subtle);
  }

  .role-planner {
    color: #0f52c8;
    background: rgba(15, 82, 200, 0.1);
    border-color: rgba(15, 82, 200, 0.28);
  }

  .role-generator {
    color: var(--dot-success);
    background: color-mix(in srgb, var(--dot-success) 10%, transparent);
    border-color: color-mix(in srgb, var(--dot-success) 28%, transparent);
  }

  .role-evaluator {
    color: var(--dot-warn);
    background: color-mix(in srgb, var(--dot-warn) 10%, transparent);
    border-color: color-mix(in srgb, var(--dot-warn) 28%, transparent);
  }
</style>
