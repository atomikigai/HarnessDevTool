<script lang="ts">
  import { Terminal } from '$lib/icons';

  interface Props {
    content: string;
    done?: boolean;
    agentIsWorking?: boolean;
    onSwitchToTerminal?: () => void;
  }

  let { content, done = false, agentIsWorking = false, onSwitchToTerminal }: Props = $props();

  const label = $derived(
    done || !agentIsWorking ? 'Terminal transcript available' : 'Live terminal output'
  );
  const byteLabel = $derived(
    content.length > 1000 ? `${Math.round(content.length / 100) / 10}k chars` : `${content.length} chars`
  );
</script>

<section class="pty-block" aria-label={label}>
  <div class="pty-block-summary">
    <span class="pty-block-icon">
      <Terminal size={12} />
    </span>
    <span class="pty-title">{label}</span>
    <span class="pty-byte-count">{byteLabel}</span>
    {#if onSwitchToTerminal}
      <button type="button" class="pty-terminal-link" onclick={() => onSwitchToTerminal?.()}>
        View Terminal
      </button>
    {/if}
  </div>

  <p class="pty-empty">Raw terminal output is available in the Terminal tab.</p>
</section>

<style>
  .pty-block {
    background: color-mix(in srgb, var(--surface-window) 72%, transparent);
    border: 1px solid var(--border-subtle);
    border-radius: 0.6rem;
    margin-bottom: 0.75rem;
    overflow: hidden;
  }

  .pty-block-summary {
    align-items: center;
    color: var(--fg-muted);
    display: flex;
    font-size: 0.74rem;
    gap: 0.45rem;
    min-height: 2.35rem;
    padding: 0.5rem 0.65rem;
  }

  .pty-block-icon {
    align-items: center;
    color: var(--accent);
    display: flex;
    flex-shrink: 0;
  }

  .pty-title {
    color: var(--fg-default);
    font-weight: 600;
    min-width: 0;
  }

  .pty-byte-count {
    color: var(--fg-muted);
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-size: 0.64rem;
    margin-left: 0.15rem;
  }

  .pty-empty {
    border-block: 1px solid var(--border-subtle);
    color: var(--fg-muted);
    font-size: 0.78rem;
    margin: 0;
    padding: 0.65rem 0.75rem;
  }

  .pty-terminal-link {
    background: color-mix(in srgb, var(--surface-canvas) 80%, transparent);
    border: 1px solid var(--border-subtle);
    border-radius: 999px;
    color: var(--fg-muted);
    cursor: pointer;
    font-size: 0.63rem;
    margin-left: auto;
    padding: 0.16rem 0.55rem;
    transition: background 100ms ease;
    white-space: nowrap;
  }

  .pty-terminal-link:hover {
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    border-color: var(--accent-soft-border);
    color: var(--accent);
  }

  @media (max-width: 640px) {
    .pty-block-summary {
      align-items: flex-start;
      flex-wrap: wrap;
    }

    .pty-terminal-link {
      margin-left: 0;
    }
  }
</style>
