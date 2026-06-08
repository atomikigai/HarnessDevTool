<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { init as initGhostty, Terminal, FitAddon } from 'ghostty-web';

  import { subscribeSSE, type SSEHandle } from '$lib/api/sse';
  import { api, ApiError } from '$lib/api/client';
  import { Button } from '$lib/components/ui/button';
  import { Square } from '$lib/icons';
  import { toast } from 'svelte-sonner';

  interface Props {
    threadId: string;
    sessionId: string;
    /**
     * When true, the component renders only the terminal canvas — the small
     * header (session-id + connection state + kill button) is suppressed
     * because an outer panel already provides that chrome. Used by
     * SessionMainView in the redesigned Agents view.
     */
    embedded?: boolean;
  }

  let { threadId, sessionId, embedded = false }: Props = $props();

  let containerEl: HTMLDivElement | null = $state(null);
  let term: Terminal | null = null;
  let fitAddon: FitAddon | null = null;
  let sse: SSEHandle | null = null;
  let ro: ResizeObserver | null = null;
  let resizeTimer: ReturnType<typeof setTimeout> | null = null;
  let killed = $state(false);
  let exited = $state(false);
  let connState = $state<'connecting' | 'open' | 'reconnecting' | 'closed'>('connecting');

  // Right-click context menu state.
  let menuVisible = $state(false);
  let menuX = $state(0);
  let menuY = $state(0);
  let menuSelection = $state(''); // captured at the moment the menu opened
  let currentSelection = $state('');
  // The renderer may clear the selection as soon as right-click hits it, so
  // reading `term.getSelection()` from the contextmenu listener can return
  // empty. We track the latest non-empty selection separately and snapshot it.
  let lastSelection = $state('');
  let hideSelectionTimer: ReturnType<typeof setTimeout> | null = null;

  function openContextMenu(ev: MouseEvent) {
    if (!term) return;
    // When the running TUI app (htop, vim, tmux, lazygit) has enabled mouse
    // tracking, the right-click belongs to the app — not our copy menu.
    if (term.hasMouseTracking()) return;
    ev.preventDefault();
    // Prefer the live selection if the renderer hasn't cleared it yet; fall
    // back to the cached one.
    const live = term.getSelection() ?? '';
    menuSelection = live.length > 0 ? live : lastSelection;
    menuX = ev.clientX;
    menuY = ev.clientY;
    menuVisible = true;
  }

  function closeMenu() {
    menuVisible = false;
  }

  async function copyFromMenu() {
    await copyText(menuSelection);
    closeMenu();
  }

  async function copyText(text: string) {
    const value = text.trimEnd();
    if (!value) {
      closeMenu();
      return;
    }
    if (await writeClipboard(value)) {
      toast.success('Copied to clipboard');
    } else {
      toast.error('Clipboard write blocked');
    }
  }

  async function writeClipboard(text: string): Promise<boolean> {
    if (!text) return false;
    try {
      await navigator.clipboard.writeText(text);
      return true;
    } catch {
      const ta = document.createElement('textarea');
      ta.value = text;
      ta.setAttribute('readonly', '');
      ta.style.position = 'fixed';
      ta.style.left = '-9999px';
      ta.style.top = '0';
      document.body.appendChild(ta);
      ta.select();
      try {
        return document.execCommand('copy');
      } catch {
        return false;
      } finally {
        document.body.removeChild(ta);
      }
    }
  }

  async function pasteFromMenu() {
    closeMenu();
    if (!term) return;
    try {
      const txt = await navigator.clipboard.readText();
      if (txt) term.paste(txt);
    } catch {
      toast.error('Clipboard read blocked by the browser');
    }
  }

  function selectAllFromMenu() {
    term?.selectAll();
    closeMenu();
  }

  function activeSelection(): string {
    const live = term?.getSelection() ?? '';
    return live.length > 0 ? live : currentSelection || lastSelection;
  }

  // Close on Escape / outside click while the menu is open.
  function onWindowKey(ev: KeyboardEvent) {
    if (menuVisible && ev.key === 'Escape') {
      closeMenu();
      return;
    }

    const isMac = navigator.platform.toUpperCase().startsWith('MAC');
    const key = ev.key.toLowerCase();
    const mayCopy =
      ((isMac ? ev.metaKey : ev.ctrlKey) && !ev.altKey && key === 'c') ||
      (!isMac && ev.ctrlKey && key === 'insert');
    if (!mayCopy) return;

    const selection = activeSelection();
    const copyCombo =
      selection.length > 0 &&
      (((isMac ? ev.metaKey : ev.ctrlKey) && !ev.altKey && key === 'c') ||
        (!isMac && ev.ctrlKey && key === 'insert'));
    if (!copyCombo) return;

    ev.preventDefault();
    ev.stopPropagation();
    void copyText(selection);
  }

  let reconnectAttempts = 0;
  let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  const MAX_RECONNECT_ATTEMPTS = 30;
  const encoder = new TextEncoder();
  const INPUT_FLUSH_MS = 8;
  let inputBuffer = '';
  let inputFlushTimer: ReturnType<typeof setTimeout> | null = null;
  let inputSendChain: Promise<void> = Promise.resolve();
  let outputQueue: Uint8Array[] = [];
  let outputQueuedBytes = 0;
  let outputRaf: number | null = null;
  let scrollRaf: number | null = null;
  let lastResizeCols = 0;
  let lastResizeRows = 0;
  let resizeInFlight = false;
  let resizeQueued = false;

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

  function enqueueOutput(bytes: Uint8Array) {
    if (!term || bytes.length === 0) return;
    outputQueue.push(bytes);
    outputQueuedBytes += bytes.length;
    if (outputRaf !== null) return;
    outputRaf = requestAnimationFrame(flushOutput);
  }

  function flushOutput() {
    outputRaf = null;
    if (!term || outputQueuedBytes === 0) {
      outputQueue = [];
      outputQueuedBytes = 0;
      return;
    }

    const queued = outputQueue;
    const queuedBytes = outputQueuedBytes;
    outputQueue = [];
    outputQueuedBytes = 0;
    const bytes =
      queued.length === 1
        ? queued[0]
        : (() => {
            const merged = new Uint8Array(queuedBytes);
            let offset = 0;
            for (const chunk of queued) {
              merged.set(chunk, offset);
              offset += chunk.length;
            }
            return merged;
          })();

    const followTail = term.getViewportY() <= 1 && !term.hasSelection();
    term.write(bytes, () => {
      if (followTail) scrollTerminalToBottom();
    });
  }

  function clearOutputQueue() {
    if (outputRaf !== null) {
      cancelAnimationFrame(outputRaf);
      outputRaf = null;
    }
    outputQueue = [];
    outputQueuedBytes = 0;
  }

  function scrollTerminalToBottom() {
    const t = term;
    if (!t || t.hasSelection()) return;
    if (scrollRaf !== null) return;
    scrollRaf = requestAnimationFrame(() => {
      scrollRaf = null;
      if (!term || term.hasSelection()) return;
      term.scrollToBottom();
    });
  }

  function focusGhosttyInput() {
    const input = containerEl?.querySelector<HTMLTextAreaElement>(
      'textarea[aria-label="Terminal input"]'
    );
    if (input) {
      input.focus();
      return;
    }
    containerEl?.focus();
  }

  function openSSE() {
    if (!term) return;
    connState = reconnectAttempts > 0 ? 'reconnecting' : 'connecting';
    // Reset terminal before catch-up replay so we don't duplicate output.
    if (reconnectAttempts > 0) {
      clearOutputQueue();
      term.reset();
    }

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
          if (reconnectAttempts >= MAX_RECONNECT_ATTEMPTS) {
            connState = 'closed';
            sse?.close();
            sse = null;
            toast.error('Lost connection to session after multiple retries');
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
              enqueueOutput(b64ToBytes(d.b64));
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
            flushOutput();
            term?.write(`\r\n\x1b[33m[session ended${tail}]\x1b[0m\r\n`, scrollTerminalToBottom);
            exited = true;
            connState = 'closed';
            sse?.close();
            sse = null;
          }
        }
      }
    );
  }

  function fitTerminal(): boolean {
    if (exited || killed) return false;
    if (!fitAddon || !term) return false;
    try {
      fitAddon.fit();
      if (term.getViewportY() <= 1 && !term.hasSelection()) scrollTerminalToBottom();
    } catch {
      // fit may throw if not visible yet
      return false;
    }
    const cols = term.cols;
    const rows = term.rows;
    if (cols <= 0 || rows <= 0) return false;
    return true;
  }

  async function fitAndResize(): Promise<boolean> {
    if (!fitTerminal() || !term) return false;
    const cols = term.cols;
    const rows = term.rows;
    if (cols === lastResizeCols && rows === lastResizeRows) return true;
    if (resizeInFlight) {
      resizeQueued = true;
      return true;
    }
    resizeInFlight = true;
    try {
      await api.sessions.resize(sessionId, cols, rows);
      lastResizeCols = cols;
      lastResizeRows = rows;
    } catch (err) {
      // non-fatal; backend may be down
      console.warn('resize failed', err);
    } finally {
      resizeInFlight = false;
      if (resizeQueued) {
        resizeQueued = false;
        queueMicrotask(() => {
          void fitAndResize();
        });
      }
    }
    return true;
  }

  function scheduleResize() {
    if (resizeTimer) clearTimeout(resizeTimer);
    resizeTimer = setTimeout(() => {
      void fitAndResize();
    }, 100);
  }

  function queueInput(data: string) {
    inputBuffer += data;
    if (inputFlushTimer) return;
    inputFlushTimer = setTimeout(flushInput, INPUT_FLUSH_MS);
  }

  function flushInput() {
    if (inputFlushTimer) {
      clearTimeout(inputFlushTimer);
      inputFlushTimer = null;
    }
    if (!inputBuffer || exited || killed) {
      inputBuffer = '';
      return;
    }
    const payload = inputBuffer;
    inputBuffer = '';
    inputSendChain = inputSendChain
      .catch(() => {
        // Keep later keystrokes flowing even if one request failed.
      })
      .then(async () => {
        try {
          await api.sessions.input(sessionId, encoder.encode(payload));
        } catch (err) {
          if (err instanceof ApiError && err.status === 404) {
            exited = true;
          } else {
            console.warn('input failed', err);
          }
        }
      });
  }

  onMount(() => {
    let cancelled = false;
    if (!containerEl) return;

    async function boot() {
      await initGhostty();
      if (cancelled || !containerEl) return;

      term = new Terminal({
        cursorBlink: true,
        fontFamily:
          '"JetBrains Mono", "Fira Code", ui-monospace, SFMono-Regular, Menlo, Consolas, monospace',
        fontSize: 13,
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

      // ── Clipboard wiring ───────────────────────────────────────────────
      // Ctrl+C keeps sending SIGINT when there is no selection. If text is
      // selected, it copies instead, matching embedded terminal behavior.
      term.onSelectionChange(() => {
        const s = term?.getSelection() ?? '';
        currentSelection = s;
        if (s.length > 0) {
          lastSelection = s;
          if (hideSelectionTimer) {
            clearTimeout(hideSelectionTimer);
            hideSelectionTimer = null;
          }
        } else if (lastSelection.length > 0) {
          if (hideSelectionTimer) clearTimeout(hideSelectionTimer);
          hideSelectionTimer = setTimeout(() => {
            currentSelection = '';
            lastSelection = '';
            hideSelectionTimer = null;
          }, 1200);
        }
      });

      term.attachCustomKeyEventHandler((ev) => {
        // ghostty-web differs from xterm here: returning `true` means
        // "handled; block the terminal's normal PTY encoding". Returning
        // `false` lets printable keys, Enter, arrows, etc. flow to onData.
        if (ev.type !== 'keydown') return false;
        const t = term;
        if (!t) return false;
        const isMac = navigator.platform.toUpperCase().startsWith('MAC');
        const key = ev.key.toLowerCase();
        const primaryCopyCombo =
          (isMac ? ev.metaKey : ev.ctrlKey) && !ev.shiftKey && !ev.altKey && key === 'c';
        const selectionCopyCombo = !isMac && ev.ctrlKey && key === 'insert';
        const terminalCopyCombo =
          (!isMac && ev.ctrlKey && ev.shiftKey && key === 'c') ||
          (!isMac && ev.ctrlKey && ev.altKey && key === 'c');
        const copyCombo =
          primaryCopyCombo || selectionCopyCombo || terminalCopyCombo;
        const pasteCombo =
          (isMac && ev.metaKey && key === 'v') ||
          (!isMac && ev.shiftKey && key === 'insert') ||
          (!isMac && ev.ctrlKey && ev.altKey && key === 'v');
        if (copyCombo) {
          const sel = activeSelection();
          if (!sel && !terminalCopyCombo) return false;
          ev.preventDefault();
          if (sel) {
            void writeClipboard(sel)
              .then(() => toast.success('Copied to clipboard'))
              .catch(() => toast.error('Clipboard write blocked'));
          }
          return true;
        }
        if (pasteCombo) {
          ev.preventDefault();
          void navigator.clipboard
            .readText()
            .then((txt) => {
              if (txt) t.paste(txt);
            })
            .catch(() => toast.error('Clipboard read blocked'));
          return true;
        }
        return false;
      });

      term.open(containerEl);
      await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
      fitTerminal();
      scrollTerminalToBottom();
      if (cancelled) return;

      // Buffered input forwarding. Sending one HTTP request per keystroke
      // makes terminal typing visibly lag under load; a tiny buffer keeps TTY
      // apps responsive while preserving byte order.
      term.onData((data) => {
        if (exited || killed) return;
        queueInput(data);
      });

      ro = new ResizeObserver(scheduleResize);
      ro.observe(containerEl);

      openSSE();
      void fitAndResize();
      scrollTerminalToBottom();

      window.addEventListener('keydown', onWindowKey, true);

      // Re-assert focus after the async boot finishes. `term.open()` calls
      // focus() internally, but the WASM init + initial resize await chain
      // gives the user time to click elsewhere (sidebar item, header, etc.),
      // and the focus we set during open() is long lost by the time we get
      // here. Without this the terminal renders output but `onData` never
      // fires — the keystrokes go to whatever was focused last.
      focusGhosttyInput();
    }

    void boot().catch((err) => {
      console.error('failed to initialise ghostty-web', err);
      toast.error('Terminal renderer failed to initialise');
    });

    return () => {
      cancelled = true;
    };
  });

  onDestroy(() => {
    if (hideSelectionTimer) clearTimeout(hideSelectionTimer);
    if (reconnectTimer) clearTimeout(reconnectTimer);
    if (resizeTimer) clearTimeout(resizeTimer);
    if (scrollRaf !== null) cancelAnimationFrame(scrollRaf);
    if (inputFlushTimer) flushInput();
    clearOutputQueue();
    ro?.disconnect();
    ro = null;
    sse?.close();
    sse = null;
    term?.dispose();
    term = null;
    window.removeEventListener('keydown', onWindowKey, true);
    // Intentionally do NOT kill the session here — closing the tab keeps the child alive.
  });

  async function onKill() {
    try {
      await api.sessions.kill(sessionId);
      killed = true;
      sse?.close();
      sse = null;
      connState = 'closed';
      flushOutput();
      term?.write('\r\n\x1b[31m[killed by user]\x1b[0m\r\n', scrollTerminalToBottom);
      toast.success('Session killed');
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      toast.error(`Failed to kill session: ${msg}`);
    }
  }
</script>

<div class="relative flex h-full w-full flex-col" style="background: #0b1220;">
  {#if !embedded}
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
          <span
            class="inline-flex items-center gap-1.5 text-[10px]"
            style="color: var(--dot-warn);"
          >
            <span class="h-dot h-dot--warn"></span>
            {connState}
          </span>
        {:else}
          <span
            class="inline-flex items-center gap-1.5 text-[10px]"
            style="color: var(--fg-muted);"
          >
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
  {/if}
  <!--
    Focus on click is delegated to ghostty-web: its canvas mousedown handler
    `preventDefault()`s and focuses the hidden textarea inside the container.
    We deliberately do NOT call `term.focus()` here — that wraps `element.focus()`
    in a `setTimeout(0)`, which fires AFTER the canvas handler and snatches the
    focus back to the container, leaving the textarea blurred and the
    keystrokes going to whichever element the browser settles on.
  -->
  <div
    bind:this={containerEl}
    role="application"
    aria-label="Agent terminal"
    class="min-h-0 flex-1 overflow-hidden"
    oncontextmenu={openContextMenu}
  ></div>
</div>

{#if menuVisible}
  <!-- Backdrop closes the menu on outside click. Pointer-events:auto only on
       the menu itself; the backdrop is transparent and full-screen. -->
  <button
    type="button"
    class="fixed inset-0 z-40 cursor-default bg-transparent"
    aria-label="Close context menu"
    onclick={closeMenu}
    oncontextmenu={(e) => {
      e.preventDefault();
      closeMenu();
    }}
  ></button>
  <div
    role="menu"
    class="fixed z-50 min-w-[180px] overflow-hidden rounded-md border shadow-lg"
    style="
      left: {menuX}px;
      top: {menuY}px;
      background: var(--surface-panel);
      border-color: var(--border-subtle);
    "
  >
    <button
      type="button"
      role="menuitem"
      onclick={copyFromMenu}
      disabled={!menuSelection}
      class="flex w-full items-center justify-between gap-3 px-3 py-1.5 text-left text-[12.5px] transition-colors disabled:opacity-40 enabled:hover:bg-[var(--accent-soft)]"
      style="color: var(--fg-default);"
    >
      <span>Copy selection</span>
      <span class="font-mono text-[10px]" style="color: var(--fg-muted);">Ctrl+Alt+C</span>
    </button>
    <button
      type="button"
      role="menuitem"
      onclick={pasteFromMenu}
      class="flex w-full items-center justify-between gap-3 px-3 py-1.5 text-left text-[12.5px] transition-colors hover:bg-[var(--accent-soft)]"
      style="color: var(--fg-default);"
    >
      <span>Paste</span>
      <span class="font-mono text-[10px]" style="color: var(--fg-muted);">Ctrl+Alt+V</span>
    </button>
    <div class="h-px" style="background: var(--border-subtle);"></div>
    <button
      type="button"
      role="menuitem"
      onclick={selectAllFromMenu}
      class="flex w-full items-center justify-between gap-3 px-3 py-1.5 text-left text-[12.5px] transition-colors hover:bg-[var(--accent-soft)]"
      style="color: var(--fg-default);"
    >
      <span>Select all</span>
    </button>
  </div>
{/if}
