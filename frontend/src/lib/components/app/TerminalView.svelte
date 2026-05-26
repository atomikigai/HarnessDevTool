<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { Terminal } from '@xterm/xterm';
  import { FitAddon } from '@xterm/addon-fit';
  import { WebLinksAddon } from '@xterm/addon-web-links';
  import { Unicode11Addon } from '@xterm/addon-unicode11';
  import '@xterm/xterm/css/xterm.css';

  import { subscribeSSE, type SSEHandle } from '$lib/api/sse';
  import { api, ApiError } from '$lib/api/client';
  import { Button } from '$lib/components/ui/button';
  import { Square } from '$lib/icons';
  import { toast } from 'svelte-sonner';

  interface Props {
    threadId: string;
    sessionId: string;
  }

  let { threadId, sessionId }: Props = $props();

  let containerEl: HTMLDivElement | null = $state(null);
  let term: Terminal | null = null;
  let fitAddon: FitAddon | null = null;
  let sse: SSEHandle | null = null;
  let ro: ResizeObserver | null = null;
  let resizeTimer: ReturnType<typeof setTimeout> | null = null;
  let killed = $state(false);
  let exited = $state(false);
  let connState = $state<'connecting' | 'open' | 'reconnecting' | 'closed'>('connecting');

  let reconnectAttempts = 0;
  let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  const encoder = new TextEncoder();

  // Reconnection policy: when SSE errors out, we close the current EventSource and
  // re-open after exponential backoff (max 10s). Because backend replays full
  // history on reconnect with seqs from 0, we reset the terminal before re-subscribing.
  // This avoids any seq-tracking gymnastics on the client.

  function b64ToBytes(b64: string): Uint8Array {
    const bin = atob(b64);
    const out = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
    return out;
  }

  function writeBytes(bytes: Uint8Array) {
    if (!term) return;
    // xterm.js accepts Uint8Array on write().
    term.write(bytes);
  }

  function openSSE() {
    if (!term) return;
    connState = reconnectAttempts > 0 ? 'reconnecting' : 'connecting';
    // Reset terminal before catch-up replay so we don't duplicate output.
    if (reconnectAttempts > 0) term.reset();

    sse = subscribeSSE(
      `/events?thread=${encodeURIComponent(threadId)}&session=${encodeURIComponent(sessionId)}`,
      () => {
        // anonymous messages: ignored — backend uses named events.
      },
      {
        onOpen: () => {
          connState = 'open';
          reconnectAttempts = 0;
        },
        onError: () => {
          if (exited || killed) {
            connState = 'closed';
            return;
          }
          // Schedule reconnect with backoff.
          connState = 'reconnecting';
          sse?.close();
          sse = null;
          const delay = Math.min(10_000, 500 * 2 ** reconnectAttempts);
          reconnectAttempts++;
          reconnectTimer = setTimeout(openSSE, delay);
        },
        events: {
          'session.started': () => {
            // could update PID; we re-fetch metadata on parent route.
          },
          'session.output': (data) => {
            const d = data as { session_id?: string; seq?: number; b64?: string };
            if (!d || typeof d.b64 !== 'string') return;
            try {
              writeBytes(b64ToBytes(d.b64));
            } catch (err) {
              console.error('failed to decode session.output', err);
            }
          },
          'session.exit': (data) => {
            const d = (data as { code?: number | null; signal?: string | null }) ?? {};
            const parts: string[] = [];
            if (d.code !== undefined && d.code !== null) parts.push(`code ${d.code}`);
            if (d.signal) parts.push(`signal ${d.signal}`);
            const tail = parts.length > 0 ? ` (${parts.join(' | ')})` : '';
            term?.write(`\r\n\x1b[33m[session ended${tail}]\x1b[0m\r\n`);
            exited = true;
            connState = 'closed';
            sse?.close();
            sse = null;
          }
        }
      }
    );
  }

  function scheduleResize() {
    if (resizeTimer) clearTimeout(resizeTimer);
    resizeTimer = setTimeout(async () => {
      if (!fitAddon || !term) return;
      try {
        fitAddon.fit();
      } catch {
        // fit may throw if not visible yet
        return;
      }
      const cols = term.cols;
      const rows = term.rows;
      try {
        await api.sessions.resize(sessionId, cols, rows);
      } catch (err) {
        // non-fatal; backend may be down
        console.warn('resize failed', err);
      }
    }, 100);
  }

  onMount(() => {
    if (!containerEl) return;

    term = new Terminal({
      cursorBlink: true,
      fontFamily:
        '"JetBrains Mono", "Fira Code", ui-monospace, SFMono-Regular, Menlo, Consolas, monospace',
      fontSize: 13,
      lineHeight: 1.2,
      allowProposedApi: true,
      scrollback: 5000,
      theme: {
        background: '#0b1220',
        foreground: '#e2e8f0',
        cursor: '#e2e8f0',
        cursorAccent: '#0b1220',
        selectionBackground: '#33415580',
        black: '#1e293b',
        red: '#f87171',
        green: '#4ade80',
        yellow: '#facc15',
        blue: '#60a5fa',
        magenta: '#c084fc',
        cyan: '#22d3ee',
        white: '#e2e8f0',
        brightBlack: '#475569',
        brightRed: '#fca5a5',
        brightGreen: '#86efac',
        brightYellow: '#fde047',
        brightBlue: '#93c5fd',
        brightMagenta: '#d8b4fe',
        brightCyan: '#67e8f9',
        brightWhite: '#f1f5f9'
      }
    });

    fitAddon = new FitAddon();
    term.loadAddon(fitAddon);
    term.loadAddon(new WebLinksAddon());
    const u11 = new Unicode11Addon();
    term.loadAddon(u11);
    term.unicode.activeVersion = '11';

    term.open(containerEl);
    try {
      fitAddon.fit();
    } catch {
      // ignored
    }

    // Char-at-a-time input forwarding — best UX for typical TTY applications.
    term.onData(async (data) => {
      if (exited || killed) return;
      try {
        await api.sessions.input(sessionId, encoder.encode(data));
      } catch (err) {
        if (err instanceof ApiError && err.status === 404) {
          // session gone; mark exited
          exited = true;
        } else {
          console.warn('input failed', err);
        }
      }
    });

    ro = new ResizeObserver(scheduleResize);
    ro.observe(containerEl);

    openSSE();
    // Initial resize push once we know cols/rows.
    scheduleResize();
  });

  onDestroy(() => {
    if (reconnectTimer) clearTimeout(reconnectTimer);
    if (resizeTimer) clearTimeout(resizeTimer);
    ro?.disconnect();
    ro = null;
    sse?.close();
    sse = null;
    term?.dispose();
    term = null;
    // Intentionally do NOT kill the session here — closing the tab keeps the child alive.
  });

  async function onKill() {
    try {
      await api.sessions.kill(sessionId);
      killed = true;
      sse?.close();
      sse = null;
      connState = 'closed';
      term?.write('\r\n\x1b[31m[killed by user]\x1b[0m\r\n');
      toast.success('Session killed');
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      toast.error(`Failed to kill session: ${msg}`);
    }
  }
</script>

<div class="flex h-full w-full flex-col" style="background: #0b1220;">
  <div
    class="flex items-center justify-between border-b px-3 py-1.5"
    style="background: var(--surface-panel); border-color: var(--border-subtle);"
  >
    <div class="flex items-center gap-2 text-xs" style="color: var(--fg-muted);">
      <span class="font-mono">{sessionId.slice(0, 8)}</span>
      {#if connState === 'open'}
        <span
          class="inline-flex items-center gap-1.5 text-[10px]"
          style="color: var(--dot-success);"
        >
          <span class="h-dot h-dot--ok"></span>
          live
        </span>
      {:else if connState === 'connecting' || connState === 'reconnecting'}
        <span class="inline-flex items-center gap-1.5 text-[10px]" style="color: var(--dot-warn);">
          <span class="h-dot h-dot--warn"></span>
          {connState}
        </span>
      {:else}
        <span class="inline-flex items-center gap-1.5 text-[10px]" style="color: var(--fg-muted);">
          <span class="h-dot"></span>
          closed
        </span>
      {/if}
    </div>
    <Button
      variant="outline"
      size="sm"
      onclick={onKill}
      disabled={killed || exited}
      title="Send SIGTERM and remove the session"
      class="!text-[var(--dot-danger)] !border-[color-mix(in_srgb,var(--dot-danger)_35%,transparent)] hover:!bg-[color-mix(in_srgb,var(--dot-danger)_10%,transparent)]"
    >
      <Square class="h-3.5 w-3.5" />
      Kill
    </Button>
  </div>
  <div bind:this={containerEl} class="min-h-0 flex-1 overflow-hidden"></div>
</div>
