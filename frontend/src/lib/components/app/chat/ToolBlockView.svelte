<script lang="ts">
  import { ChevronDown, ChevronUp, CircleAlert, CircleCheck, Loader2 } from '$lib/icons';
  import type { ToolBlock } from './types';
  import { extractToolResultImages, hasNonImageContent } from './media';
  import { prettyJson, toolPreview, toolState } from './format';

  interface Props {
    block: ToolBlock;
    openLightbox: (src: string) => void;
    onToggle: () => void;
  }

  let { block, openLightbox, onToggle }: Props = $props();
  let state = $derived(toolState(block));
  let resultImages = $derived(
    block.result !== undefined ? extractToolResultImages(block.result) : []
  );
  let hasRichResult = $derived(
    block.resultExcalidrawScenes.length > 0 ||
      resultImages.length > 0 ||
      block.resultInlineImages.length > 0 ||
      Boolean(block.resultText)
  );
</script>

<div
  class="action-block tool-block"
  class:tool-error={state === 'error'}
  class:action-open={block.expanded}
>
  <button type="button" onclick={onToggle} class="action-header">
    <span class="action-icon" class:error={state === 'error'} class:done={state === 'done'}>
      {#if state === 'running'}
        <Loader2 size={13} class="animate-spin" />
      {:else if state === 'error'}
        <CircleAlert size={13} />
      {:else}
        <CircleCheck size={13} />
      {/if}
    </span>
    <span class="action-title font-mono">{block.name}</span>
    <span class="action-preview">{toolPreview(block.args)}</span>
    <span class="action-state">{state}</span>
    <span class="action-caret">
      {#if block.expanded}<ChevronUp size={13} />{:else}<ChevronDown size={13} />{/if}
    </span>
  </button>
  {#if block.expanded}
    <div class="action-detail">
      <div class="action-label">Arguments</div>
      <pre class="action-json">{prettyJson(block.args)}</pre>
      {#if block.result !== undefined}
        <div class="action-label">{block.isError ? 'Error' : 'Result'}</div>
        {#if block.resultExcalidrawScenes.length > 0}
          {#each block.resultExcalidrawScenes as scene, i (i)}
            {#if scene.svgHtml}
              <div class="excalidraw-container tool-result-diagram">
                <!-- eslint-disable-next-line svelte/no-at-html-tags -->
                {@html scene.svgHtml}
              </div>
            {:else if scene.failed}
              <details class="excalidraw-fallback">
                <summary class="excalidraw-fallback-summary"
                  >Excalidraw scene (SVG render unavailable)</summary
                >
                <pre class="action-json">{scene.raw}</pre>
              </details>
            {:else}
              <div class="excalidraw-loading">
                <Loader2 size={14} class="animate-spin" />
                <span>Rendering diagram…</span>
              </div>
            {/if}
          {/each}
        {/if}
        {#if resultImages.length > 0}
          <div class="tool-result-images">
            {#each resultImages as imgSrc (imgSrc)}
              <button
                type="button"
                class="img-button"
                onclick={() => openLightbox(imgSrc)}
                title="Click to view full size"
              >
                <img src={imgSrc} alt="Tool result" class="tool-result-image" />
              </button>
            {/each}
          </div>
        {/if}
        {#if block.resultInlineImages.length > 0}
          <div class="tool-result-images">
            {#each block.resultInlineImages as imgSrc (imgSrc)}
              <button
                type="button"
                class="img-button"
                onclick={() => openLightbox(imgSrc)}
                title="Click to view full size"
              >
                <img src={imgSrc} alt="Tool result" class="tool-result-image" />
              </button>
            {/each}
          </div>
        {/if}
        {#if block.resultText}
          <pre class="tool-result-text" class:error-json={block.isError}>{block.resultText}</pre>
        {/if}
        {#if !hasRichResult || (hasNonImageContent(block.result) && !block.resultText && block.resultExcalidrawScenes.length === 0)}
          <details class="tool-raw-result" open={!hasRichResult}>
            <summary class="tool-raw-summary">Raw result</summary>
            <pre class="action-json" class:error-json={block.isError}>{prettyJson(
                block.result
              )}</pre>
          </details>
        {/if}
      {/if}
    </div>
  {/if}
</div>

<style>
  .action-block {
    background: color-mix(in srgb, var(--surface-window) 54%, transparent);
    border: 1px solid var(--border-subtle);
    border-radius: 0.6rem;
    margin-bottom: 0.55rem;
    overflow: hidden;
  }

  .action-open {
    background: var(--surface-window);
  }

  .tool-error {
    border-color: color-mix(in srgb, #ef4444 48%, var(--border-subtle));
  }

  .action-header {
    align-items: center;
    color: var(--fg-muted);
    display: grid;
    font-size: 0.74rem;
    gap: 0.5rem;
    grid-template-columns: 1.15rem minmax(7rem, 10rem) minmax(0, 1fr) auto auto;
    line-height: 1.4;
    min-height: 2.2rem;
    padding: 0.42rem 0.6rem;
    text-align: left;
    width: 100%;
  }

  .action-header:hover {
    background: color-mix(in srgb, var(--fg-default) 4%, transparent);
  }

  .action-icon,
  .action-caret {
    align-items: center;
    color: var(--fg-muted);
    display: flex;
    justify-content: center;
  }

  .action-icon.done {
    color: #34d399;
  }

  .action-icon.error {
    color: #f87171;
  }

  .action-title,
  .action-preview {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .action-title {
    color: var(--fg-default);
  }

  .action-preview {
    color: var(--fg-muted);
    min-width: 0;
  }

  .action-state {
    border: 1px solid var(--border-subtle);
    border-radius: 999px;
    color: var(--fg-muted);
    font-size: 0.63rem;
    padding: 0.06rem 0.42rem;
    white-space: nowrap;
  }

  .action-detail {
    border-top: 1px solid var(--border-subtle);
    padding: 0.65rem;
  }

  @media (max-width: 640px) {
    .action-header {
      grid-template-columns: 1.15rem minmax(0, 1fr) auto auto;
    }

    .action-preview {
      display: none;
    }
  }

  .action-label {
    color: var(--fg-muted);
    font-size: 0.62rem;
    font-weight: 700;
    letter-spacing: 0;
    margin-bottom: 0.35rem;
    text-transform: uppercase;
  }

  .action-label:not(:first-child) {
    margin-top: 0.75rem;
  }

  .action-json,
  .tool-result-text {
    background: var(--surface-canvas);
    border: 1px solid var(--border-subtle);
    border-radius: 0.45rem;
    margin: 0;
    overflow-x: auto;
    white-space: pre-wrap;
    word-break: break-word;
  }

  .action-json {
    color: var(--fg-muted);
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-size: 0.7rem;
    line-height: 1.55;
    padding: 0.65rem 0.75rem;
  }

  .tool-result-text {
    color: var(--fg-default);
    font-family:
      ui-sans-serif,
      system-ui,
      -apple-system,
      BlinkMacSystemFont,
      'Segoe UI',
      sans-serif;
    font-size: 0.82rem;
    line-height: 1.65;
    max-height: 420px;
    overflow: auto;
    padding: 0.72rem 0.8rem;
  }

  .error-json {
    color: #fca5a5;
  }

  .tool-raw-result,
  .excalidraw-fallback {
    margin-top: 0.65rem;
  }

  .tool-raw-summary,
  .excalidraw-fallback-summary {
    color: var(--fg-muted);
    cursor: pointer;
    font-size: 0.68rem;
    font-weight: 600;
    margin-bottom: 0.45rem;
  }

  .tool-result-images {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    margin-top: 0.35rem;
  }

  .img-button {
    background: none;
    border: none;
    cursor: zoom-in;
    display: block;
    padding: 0;
  }

  .tool-result-image {
    border: 1px solid var(--border-subtle);
    border-radius: 0.45rem;
    display: block;
    max-height: 320px;
    max-width: 100%;
    object-fit: contain;
  }

  .excalidraw-container {
    background: #fff;
    border: 1px solid var(--border-subtle);
    border-radius: 0.6rem;
    margin-top: 0.75rem;
    max-height: 420px;
    overflow: auto;
    padding: 0.5rem;
  }

  :global(.excalidraw-container svg) {
    display: block;
    height: auto;
    max-width: 100%;
  }

  .tool-result-diagram {
    margin-bottom: 0.7rem;
  }

  .excalidraw-loading {
    align-items: center;
    color: var(--fg-muted);
    display: flex;
    font-size: 0.75rem;
    gap: 0.45rem;
    margin-top: 0.65rem;
  }
</style>
