<script lang="ts">
  import { api, ApiError, type AttachedFile } from '$lib/api/client';
  import { Loader2, Paperclip, RotateCcw, Send } from '$lib/icons';
  import { toast } from 'svelte-sonner';
  import AttachmentBar from './AttachmentBar.svelte';

  interface Props {
    sessionId: string | null;
    stopped: boolean;
    attachments: AttachedFile[];
    openLightbox: (src: string) => void;
    onAttachmentsChanged?: () => void | Promise<void>;
    onSent?: () => void;
    onRestart?: () => void;
  }

  let {
    sessionId,
    stopped,
    attachments,
    openLightbox,
    onAttachmentsChanged,
    onSent,
    onRestart
  }: Props = $props();

  let input = $state('');
  let sending = $state(false);
  let attaching = $state(false);
  let textareaEl: HTMLTextAreaElement | null = $state(null);
  let fileInputEl: HTMLInputElement | null = $state(null);

  const encoder = new TextEncoder();

  function pickFiles(): void {
    if (!sessionId || stopped || attaching) return;
    fileInputEl?.click();
  }

  async function onFilesPicked(ev: Event): Promise<void> {
    const sid = sessionId;
    if (!sid) return;
    const inputEl = ev.currentTarget as HTMLInputElement;
    const files = inputEl.files ? Array.from(inputEl.files) : [];
    inputEl.value = '';
    if (!files.length) return;

    attaching = true;
    try {
      const saved = await api.sessions.attach(sid, files);
      const summary = saved.map((file) => file.name).join(', ');
      toast.success(`Attached ${saved.length} file${saved.length === 1 ? '' : 's'}: ${summary}`);
      await onAttachmentsChanged?.();
    } catch (err) {
      const msg = err instanceof ApiError ? err.message : String(err);
      toast.error(`Attach failed: ${msg}`);
    } finally {
      attaching = false;
    }
  }

  async function sendInput(): Promise<void> {
    const sid = sessionId;
    if (!sid || !input.trim() || sending || stopped) return;

    sending = true;
    const attachmentNames = attachments.map((file) => file.name);
    const payload =
      attachmentNames.length > 0
        ? `${input}\n\n[Harness attachments available: ${attachmentNames.join(', ')}. Use MCP tools attach_list and attach_read to inspect them before answering.]`
        : input;
    input = '';

    try {
      await api.sessions.input(sid, encoder.encode(payload));
      await new Promise((resolve) => setTimeout(resolve, 60));
      await api.sessions.input(sid, encoder.encode('\r'));
      onSent?.();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      toast.error(`Send failed: ${msg}`);
    } finally {
      sending = false;
    }
  }

  function onKeydown(ev: KeyboardEvent): void {
    if (ev.key === 'Enter' && !ev.shiftKey) {
      ev.preventDefault();
      void sendInput();
    }
  }

  function onTextareaInput(): void {
    if (!textareaEl) return;
    textareaEl.style.height = 'auto';
    const lineHeight = 20;
    const maxHeight = lineHeight * 6 + 20;
    textareaEl.style.height = Math.min(textareaEl.scrollHeight, maxHeight) + 'px';
  }
</script>

<div class="chat-composer-wrap shrink-0 px-4 pb-4 pt-3">
  <div class="chat-composer mx-auto">
    {#if attachments.length > 0 && sessionId}
      <AttachmentBar {sessionId} {attachments} {openLightbox} />
    {/if}

    <div class="px-4 pt-3 pb-1">
      {#if stopped && onRestart}
        <div class="stopped-cta">
          <span class="stopped-label">Session not running</span>
          <button type="button" class="stopped-restart-btn" onclick={onRestart}>
            <RotateCcw size={12} />
            Restart
          </button>
        </div>
      {:else}
        <textarea
          bind:this={textareaEl}
          bind:value={input}
          onkeydown={onKeydown}
          oninput={onTextareaInput}
          placeholder={stopped ? 'Session not running' : 'Message the agent…'}
          disabled={stopped}
          rows={1}
          class="composer-textarea w-full resize-none bg-transparent outline-none disabled:cursor-not-allowed"
        ></textarea>
      {/if}
    </div>

    <div class="flex items-center justify-between px-3 pb-3">
      <input type="file" multiple class="hidden" bind:this={fileInputEl} onchange={onFilesPicked} />
      <button
        type="button"
        onclick={pickFiles}
        disabled={stopped || attaching}
        class="composer-icon-button"
        class:composer-attaching={attaching}
        title={attaching ? 'Uploading…' : 'Attach file to session'}
        aria-label="Attach file"
      >
        {#if attaching}
          <Loader2 size={16} class="animate-spin" />
        {:else}
          <Paperclip size={16} />
        {/if}
      </button>
      <button
        type="button"
        onclick={() => void sendInput()}
        disabled={sending || stopped || !input.trim()}
        class="composer-send-button"
        title="Send (Enter)"
        aria-label="Send"
      >
        <Send size={14} />
      </button>
    </div>
  </div>
</div>

<style>
  .chat-composer-wrap {
    background: linear-gradient(
      to top,
      var(--surface-canvas) 0%,
      var(--surface-canvas) 72%,
      color-mix(in srgb, var(--surface-canvas) 0%, transparent) 100%
    );
  }

  .chat-composer {
    background: var(--surface-window);
    border: 1px solid var(--border-subtle);
    border-radius: 1.05rem;
    box-shadow: 0 18px 50px rgb(0 0 0 / 0.18);
    max-width: 780px;
  }

  .composer-textarea {
    color: var(--fg-default);
    font-size: 0.9rem;
    line-height: 1.6;
    max-height: 144px;
    overflow-y: auto;
  }

  .composer-textarea::placeholder {
    color: var(--fg-muted);
    opacity: 0.72;
  }

  .composer-icon-button,
  .composer-send-button {
    align-items: center;
    border-radius: 0.7rem;
    display: flex;
    height: 2rem;
    justify-content: center;
    transition:
      opacity 120ms ease,
      transform 120ms ease,
      background 120ms ease;
    width: 2rem;
  }

  .composer-icon-button {
    color: var(--fg-muted);
  }

  .composer-icon-button:hover:not(:disabled) {
    background: color-mix(in srgb, var(--fg-default) 6%, transparent);
    color: var(--fg-default);
  }

  .composer-send-button {
    background: var(--fg-default);
    color: var(--surface-canvas);
  }

  .composer-send-button:hover {
    opacity: 0.9;
  }

  .composer-send-button:active {
    transform: scale(0.96);
  }

  .composer-icon-button:disabled,
  .composer-send-button:disabled {
    opacity: 0.34;
  }

  .stopped-cta {
    align-items: center;
    display: flex;
    gap: 0.75rem;
    justify-content: center;
    padding: 0.45rem 0;
  }

  .stopped-label {
    color: var(--fg-muted);
    font-size: 0.8rem;
  }

  .stopped-restart-btn {
    align-items: center;
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    border: 1px solid var(--accent-soft-border);
    border-radius: 0.5rem;
    color: var(--accent);
    display: inline-flex;
    font-size: 0.75rem;
    font-weight: 600;
    gap: 0.35rem;
    padding: 0.3rem 0.7rem;
    transition: background 120ms ease;
  }

  .stopped-restart-btn:hover {
    background: color-mix(in srgb, var(--accent) 20%, transparent);
  }
</style>
