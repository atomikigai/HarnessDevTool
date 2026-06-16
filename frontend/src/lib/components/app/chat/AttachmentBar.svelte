<script lang="ts">
  import { Download, FileCode2, FileJson, FileSpreadsheet, FileText } from '$lib/icons';
  import { attachmentUrl, type AttachedFile } from '$lib/api/client';
  import { fileIconName, formatSize, isImageMime } from './format';

  interface Props {
    sessionId: string;
    attachments: AttachedFile[];
    openLightbox: (src: string) => void;
  }

  let { sessionId, attachments, openLightbox }: Props = $props();
</script>

{#if attachments.length > 0}
  <div class="attachment-bar">
    {#each attachments as file (file.name)}
      {#if isImageMime(file.mime)}
        <button
          type="button"
          class="attachment-thumb"
          onclick={() => openLightbox(attachmentUrl(sessionId, file.name))}
          title={file.name}
        >
          <img
            src={attachmentUrl(sessionId, file.name)}
            alt={file.name}
            class="attachment-thumb-img"
          />
        </button>
      {:else}
        {@const icon = fileIconName(file.mime)}
        <a
          href={attachmentUrl(sessionId, file.name)}
          download={file.name}
          target="_blank"
          rel="noreferrer"
          class="attachment-doc"
          title={`Download ${file.name}`}
        >
          <span class="attachment-doc-icon">
            {#if icon === 'json'}
              <FileJson size={14} />
            {:else if icon === 'spreadsheet'}
              <FileSpreadsheet size={14} />
            {:else if icon === 'code'}
              <FileCode2 size={14} />
            {:else}
              <FileText size={14} />
            {/if}
          </span>
          <span class="attachment-doc-name">{file.name}</span>
          <span class="attachment-doc-size">{formatSize(file.size)}</span>
          <span class="attachment-doc-dl"><Download size={11} /></span>
        </a>
      {/if}
    {/each}
  </div>
{/if}

<style>
  .attachment-bar {
    border-bottom: 1px solid var(--border-subtle);
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    padding: 0.6rem 1rem;
  }

  .attachment-thumb {
    background: none;
    border: 1px solid var(--border-subtle);
    border-radius: 0.45rem;
    cursor: zoom-in;
    display: block;
    flex-shrink: 0;
    overflow: hidden;
    padding: 0;
  }

  .attachment-thumb-img {
    display: block;
    height: 48px;
    object-fit: cover;
    width: 48px;
  }

  .attachment-doc {
    align-items: center;
    background: color-mix(in srgb, var(--surface-canvas) 60%, transparent);
    border: 1px solid var(--border-subtle);
    border-radius: 0.45rem;
    color: var(--fg-muted);
    display: inline-flex;
    flex-shrink: 0;
    font-size: 0.72rem;
    gap: 0.35rem;
    max-width: 180px;
    padding: 0.35rem 0.5rem;
    text-decoration: none;
    transition: background 120ms ease;
  }

  .attachment-doc:hover {
    background: color-mix(in srgb, var(--fg-default) 6%, transparent);
    color: var(--fg-default);
  }

  .attachment-doc-icon,
  .attachment-doc-dl {
    display: flex;
    flex-shrink: 0;
  }

  .attachment-doc-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .attachment-doc-size {
    flex-shrink: 0;
    font-size: 0.63rem;
    opacity: 0.7;
  }

  .attachment-doc-dl {
    opacity: 0.6;
  }
</style>
