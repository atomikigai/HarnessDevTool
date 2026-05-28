<script lang="ts">
  import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle
  } from '$lib/components/ui/dialog';
  import { Button } from '$lib/components/ui/button';
  import { approvalsState } from '$lib/stores/approvals.svelte';
  import type { Decision } from '$lib/api/types/Decision';
  import type { RememberScope } from '$lib/api/types/RememberScope';

  type ScopeOption = {
    value: RememberScope;
    label: string;
    disabled?: boolean;
  };

  const SCOPES: ScopeOption[] = [
    { value: 'this_call', label: 'This call', disabled: true },
    { value: 'tool_only', label: 'Tool only' },
    { value: 'tool_and_args', label: 'Tool and args' }
  ];

  let remember = $state(false);
  let scope = $state<RememberScope>('tool_only');
  let submitting = $state(false);
  let error = $state<string | null>(null);

  const approval = $derived(approvalsState.pending[0]);
  const open = $derived(Boolean(approval));
  const origin = $derived.by(() => {
    if (!approval) return [];
    return [
      ['thread_id', approval.thread_id],
      ['session_id', approval.session_id],
      ['agent_id', approval.agent_id]
    ].filter((item): item is [string, string] => Boolean(item[1]));
  });

  let lastApprovalId: string | null = null;
  $effect(() => {
    if (approval?.id === lastApprovalId) return;
    lastApprovalId = approval?.id ?? null;
    remember = false;
    scope = 'tool_only';
    submitting = false;
    error = null;
  });

  function formatArgs(args: unknown): string {
    try {
      return JSON.stringify(args, null, 2);
    } catch {
      return String(args);
    }
  }

  function relativeTime(value: string): string {
    const time = new Date(value).getTime();
    if (Number.isNaN(time)) return value;
    const seconds = Math.max(0, Math.floor((Date.now() - time) / 1000));
    if (seconds < 60) return `${seconds}s ago`;
    const minutes = Math.floor(seconds / 60);
    if (minutes < 60) return `${minutes}m ago`;
    const hours = Math.floor(minutes / 60);
    if (hours < 24) return `${hours}h ago`;
    const days = Math.floor(hours / 24);
    return `${days}d ago`;
  }

  async function decide(decision: Decision) {
    if (!approval || submitting) return;
    submitting = true;
    error = null;
    try {
      await approvalsState.decide(approval.id, decision, remember ? scope : undefined);
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to submit decision';
    } finally {
      submitting = false;
    }
  }
</script>

{#if approval}
  <Dialog {open}>
    <DialogContent class="sm:max-w-2xl" showClose={false}>
      <DialogHeader>
        <DialogTitle>Tool approval required</DialogTitle>
        <DialogDescription>
          Review the pending tool call before allowing it to continue.
        </DialogDescription>
      </DialogHeader>

      <div class="flex flex-col gap-4 py-2">
        <div class="flex flex-col gap-1">
          <span class="text-xs font-medium uppercase text-[var(--fg-muted)]">Tool</span>
          <code class="rounded border border-[var(--border-subtle)] px-2 py-1 text-sm">
            {approval.tool}
          </code>
        </div>

        <div class="flex flex-col gap-1">
          <span class="text-xs font-medium uppercase text-[var(--fg-muted)]">Args</span>
          <pre
            class="max-h-72 overflow-auto rounded-md border border-[var(--border-subtle)] bg-[var(--surface-panel)] p-3 text-xs leading-5"
          >{formatArgs(approval.args)}</pre>
        </div>

        <div class="flex flex-col gap-1 text-xs text-[var(--fg-muted)]">
          {#each origin as [label, value] (label)}
            <div><span class="font-medium">{label}:</span> {value}</div>
          {/each}
          <div><span class="font-medium">created:</span> {relativeTime(approval.created_at)}</div>
        </div>

        <label class="flex items-center gap-2 text-sm">
          <input
            type="checkbox"
            class="h-4 w-4 rounded border-[var(--border-input)]"
            bind:checked={remember}
          />
          <span>Remember this decision</span>
        </label>

        {#if remember}
          <div class="flex flex-col gap-2" role="radiogroup" aria-label="Remember scope">
            {#each SCOPES as option (option.value)}
              <label
                class="flex items-center gap-2 text-sm data-[disabled=true]:opacity-50"
                data-disabled={option.disabled}
              >
                <input
                  type="radio"
                  name="approval-scope"
                  value={option.value}
                  checked={scope === option.value}
                  disabled={option.disabled}
                  onchange={() => {
                    scope = option.value;
                  }}
                />
                <span>{option.label}</span>
              </label>
            {/each}
          </div>
        {/if}
      </div>

      <DialogFooter class="flex-col items-stretch gap-2 sm:flex-row sm:items-center">
        {#if error}
          <p class="text-sm text-destructive sm:mr-auto">{error}</p>
        {/if}
        <Button variant="destructive" onclick={() => decide('deny')} disabled={submitting}>
          Deny
        </Button>
        <Button onclick={() => decide('allow')} disabled={submitting}>Allow</Button>
      </DialogFooter>
    </DialogContent>
  </Dialog>
{/if}
