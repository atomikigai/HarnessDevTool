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
    done || !agentIsWorking ? 'Response (terminal output)' : 'Live terminal output…'
  );
</script>

<details class="pty-block" open={done || !agentIsWorking}>
  <summary class="pty-block-summary">
    <Terminal size={11} class="pty-block-icon" />
    <span>{label}</span>
    {#if onSwitchToTerminal}
      <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
      <span
        role="button"
        tabindex="0"
        class="pty-terminal-link"
        onclick={(e) => {
          e.preventDefault();
          e.stopPropagation();
          onSwitchToTerminal?.();
        }}
      >
        View in Terminal tab
      </span>
    {/if}
  </summary>
  <pre class="pty-fallback-text">{content}</pre>
</details>

<style>
  .pty-block {
    background: color-mix(in srgb, var(--surface-window) 60%, transparent);
    border: 1px solid var(--border-subtle);
    border-radius: 0.55rem;
    margin-bottom: 0.5rem;
    overflow: hidden;
  }

  .pty-block-summary {
    align-items: center;
    color: var(--fg-muted);
    cursor: pointer;
    display: flex;
    font-size: 0.74rem;
    gap: 0.45rem;
    list-style: none;
    padding: 0.45rem 0.65rem;
    user-select: none;
  }

  .pty-block-summary::-webkit-details-marker {
    display: none;
  }

  .pty-block-summary:hover {
    background: color-mix(in srgb, var(--fg-default) 4%, transparent);
  }

  :global(.pty-block-icon) {
    color: var(--accent);
    flex-shrink: 0;
  }

  .pty-fallback-text {
    background: transparent;
    border: 0;
    color: var(--fg-default);
    font-family:
      ui-sans-serif,
      system-ui,
      -apple-system,
      BlinkMacSystemFont,
      'Segoe UI',
      sans-serif;
    font-size: 0.9rem;
    line-height: 1.72;
    margin: 0;
    max-width: 74ch;
    overflow-x: auto;
    padding: 0;
    white-space: pre-wrap;
    word-break: break-word;
  }

  .pty-terminal-link {
    border: 1px solid var(--border-subtle);
    border-radius: 999px;
    cursor: pointer;
    font-size: 0.63rem;
    margin-left: auto;
    padding: 0.1rem 0.5rem;
    transition: background 100ms ease;
  }

  .pty-terminal-link:hover {
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    border-color: var(--accent-soft-border);
    color: var(--accent);
  }
</style>
