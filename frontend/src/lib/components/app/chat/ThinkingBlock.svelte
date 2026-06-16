<script lang="ts">
  import { ChevronDown, ChevronUp, Loader2 } from '$lib/icons';
  import { formatDuration, thinkingTail } from './format';

  interface Props {
    thinking: string;
    streaming: boolean;
    expanded: boolean;
    durationMs?: number;
    onToggle: () => void;
  }

  let { thinking, streaming, expanded, durationMs, onToggle }: Props = $props();

  function thinkingScroll(
    node: HTMLElement,
    _v: string
  ): { update: () => void; destroy: () => void } {
    node.scrollTop = node.scrollHeight;
    return {
      update() {
        node.scrollTop = node.scrollHeight;
      },
      destroy() {}
    };
  }
</script>

<div class="action-block thinking-block" class:action-open={expanded}>
  <button type="button" onclick={onToggle} class="action-header">
    <span class="action-icon subtle">
      {#if streaming}
        <Loader2 size={13} class="animate-spin" />
      {:else}
        <ChevronDown size={13} />
      {/if}
    </span>
    {#if streaming}
      <span class="action-title thinking-live-title">
        Thinking<span class="thinking-dots" aria-hidden="true"
          ><span>.</span><span>.</span><span>.</span></span
        >
      </span>
      <span class="action-preview"></span>
    {:else}
      <span class="action-title">
        Thought{#if durationMs}&thinsp;<span class="thinking-dur"
            >({formatDuration(durationMs)})</span
          >{/if}
      </span>
      <span class="action-preview">{thinking.slice(0, 90)}</span>
    {/if}
    <span class="action-caret">
      {#if expanded}<ChevronUp size={13} />{:else}<ChevronDown size={13} />{/if}
    </span>
  </button>
  {#if expanded}
    {#if streaming}
      <div class="action-detail thinking-detail thinking-tail" use:thinkingScroll={thinking}>
        {thinkingTail(thinking)}
      </div>
    {:else}
      <div class="action-detail thinking-detail">
        {thinking}
      </div>
    {/if}
  {/if}
</div>

<style>
  .action-block {
    background: color-mix(in srgb, var(--surface-window) 66%, transparent);
    border: 1px solid var(--border-subtle);
    border-radius: 0.7rem;
    margin-bottom: 0.75rem;
    overflow: hidden;
  }

  .action-open {
    background: var(--surface-window);
  }

  .action-header {
    align-items: center;
    color: var(--fg-muted);
    display: grid;
    font-size: 0.74rem;
    gap: 0.55rem;
    grid-template-columns: 1.15rem minmax(max-content, 11rem) minmax(0, 1fr) auto;
    line-height: 1.4;
    min-height: 2.35rem;
    padding: 0.48rem 0.65rem;
    text-align: left;
    width: 100%;
  }

  .action-header:hover {
    background: color-mix(in srgb, var(--fg-default) 4%, transparent);
  }

  .action-icon {
    align-items: center;
    border-radius: 999px;
    color: var(--fg-muted);
    display: flex;
    justify-content: center;
  }

  .action-icon.subtle {
    color: var(--accent);
  }

  .action-title {
    color: var(--fg-default);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .action-preview {
    color: var(--fg-muted);
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .action-caret {
    color: var(--fg-muted);
    display: flex;
  }

  .action-detail {
    border-top: 1px solid var(--border-subtle);
    padding: 0.65rem;
  }

  .thinking-detail {
    color: var(--fg-muted);
    font-size: 0.78rem;
    font-style: italic;
    line-height: 1.65;
    white-space: pre-wrap;
    word-break: break-word;
  }

  .thinking-tail {
    max-height: 200px;
    overflow-y: auto;
    scroll-behavior: auto;
  }

  .thinking-live-title {
    white-space: nowrap;
  }

  .thinking-dots span {
    animation: thinking-blink 1.4s ease-in-out infinite;
    display: inline;
    opacity: 0;
  }

  .thinking-dots span:nth-child(2) {
    animation-delay: 0.22s;
  }

  .thinking-dots span:nth-child(3) {
    animation-delay: 0.44s;
  }

  @keyframes thinking-blink {
    0%,
    80%,
    100% {
      opacity: 0;
    }
    40% {
      opacity: 1;
    }
  }

  .thinking-dur {
    color: var(--fg-muted);
    font-size: 0.64rem;
    font-weight: 400;
  }
</style>
