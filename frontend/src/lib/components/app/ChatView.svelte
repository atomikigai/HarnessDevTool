<script lang="ts">
  import { onDestroy } from 'svelte';
  import { api, type SessionMeta, type TranscriptEvent } from '$lib/api/client';
  import { subscribeSSE, type SSEHandle } from '$lib/api/sse';
  import { Bot, CircleAlert, Loader2, Send, User, Wrench } from '$lib/icons';
  import { toast } from 'svelte-sonner';

  interface Props {
    session: SessionMeta;
    stopped?: boolean;
  }

  let { session, stopped = false }: Props = $props();

  type LocalEvent = TranscriptEvent & { local?: boolean };

  let events = $state<LocalEvent[]>([]);
  let seenSeq = new Set<number>();
  let input = $state('');
  let sending = $state(false);
  let connected = $state(false);
  let transcriptUnavailable = $state(false);
  let sse: SSEHandle | null = null;

  const encoder = new TextEncoder();

  const visibleEvents = $derived(
    events.filter((event) => event.kind !== 'meta' && event.kind !== 'system_note')
  );

  function closeStream() {
    sse?.close();
    sse = null;
  }

  function resetStream() {
    closeStream();
    events = [];
    seenSeq = new Set();
    connected = false;
    transcriptUnavailable = !session.has_transcript;
    if (!session.has_transcript) return;

    sse = subscribeSSE<TranscriptEvent>(
      `/sessions/${encodeURIComponent(session.id)}/transcript`,
      () => {},
      {
        reconnect: true,
        maxReconnectAttempts: 20,
        onOpen: () => {
          connected = true;
        },
        onError: () => {
          connected = false;
        },
        events: {
          transcript: (data) => {
            const event = data as TranscriptEvent;
            if (!event || typeof event.seq !== 'number') return;
            if (seenSeq.has(event.seq)) return;
            seenSeq.add(event.seq);
            removeMatchingOptimisticUserMessage(event);
            events = [...events, event];
          }
        }
      }
    );
  }

  function removeMatchingOptimisticUserMessage(event: TranscriptEvent) {
    if (event.kind !== 'message' || event.role !== 'user' || !event.content) return;
    const index = events.findIndex(
      (candidate) =>
        candidate.local &&
        candidate.kind === 'message' &&
        candidate.role === 'user' &&
        candidate.content?.trim() === event.content?.trim()
    );
    if (index >= 0) {
      events = [...events.slice(0, index), ...events.slice(index + 1)];
    }
  }

  async function sendMessage() {
    const text = input.trim();
    if (!text || sending || stopped) return;
    sending = true;
    input = '';
    events = [
      ...events,
      {
        seq: -Date.now(),
        session_id: session.id,
        ts: new Date().toISOString(),
        source: session.kind === 'codex' ? 'codex' : 'claude',
        kind: 'message',
        role: 'user',
        content: text,
        local: true
      }
    ];

    try {
      await api.sessions.input(session.id, encoder.encode(text));
      await new Promise((resolve) => setTimeout(resolve, 60));
      await api.sessions.input(session.id, encoder.encode('\r'));
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      toast.error(`Send failed: ${message}`);
      events = [
        ...events,
        {
          seq: -Date.now() - 1,
          session_id: session.id,
          ts: new Date().toISOString(),
          source: session.kind === 'codex' ? 'codex' : 'claude',
          kind: 'system_note',
          subtype: 'send_failed',
          content: message,
          is_error: true,
          local: true
        }
      ];
    } finally {
      sending = false;
    }
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault();
      void sendMessage();
    }
  }

  function displayJson(value: unknown): string {
    if (value == null) return '';
    if (typeof value === 'string') return value;
    try {
      return JSON.stringify(value, null, 2);
    } catch {
      return String(value);
    }
  }

  function clipped(value: unknown, max = 2400): string {
    const text = displayJson(value);
    if (text.length <= max) return text;
    return `${text.slice(0, max)}\n\n[truncated ${text.length - max} chars]`;
  }

  function eventTitle(event: LocalEvent): string {
    if (event.kind === 'thinking') return 'Reasoning';
    if (event.kind === 'tool_call') return event.tool_name ?? 'Tool';
    if (event.kind === 'tool_result') return event.is_error ? 'Tool error' : 'Tool result';
    if (event.role === 'user') return 'You';
    return session.kind;
  }

  $effect(() => {
    session.id;
    resetStream();
  });

  onDestroy(closeStream);
</script>

<section class="flex h-full min-h-0 flex-col bg-[var(--surface-canvas)]">
  <div
    class="flex h-9 shrink-0 items-center justify-between border-b px-4 text-xs"
    style="border-color: var(--border-subtle); background: var(--surface-window); color: var(--fg-muted);"
  >
    <div class="flex min-w-0 items-center gap-2">
      <Bot class="h-3.5 w-3.5 shrink-0" />
      <span class="truncate font-medium" style="color: var(--fg-default);">
        {session.kind} chat
      </span>
      <span class="font-mono">· {session.id.slice(0, 8)}</span>
    </div>
    <div class="flex items-center gap-2">
      {#if transcriptUnavailable}
        <span class="inline-flex items-center gap-1.5" style="color: var(--fg-muted);">
          <CircleAlert class="h-3.5 w-3.5" />
          transcript unavailable
        </span>
      {:else if connected}
        <span class="inline-flex items-center gap-1.5" style="color: var(--dot-success);">
          <span class="h-1.5 w-1.5 rounded-full" style="background: var(--dot-success);"></span>
          live
        </span>
      {:else}
        <span class="inline-flex items-center gap-1.5">
          <Loader2 class="h-3.5 w-3.5 animate-spin" />
          connecting
        </span>
      {/if}
    </div>
  </div>

  <div class="min-h-0 flex-1 overflow-y-auto px-4 py-4">
    {#if visibleEvents.length === 0}
      <div class="flex h-full items-center justify-center">
        <div class="max-w-md text-center">
          <p class="text-sm font-medium" style="color: var(--fg-default);">
            Start a conversation with this session.
          </p>
          <p class="mt-1 text-xs" style="color: var(--fg-muted);">
            Messages, reasoning, and tool activity appear here when the CLI exposes a structured transcript.
          </p>
        </div>
      </div>
    {:else}
      <div class="mx-auto flex max-w-4xl flex-col gap-3">
        {#each visibleEvents as event (`${event.seq}-${event.kind}-${event.tool_use_id ?? ''}`)}
          {@const isUser = event.role === 'user'}
          {@const isTool = event.kind === 'tool_call' || event.kind === 'tool_result'}
          <article
            class="flex gap-3 rounded-md border px-3 py-3"
            style="
              border-color: {isUser ? 'var(--accent-soft-border)' : 'var(--border-subtle)'};
              background: {isUser ? 'var(--accent-soft)' : 'var(--surface-panel)'};
            "
          >
            <div
              class="flex h-7 w-7 shrink-0 items-center justify-center rounded-md border"
              style="border-color: var(--border-subtle); background: var(--surface-titlebar); color: {isUser ? 'var(--accent)' : 'var(--fg-muted)'};"
            >
              {#if isUser}
                <User class="h-4 w-4" />
              {:else if isTool}
                <Wrench class="h-4 w-4" />
              {:else}
                <Bot class="h-4 w-4" />
              {/if}
            </div>
            <div class="min-w-0 flex-1">
              <div class="mb-1 flex items-center gap-2 text-xs font-semibold" style="color: var(--fg-default);">
                <span>{eventTitle(event)}</span>
                {#if event.local}
                  <span class="font-normal" style="color: var(--fg-muted);">sending</span>
                {/if}
              </div>
              {#if event.kind === 'message' || event.kind === 'thinking'}
                <div class="whitespace-pre-wrap text-sm leading-6" style="color: var(--fg-default);">
                  {event.content}
                </div>
              {:else if event.kind === 'tool_call'}
                <pre class="max-h-72 overflow-auto rounded border p-3 text-xs" style="border-color: var(--border-subtle); background: var(--surface-canvas); color: var(--fg-muted);">{clipped(event.tool_args)}</pre>
              {:else if event.kind === 'tool_result'}
                <pre class="max-h-72 overflow-auto rounded border p-3 text-xs" style="border-color: var(--border-subtle); background: var(--surface-canvas); color: var(--fg-muted);">{clipped(event.tool_result)}</pre>
              {:else}
                <pre class="max-h-72 overflow-auto rounded border p-3 text-xs" style="border-color: var(--border-subtle); background: var(--surface-canvas); color: var(--fg-muted);">{clipped(event.raw ?? event.content)}</pre>
              {/if}
            </div>
          </article>
        {/each}
      </div>
    {/if}
  </div>

  <div
    class="shrink-0 border-t p-3"
    style="border-color: var(--border-subtle); background: var(--surface-window);"
  >
    <div class="mx-auto flex max-w-4xl items-end gap-2">
      <textarea
        bind:value={input}
        onkeydown={onKeydown}
        rows="2"
        disabled={stopped}
        placeholder={stopped ? 'Session is stopped' : 'Ask the agent...'}
        class="min-h-11 flex-1 resize-none rounded-md border px-3 py-2 text-sm outline-none"
        style="border-color: var(--border-input); background: var(--surface-panel); color: var(--fg-default);"
      ></textarea>
      <button
        type="button"
        onclick={sendMessage}
        disabled={sending || stopped || input.trim().length === 0}
        class="inline-flex h-11 shrink-0 items-center gap-2 rounded-md px-4 text-sm font-semibold text-white disabled:opacity-50"
        style="background: var(--accent);"
      >
        <Send class="h-4 w-4" />
        Send
      </button>
    </div>
  </div>
</section>
