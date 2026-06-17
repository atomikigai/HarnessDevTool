<script lang="ts">
  import { ChevronDown, Terminal } from '$lib/icons';

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
  const compactPreview = $derived(toCompactPreview(content));
  const byteLabel = $derived(
    content.length > 1000 ? `${Math.round(content.length / 100) / 10}k chars` : `${content.length} chars`
  );

  function toCompactPreview(value: string): string {
    const lines = value
      .replace(/[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/g, '')
      .split('\n')
      .map((line) => line.trim())
      .filter((line) => {
        if (!line) return false;
        if (/^[─━═╭╮╰╯│┃\s|+_-]+$/.test(line)) return false;
        if (/^[✢✳✶✻*·\s]+$/.test(line)) return false;
        return true;
      });

    return lines.slice(-4).join('\n');
  }
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

  {#if compactPreview}
    <pre class="pty-preview">{compactPreview}</pre>
  {:else}
    <p class="pty-empty">Structured transcript is not available yet.</p>
  {/if}

  <details class="pty-raw">
    <summary>
      <ChevronDown size={12} />
      <span>Raw PTY text</span>
    </summary>
    <pre class="pty-fallback-text">{content}</pre>
  </details>
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

  .pty-preview {
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 4;
    background: var(--surface-canvas);
    border-block: 1px solid var(--border-subtle);
    color: var(--fg-default);
    display: -webkit-box;
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-size: 0.73rem;
    line-height: 1.55;
    line-clamp: 4;
    margin: 0;
    max-height: 6.4rem;
    overflow: hidden;
    padding: 0.65rem 0.75rem;
    white-space: pre-wrap;
    word-break: break-word;
  }

  .pty-empty {
    border-block: 1px solid var(--border-subtle);
    color: var(--fg-muted);
    font-size: 0.78rem;
    margin: 0;
    padding: 0.65rem 0.75rem;
  }

  .pty-raw {
    color: var(--fg-muted);
    font-size: 0.7rem;
  }

  .pty-raw summary {
    align-items: center;
    cursor: pointer;
    display: flex;
    gap: 0.35rem;
    padding: 0.42rem 0.65rem;
    user-select: none;
  }

  .pty-raw summary:hover {
    color: var(--fg-default);
  }

  .pty-raw summary::-webkit-details-marker {
    display: none;
  }

  .pty-fallback-text {
    background: var(--surface-canvas);
    border-top: 1px solid var(--border-subtle);
    color: var(--fg-default);
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-size: 0.7rem;
    line-height: 1.5;
    margin: 0;
    max-height: 18rem;
    overflow: auto;
    padding: 0.65rem 0.75rem;
    white-space: pre-wrap;
    word-break: break-word;
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
