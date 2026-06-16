<script lang="ts">
  import 'katex/dist/katex.min.css';
  import { AlertTriangle, Bot, Link2, Loader2, User } from '$lib/icons';
  import { formatDuration, formatInt, usageLabel } from './format';
  import { highlightRenderedMarkdown } from './markdown';
  import PtyFallbackBlock from './PtyFallbackBlock.svelte';
  import ThinkingBlock from './ThinkingBlock.svelte';
  import ToolBlockView from './ToolBlockView.svelte';
  import './prose.css';
  import type { ChatTurn, PrevTurn } from './types';

  interface Props {
    turns: ChatTurn[];
    historicalTurns: PrevTurn[];
    historicalLoaded: boolean;
    fallbackOutputBytes: number;
    fallbackDone: boolean;
    agentIsWorking: boolean;
    showWorkingIndicator: boolean;
    workingStatus:
      | { kind: 'idle' }
      | { kind: 'tool'; name: string }
      | { kind: 'thinking'; tail: string };
    stopped: boolean;
    openLightbox: (src: string) => void;
    onSwitchToTerminal?: () => void;
    onCliApprovalChoice?: (choice: '1' | '2' | '3') => void | Promise<void>;
  }

  let {
    turns,
    historicalTurns,
    historicalLoaded,
    fallbackOutputBytes,
    fallbackDone,
    agentIsWorking,
    showWorkingIndicator,
    workingStatus,
    stopped,
    openLightbox,
    onSwitchToTerminal,
    onCliApprovalChoice
  }: Props = $props();

  let thinkingExpanded = $state<Record<string, boolean>>({});

  function hlAction(node: HTMLElement): { destroy: () => void } {
    return highlightRenderedMarkdown(node, openLightbox);
  }

  function toggleThinking(turnId: string, streaming: boolean): void {
    thinkingExpanded[turnId] = !(thinkingExpanded[turnId] ?? streaming);
  }

  function isThinkingExpanded(turnId: string, streaming: boolean): boolean {
    if (turnId in thinkingExpanded) return thinkingExpanded[turnId];
    return streaming;
  }
</script>

{#snippet workingIndicator()}
  <div
    class="chat-turn assistant-turn awaiting-turn"
    aria-live="polite"
    aria-label="Agent is processing"
  >
    <div class="turn-rail">
      <div class="turn-avatar agent-avatar"><Bot size={14} /></div>
    </div>
    <div class="turn-body">
      <div class="turn-meta"><span>Agent</span></div>
      <div class="awaiting-indicator">
        <span class="processing-dots" aria-label="Processing">
          <span></span><span></span><span></span>
        </span>
        {#if workingStatus.kind === 'tool'}
          <span class="working-status-line"
            >Running <span class="working-status-name">{workingStatus.name}</span>…</span
          >
        {:else if workingStatus.kind === 'thinking'}
          <span class="working-status-line working-status-thinking">{workingStatus.tail}</span>
        {:else}
          <span class="working-status-line working-status-idle">Working…</span>
        {/if}
      </div>
    </div>
  </div>
{/snippet}

<div class="chat-thread mx-auto max-w-[820px] px-5 py-8 sm:px-7">
  {#if historicalLoaded && historicalTurns.length > 0}
    <div class="prev-history-wrap">
      {#each historicalTurns as ht (ht.id)}
        <div class="prev-turn prev-turn-{ht.role}">
          <span class="prev-turn-label">{ht.role === 'user' ? 'You' : 'Agent'}</span>
          <p class="prev-turn-content">
            {ht.content.length > 600 ? ht.content.slice(0, 600) + '…' : ht.content}
          </p>
        </div>
      {/each}
    </div>
    <div class="session-restart-sep">— session restarted —</div>
  {/if}

  {#each turns as turn (turn.id)}
    {#if turn.role === 'user'}
      <div
        class="chat-turn user-turn"
        style="contain: content; content-visibility: auto; contain-intrinsic-size: 80px;"
      >
        <div class="turn-rail">
          <div class="turn-avatar user-avatar"><User size={14} /></div>
          <div class="turn-line"></div>
        </div>
        <div class="turn-body">
          <div class="turn-meta">
            <span>You</span>
          </div>
          <div class="chat-user-text whitespace-pre-wrap break-words">
            {turn.content}
          </div>
        </div>
      </div>
    {:else if turn.role === 'assistant'}
      <div
        class="chat-turn assistant-turn"
        style="contain: content; content-visibility: auto; contain-intrinsic-size: 220px;"
      >
        <div class="turn-rail">
          <div class="turn-avatar agent-avatar"><Bot size={14} /></div>
          <div class="turn-line"></div>
        </div>
        <div class="turn-body">
          <div class="turn-meta">
            <span>Agent</span>
            {#if turn.model}
              <span class="meta-chip">{turn.model}</span>
            {:else if turn.source}
              <span class="meta-chip">{turn.source}</span>
            {/if}
            {#if turn.source === 'pty' && fallbackOutputBytes > 0}
              <span class="meta-dot"></span>
              <span class="usage-chip">{formatInt(fallbackOutputBytes)} chars</span>
            {/if}
            {#if usageLabel(turn.usage)}
              <span class="meta-dot"></span>
              <span class="usage-chip">{usageLabel(turn.usage)}</span>
            {/if}
          </div>

          {#if turn.thinking}
            {@const thinkingActive = turn.isStreaming && !turn.content}
            {@const expanded = isThinkingExpanded(turn.id, thinkingActive)}
            <ThinkingBlock
              thinking={turn.thinking}
              streaming={thinkingActive}
              {expanded}
              durationMs={turn.durationMs}
              onToggle={() => toggleThinking(turn.id, thinkingActive)}
            />
          {/if}

          {#each turn.toolBlocks as block (block.id)}
            <ToolBlockView
              {block}
              {openLightbox}
              onToggle={() => {
                block.expanded = !block.expanded;
              }}
            />
          {/each}

          {#if turn.content}
            {#if turn.source === 'pty'}
              <PtyFallbackBlock
                content={turn.content}
                done={fallbackDone}
                {agentIsWorking}
                {onSwitchToTerminal}
              />
            {:else if turn.renderedHtml}
              <div class="chat-prose max-w-none leading-relaxed" use:hlAction>
                <!-- eslint-disable-next-line svelte/no-at-html-tags -->
                {@html turn.renderedHtml}
              </div>
              {#if turn.inlineImages.length > 0}
                <div class="inline-media">
                  {#each turn.inlineImages as imgSrc (imgSrc)}
                    <button
                      type="button"
                      class="img-button"
                      onclick={() => openLightbox(imgSrc)}
                      title="Click to view full size"
                    >
                      <img src={imgSrc} alt="content" class="inline-image" />
                    </button>
                  {/each}
                </div>
              {/if}
              {#each turn.excalidrawScenes as scene, i (i)}
                {#if scene.svgHtml}
                  <div class="diagram-container excalidraw-container">
                    <!-- eslint-disable-next-line svelte/no-at-html-tags -->
                    {@html scene.svgHtml}
                  </div>
                {:else if scene.failed}
                  <details class="excalidraw-fallback">
                    <summary class="excalidraw-fallback-summary">
                      Excalidraw scene (SVG render unavailable)
                    </summary>
                    <pre class="action-json">{scene.raw}</pre>
                  </details>
                {:else}
                  <div class="excalidraw-loading">
                    <Loader2 size={14} class="animate-spin" />
                    <span>Rendering diagram…</span>
                  </div>
                {/if}
              {/each}
              {#each turn.mermaidScenes as scene, i (i)}
                {#if scene.svgHtml}
                  <div class="diagram-container">
                    <!-- eslint-disable-next-line svelte/no-at-html-tags -->
                    {@html scene.svgHtml}
                  </div>
                {:else if scene.failed}
                  <details class="excalidraw-fallback diagram-fallback" open>
                    <summary class="excalidraw-fallback-summary"
                      >Mermaid diagram (render unavailable)</summary
                    >
                    {#if scene.error}<p class="diagram-error">{scene.error}</p>{/if}
                    <pre class="action-json">{scene.raw}</pre>
                  </details>
                {:else}
                  <div class="excalidraw-loading">
                    <Loader2 size={14} class="animate-spin" />
                    <span>Rendering diagram…</span>
                  </div>
                {/if}
              {/each}
              {#each turn.chartScenes as scene, i (i)}
                {#if scene.svgHtml}
                  <div class="diagram-container chart-container">
                    <!-- eslint-disable-next-line svelte/no-at-html-tags -->
                    {@html scene.svgHtml}
                  </div>
                {:else}
                  <details class="excalidraw-fallback diagram-fallback" open>
                    <summary class="excalidraw-fallback-summary">Chart (render unavailable)</summary
                    >
                    {#if scene.error}<p class="diagram-error">{scene.error}</p>{/if}
                    <pre class="action-json">{scene.raw}</pre>
                  </details>
                {/if}
              {/each}
            {:else}
              <p class="chat-streaming-text whitespace-pre-wrap break-words">
                {turn.content}
              </p>
            {/if}
          {/if}

          {#if turn.durationMs}
            <div class="turn-duration-row">
              <span class="usage-chip" title="Turn duration"
                >&#x23F1; {formatDuration(turn.durationMs)}</span
              >
            </div>
          {/if}

          {#if turn.isStreaming && !turn.content && !turn.thinking && turn.toolBlocks.length === 0}
            <span class="stream-cursor"></span>
          {/if}
        </div>
      </div>
    {:else}
      <div
        class="system-turn my-4 flex justify-center"
        style="contain: content; content-visibility: auto; contain-intrinsic-size: 32px;"
      >
        {#if turn.systemKind === 'link' && turn.systemHref}
          <a
            href={turn.systemHref}
            target="_blank"
            rel="noreferrer"
            class="system-card system-link"
          >
            <Link2 size={13} />
            <span class="system-main">{turn.content}</span>
            {#if turn.systemDetail}<span class="system-detail">{turn.systemDetail}</span>{/if}
          </a>
        {:else if turn.systemKind === 'approval'}
          <span class="system-card system-approval">
            <AlertTriangle size={13} />
            <span class="system-main">{turn.content}</span>
            {#if turn.systemDetail}<span class="system-detail">{turn.systemDetail}</span>{/if}
            {#if !stopped && onCliApprovalChoice}
              <span class="system-approval-actions">
                <button type="button" onclick={() => void onCliApprovalChoice?.('1')}>Yes</button>
                <button type="button" onclick={() => void onCliApprovalChoice?.('2')}>Always</button
                >
                <button type="button" onclick={() => void onCliApprovalChoice?.('3')}>No</button>
              </span>
            {/if}
          </span>
        {:else}
          <span>{turn.content}</span>
        {/if}
      </div>
    {/if}
  {/each}

  {#if showWorkingIndicator}
    {@render workingIndicator()}
  {/if}
</div>

<style>
  .chat-thread {
    padding-bottom: 1.5rem;
  }

  .chat-turn {
    display: grid;
    gap: 0.85rem;
    grid-template-columns: 1.75rem minmax(0, 1fr);
    color: var(--fg-default);
    margin-bottom: 2rem;
  }

  .assistant-turn {
    margin-bottom: 2.35rem;
  }

  .turn-rail {
    align-items: center;
    display: flex;
    flex-direction: column;
    gap: 0.55rem;
    padding-top: 0.1rem;
  }

  .turn-avatar {
    align-items: center;
    border: 1px solid var(--border-subtle);
    border-radius: 999px;
    display: flex;
    height: 1.75rem;
    justify-content: center;
    width: 1.75rem;
  }

  .user-avatar {
    background: var(--surface-window);
    color: var(--fg-muted);
  }

  .agent-avatar {
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    border-color: color-mix(in srgb, var(--accent) 38%, var(--border-subtle));
    color: var(--accent);
  }

  .turn-line {
    background: var(--border-subtle);
    flex: 1;
    min-height: 0.5rem;
    opacity: 0.7;
    width: 1px;
  }

  .chat-turn:last-child .turn-line {
    display: none;
  }

  .turn-body {
    min-width: 0;
  }

  .turn-meta {
    align-items: center;
    color: var(--fg-muted);
    display: flex;
    flex-wrap: wrap;
    font-size: 0.72rem;
    font-weight: 600;
    gap: 0.45rem;
    line-height: 1.4;
    margin-bottom: 0.55rem;
  }

  .meta-chip,
  .usage-chip {
    border: 1px solid var(--border-subtle);
    border-radius: 999px;
    color: var(--fg-muted);
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-size: 0.64rem;
    font-weight: 500;
    padding: 0.08rem 0.45rem;
  }

  .usage-chip {
    border-color: transparent;
    padding-inline: 0;
  }

  .meta-dot {
    background: var(--border-subtle);
    border-radius: 999px;
    height: 0.25rem;
    width: 0.25rem;
  }

  .chat-user-text,
  .chat-streaming-text {
    color: var(--fg-default);
    font-size: 0.92rem;
    line-height: 1.75;
    max-width: 74ch;
  }

  .chat-user-text {
    background: color-mix(in srgb, var(--surface-window) 72%, transparent);
    border: 1px solid var(--border-subtle);
    border-radius: 0.85rem;
    padding: 0.8rem 0.95rem;
    width: fit-content;
    max-width: min(74ch, 100%);
  }

  .turn-duration-row {
    margin-top: 0.45rem;
  }

  .awaiting-indicator {
    align-items: center;
    color: var(--fg-muted);
    display: inline-flex;
    font-size: 0.82rem;
    gap: 0.65rem;
    max-width: min(100%, 42rem);
  }

  .processing-dots {
    align-items: center;
    display: inline-flex;
    gap: 0.22rem;
  }

  .processing-dots span {
    animation: dotPulse 1.2s ease-in-out infinite;
    background: var(--accent);
    border-radius: 999px;
    display: block;
    height: 0.36rem;
    opacity: 0.35;
    width: 0.36rem;
  }

  .processing-dots span:nth-child(2) {
    animation-delay: 160ms;
  }

  .processing-dots span:nth-child(3) {
    animation-delay: 320ms;
  }

  .working-status-line {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: pre-wrap;
  }

  .working-status-name {
    color: var(--fg-default);
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
  }

  .action-json {
    background: var(--surface-canvas);
    border: 1px solid var(--border-subtle);
    border-radius: 0.45rem;
    color: var(--fg-muted);
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-size: 0.7rem;
    line-height: 1.55;
    margin: 0;
    overflow-x: auto;
    padding: 0.65rem 0.75rem;
    white-space: pre-wrap;
    word-break: break-word;
  }

  .img-button {
    background: none;
    border: none;
    cursor: zoom-in;
    display: block;
    padding: 0;
  }

  .inline-media {
    display: flex;
    flex-direction: column;
    gap: 0.65rem;
    margin-top: 0.75rem;
  }

  .inline-image {
    border: 1px solid var(--border-subtle);
    border-radius: 0.5rem;
    display: block;
    max-height: 440px;
    max-width: 100%;
    object-fit: contain;
  }

  .diagram-container,
  .excalidraw-container {
    background: #fff;
    border: 1px solid var(--border-subtle);
    border-radius: 0.6rem;
    margin-top: 0.75rem;
    max-height: 420px;
    overflow: auto;
    padding: 0.5rem;
  }

  :global(.diagram-container svg),
  :global(.excalidraw-container svg) {
    display: block;
    max-width: 100%;
    height: auto;
  }

  .chart-container {
    background: var(--surface-window);
  }

  .diagram-error {
    color: var(--danger);
    font-size: 0.75rem;
    margin: 0 0 0.4rem;
  }

  .excalidraw-fallback {
    margin-top: 0.65rem;
  }

  .excalidraw-fallback-summary {
    color: var(--fg-muted);
    cursor: pointer;
    font-size: 0.74rem;
    padding: 0.3rem 0;
    user-select: none;
  }

  .excalidraw-fallback-summary:hover {
    color: var(--fg-default);
  }

  .excalidraw-loading {
    align-items: center;
    color: var(--fg-muted);
    display: flex;
    font-size: 0.75rem;
    gap: 0.45rem;
    margin-top: 0.65rem;
  }

  .stream-cursor {
    animation: pulse 1.1s ease-in-out infinite;
    background: var(--fg-default);
    display: inline-block;
    height: 1rem;
    opacity: 0.7;
    vertical-align: -0.15rem;
    width: 0.45rem;
  }

  .system-turn > span:not(.system-card) {
    background: var(--surface-window);
    border: 1px solid var(--border-subtle);
    border-radius: 999px;
    color: var(--fg-muted);
    font-size: 0.72rem;
    padding: 0.28rem 0.75rem;
  }

  .system-card {
    align-items: center;
    background: var(--surface-window);
    border: 1px solid var(--border-subtle);
    border-radius: 999px;
    color: var(--fg-muted);
    display: inline-flex;
    flex-wrap: wrap;
    font-size: 0.72rem;
    gap: 0.35rem;
    max-width: min(100%, 42rem);
    padding: 0.28rem 0.75rem;
    text-decoration: none;
  }

  .system-card :global(svg) {
    flex: 0 0 auto;
  }

  .system-link:hover {
    border-color: var(--accent-soft-border);
    color: var(--accent);
  }

  .system-main {
    color: var(--fg-default);
    font-weight: 600;
  }

  .system-detail {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .system-approval {
    border-color: color-mix(in srgb, var(--warning, #d97706) 45%, var(--border-subtle));
  }

  .system-approval-actions {
    display: inline-flex;
    gap: 0.25rem;
    margin-left: 0.2rem;
  }

  .system-approval-actions button {
    border: 1px solid var(--border-subtle);
    border-radius: 999px;
    color: var(--fg-muted);
    font-size: 0.66rem;
    padding: 0.08rem 0.45rem;
  }

  .system-approval-actions button:hover {
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    color: var(--accent);
  }

  .prev-history-wrap {
    margin-bottom: 0.5rem;
    opacity: 0.42;
  }

  .prev-turn {
    margin-bottom: 0.85rem;
  }

  .prev-turn-label {
    color: var(--fg-muted);
    display: block;
    font-size: 0.66rem;
    font-weight: 600;
    letter-spacing: 0.03em;
    margin-bottom: 0.25rem;
    text-transform: uppercase;
  }

  .prev-turn-content {
    color: var(--fg-default);
    font-size: 0.85rem;
    line-height: 1.65;
    margin: 0;
    white-space: pre-wrap;
    word-break: break-word;
  }

  .prev-turn-user .prev-turn-content {
    background: color-mix(in srgb, var(--surface-window) 72%, transparent);
    border: 1px solid var(--border-subtle);
    border-radius: 0.7rem;
    display: inline-block;
    max-width: min(74ch, 100%);
    padding: 0.55rem 0.75rem;
  }

  .session-restart-sep {
    align-items: center;
    color: var(--fg-muted);
    display: flex;
    font-size: 0.7rem;
    gap: 0.75rem;
    letter-spacing: 0.02em;
    margin-bottom: 1.5rem;
    text-align: center;
  }

  .session-restart-sep::before,
  .session-restart-sep::after {
    background: var(--border-subtle);
    content: '';
    flex: 1;
    height: 1px;
  }

  @media (max-width: 640px) {
    .chat-thread {
      padding-inline: 1rem;
    }

    .chat-turn {
      gap: 0.65rem;
      grid-template-columns: 1.5rem minmax(0, 1fr);
    }

    .turn-avatar {
      height: 1.5rem;
      width: 1.5rem;
    }
  }

  @keyframes dotPulse {
    0%,
    80%,
    100% {
      opacity: 0.35;
      transform: translateY(0);
    }
    40% {
      opacity: 1;
      transform: translateY(-2px);
    }
  }
</style>
