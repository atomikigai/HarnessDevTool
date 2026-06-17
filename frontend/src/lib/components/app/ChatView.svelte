<!--
  ChatView — structured transcript viewer styled like Claude.ai chat.

  Subscribes to GET /api/sessions/:sid/transcript?since=0 (SSE, event "transcript")
  and groups TranscriptEvents into ChatTurns for display.

  Rich rendering (completed turns only, to preserve streaming performance):
    • Markdown via marked + DOMPurify (browser) or pulldown-cmark Tauri IPC
    • Syntax highlighting of code fences via highlight.js (dynamic import)
    • Inline images: markdown ![](), standalone URLs, data-URIs, tool result base64
    • Excalidraw fences/JSON rendered as SVG via @excalidraw/utils (dynamic import)
    • Attachment bar: upload via Paperclip, list thumbnails + download cards
    • Lightbox overlay for any clicked image

  Props: session: SessionMeta | null
-->
<script lang="ts">
  import { api, type SessionMeta, type TranscriptEvent, type AttachedFile } from '$lib/api/client';
  import type { Decision } from '$lib/api/types/Decision';
  import { subscribeSSE, type SSEHandle } from '$lib/api/sse';
  import { ChevronDown } from '$lib/icons';
  import { approvalsState } from '$lib/stores/approvals.svelte';
  import { toast } from 'svelte-sonner';
  import ChatComposer from './chat/ChatComposer.svelte';
  import ChatThread from './chat/ChatThread.svelte';
  import Lightbox from './chat/Lightbox.svelte';
  import {
    extractChartBlocks,
    extractMermaidBlocks,
    renderMermaid,
    renderSimpleChart
  } from './chat/diagrams';
  import { fetchPreviousTurns } from './chat/history';
  import { formatDuration, formatInt, toolState } from './chat/format';
  import { extractStandaloneImages } from './chat/media';
  import { extractExcalidrawBlocks, renderExcalidraw } from './chat/excalidraw';
  import { renderMarkdownBatch } from './chat/markdown';
  import { createPtyFallbackTurn, ptyTextFromEvent } from './chat/pty';
  import { hydrateToolResult } from './chat/toolResults';
  import type { ChatTurn, PrevTurn, PtyOutputEvent } from './chat/types';

  interface Props {
    session: SessionMeta | null;
    /** Previous session ID (from restart) — ChatView fetches its transcript and shows it dimmed above the separator. */
    prevSid?: string | null;
    /** Called when user clicks "View in Terminal tab" inside the PTY fallback block. */
    onSwitchToTerminal?: () => void;
    /** Called when user clicks "Restart" CTA shown when the session is stopped. */
    onRestart?: () => void;
    /** Emitted whenever aggregate token usage from the transcript changes. */
    onTotalTokens?: (inputTok: number, outputTok: number) => void;
  }

  let { session, prevSid = null, onSwitchToTerminal, onRestart, onTotalTokens }: Props = $props();

  // ---- State ----------------------------------------------------------------

  let turns = $state<ChatTurn[]>([]);
  let scrollEl: HTMLDivElement | null = $state(null);
  let fallbackArmed = $state(false);
  let transcriptSeen = $state(false);

  // Lightbox
  let lightboxSrc = $state<string | null>(null);

  // Attachments
  let attachments = $state<AttachedFile[]>([]);

  // Scroll batching (Fix 2)
  let scrollPending = false;

  // Event queue for batched RAF processing (Fix 3)
  let eventQueue: TranscriptEvent[] = [];
  let flushPending = false;

  // Last received seq for reconnect tracking (Fix 4)
  let lastSeq = 0;
  let renderingTurnIds = new Set<string>();
  // P1-A: tracks render tasks that were started but became stale because new content
  // arrived while the async renderMarkdown was in flight. The .then() checks this
  // set and discards the result instead of writing stale HTML.
  let staleRenders = new Set<string>();
  let fallbackOutputBytes = $state(0);
  let lastPtyFallbackSeq = 0;
  let fallbackTimer: ReturnType<typeof setTimeout> | null = null;
  let fallbackSawWorking = false;
  let fallbackDone = $state(false);
  let awaitingResponse = $state(false);
  let awaitingResponseTimer: ReturnType<typeof setTimeout> | null = null;
  let hadWorkingState = false;
  let sendTurnCount = 0;

  // Fix 1 — Non-reactive guard: tracks which session ID the SSE is actually open for.
  // Prevents re-opening on every poll tick that produces a new object reference for the same session.
  let openedSid: string | null = null;

  // BUG A — SSE reconnect with since=lastSeq (non-reactive control vars)
  let sseReconnectTimer: ReturnType<typeof setTimeout> | null = null;
  let sseAttempts = 0;
  // P2 fix: cap reconnect attempts to avoid infinite retries against a dead/tombstoned session.
  const SSE_MAX_ATTEMPTS = 20;
  // Shown in the empty state when the cap is hit.
  let transcriptUnavailable = $state(false);

  // BUG B — auto-scroll tracking (non-reactive base count + reactive UI state)
  let atBottom = $state(true);
  let newMsgWhileScrolledUp = $state(0);
  let lastKnownTurnsCount = 0; // non-reactive
  let forceNextScroll = false; // set true in openSSE so the first batch scrolls unconditionally

  // BUG D — historical turns from previous session (set after prevSid fetch completes)
  let historicalTurns = $state<PrevTurn[]>([]);
  let historicalLoaded = $state(false);

  // Fix 3 — Debounce timers per turn ID (non-reactive Map).
  // A timer fires 1.2s after the last content append to mark the turn "settled"
  // and trigger markdown render even if detected_state is still 'working'.
  const settledTimers = new Map<string, ReturnType<typeof setTimeout>>();

  const stopped = $derived(!session || session.status !== 'running');
  const sessionApprovals = $derived(
    approvalsState.pending.filter((approval) => !session?.id || approval.session_id === session.id)
  );
  const currentApproval = $derived(sessionApprovals[0] ?? null);
  const agentIsWorking = $derived(session?.detected_state === 'working');
  const workingActiveTurn = $derived.by(() => {
    const last = lastAssistantTurn();
    if (!last) return null;
    return last.isStreaming && !last.content && !last.renderedHtml ? last : null;
  });
  const showWorkingIndicator = $derived(
    !stopped && (awaitingResponse || workingActiveTurn !== null)
  );

  const encoder = new TextEncoder();

  // ---- Attachment helpers ---------------------------------------------------

  async function loadAttachments(): Promise<void> {
    const sid = session?.id;
    if (!sid) return;
    try {
      const res = await api.sessions.listAttachments(sid);
      attachments = res.data ?? [];
    } catch {
      // silently ignore – non-critical
    }
  }

  // ---- Lightbox -------------------------------------------------------------

  function openLightbox(src: string): void {
    lightboxSrc = src;
  }

  function closeLightbox(): void {
    lightboxSrc = null;
  }

  function onWindowKeyup(ev: KeyboardEvent): void {
    if (ev.key === 'Escape') closeLightbox();
  }

  function onWindowResize(): void {
    if (!atBottom) return;
    requestAnimationFrame(scrollToBottom);
  }

  // ---- Turn processing ------------------------------------------------------

  function lastAssistantTurn(): ChatTurn | null {
    for (let i = turns.length - 1; i >= 0; i--) {
      if (turns[i].role === 'assistant') return turns[i];
    }
    return null;
  }

  function lastTurn(): ChatTurn | null {
    return turns.length > 0 ? turns[turns.length - 1] : null;
  }

  function currentAssistantTurn(ev: TranscriptEvent): ChatTurn {
    const current = lastTurn();
    if (current?.role === 'assistant') return current;

    const next: ChatTurn = {
      id: String(ev.seq),
      role: 'assistant',
      content: '',
      toolBlocks: [],
      isStreaming: true,
      renderedHtml: '',
      excalidrawScenes: [],
      mermaidScenes: [],
      chartScenes: [],
      inlineImages: [],
      model: ev.model ?? undefined,
      source: ev.source,
      usage: ev.usage ?? undefined
    };
    turns.push(next);
    return next;
  }

  // Fix 3 helpers — debounce / settle ----------------------------------------

  // Resets the 1.2s inactivity timer for a streaming turn.
  // When the timer fires, marks the turn as "settled" so markdown renders
  // even if detected_state remains 'working'.
  function scheduleSettleRender(turn: ChatTurn) {
    const existing = settledTimers.get(turn.id);
    if (existing) clearTimeout(existing);
    const timer = setTimeout(() => {
      settledTimers.delete(turn.id);
      if (turn.isStreaming && turn.content && !turn.renderedHtml) {
        turn.settled = true;
      }
    }, 1200);
    settledTimers.set(turn.id, timer);
  }

  // Boundary event: settle the turn immediately (no waiting for the timer).
  function markTurnSettled(turn: ChatTurn) {
    const existing = settledTimers.get(turn.id);
    if (existing) {
      clearTimeout(existing);
      settledTimers.delete(turn.id);
    }
    if (turn.isStreaming && turn.content && !turn.renderedHtml) {
      turn.settled = true;
    }
  }

  // ---------------------------------------------------------------------------

  function processEvent(ev: TranscriptEvent) {
    if (ev.kind === 'meta') {
      const note = metaEventLabel(ev);
      if (!note) return;
      const prevAssistant = lastAssistantTurn();
      if (prevAssistant?.isStreaming) markTurnSettled(prevAssistant);
      turns.push(systemTurn(ev.seq, note));
      return;
    }

    if (ev.kind === 'system_note') {
      // Fix 4: turn_duration — attach ms to last assistant turn, never show as pill.
      if (ev.subtype === 'turn_duration') {
        const raw = ev.raw as Record<string, unknown> | null | undefined;
        const ms =
          typeof raw?.durationMs === 'number'
            ? raw.durationMs
            : typeof raw?.duration_ms === 'number'
              ? raw.duration_ms
              : typeof raw?.duration === 'number'
                ? raw.duration
                : null;
        if (ms !== null) {
          const last = lastAssistantTurn();
          if (last) last.durationMs = ms;
        }
        return;
      }
      const note = ev.content ?? systemNoteLabel(ev);
      if (note == null) return;
      // Boundary: immediately settle any streaming assistant turn.
      const prevAssistant = lastAssistantTurn();
      if (prevAssistant?.isStreaming) markTurnSettled(prevAssistant);
      turns.push(systemTurn(ev.seq, note));
      return;
    }

    if (ev.kind === 'message') {
      if (ev.role === 'user') {
        // Boundary: finalize any streaming assistant turn immediately.
        const prevAssistant = lastAssistantTurn();
        if (prevAssistant?.isStreaming) markTurnSettled(prevAssistant);
        turns.push({
          id: String(ev.seq),
          role: 'user',
          content: ev.content ?? '',
          toolBlocks: [],
          isStreaming: false,
          renderedHtml: '',
          excalidrawScenes: [],
          mermaidScenes: [],
          chartScenes: [],
          inlineImages: []
        });
      } else if (ev.role === 'assistant') {
        const content = ev.content ?? '';
        const previousAssistant = lastAssistantTurn();
        if (
          content &&
          previousAssistant &&
          previousAssistant.content === content &&
          previousAssistant.source === ev.source
        ) {
          return;
        }
        const last = currentAssistantTurn(ev);
        // P1-A: always invalidate when new content arrives — whether renderedHtml is
        // already set (old check covered this) OR a render is in-flight (renderedHtml
        // still '' but renderingTurnIds has the id). The old guard
        // `if (last.settled && last.renderedHtml)` missed the in-flight case: the
        // render would complete and overwrite with stale HTML that missed the new chunk.
        if (renderingTurnIds.has(last.id)) staleRenders.add(last.id);
        if (last.renderedHtml) {
          last.renderedHtml = '';
          last.cleanedContent = undefined;
        }
        last.settled = false;
        last.content += content;
        if (ev.model) last.model = ev.model;
        if (ev.usage) last.usage = ev.usage;
        scheduleSettleRender(last);
      }
      return;
    }

    if (ev.kind === 'thinking') {
      const last = currentAssistantTurn(ev);
      // P1-A: same invalidation pattern as message events — always mark in-flight
      // renders stale and clear any already-completed renderedHtml when new thinking
      // arrives (thinking shares the turn and can arrive interleaved with content).
      if (renderingTurnIds.has(last.id)) staleRenders.add(last.id);
      if (last.renderedHtml) {
        last.renderedHtml = '';
        last.cleanedContent = undefined;
      }
      last.settled = false;
      last.thinking = (last.thinking ?? '') + (ev.content ?? '');
      scheduleSettleRender(last);
      return;
    }

    if (ev.kind === 'tool_call') {
      const last = currentAssistantTurn(ev);
      last.toolBlocks.push({
        id: ev.tool_use_id ?? String(ev.seq),
        name: ev.tool_name ?? '(unknown)',
        args: ev.tool_args,
        resultExcalidrawScenes: [],
        resultInlineImages: [],
        isError: false,
        expanded: false
      });
      return;
    }

    if (ev.kind === 'tool_result') {
      const toolId = ev.tool_use_id;
      if (toolId) {
        for (const turn of turns) {
          const block = turn.toolBlocks.find((b) => b.id === toolId);
          if (block) {
            block.result = ev.tool_result;
            block.isError = ev.is_error ?? false;
            hydrateToolResult(block);
            break;
          }
        }
      }
      return;
    }

    const special = specialSystemTurn(ev);
    if (special) {
      const prevAssistant = lastAssistantTurn();
      if (prevAssistant?.isStreaming) markTurnSettled(prevAssistant);
      turns.push(special);
      return;
    }

    const note = unknownEventLabel(ev);
    if (note) turns.push(systemTurn(ev.seq, note));
  }

  function systemTurn(
    seq: number,
    content: string,
    opts: Pick<ChatTurn, 'systemKind' | 'systemHref' | 'systemDetail'> = {}
  ): ChatTurn {
    return {
      id: String(seq),
      role: 'system',
      content,
      toolBlocks: [],
      isStreaming: false,
      renderedHtml: '',
      excalidrawScenes: [],
      mermaidScenes: [],
      chartScenes: [],
      inlineImages: [],
      ...opts
    };
  }

  // ---- Streaming state sync -------------------------------------------------

  $effect(() => {
    const state = session?.detected_state ?? null;
    const isWorking = state === 'working';

    // Collect assistant turns that need HTML rendering:
    // • completed turns (!isStreaming), OR
    // • Fix 3: turns still technically streaming but quiet for 1.2s (settled=true)
    const pending = turns.filter(
      (t) =>
        t.role === 'assistant' &&
        t.source !== 'pty' &&
        (!t.isStreaming || t.settled) &&
        t.content &&
        !t.renderedHtml &&
        !renderingTurnIds.has(t.id)
    );

    // Pre-process each pending turn: extract excalidraw scenes and inline images
    // from the raw content so markdown is rendered without excalidraw fences.
    for (const turn of pending) {
      if (!turn.cleanedContent) {
        const excalidraw = extractExcalidrawBlocks(turn.content);
        const mermaid = extractMermaidBlocks(excalidraw.cleaned);
        const charts = extractChartBlocks(mermaid.cleaned);
        turn.cleanedContent = charts.cleaned || turn.content;
        turn.excalidrawScenes = excalidraw.scenes.map((raw) => ({ raw }));
        turn.mermaidScenes = mermaid.scenes.map((raw) => ({ raw }));
        turn.chartScenes = charts.scenes.map((raw) => renderSimpleChart(raw));
        turn.inlineImages = extractStandaloneImages(charts.cleaned);
        for (const scene of turn.excalidrawScenes) {
          void renderExcalidraw(scene);
        }
        for (const scene of turn.mermaidScenes) {
          void renderMermaid(scene);
        }
      }
    }

    if (pending.length > 0) {
      for (const turn of pending) renderingTurnIds.add(turn.id);
      void renderMarkdownBatch(pending.map((t) => t.cleanedContent ?? t.content))
        .then((htmls) => {
          htmls.forEach((html, i) => {
            const t = pending[i];
            if (staleRenders.has(t.id)) {
              staleRenders.delete(t.id);
              return;
            }
            t.renderedHtml = html;
          });
        })
        .finally(() => {
          for (const turn of pending) renderingTurnIds.delete(turn.id);
        });
    }

    // Sync isStreaming flags
    for (const turn of turns) {
      if (turn.role === 'assistant') {
        turn.isStreaming = isWorking && turn === lastAssistantTurn();
      }
    }
    if (isWorking) {
      const last = lastAssistantTurn();
      if (last) last.isStreaming = true;
    }
  });

  // ---- SSE subscription -----------------------------------------------------

  let sseHandle: SSEHandle | null = null;
  let ptyFallbackHandle: SSEHandle | null = null;

  function openSSE(sessionId: string) {
    closeSSE(); // also clears sseReconnectTimer
    closePtyFallback();
    turns = [];
    renderingTurnIds = new Set();
    // P1-B: clear orphaned debounce timers from the previous session so they
    // don't fire against turns that no longer exist.
    settledTimers.forEach((t) => clearTimeout(t));
    settledTimers.clear();
    staleRenders.clear();
    lastSeq = 0;
    fallbackOutputBytes = 0;
    lastPtyFallbackSeq = 0;
    fallbackArmed = false;
    fallbackSawWorking = false;
    fallbackDone = false;
    awaitingResponse = false;
    hadWorkingState = false;
    if (awaitingResponseTimer) {
      clearTimeout(awaitingResponseTimer);
      awaitingResponseTimer = null;
    }
    transcriptSeen = false;
    eventQueue = [];
    sseAttempts = 0;
    transcriptUnavailable = false;
    forceNextScroll = true; // BUG B: first event batch scrolls unconditionally
    lastKnownTurnsCount = 0;
    void loadAttachments();
    openTranscriptSSE(sessionId); // BUG A: manual reconnect with since=lastSeq

    fallbackTimer = setTimeout(() => {
      if (!transcriptSeen && turns.length === 0) openPtyFallback(sessionId);
    }, 900);
  }

  // BUG A: Opens the transcript SSE with the current lastSeq so reconnects
  // don't replay already-processed events. Called by openSSE and scheduleTranscriptReconnect.
  function openTranscriptSSE(sessionId: string) {
    sseHandle?.close();
    sseHandle = null;
    const url = `/sessions/${sessionId}/transcript?since=${lastSeq}`;
    sseHandle = subscribeSSE(url, () => {}, {
      reconnect: false, // manual reconnect below so we can update since=lastSeq each time
      onError: () => scheduleTranscriptReconnect(sessionId),
      onLagged: () => scheduleTranscriptReconnect(sessionId),
      events: {
        transcript: (data) => {
          const ev = data as TranscriptEvent;
          // BUG A: dedup — replay/live border can re-send events we've already seen
          if (ev.seq <= lastSeq && lastSeq > 0) return;
          // First transcript event: clear PTY fallback turns and stop the fallback SSE
          if (!transcriptSeen) {
            transcriptSeen = true;
            closePtyFallback();
            // Remove any PTY blob turns so transcript turns appear cleanly
            if (turns.some((t) => t.source === 'pty')) {
              turns = turns.filter((t) => t.source !== 'pty');
            }
          }
          if (ev.seq > lastSeq) lastSeq = ev.seq;
          sseAttempts = 0; // reset backoff on successful event
          enqueueEvent(ev);
        }
      }
    });
  }

  // BUG A: Schedules a reconnect with exponential backoff (500ms → 1s → 2s → max 5s).
  function scheduleTranscriptReconnect(sessionId: string) {
    if (sseReconnectTimer) return; // already pending
    // P2-cap: stop retrying after SSE_MAX_ATTEMPTS to avoid infinite retries
    // against a dead or tombstoned session.
    if (sseAttempts >= SSE_MAX_ATTEMPTS) {
      closeSSE();
      if (!transcriptSeen) transcriptUnavailable = true;
      return;
    }
    sseHandle?.close();
    sseHandle = null;
    // P2-fallback: reset the 900ms fallback window from THIS reconnect attempt.
    // Without this the fallbackTimer started in openSSE fires mid-reconnect (e.g.
    // if the first SSE attempt fails before 900ms) and opens the PTY spinner while
    // the transcript reconnect is still in flight.
    if (!transcriptSeen && fallbackTimer !== null) {
      clearTimeout(fallbackTimer);
      fallbackTimer = setTimeout(() => {
        if (!transcriptSeen && turns.length === 0) openPtyFallback(sessionId);
      }, 900);
    }
    const cap = Math.min(5000, 500 * Math.pow(2, sseAttempts));
    const delay = Math.max(500, Math.random() * cap);
    sseAttempts++;
    sseReconnectTimer = setTimeout(() => {
      sseReconnectTimer = null;
      openTranscriptSSE(sessionId);
    }, delay);
  }

  function closeSSE() {
    sseHandle?.close();
    sseHandle = null;
    if (fallbackTimer) {
      clearTimeout(fallbackTimer);
      fallbackTimer = null;
    }
    // BUG A: also cancel any pending reconnect so it doesn't fire after session switch/destroy
    if (sseReconnectTimer) {
      clearTimeout(sseReconnectTimer);
      sseReconnectTimer = null;
    }
  }

  function openPtyFallback(sessionId: string) {
    if (ptyFallbackHandle || transcriptSeen) return;
    fallbackArmed = true;
    ptyFallbackHandle = subscribeSSE(`/events?session=${encodeURIComponent(sessionId)}`, () => {}, {
      reconnect: true,
      events: {
        'session.output': (data) => {
          if (transcriptSeen) return;
          appendPtyFallback(data as PtyOutputEvent);
        }
      }
    });
  }

  function closePtyFallback() {
    ptyFallbackHandle?.close();
    ptyFallbackHandle = null;
    fallbackArmed = false;
  }

  // P0 — Teardown-only effect: reads NO reactive state so it runs once on mount
  // and its cleanup runs ONLY on component unmount (never on re-run).
  //
  // WHY SEPARATE from the session-tracking effect below:
  // In Svelte 5, a $effect's teardown runs SYNCHRONOUSLY before the next re-run.
  // The old single effect returned `() => { openedSid = null; ... }` as its
  // teardown. When the poller produced a new object reference for the *same*
  // session (~1.5s tick), Svelte re-ran the effect: teardown reset openedSid=null
  // → body found `sid !== null (=== null)` → called openSSE → cleared turns →
  // flash of PTY fallback. Moving SSE/PTY close here prevents that cycle: this
  // cleanup never runs on re-run, only on destroy.
  $effect(() => {
    return () => {
      settledTimers.forEach((t) => clearTimeout(t));
      settledTimers.clear();
      if (awaitingResponseTimer) {
        clearTimeout(awaitingResponseTimer);
        awaitingResponseTimer = null;
      }
      closeSSE();
      closePtyFallback();
    };
  });

  $effect(() => {
    // P0: compare by session ID string, not object reference.
    // selectedSession is $derived(allSessions.find(...)) → new object ref each
    // poll tick even for the same session. The guard survives across ticks because
    // NOTHING (no teardown, no side effect) ever resets openedSid once set.
    // Sequence for same-session tick: prop changes ref → effect re-runs →
    // sid === openedSid → early return with ZERO side effects (no closeSSE, no turns=[]).
    const sid = session?.id ?? null;
    if (sid === openedSid) return;
    openedSid = sid;
    if (sid) {
      openSSE(sid); // openSSE calls closeSSE/closePtyFallback internally
    } else {
      closeSSE();
      closePtyFallback();
      turns = [];
      attachments = [];
    }
  });

  // ---- Auto-scroll (BUG B: initial forced scroll + stick-to-bottom + pill) ----

  // Called by scroll event on the chat-scroll element to track user position.
  function onChatScrolled() {
    if (!scrollEl) return;
    const { scrollTop, scrollHeight, clientHeight } = scrollEl;
    const isAtBottom = scrollHeight - scrollTop - clientHeight < 120;
    atBottom = isAtBottom;
    if (isAtBottom) {
      newMsgWhileScrolledUp = 0;
      lastKnownTurnsCount = turns.length;
    }
  }

  // Scroll to bottom imperatively (pill click, or after restart).
  function scrollToBottom() {
    if (!scrollEl) return;
    scrollEl.scrollTop = scrollEl.scrollHeight;
    atBottom = true;
    newMsgWhileScrolledUp = 0;
    forceNextScroll = false;
    lastKnownTurnsCount = turns.length;
  }

  function scheduleScroll() {
    if (scrollPending) return;
    scrollPending = true;
    requestAnimationFrame(() => {
      scrollPending = false;
      if (!scrollEl) return;
      const { scrollTop, scrollHeight, clientHeight } = scrollEl;
      const distFromBottom = scrollHeight - scrollTop - clientHeight;

      // forceNextScroll: set when openSSE is called — the first event batch scrolls
      // unconditionally to show the bottom of the replay history on mount.
      if (forceNextScroll) {
        forceNextScroll = false;
        scrollEl.scrollTop = scrollEl.scrollHeight;
        atBottom = true;
        newMsgWhileScrolledUp = 0;
        lastKnownTurnsCount = turns.length;
        return;
      }

      const isAtBottom = distFromBottom < 120;
      if (isAtBottom || atBottom) {
        scrollEl.scrollTop = scrollEl.scrollHeight;
        atBottom = true;
        newMsgWhileScrolledUp = 0;
        lastKnownTurnsCount = turns.length;
      } else {
        atBottom = false;
        const newCount = turns.length - lastKnownTurnsCount;
        if (newCount > 0) {
          newMsgWhileScrolledUp += newCount;
          lastKnownTurnsCount = turns.length;
        }
      }
    });
  }

  // ---- Event queue (Fix 3: batch burst events into a single RAF flush) ------

  function enqueueEvent(ev: TranscriptEvent) {
    eventQueue.push(ev);
    if (flushPending) return;
    flushPending = true;
    requestAnimationFrame(() => {
      flushPending = false;
      const batch = eventQueue.splice(0);
      for (const e of batch) processEvent(e);
      scheduleScroll();
    });
  }

  // ---- PTY fallback ---------------------------------------------------------

  function appendPtyFallback(ev: PtyOutputEvent) {
    if (ev.seq <= lastPtyFallbackSeq && lastPtyFallbackSeq > 0) return;
    if (ev.seq > lastPtyFallbackSeq) lastPtyFallbackSeq = ev.seq;

    const text = ptyTextFromEvent(ev);
    if (!text.trim()) return;

    let turn = turns.find((t) => t.id === 'pty-fallback');
    if (!turn) {
      turn = createPtyFallbackTurn(session?.kind);
      turns = [turn];
    }

    fallbackOutputBytes += text.length;
    const next = `${turn.content}${text}`;
    turn.content = next.length > 120_000 ? next.slice(next.length - 120_000) : next;
    scheduleScroll();
  }

  // BUG D — Historical turns from prevSid (previous session after restart).
  // Uses the indexed transcript query instead of SSE so this one-shot history
  // fetch cannot leave an EventSource open after restart.
  $effect(() => {
    const pid = prevSid;
    if (!pid) {
      historicalTurns = [];
      historicalLoaded = false;
      return;
    }

    let cancelled = false;
    const controller = new AbortController();
    const maxTimer = setTimeout(() => controller.abort(), 5000);

    void (async () => {
      let tempTurns: PrevTurn[] = [];
      try {
        tempTurns = await fetchPreviousTurns(pid, controller.signal);
      } catch {
        // Historical context is best effort; the new live session remains usable.
      } finally {
        clearTimeout(maxTimer);
        if (!cancelled) {
          historicalTurns = tempTurns;
          historicalLoaded = true;
          forceNextScroll = true;
          scheduleScroll();
        }
      }
    })();

    return () => {
      if (maxTimer) clearTimeout(maxTimer);
      cancelled = true;
      controller.abort();
    };
  });

  // BUG E — Aggregate token usage from all transcript turns.
  const totalInputTok = $derived(turns.reduce((s, t) => s + (t.usage?.input_tokens ?? 0), 0));
  const totalOutputTok = $derived(turns.reduce((s, t) => s + (t.usage?.output_tokens ?? 0), 0));

  $effect(() => {
    onTotalTokens?.(totalInputTok, totalOutputTok);
  });

  $effect(() => {
    const isWorking = session?.detected_state === 'working';

    if (awaitingResponse) {
      if (isWorking) hadWorkingState = true;

      if (turns.findIndex((t, i) => i >= sendTurnCount && t.role === 'assistant') !== -1) {
        clearAwaitingResponse();
        return;
      }

      if (hadWorkingState && !isWorking) {
        clearAwaitingResponse();
        hadWorkingState = false;
      }
    } else {
      hadWorkingState = false;
    }
  });

  $effect(() => {
    if (fallbackArmed && agentIsWorking) fallbackSawWorking = true;

    if (
      fallbackArmed &&
      !transcriptSeen &&
      (fallbackSawWorking || fallbackOutputBytes > 0) &&
      !agentIsWorking &&
      !awaitingResponse
    ) {
      fallbackDone = true;
    }

    const userHasInteracted = turns.some((t) => t.role === 'user');
    if (
      fallbackArmed &&
      !transcriptSeen &&
      !agentIsWorking &&
      !awaitingResponse &&
      !fallbackSawWorking &&
      fallbackOutputBytes === 0 &&
      !userHasInteracted
    ) {
      const autoHideTimer = setTimeout(() => {
        const stillNoInteraction = !turns.some((t) => t.role === 'user');
        if (
          fallbackArmed &&
          !transcriptSeen &&
          !agentIsWorking &&
          !awaitingResponse &&
          !fallbackSawWorking &&
          fallbackOutputBytes === 0 &&
          stillNoInteraction
        ) {
          turns = turns.filter((t) => t.source !== 'pty');
          closePtyFallback();
        }
      }, 600);
      return () => clearTimeout(autoHideTimer);
    }
  });

  function clearAwaitingResponse(): void {
    awaitingResponse = false;
    if (awaitingResponseTimer) {
      clearTimeout(awaitingResponseTimer);
      awaitingResponseTimer = null;
    }
  }

  function markAwaitingResponse(): void {
    sendTurnCount = turns.length;
    awaitingResponse = true;
    hadWorkingState = false;
    scheduleScroll();
    if (awaitingResponseTimer) clearTimeout(awaitingResponseTimer);
    awaitingResponseTimer = setTimeout(() => {
      awaitingResponse = false;
      awaitingResponseTimer = null;
    }, 90_000);
  }

  function rawObject(ev: TranscriptEvent): Record<string, unknown> {
    return ev.raw && typeof ev.raw === 'object' ? (ev.raw as Record<string, unknown>) : {};
  }

  function firstString(raw: Record<string, unknown>, keys: string[]): string | null {
    for (const key of keys) {
      const value = raw[key];
      if (typeof value === 'string' && value.trim()) return value.trim();
      if (typeof value === 'number' && Number.isFinite(value)) return String(value);
    }
    return null;
  }

  function metaEventLabel(ev: TranscriptEvent): string | null {
    const raw = rawObject(ev);
    const topType = typeof raw.type === 'string' ? raw.type : '';
    const payload =
      raw.payload && typeof raw.payload === 'object'
        ? (raw.payload as Record<string, unknown>)
        : raw;
    const payloadType = typeof payload.type === 'string' ? payload.type : topType;

    if (topType === 'permission-mode') {
      const mode = String(raw.permissionMode ?? raw.mode ?? 'unknown');
      return `Permission mode: ${mode}`;
    }
    if (payloadType.includes('approval') || payloadType.includes('permission')) {
      return `Approval event: ${payloadType}`;
    }
    if (payloadType === 'task_complete') {
      const duration =
        typeof payload.duration_ms === 'number' ? ` in ${formatDuration(payload.duration_ms)}` : '';
      return `Task complete${duration}`;
    }
    if (payloadType === 'turn_context' || topType === 'session_meta' || topType === 'ai-title')
      return null;
    return null;
  }

  function specialSystemTurn(ev: TranscriptEvent): ChatTurn | null {
    const raw = rawObject(ev);
    const type = typeof raw.type === 'string' ? raw.type : '';

    if (type === 'pr-link') {
      const href = firstString(raw, ['url', 'href', 'link', 'pr_url']);
      const number = firstString(raw, ['number', 'pr_number']);
      const title = firstString(raw, ['title', 'name']);
      return systemTurn(ev.seq, number ? `Pull request #${number}` : 'Pull request created', {
        systemKind: 'link',
        systemHref: href ?? undefined,
        systemDetail: title ?? href ?? undefined
      });
    }

    if (type.includes('approval') || type.includes('permission')) {
      const tool = firstString(raw, ['tool', 'tool_name', 'name', 'command']);
      return systemTurn(ev.seq, tool ? `Approval required: ${tool}` : 'Approval required', {
        systemKind: 'approval',
        systemDetail: tool ? undefined : type
      });
    }

    return null;
  }

  function systemNoteLabel(ev: TranscriptEvent): string | null {
    if (ev.subtype === 'token_count' && ev.usage) {
      const total = ev.usage.total_tokens;
      return typeof total === 'number'
        ? `Token usage: ${formatInt(total)} total`
        : 'Token usage updated';
    }
    if (ev.subtype === 'init') return 'Session initialized';
    if (ev.subtype === 'compact') return 'Context compacted';
    return ev.subtype ? ev.subtype.replaceAll('_', ' ') : null;
  }

  function unknownEventLabel(ev: TranscriptEvent): string | null {
    const raw = rawObject(ev);
    const type = typeof raw.type === 'string' ? raw.type : ev.kind;
    return type ? `Unparsed ${ev.source} event: ${type}` : null;
  }

  function thinkingTailShort(thinking: string): string {
    const lines = thinking.split('\n').filter((line) => line.trim().length > 0);
    return lines.length <= 2 ? lines.join('\n') : lines.slice(lines.length - 2).join('\n');
  }

  function workingStatusLine(turn: ChatTurn | null) {
    if (!turn) return { kind: 'idle' as const };
    const runningBlock = [...turn.toolBlocks]
      .reverse()
      .find((block) => toolState(block) === 'running');
    if (runningBlock) return { kind: 'tool' as const, name: runningBlock.name };
    if (turn.thinking) return { kind: 'thinking' as const, tail: thinkingTailShort(turn.thinking) };
    return { kind: 'idle' as const };
  }

  async function decideCurrentApproval(decision: Decision): Promise<void> {
    if (!currentApproval) return;
    try {
      await approvalsState.decide(currentApproval.id, decision);
      toast.success(decision === 'allow' ? 'Approval allowed' : 'Approval denied');
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to submit approval decision');
    }
  }

  async function sendCliApprovalChoice(choice: '1' | '2' | '3'): Promise<void> {
    if (!session || stopped) return;
    try {
      await api.sessions.input(session.id, encoder.encode(choice));
      await new Promise((resolve) => setTimeout(resolve, 40));
      await api.sessions.input(session.id, encoder.encode('\r'));
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to send approval choice');
    }
  }
</script>

<svelte:window onkeyup={onWindowKeyup} onresize={onWindowResize} />

<Lightbox src={lightboxSrc} onClose={closeLightbox} />

<div
  class="chat-shell flex h-full flex-col overflow-hidden"
  style="background: var(--surface-canvas);"
>
  <!-- Message list -->
  <div
    class="chat-scroll min-h-0 flex-1 overflow-y-auto"
    style="transform: translateZ(0); will-change: scroll-position;"
    bind:this={scrollEl}
    onscroll={onChatScrolled}
  >
    {#if !session}
      <!-- No session selected -->
      <div
        class="empty-chat mx-auto flex h-full max-w-[760px] flex-col items-center justify-center gap-4 px-6 text-center"
      >
        <div class="empty-mark" aria-hidden="true">✳</div>
        <p class="empty-title">Select a session</p>
        <p class="empty-subtitle">The structured transcript will appear here.</p>
      </div>
    {:else if turns.length === 0}
      {#if showWorkingIndicator}
        <ChatThread
          {turns}
          {historicalTurns}
          {historicalLoaded}
          {fallbackOutputBytes}
          {fallbackDone}
          {agentIsWorking}
          {showWorkingIndicator}
          workingStatus={workingStatusLine(workingActiveTurn)}
          {stopped}
          {openLightbox}
          {onSwitchToTerminal}
          onCliApprovalChoice={sendCliApprovalChoice}
        />
      {:else}
        <!-- Empty state -->
        <div
          class="empty-chat mx-auto flex h-full max-w-[760px] flex-col items-center justify-center gap-4 px-6 text-center"
        >
          <div class="empty-mark" aria-hidden="true">✳</div>
          <p class="empty-title">Ready when you are</p>
          <p class="empty-subtitle">
            {#if session.has_transcript === false}
              No transcript yet. The agent will write one as it works.
            {:else if transcriptUnavailable}
              Transcript not available.
            {:else if fallbackArmed}
              Reading live agent output…
            {:else}
              Waiting for transcript events…
            {/if}
          </p>
        </div>
      {/if}
    {:else}
      <ChatThread
        {turns}
        {historicalTurns}
        {historicalLoaded}
        {fallbackOutputBytes}
        {fallbackDone}
        {agentIsWorking}
        {showWorkingIndicator}
        workingStatus={workingStatusLine(workingActiveTurn)}
        {stopped}
        {openLightbox}
        {onSwitchToTerminal}
        onCliApprovalChoice={sendCliApprovalChoice}
      />
    {/if}
  </div>

  <!-- BUG B: scroll-to-bottom pill — shown when user has scrolled up and new messages arrived -->
  {#if !atBottom && newMsgWhileScrolledUp > 0}
    <div class="scroll-pill-wrap shrink-0 flex justify-center py-1.5">
      <button type="button" class="scroll-pill" onclick={scrollToBottom}>
        <ChevronDown size={12} />
        {newMsgWhileScrolledUp} new message{newMsgWhileScrolledUp === 1 ? '' : 's'}
      </button>
    </div>
  {/if}

  <div class="chat-footer">
    {#if currentApproval}
      <div class="approval-inline mx-auto" role="status" aria-live="polite">
        <div class="approval-inline-icon">!</div>
        <div class="approval-inline-copy">
          <span class="approval-inline-title">Approval required</span>
          <span class="approval-inline-detail">
            {currentApproval.tool}
            {#if sessionApprovals.length > 1}
              · {sessionApprovals.length - 1} more pending
            {/if}
          </span>
        </div>
        <button
          type="button"
          class="approval-inline-btn approval-deny"
          onclick={() => void decideCurrentApproval('deny')}
        >
          Deny
        </button>
        <button
          type="button"
          class="approval-inline-btn approval-allow"
          onclick={() => void decideCurrentApproval('allow')}
        >
          Allow
        </button>
      </div>
    {/if}

    <ChatComposer
      sessionId={session?.id ?? null}
      {stopped}
      {attachments}
      {openLightbox}
      onAttachmentsChanged={loadAttachments}
      onSent={markAwaitingResponse}
      {onRestart}
    />
  </div>
</div>

<style>
  /* ---- Shell & scroll ---------------------------------------------------- */
  .chat-shell {
    color: var(--fg-default);
  }

  .chat-scroll {
    scrollbar-gutter: stable;
  }

  /* ---- Empty state ------------------------------------------------------- */
  .empty-mark {
    color: var(--accent);
    font-size: 2rem;
    line-height: 1;
  }

  .empty-title {
    color: var(--fg-default);
    font-size: 1.55rem;
    line-height: 1.2;
    margin: 0;
  }

  .empty-subtitle {
    color: var(--fg-muted);
    font-size: 0.9rem;
    line-height: 1.6;
    margin: 0;
    max-width: 32rem;
  }

  /* ---- Scroll pill (BUG B) ----------------------------------------------- */
  .scroll-pill-wrap {
    pointer-events: none; /* allow interactions only on the button */
  }

  .scroll-pill {
    align-items: center;
    background: var(--surface-window);
    border: 1px solid var(--border-subtle);
    border-radius: 999px;
    box-shadow: 0 2px 8px rgb(0 0 0 / 0.18);
    color: var(--fg-muted);
    display: inline-flex;
    font-size: 0.72rem;
    font-weight: 500;
    gap: 0.35rem;
    padding: 0.32rem 0.85rem;
    pointer-events: auto;
    transition:
      background 120ms ease,
      color 120ms ease;
  }

  .scroll-pill:hover {
    background: color-mix(in srgb, var(--fg-default) 8%, var(--surface-window));
    color: var(--fg-default);
  }

  .chat-footer {
    background: linear-gradient(
      to top,
      var(--surface-canvas) 0%,
      var(--surface-canvas) 72%,
      color-mix(in srgb, var(--surface-canvas) 0%, transparent) 100%
    );
  }

  .approval-inline {
    align-items: center;
    background: var(--surface-window);
    border: 1px solid color-mix(in srgb, var(--warning, #d97706) 45%, var(--border-subtle));
    border-radius: 0.75rem;
    color: var(--fg-default);
    display: flex;
    gap: 0.65rem;
    margin-bottom: 0.45rem;
    max-width: 780px;
    padding: 0.55rem 0.65rem;
  }

  .approval-inline-icon {
    align-items: center;
    background: color-mix(in srgb, var(--warning, #d97706) 14%, transparent);
    border-radius: 999px;
    color: var(--warning, #d97706);
    display: flex;
    flex: 0 0 auto;
    font-size: 0.75rem;
    font-weight: 700;
    height: 1.55rem;
    justify-content: center;
    width: 1.55rem;
  }

  .approval-inline-copy {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
  }

  .approval-inline-title {
    font-size: 0.78rem;
    font-weight: 700;
  }

  .approval-inline-detail {
    color: var(--fg-muted);
    font-size: 0.72rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .approval-inline-btn {
    border: 1px solid var(--border-subtle);
    border-radius: 0.5rem;
    font-size: 0.74rem;
    font-weight: 650;
    padding: 0.3rem 0.65rem;
  }

  .approval-deny {
    color: var(--fg-muted);
  }

  .approval-allow {
    background: var(--fg-default);
    color: var(--surface-canvas);
  }

  @media (max-width: 640px) {
    .approval-inline {
      align-items: stretch;
      flex-wrap: wrap;
      margin-inline: 1rem;
    }
  }
</style>
