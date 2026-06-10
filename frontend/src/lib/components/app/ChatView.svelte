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
  import { marked } from 'marked';
  import DOMPurify from 'dompurify';
  import { isTauri, invokeCommand } from '$lib/tauri';
  import {
    api,
    attachmentUrl,
    ApiError,
    type SessionMeta,
    type TranscriptEvent,
    type TranscriptUsage,
    type AttachedFile
  } from '$lib/api/client';
  import { subscribeSSE, type SSEHandle } from '$lib/api/sse';
  import {
    Bot,
    Send,
    ChevronDown,
    ChevronUp,
    Paperclip,
    User,
    CircleCheck,
    CircleAlert,
    Loader2,
    X,
    Download,
    FileText,
    FileJson,
    FileCode2,
    FileSpreadsheet,
    RotateCcw
  } from '$lib/icons';
  import { toast } from 'svelte-sonner';

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

  // ---- Types ----------------------------------------------------------------

  type ToolBlock = {
    id: string;
    name: string;
    args: unknown;
    result?: unknown;
    isError: boolean;
    expanded: boolean;
  };

  type ExcalidrawScene = {
    raw: string;      // raw JSON string from the fence
    svgHtml?: string; // set when successfully rendered
    failed?: boolean; // set when rendering fails; shows JSON fallback
  };

  type ChatTurn = {
    id: string;
    role: 'user' | 'assistant' | 'system';
    content: string;
    thinking?: string;
    toolBlocks: ToolBlock[];
    isStreaming: boolean;
    renderedHtml: string; // empty while streaming; populated once when isStreaming→false
    cleanedContent?: string; // content with excalidraw fences stripped, used for markdown
    excalidrawScenes: ExcalidrawScene[]; // scenes extracted from content fences
    inlineImages: string[]; // standalone image URLs / data-URIs detected in content
    model?: string;
    source?: string;
    usage?: TranscriptUsage;
    /** Duration in ms from the turn_duration system_note following this turn. */
    durationMs?: number;
    /** Set by 1.2s inactivity debounce: triggers markdown render while isStreaming is still true. */
    settled?: boolean;
  };

  type PtyOutputEvent = {
    type: 'session.output';
    session_id: string;
    seq: number;
    b64: string;
  };

  // ---- State ----------------------------------------------------------------

  let turns = $state<ChatTurn[]>([]);
  let input = $state('');
  let sending = $state(false);
  let textareaEl: HTMLTextAreaElement | null = $state(null);
  let scrollEl: HTMLDivElement | null = $state(null);
  let fallbackArmed = $state(false);
  let transcriptSeen = $state(false);

  // Lightbox
  let lightboxSrc = $state<string | null>(null);

  // Attachments
  let attachments = $state<AttachedFile[]>([]);
  let attaching = $state(false);
  let fileInputEl: HTMLInputElement | null = $state(null);

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
  let fallbackTimer: ReturnType<typeof setTimeout> | null = null;

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
  let forceNextScroll = false;  // set true in openSSE so the first batch scrolls unconditionally

  // BUG D — historical turns from previous session (set after prevSid fetch completes)
  type PrevTurn = { id: string; role: 'user' | 'assistant'; content: string };
  let historicalTurns = $state<PrevTurn[]>([]);
  let historicalLoaded = $state(false);

  // Fix 3 — Debounce timers per turn ID (non-reactive Map).
  // A timer fires 1.2s after the last content append to mark the turn "settled"
  // and trigger markdown render even if detected_state is still 'working'.
  const settledTimers = new Map<string, ReturnType<typeof setTimeout>>();

  const encoder = new TextEncoder();

  const stopped = $derived(!session || session.status !== 'running');

  // ---- DOMPurify config (allows data:image/ URIs) ---------------------------

  const PURIFY_CFG = {
    // Extends the default ALLOWED_URI_REGEXP to permit data:image/ base64 URIs
    // which are safe (images can't execute JS). All other data: schemes are blocked.
    ALLOWED_URI_REGEXP:
      /^(?:(?:(?:f|ht)tps?|mailto|tel|callto|sms|cid|xmpp):|data:image\/[a-z+]+;base64,|[^a-z]|[a-z+.\-]+(?:[^a-z+.\-:]|$))/i
  };

  // ---- Markdown helper ------------------------------------------------------

  async function renderMarkdown(text: string): Promise<string> {
    if (isTauri) {
      // Sanitize pulldown-cmark output even in Tauri: agent content is untrusted and
      // in Tauri an XSS can reach native IPC. DOMPurify is available as a normal
      // bundle import on both web and Tauri paths.
      const html = await invokeCommand<string>('parse_markdown', { text });
      return DOMPurify.sanitize(html, PURIFY_CFG);
    }
    const html = marked.parse(text, { breaks: true, gfm: true });
    return DOMPurify.sanitize(typeof html === 'string' ? html : '', PURIFY_CFG);
  }

  // ---- Image detection helpers ----------------------------------------------

  // Matches standalone http(s) image URLs not inside markdown image syntax
  const IMG_URL_RE =
    /(?<![[(])https?:\/\/[^\s<>"')\]]+\.(?:png|jpe?g|gif|webp|svg)(?:[?#][^\s<>"')\]]*)?/gi;
  // Matches data:image/ base64 URIs
  const DATA_IMG_RE = /data:image\/[a-z+]+;base64,[A-Za-z0-9+/]+=*/gi;

  function extractStandaloneImages(content: string): string[] {
    const imgs = new Set<string>();
    for (const m of content.matchAll(IMG_URL_RE)) imgs.add(m[0]);
    for (const m of content.matchAll(DATA_IMG_RE)) imgs.add(m[0]);
    return [...imgs];
  }

  // ---- Excalidraw detection -------------------------------------------------

  function extractExcalidrawBlocks(content: string): { cleaned: string; scenes: string[] } {
    const scenes: string[] = [];
    const cleaned = content.replace(/```excalidraw\r?\n([\s\S]*?)```/g, (_match, body: string) => {
      scenes.push(body.trim());
      return ''; // remove fence from rendered markdown
    });
    return { cleaned, scenes };
  }

  function isExcalidrawJson(val: unknown): boolean {
    if (!val || typeof val !== 'object') return false;
    const obj = val as Record<string, unknown>;
    return obj.type === 'excalidraw' && Array.isArray(obj.elements);
  }

  // Cached excalidraw module promise (loaded once)
  let excalidrawModPromise: Promise<{ exportToSvg: Function } | null> | null = null;

  function getExcalidrawMod(): Promise<{ exportToSvg: Function } | null> {
    if (!excalidrawModPromise) {
      // NOTE: @excalidraw/utils has no stable release as of 2026-06-10 — all published
      // versions are pre-release (0.1.3-testN). The version is pinned exactly in
      // package.json (no ^ range). If the import fails at runtime the scene falls back
      // to the collapsible JSON block below (scene.failed = true path).
      excalidrawModPromise = import('@excalidraw/utils')
        .then((m) => m as { exportToSvg: Function })
        .catch(() => null);
    }
    return excalidrawModPromise;
  }

  async function renderExcalidraw(scene: ExcalidrawScene): Promise<void> {
    if (scene.svgHtml !== undefined || scene.failed) return;
    try {
      const parsed: unknown = JSON.parse(scene.raw);
      if (!isExcalidrawJson(parsed)) {
        scene.failed = true;
        return;
      }
      const mod = await getExcalidrawMod();
      if (!mod) {
        scene.failed = true;
        return;
      }
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const svgEl: SVGSVGElement = await (mod.exportToSvg as any)({
        elements: (parsed as Record<string, unknown>).elements,
        appState: (parsed as Record<string, unknown>).appState ?? {},
        files: (parsed as Record<string, unknown>).files ?? null
      });
      // Sanitize before storing: the scene comes from agent output (not trusted).
      // DOMPurify with SVG profiles removes <script> elements and event-handler
      // attributes (e.g. onload="...") that can execute JS inside an inline SVG.
      // Used unconditionally — DOMPurify is bundled and works in both browser and Tauri.
      scene.svgHtml = DOMPurify.sanitize(svgEl.outerHTML, {
        USE_PROFILES: { svg: true, svgFilters: true }
      });
    } catch {
      scene.failed = true;
    }
  }

  // ---- Tool result image extraction -----------------------------------------

  function extractSingleImageBlock(item: unknown): string | null {
    if (!item || typeof item !== 'object') return null;
    const obj = item as Record<string, unknown>;

    // Anthropic format: {type:'image', source:{type:'base64', media_type, data}}
    if (obj.type === 'image' && obj.source && typeof obj.source === 'object') {
      const src = obj.source as Record<string, unknown>;
      if (src.type === 'base64' && src.media_type && src.data) {
        return `data:${src.media_type};base64,${src.data}`;
      }
      if (src.type === 'url' && typeof src.url === 'string') return src.url;
    }

    // OpenAI-style: {type:'image_url', image_url:{url}}
    if (obj.type === 'image_url' && obj.image_url && typeof obj.image_url === 'object') {
      const iu = obj.image_url as Record<string, unknown>;
      if (typeof iu.url === 'string') return iu.url;
    }

    // Flat base64 fields (some custom tools): {base64, mime_type} or {data, media_type}
    const maybeBase64 = obj.base64 ?? obj.data;
    const maybeMime = obj.mime_type ?? obj.media_type;
    if (typeof maybeBase64 === 'string' && typeof maybeMime === 'string' && maybeMime.startsWith('image/')) {
      return `data:${maybeMime};base64,${maybeBase64}`;
    }

    return null;
  }

  function extractToolResultImages(result: unknown): string[] {
    if (!result) return [];
    if (Array.isArray(result)) {
      const images: string[] = [];
      for (const item of result as unknown[]) {
        const img = extractSingleImageBlock(item);
        if (img) images.push(img);
      }
      return images;
    }
    const img = extractSingleImageBlock(result);
    return img ? [img] : [];
  }

  function hasNonImageContent(result: unknown): boolean {
    if (!result || !Array.isArray(result)) return false;
    return (result as unknown[]).some((item) => {
      if (!item || typeof item !== 'object') return true;
      const obj = item as Record<string, unknown>;
      return obj.type !== 'image' && obj.type !== 'image_url';
    });
  }

  // ---- Syntax highlighting (highlight.js, dynamic import) -------------------

  type HljsCore = {
    highlightElement: (el: HTMLElement) => void;
    registerLanguage: (name: string, lang: unknown) => void;
  };

  let hljsPromise: Promise<HljsCore> | null = null;

  function getHljs(): Promise<HljsCore> {
    if (!hljsPromise) {
      hljsPromise = (async () => {
        const { default: core } = await import('highlight.js/lib/core');
        const [js, ts, rust, python, bash, json, xml, css, sql] = await Promise.all([
          import('highlight.js/lib/languages/javascript'),
          import('highlight.js/lib/languages/typescript'),
          import('highlight.js/lib/languages/rust'),
          import('highlight.js/lib/languages/python'),
          import('highlight.js/lib/languages/bash'),
          import('highlight.js/lib/languages/json'),
          import('highlight.js/lib/languages/xml'), // html / xml
          import('highlight.js/lib/languages/css'),
          import('highlight.js/lib/languages/sql')
        ]);
        core.registerLanguage('javascript', js.default);
        core.registerLanguage('js', js.default);
        core.registerLanguage('typescript', ts.default);
        core.registerLanguage('ts', ts.default);
        core.registerLanguage('rust', rust.default);
        core.registerLanguage('python', python.default);
        core.registerLanguage('py', python.default);
        core.registerLanguage('bash', bash.default);
        core.registerLanguage('sh', bash.default);
        core.registerLanguage('shell', bash.default);
        core.registerLanguage('json', json.default);
        core.registerLanguage('html', xml.default);
        core.registerLanguage('xml', xml.default);
        core.registerLanguage('css', css.default);
        core.registerLanguage('sql', sql.default);
        return core as HljsCore;
      })();
    }
    return hljsPromise;
  }

  // Svelte action applied to rendered prose containers: applies syntax highlighting
  // and delegates img clicks to the lightbox (avoids putting onclick on a non-interactive div).
  function hlAction(node: HTMLElement): { destroy: () => void } {
    getHljs()
      .then((hljs) => {
        node.querySelectorAll('pre code').forEach((block) => {
          const el = block as HTMLElement;
          if (!el.dataset.highlighted) {
            hljs.highlightElement(el);
          }
        });
      })
      .catch(() => {/* ignore if hljs unavailable */});

    function handleImgClick(ev: MouseEvent) {
      const target = ev.target as HTMLElement;
      if (target.tagName === 'IMG') {
        const src = (target as HTMLImageElement).src;
        if (src) openLightbox(src);
      }
    }

    node.addEventListener('click', handleImgClick);
    return { destroy: () => node.removeEventListener('click', handleImgClick) };
  }

  // ---- Attachment helpers ---------------------------------------------------

  function isImageMime(mime: string): boolean {
    return mime.startsWith('image/');
  }

  function formatSize(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  }

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

  function pickFiles(): void {
    if (!session || stopped || attaching) return;
    fileInputEl?.click();
  }

  async function onFilesPicked(ev: Event): Promise<void> {
    const sid = session?.id;
    if (!sid) return;
    const t = ev.currentTarget as HTMLInputElement;
    const files = t.files ? Array.from(t.files) : [];
    t.value = '';
    if (!files.length) return;
    attaching = true;
    try {
      const saved = await api.sessions.attach(sid, files);
      const summary = saved.map((f) => f.name).join(', ');
      toast.success(`Attached ${saved.length} file${saved.length === 1 ? '' : 's'}: ${summary}`);
      await loadAttachments();
    } catch (err) {
      const msg = err instanceof ApiError ? err.message : String(err);
      toast.error(`Attach failed: ${msg}`);
    } finally {
      attaching = false;
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
    if (existing) { clearTimeout(existing); settledTimers.delete(turn.id); }
    if (turn.isStreaming && turn.content && !turn.renderedHtml) {
      turn.settled = true;
    }
  }

  // Fix 4 helpers — duration display -------------------------------------------

  function formatDuration(ms: number): string {
    if (ms < 60000) return `${(ms / 1000).toFixed(1)}s`;
    const m = Math.floor(ms / 60000);
    const s = Math.round((ms % 60000) / 1000);
    return `${m}m ${s}s`;
  }

  // Fix 2 helpers — thinking tail / auto-scroll action -------------------------

  // Returns the last 10 lines of thinking text for the live streaming view.
  function thinkingTail(thinking: string): string {
    const lines = thinking.split('\n');
    return lines.length <= 10 ? thinking : lines.slice(lines.length - 10).join('\n');
  }

  // Svelte action: scrolls the node to its bottom whenever the parameter changes.
  // Used on the thinking-tail container so it tracks new content without
  // conflicting with scheduleScroll (they operate on different elements).
  function thinkingScroll(
    node: HTMLElement,
    _v: string
  ): { update: (v: string) => void; destroy: () => void } {
    node.scrollTop = node.scrollHeight;
    return {
      update() { node.scrollTop = node.scrollHeight; },
      destroy() {}
    };
  }

  // ---------------------------------------------------------------------------

  function processEvent(ev: TranscriptEvent) {
    if (ev.kind === 'meta') return;

    if (ev.kind === 'system_note') {
      // Fix 4: turn_duration — attach ms to last assistant turn, never show as pill.
      if (ev.subtype === 'turn_duration') {
        const raw = ev.raw as Record<string, unknown> | null | undefined;
        const ms =
          typeof raw?.durationMs === 'number' ? raw.durationMs
          : typeof raw?.duration_ms === 'number' ? raw.duration_ms
          : typeof raw?.duration === 'number' ? raw.duration
          : null;
        if (ms !== null) {
          const last = lastAssistantTurn();
          if (last) last.durationMs = ms;
        }
        return;
      }
      // Skip system_notes with null content — these are internal Claude lifecycle
      // events (e.g. other subtypes) that have no human-readable label.
      if (ev.content == null) return;
      // Boundary: immediately settle any streaming assistant turn.
      const prevAssistant = lastAssistantTurn();
      if (prevAssistant?.isStreaming) markTurnSettled(prevAssistant);
      turns.push({
        id: String(ev.seq),
        role: 'system',
        content: ev.content,
        toolBlocks: [],
        isStreaming: false,
        renderedHtml: '',
        excalidrawScenes: [],
        inlineImages: []
      });
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
          inlineImages: []
        });
      } else if (ev.role === 'assistant') {
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
        last.content += ev.content ?? '';
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
        isError: false,
        expanded: true
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
            block.expanded = block.isError;
            break;
          }
        }
      }
      return;
    }
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
        const { cleaned, scenes } = extractExcalidrawBlocks(turn.content);
        turn.cleanedContent = cleaned || turn.content;
        turn.excalidrawScenes = scenes.map((raw) => ({ raw }));
        turn.inlineImages = extractStandaloneImages(cleaned);
        for (const scene of turn.excalidrawScenes) {
          void renderExcalidraw(scene);
        }
      }
    }

    if (pending.length > 1 && isTauri) {
      // Batch path: rayon renders all in parallel in one IPC roundtrip
      for (const turn of pending) renderingTurnIds.add(turn.id);
      void invokeCommand<string[]>('parse_markdown_batch', {
        texts: pending.map((t) => t.cleanedContent ?? t.content)
      })
        .then((htmls) => {
          // Sanitize pulldown-cmark output before insertion: in Tauri, XSS reaches
          // native IPC. DOMPurify is available as a normal import on both targets.
          htmls.forEach((html, i) => {
            const t = pending[i];
            // P1-A: discard stale batch renders — new content arrived while in-flight.
            if (staleRenders.has(t.id)) { staleRenders.delete(t.id); return; }
            t.renderedHtml = DOMPurify.sanitize(html, PURIFY_CFG);
          });
        })
        .finally(() => {
          for (const turn of pending) renderingTurnIds.delete(turn.id);
        });
    } else {
      // Single or browser fallback
      for (const turn of pending) {
        renderingTurnIds.add(turn.id);
        void renderMarkdown(turn.cleanedContent ?? turn.content)
          .then((html) => {
            // P1-A: discard if new content arrived while this render was in flight.
            if (staleRenders.has(turn.id)) { staleRenders.delete(turn.id); return; }
            turn.renderedHtml = html;
          })
          .finally(() => {
            renderingTurnIds.delete(turn.id);
          });
      }
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
    settledTimers.forEach((t) => clearTimeout(t)); settledTimers.clear();
    staleRenders.clear();
    lastSeq = 0;
    fallbackOutputBytes = 0;
    fallbackArmed = false;
    transcriptSeen = false;
    eventQueue = [];
    sseAttempts = 0;
    transcriptUnavailable = false;
    forceNextScroll = true;   // BUG B: first event batch scrolls unconditionally
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
    if (!ev.b64) return;
    const text = cleanPtyText(decodeBase64Utf8(ev.b64));
    if (!text.trim()) return;

    let turn = turns.find((t) => t.id === 'pty-fallback');
    if (!turn) {
      turn = {
        id: 'pty-fallback',
        role: 'assistant',
        content: '',
        toolBlocks: [],
        isStreaming: true,
        renderedHtml: '',
        excalidrawScenes: [],
        inlineImages: [],
        source: 'pty',
        model: session?.kind ? `${session.kind} output` : 'agent output'
      };
      turns = [turn];
    }

    fallbackOutputBytes += text.length;
    const next = `${turn.content}${text}`;
    turn.content = next.length > 120_000 ? next.slice(next.length - 120_000) : next;
    scheduleScroll();
  }

  function decodeBase64Utf8(value: string): string {
    try {
      const binary = atob(value);
      const bytes = Uint8Array.from(binary, (char) => char.charCodeAt(0));
      return new TextDecoder().decode(bytes);
    } catch {
      return '';
    }
  }

  function cleanPtyText(value: string): string {
    return value
      .replace(/\x1b\][^\x07]*(?:\x07|\x1b\\)/g, '')
      .replace(/\x1b\[[0-?]*[ -/]*[@-~]/g, '')
      .replace(/\x1b[=>]/g, '')
      .replace(/\r\n/g, '\n')
      .replace(/\r/g, '\n')
      .replace(//g, '')
      .replace(/\n{4,}/g, '\n\n\n');
  }

  // BUG D — Historical turns from prevSid (previous session after restart).
  // Fetches the old transcript via SSE, collects replay events, and closes after
  // 600ms of silence (end-of-replay detection) or 5s safety cap.
  $effect(() => {
    const pid = prevSid;
    if (!pid) {
      if (historicalTurns.length > 0) { historicalTurns = []; historicalLoaded = false; }
      return;
    }

    const tempTurns: PrevTurn[] = [];
    let histHandle: SSEHandle | null = null;
    let doneTimer: ReturnType<typeof setTimeout> | null = null;
    let maxTimer: ReturnType<typeof setTimeout> | null = null;
    let done = false;

    const finish = () => {
      if (done) return;
      done = true;
      if (doneTimer) { clearTimeout(doneTimer); doneTimer = null; }
      if (maxTimer) { clearTimeout(maxTimer); maxTimer = null; }
      histHandle?.close();
      histHandle = null;
      historicalTurns = tempTurns;
      historicalLoaded = true;
      forceNextScroll = true;
      scheduleScroll();
    };

    const resetIdle = () => {
      if (doneTimer) clearTimeout(doneTimer);
      doneTimer = setTimeout(finish, 600);
    };

    histHandle = subscribeSSE(`/sessions/${pid}/transcript?since=0`, () => {}, {
      reconnect: false,
      // BUG D fix: start the idle timer only once the SSE connection is open.
      // The old code called resetIdle() immediately (before the connection was
      // established), so the 600ms window elapsed before replay events arrived
      // (backend delivers them in up to ~2.5s). Moving to onOpen ensures the
      // idle clock starts after the HTTP stream is open and replaying.
      onOpen: () => resetIdle(),
      onError: () => finish(),
      events: {
        transcript: (data) => {
          if (done) return;
          const ev = data as TranscriptEvent;
          if (ev.kind === 'message' && ev.content != null) {
            const last = tempTurns[tempTurns.length - 1];
            if (ev.role === 'user') {
              tempTurns.push({ id: `prev-${ev.seq}`, role: 'user', content: ev.content });
            } else if (ev.role === 'assistant') {
              if (last?.role === 'assistant') {
                last.content += ev.content;
              } else {
                tempTurns.push({ id: `prev-${ev.seq}`, role: 'assistant', content: ev.content });
              }
            }
          }
          resetIdle();
        }
      }
    });

    maxTimer = setTimeout(finish, 5000);   // safety: don't block on still-live old sessions

    return () => {
      done = true;
      if (doneTimer) clearTimeout(doneTimer);
      if (maxTimer) clearTimeout(maxTimer);
      histHandle?.close();
    };
  });

  // BUG E — Aggregate token usage from all transcript turns.
  const totalInputTok = $derived(turns.reduce((s, t) => s + (t.usage?.input_tokens ?? 0), 0));
  const totalOutputTok = $derived(turns.reduce((s, t) => s + (t.usage?.output_tokens ?? 0), 0));

  $effect(() => {
    onTotalTokens?.(totalInputTok, totalOutputTok);
  });

  // ---- Input handling -------------------------------------------------------

  async function sendInput() {
    if (!session || !input.trim() || sending || stopped) return;
    sending = true;
    const payload = input;
    input = '';
    try {
      await api.sessions.input(session.id, encoder.encode(payload));
      await new Promise((r) => setTimeout(r, 60));
      await api.sessions.input(session.id, encoder.encode('\r'));
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      toast.error(`Send failed: ${msg}`);
    } finally {
      sending = false;
    }
  }

  function onKeydown(ev: KeyboardEvent) {
    if (ev.key === 'Enter' && !ev.shiftKey) {
      ev.preventDefault();
      void sendInput();
    }
  }

  function onTextareaInput() {
    if (!textareaEl) return;
    textareaEl.style.height = 'auto';
    const lineHeight = 20;
    const maxHeight = lineHeight * 6 + 20; // up to 6 lines (LAYOUT: compact composer)
    textareaEl.style.height = Math.min(textareaEl.scrollHeight, maxHeight) + 'px';
  }

  // ---- Thinking block toggle ------------------------------------------------

  let thinkingExpanded = $state<Record<string, boolean>>({});

  function toggleThinking(turnId: string, streaming: boolean) {
    thinkingExpanded[turnId] = !(thinkingExpanded[turnId] ?? streaming);
  }

  function isThinkingExpanded(turnId: string, streaming: boolean): boolean {
    if (turnId in thinkingExpanded) return thinkingExpanded[turnId];
    return streaming;
  }

  // ---- JSON pretty print ----------------------------------------------------

  function prettyJson(val: unknown): string {
    try {
      return JSON.stringify(val, null, 2);
    } catch {
      return String(val);
    }
  }

  function formatInt(value: number | null | undefined): string {
    return new Intl.NumberFormat().format(value ?? 0);
  }

  function usageLabel(usage: TranscriptUsage | undefined): string | null {
    if (!usage) return null;
    const parts: string[] = [];
    if (usage.input_tokens != null) parts.push(`${formatInt(usage.input_tokens)} in`);
    if (usage.output_tokens != null) parts.push(`${formatInt(usage.output_tokens)} out`);
    return parts.length > 0 ? parts.join(' · ') : null;
  }

  function toolState(block: ToolBlock): 'error' | 'done' | 'running' {
    if (block.isError) return 'error';
    if (block.result !== undefined) return 'done';
    return 'running';
  }

  function toolPreview(value: unknown): string {
    if (value == null) return '';
    if (typeof value === 'string') return value.slice(0, 140);
    if (Array.isArray(value)) return `${value.length} item${value.length === 1 ? '' : 's'}`;
    if (typeof value === 'object') return Object.keys(value as Record<string, unknown>).join(', ');
    return String(value);
  }

  // ---- Attachment file icon helper ------------------------------------------

  function fileIconName(mime: string): string {
    if (mime.startsWith('image/')) return 'image';
    if (mime === 'application/json') return 'json';
    if (
      mime.includes('spreadsheet') ||
      mime === 'text/csv' ||
      mime.includes('excel')
    )
      return 'spreadsheet';
    if (mime.includes('zip') || mime.includes('tar') || mime.includes('gzip')) return 'code';
    return 'text';
  }
</script>

<svelte:window onkeyup={onWindowKeyup} />

<!-- Lightbox overlay -->
{#if lightboxSrc}
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
  <div
    class="lightbox-overlay"
    onclick={closeLightbox}
    role="dialog"
    tabindex="-1"
    aria-modal="true"
    aria-label="Image preview"
  >
    <button
      type="button"
      class="lightbox-close"
      onclick={closeLightbox}
      aria-label="Close lightbox"
    >
      <X size={20} />
    </button>
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <img
      src={lightboxSrc}
      alt="Preview"
      class="lightbox-img"
      onclick={(e) => e.stopPropagation()}
    />
  </div>
{/if}

<div class="chat-shell flex h-full flex-col overflow-hidden" style="background: var(--surface-canvas);">
  <!-- Message list -->
  <div
    class="chat-scroll min-h-0 flex-1 overflow-y-auto"
    style="transform: translateZ(0); will-change: scroll-position;"
    bind:this={scrollEl}
    onscroll={onChatScrolled}
  >
    {#if !session}
      <!-- No session selected -->
      <div class="empty-chat mx-auto flex h-full max-w-[760px] flex-col items-center justify-center gap-4 px-6 text-center">
        <div class="empty-mark" aria-hidden="true">✳</div>
        <p class="empty-title">Select a session</p>
        <p class="empty-subtitle">The structured transcript will appear here.</p>
      </div>
    {:else if turns.length === 0}
      <!-- Empty state -->
      <div class="empty-chat mx-auto flex h-full max-w-[760px] flex-col items-center justify-center gap-4 px-6 text-center">
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
    {:else}
      <div class="chat-thread mx-auto max-w-[820px] px-5 py-8 sm:px-7">
        <!-- BUG D: historical turns from previous session (shown dimmed before separator) -->
        {#if historicalLoaded && historicalTurns.length > 0}
          <div class="prev-history-wrap">
            {#each historicalTurns as ht (ht.id)}
              <div class="prev-turn prev-turn-{ht.role}">
                <span class="prev-turn-label">{ht.role === 'user' ? 'You' : 'Agent'}</span>
                <p class="prev-turn-content">{ht.content.length > 600 ? ht.content.slice(0, 600) + '…' : ht.content}</p>
              </div>
            {/each}
          </div>
          <div class="session-restart-sep">— session restarted —</div>
        {/if}
        {#each turns as turn (turn.id)}
          {#if turn.role === 'user'}
            <!-- User turn -->
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
            <!-- Assistant turn -->
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

                <!-- Thinking block (Fix 2: live tail while streaming, collapse on content/done) -->
                {#if turn.thinking}
                  {@const thinkingActive = turn.isStreaming && !turn.content}
                  {@const expanded = isThinkingExpanded(turn.id, thinkingActive)}
                  <div
                    class="action-block thinking-block"
                    class:action-open={expanded}
                  >
                    <button
                      type="button"
                      onclick={() => toggleThinking(turn.id, thinkingActive)}
                      class="action-header"
                    >
                      <span class="action-icon subtle">
                        {#if thinkingActive}
                          <Loader2 size={13} class="animate-spin" />
                        {:else}
                          <ChevronDown size={13} />
                        {/if}
                      </span>
                      {#if thinkingActive}
                        <!-- Live: animated dots while actively thinking -->
                        <span class="action-title thinking-live-title">
                          Thinking<span class="thinking-dots" aria-hidden="true"><span>.</span><span>.</span><span>.</span></span>
                        </span>
                        <span class="action-preview"></span>
                      {:else}
                        <!-- Completed: "Thought (N.Ns)" summary -->
                        <span class="action-title">
                          Thought{#if turn.durationMs}&thinsp;<span class="thinking-dur">({formatDuration(turn.durationMs)})</span>{/if}
                        </span>
                        <span class="action-preview">{turn.thinking.slice(0, 90)}</span>
                      {/if}
                      <span class="action-caret">
                        {#if expanded}<ChevronUp size={13} />{:else}<ChevronDown size={13} />{/if}
                      </span>
                    </button>
                    {#if expanded}
                      {#if thinkingActive}
                        <!-- Live tail: last ~10 lines, auto-scrolled; does not conflict with
                             scheduleScroll which operates on the outer chat-scroll element. -->
                        <div
                          class="action-detail thinking-detail thinking-tail"
                          use:thinkingScroll={turn.thinking}
                        >
                          {thinkingTail(turn.thinking)}
                        </div>
                      {:else}
                        <!-- Completed: full thinking text -->
                        <div class="action-detail thinking-detail">
                          {turn.thinking}
                        </div>
                      {/if}
                    {/if}
                  </div>
                {/if}

                <!-- Tool call blocks -->
                {#each turn.toolBlocks as block (block.id)}
                  {@const state = toolState(block)}
                  {@const resultImages = block.result !== undefined ? extractToolResultImages(block.result) : []}
                  <div
                    class="action-block tool-block"
                    class:tool-error={state === 'error'}
                    class:action-open={block.expanded}
                  >
                    <button
                      type="button"
                      onclick={() => {
                        block.expanded = !block.expanded;
                      }}
                      class="action-header"
                    >
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
                        <!-- Args -->
                        <div class="action-label">Arguments</div>
                        <pre class="action-json">{prettyJson(block.args)}</pre>
                        <!-- Result (if available) -->
                        {#if block.result !== undefined}
                          <div class="action-label">{block.isError ? 'Error' : 'Result'}</div>
                          {#if resultImages.length > 0}
                            <!-- Show extracted images instead of raw JSON -->
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
                            {#if hasNonImageContent(block.result)}
                              <pre class="action-json" class:error-json={block.isError}>{prettyJson(block.result)}</pre>
                            {/if}
                          {:else}
                            <pre class="action-json" class:error-json={block.isError}>{prettyJson(block.result)}</pre>
                          {/if}
                        {/if}
                      </div>
                    {/if}
                  </div>
                {/each}

                <!-- Main content: raw text while streaming, rendered HTML when done -->
                {#if turn.content}
                  {#if turn.source === 'pty'}
                    <!-- BUG C: PTY blob shown as collapsed block; transcript arrival removes this turn -->
                    <details class="pty-block">
                      <summary class="pty-block-summary">
                        <Loader2 size={11} class="animate-spin pty-block-icon" />
                        <span>Terminal output (waiting for transcript…)</span>
                        {#if onSwitchToTerminal}
                          <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
                          <span
                            role="button"
                            tabindex="0"
                            class="pty-terminal-link"
                            onclick={(e) => { e.preventDefault(); e.stopPropagation(); onSwitchToTerminal!(); }}
                          >View in Terminal tab</span>
                        {/if}
                      </summary>
                      <pre class="pty-fallback-text">{turn.content}</pre>
                    </details>
                  {:else if turn.renderedHtml}
                    <!-- Completed turn: safe rendered markdown.
                         hlAction handles syntax highlighting AND img click → lightbox via DOM listener. -->
                    <div
                      class="chat-prose max-w-none leading-relaxed"
                      use:hlAction
                    >
                      <!-- eslint-disable-next-line svelte/no-at-html-tags -->
                      {@html turn.renderedHtml}
                    </div>
                    <!-- Standalone images detected in content (not already in markdown) -->
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
                    <!-- Excalidraw scenes extracted from fences -->
                    {#each turn.excalidrawScenes as scene, i (i)}
                      {#if scene.svgHtml}
                        <div class="excalidraw-container">
                          <!-- eslint-disable-next-line svelte/no-at-html-tags -->
                          {@html scene.svgHtml}
                        </div>
                      {:else if scene.failed}
                        <!-- Fallback: collapsed JSON block -->
                        <details class="excalidraw-fallback">
                          <summary class="excalidraw-fallback-summary">
                            Excalidraw scene (SVG render unavailable)
                          </summary>
                          <pre class="action-json">{scene.raw}</pre>
                        </details>
                      {:else}
                        <!-- Still rendering -->
                        <div class="excalidraw-loading">
                          <Loader2 size={14} class="animate-spin" />
                          <span>Rendering diagram…</span>
                        </div>
                      {/if}
                    {/each}
                  {:else}
                    <!-- Streaming turn: raw text, no markdown parse overhead -->
                    <p class="chat-streaming-text whitespace-pre-wrap break-words">
                      {turn.content}
                    </p>
                  {/if}
                {/if}

                <!-- Fix 4: Duration metadata (turn_duration system_note) — subtle line under content -->
                {#if turn.durationMs}
                  <div class="turn-duration-row">
                    <span class="usage-chip" title="Turn duration">&#x23F1; {formatDuration(turn.durationMs)}</span>
                  </div>
                {/if}

                <!-- Streaming cursor -->
                {#if turn.isStreaming && !turn.content && !turn.thinking && turn.toolBlocks.length === 0}
                  <span class="stream-cursor"></span>
                {/if}
              </div>
            </div>
          {:else}
            <!-- System turn — centered pill -->
            <div
              class="system-turn my-4 flex justify-center"
              style="contain: content; content-visibility: auto; contain-intrinsic-size: 32px;"
            >
              <span>{turn.content}</span>
            </div>
          {/if}
        {/each}
      </div>
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

  <!-- Chat input footer -->
  <div class="chat-composer-wrap shrink-0 px-4 pb-4 pt-3">
    <div class="chat-composer mx-auto">
      <!-- Attachment bar (shown when session has attached files) -->
      {#if attachments.length > 0 && session}
        {@const sid = session.id}
        <div class="attachment-bar">
          {#each attachments as file (file.name)}
            {#if isImageMime(file.mime)}
              <!-- Image thumbnail — click to lightbox -->
              <button
                type="button"
                class="attachment-thumb"
                onclick={() => openLightbox(attachmentUrl(sid, file.name))}
                title={file.name}
              >
                <img
                  src={attachmentUrl(sid, file.name)}
                  alt={file.name}
                  class="attachment-thumb-img"
                />
              </button>
            {:else}
              <!-- Document card with download -->
              <a
                href={attachmentUrl(sid, file.name)}
                download={file.name}
                target="_blank"
                rel="noreferrer"
                class="attachment-doc"
                title={`Download ${file.name}`}
              >
                <span class="attachment-doc-icon">
                  {#if fileIconName(file.mime) === 'json'}
                    <FileJson size={14} />
                  {:else if fileIconName(file.mime) === 'spreadsheet'}
                    <FileSpreadsheet size={14} />
                  {:else if fileIconName(file.mime) === 'code'}
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

      <!-- Textarea row (BUG E: show restart CTA when session is stopped) -->
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
      <!-- Action bar: attach left, send right -->
      <div class="flex items-center justify-between px-3 pb-3">
        <!-- Hidden file input -->
        <input
          type="file"
          multiple
          class="hidden"
          bind:this={fileInputEl}
          onchange={onFilesPicked}
        />
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
</div>

<style>
  /* ---- Lightbox ---------------------------------------------------------- */
  .lightbox-overlay {
    align-items: center;
    background: rgba(0, 0, 0, 0.92);
    bottom: 0;
    cursor: pointer;
    display: flex;
    justify-content: center;
    left: 0;
    position: fixed;
    right: 0;
    top: 0;
    z-index: 9999;
  }

  .lightbox-close {
    align-items: center;
    background: rgba(255, 255, 255, 0.1);
    border: 1px solid rgba(255, 255, 255, 0.18);
    border-radius: 999px;
    color: #fff;
    cursor: pointer;
    display: flex;
    height: 2.25rem;
    justify-content: center;
    position: fixed;
    right: 1.25rem;
    top: 1.25rem;
    transition: background 150ms ease;
    width: 2.25rem;
    z-index: 10000;
  }

  .lightbox-close:hover {
    background: rgba(255, 255, 255, 0.2);
  }

  .lightbox-img {
    border-radius: 0.5rem;
    cursor: default;
    max-height: 90vh;
    max-width: 90vw;
    object-fit: contain;
  }

  /* ---- Shell & scroll ---------------------------------------------------- */
  .chat-shell {
    color: var(--fg-default);
  }

  .chat-scroll {
    scrollbar-gutter: stable;
  }

  .chat-thread {
    padding-bottom: 1.5rem;
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

  /* ---- Turn layout ------------------------------------------------------- */
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

  /* ---- Turn meta --------------------------------------------------------- */
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

  /* ---- Content text ------------------------------------------------------ */
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

  .pty-fallback-text {
    background: transparent;
    border: 0;
    color: var(--fg-default);
    font-family:
      ui-sans-serif,
      system-ui,
      -apple-system,
      BlinkMacSystemFont,
      "Segoe UI",
      sans-serif;
    font-size: 0.9rem;
    line-height: 1.72;
    margin: 0;
    max-width: 74ch;
    overflow-x: auto;
    padding: 0;
    white-space: pre-wrap;
    word-break: break-word;
  }

  /* ---- Action blocks (tools / thinking) ---------------------------------- */
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

  .tool-error {
    border-color: color-mix(in srgb, #ef4444 48%, var(--border-subtle));
  }

  .action-header {
    align-items: center;
    color: var(--fg-muted);
    display: grid;
    font-size: 0.74rem;
    gap: 0.55rem;
    grid-template-columns: 1.15rem minmax(max-content, 11rem) minmax(0, 1fr) auto auto;
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

  .action-icon.done {
    color: #34d399;
  }

  .action-icon.error {
    color: #f87171;
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

  .action-state {
    border: 1px solid var(--border-subtle);
    border-radius: 999px;
    color: var(--fg-muted);
    font-size: 0.63rem;
    padding: 0.06rem 0.42rem;
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

  /* Fix 2 — Live thinking tail: capped height, scrollable, separate from global scroll */
  .thinking-tail {
    max-height: 200px;
    overflow-y: auto;
    scroll-behavior: auto; /* instant, not smooth — keeps up with fast thinking */
  }

  /* Fix 2 — Animated "Thinking..." dots in the header */
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
    0%, 80%, 100% { opacity: 0; }
    40% { opacity: 1; }
  }

  /* Fix 2 — Smaller muted duration shown in the collapsed "Thought" header */
  .thinking-dur {
    color: var(--fg-muted);
    font-size: 0.64rem;
    font-weight: 400;
  }

  /* Fix 4 — Duration metadata row under assistant turn content */
  .turn-duration-row {
    margin-top: 0.45rem;
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

  .error-json {
    color: #fca5a5;
  }

  /* ---- Tool result images ------------------------------------------------ */
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

  /* ---- Inline media (standalone images detected in content) -------------- */
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

  /* ---- Excalidraw -------------------------------------------------------- */
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
    max-width: 100%;
    height: auto;
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

  /* ---- Streaming cursor -------------------------------------------------- */
  .stream-cursor {
    animation: pulse 1.1s ease-in-out infinite;
    background: var(--fg-default);
    display: inline-block;
    height: 1rem;
    opacity: 0.7;
    vertical-align: -0.15rem;
    width: 0.45rem;
  }

  /* ---- System turn ------------------------------------------------------- */
  .system-turn span {
    background: var(--surface-window);
    border: 1px solid var(--border-subtle);
    border-radius: 999px;
    color: var(--fg-muted);
    font-size: 0.72rem;
    padding: 0.28rem 0.75rem;
  }

  /* ---- Composer ---------------------------------------------------------- */
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
    max-height: 120px;
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

  /* ---- Attachment bar ---------------------------------------------------- */
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

  .attachment-doc-icon {
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
    flex-shrink: 0;
    opacity: 0.6;
  }

  /* ---- chat-prose (rendered markdown) ------------------------------------ */
  .chat-prose {
    font-size: 0.9rem;
    line-height: 1.75;
  }

  :global(.chat-prose > :first-child) {
    margin-top: 0;
  }

  :global(.chat-prose > :last-child) {
    margin-bottom: 0;
  }

  :global(.chat-prose h1),
  :global(.chat-prose h2),
  :global(.chat-prose h3),
  :global(.chat-prose h4) {
    color: var(--fg-default);
    letter-spacing: 0;
    line-height: 1.25;
  }

  :global(.chat-prose h1) {
    font-size: 1.35rem;
    margin-top: 1.4em;
    margin-bottom: 0.6em;
  }

  :global(.chat-prose h2) {
    font-size: 1.15rem;
    margin-top: 1.3em;
    margin-bottom: 0.55em;
  }

  :global(.chat-prose h3) {
    font-size: 1rem;
    margin-top: 1.2em;
    margin-bottom: 0.45em;
  }

  :global(.chat-prose p),
  :global(.chat-prose ul),
  :global(.chat-prose ol),
  :global(.chat-prose blockquote),
  :global(.chat-prose pre),
  :global(.chat-prose table) {
    margin-top: 0.7em;
    margin-bottom: 0.7em;
  }

  :global(.chat-prose a) {
    color: var(--accent);
    text-decoration: underline;
    text-underline-offset: 3px;
  }

  :global(.chat-prose ul),
  :global(.chat-prose ol) {
    padding-left: 1.25rem;
  }

  :global(.chat-prose li) {
    line-height: 1.72;
  }

  :global(.chat-prose li + li) {
    margin-top: 0.2rem;
  }

  /* Fix 3 prose — bold text should be clearly readable, not just heavier */
  :global(.chat-prose strong),
  :global(.chat-prose b) {
    color: var(--fg-default);
    font-weight: 600;
  }

  /* Fix 3 prose — subtle horizontal rule */
  :global(.chat-prose hr) {
    border: none;
    border-top: 1px solid var(--border-subtle);
    margin: 1.2em 0;
    opacity: 0.7;
  }

  :global(.chat-prose blockquote) {
    border-left: 3px solid var(--border-subtle);
    color: var(--fg-muted);
    padding-left: 1rem;
    font-style: normal;
  }

  :global(.chat-prose pre) {
    background: var(--surface-window);
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
    color: var(--fg-default);
    overflow-x: auto;
    padding: 0.85rem 1rem;
  }

  :global(.chat-prose pre code) {
    background: transparent;
    border: 0;
    padding: 0;
    font-size: 0.82rem;
    line-height: 1.65;
  }

  :global(.chat-prose code:not(pre code)) {
    background: var(--surface-window);
    border: 1px solid var(--border-subtle);
    border-radius: 4px;
    color: var(--fg-default);
    font-size: 0.84em;
    padding: 0.08rem 0.28rem;
  }

  :global(.chat-prose table) {
    border-collapse: collapse;
    display: block;
    max-width: 100%;
    overflow-x: auto;
  }

  :global(.chat-prose th),
  :global(.chat-prose td) {
    border: 1px solid var(--border-subtle);
    padding: 0.45rem 0.6rem;
  }

  :global(.chat-prose th) {
    background: var(--surface-window);
    color: var(--fg-default);
    font-weight: 600;
  }

  /* Images in markdown prose: max size, border-radius, click hint */
  :global(.chat-prose img) {
    border-radius: 0.5rem;
    cursor: zoom-in;
    display: block;
    margin-bottom: 0.5rem;
    margin-top: 0.5rem;
    max-height: 480px;
    max-width: 100%;
    object-fit: contain;
  }

  /* ---- highlight.js token colors (dark theme matching project vars) ------- */

  :global(.hljs) {
    background: transparent !important;
    color: var(--fg-default);
  }

  :global(.hljs-comment),
  :global(.hljs-quote) {
    color: #6a9955;
    font-style: italic;
  }

  :global(.hljs-doctag),
  :global(.hljs-keyword),
  :global(.hljs-formula),
  :global(.hljs-selector-tag) {
    color: #569cd6;
  }

  :global(.hljs-deletion),
  :global(.hljs-name),
  :global(.hljs-section),
  :global(.hljs-selector-id),
  :global(.hljs-selector-class),
  :global(.hljs-tag) {
    color: #569cd6;
  }

  :global(.hljs-string),
  :global(.hljs-attr),
  :global(.hljs-template-tag),
  :global(.hljs-template-variable),
  :global(.hljs-type),
  :global(.hljs-addition) {
    color: #ce9178;
  }

  :global(.hljs-number),
  :global(.hljs-literal) {
    color: #b5cea8;
  }

  :global(.hljs-built_in),
  :global(.hljs-class .hljs-title),
  :global(.hljs-title.class_) {
    color: #4ec9b0;
  }

  :global(.hljs-title),
  :global(.hljs-title.function_) {
    color: #dcdcaa;
  }

  :global(.hljs-variable),
  :global(.hljs-params),
  :global(.hljs-property) {
    color: #9cdcfe;
  }

  :global(.hljs-meta),
  :global(.hljs-meta .hljs-keyword) {
    color: #569cd6;
  }

  :global(.hljs-symbol),
  :global(.hljs-bullet) {
    color: #d7ba7d;
  }

  :global(.hljs-link) {
    color: var(--accent);
    text-decoration: underline;
  }

  :global(.hljs-operator),
  :global(.hljs-punctuation) {
    color: var(--fg-default);
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
    transition: background 120ms ease, color 120ms ease;
  }

  .scroll-pill:hover {
    background: color-mix(in srgb, var(--fg-default) 8%, var(--surface-window));
    color: var(--fg-default);
  }

  /* ---- PTY fallback block (BUG C) ---------------------------------------- */
  .pty-block {
    background: color-mix(in srgb, var(--surface-window) 60%, transparent);
    border: 1px solid var(--border-subtle);
    border-radius: 0.55rem;
    margin-bottom: 0.5rem;
    overflow: hidden;
  }

  .pty-block-summary {
    align-items: center;
    color: var(--fg-muted);
    cursor: pointer;
    display: flex;
    font-size: 0.74rem;
    gap: 0.45rem;
    list-style: none;
    padding: 0.45rem 0.65rem;
    user-select: none;
  }

  .pty-block-summary::-webkit-details-marker {
    display: none;
  }

  .pty-block-summary:hover {
    background: color-mix(in srgb, var(--fg-default) 4%, transparent);
  }

  .pty-block-icon {
    color: var(--accent);
    flex-shrink: 0;
  }

  .pty-terminal-link {
    border: 1px solid var(--border-subtle);
    border-radius: 999px;
    cursor: pointer;
    font-size: 0.63rem;
    margin-left: auto;
    padding: 0.1rem 0.5rem;
    transition: background 100ms ease;
  }

  .pty-terminal-link:hover {
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    border-color: var(--accent-soft-border);
    color: var(--accent);
  }

  /* ---- Historical turns — previous session (BUG D) ----------------------- */
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

  /* ---- Stopped CTA (BUG E) ----------------------------------------------- */
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

  /* ---- Composer textarea max-height (LAYOUT: up to 6 lines) -------------- */
  .composer-textarea {
    max-height: 144px;
  }

  /* ---- Responsive -------------------------------------------------------- */
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

    .action-header {
      grid-template-columns: 1.15rem minmax(0, 1fr) auto;
    }

    .action-preview,
    .action-state {
      display: none;
    }
  }
</style>
