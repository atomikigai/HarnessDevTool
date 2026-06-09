import { API_BASE } from './client';

export interface SSEHandle {
  close: () => void;
}

export interface SubscribeOptions {
  onError?: (err: Event) => void;
  onOpen?: (ev: Event) => void;
  onLagged?: (payload: LaggedEvent, raw: MessageEvent) => void;
  onResync?: (payload: LaggedEvent, raw: MessageEvent) => void;
  reconnect?: boolean;
  maxReconnectAttempts?: number;
  baseDelayMs?: number;
  maxDelayMs?: number;
  /**
   * Map from SSE `event:` name → handler. Data is parsed as JSON when possible.
   * Anonymous (default) messages still flow through `onMessage`.
   */
  events?: Record<string, (data: unknown, raw: MessageEvent) => void>;
}

export interface LaggedEvent {
  type?: 'lagged';
  stream?: string;
  skipped?: number;
  resync?: string;
  thread_id?: string;
  session_id?: string;
}

function buildUrl(path: string): string {
  if (path.startsWith('http')) return path;
  const base = API_BASE.endsWith('/') ? API_BASE.slice(0, -1) : API_BASE;
  if (base && path.startsWith(`${base}/`)) return path;
  if (base.startsWith('http')) {
    const url = new URL(base);
    if (url.pathname !== '/' && path.startsWith(`${url.pathname}/`)) {
      return `${url.origin}${path}`;
    }
  }
  return `${base}${path.startsWith('/') ? path : `/${path}`}`;
}

function tryParse(data: string): unknown {
  try {
    return JSON.parse(data);
  } catch {
    return data;
  }
}

/**
 * Subscribe to a Server-Sent Events stream.
 * `path` is appended to the API base. Messages are parsed as JSON when possible.
 */
export function subscribeSSE<T = unknown>(
  path: string,
  onMessage: (data: T, raw: MessageEvent) => void,
  opts: SubscribeOptions = {}
): SSEHandle {
  let es: EventSource | null = null;
  let closed = false;
  let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  let attempts = 0;
  const reconnect = opts.reconnect ?? false;
  const maxAttempts = opts.maxReconnectAttempts ?? Infinity;
  const baseDelay = opts.baseDelayMs ?? 500;
  const maxDelay = opts.maxDelayMs ?? 10_000;
  const url = buildUrl(path);

  function clearReconnectTimer(): void {
    if (reconnectTimer) {
      clearTimeout(reconnectTimer);
      reconnectTimer = null;
    }
  }

  function open(): void {
    if (closed) return;
    es = new EventSource(url);

    es.onmessage = (ev) => {
      onMessage(tryParse(ev.data) as T, ev);
    };

    es.addEventListener('lagged', (ev) => {
      const me = ev as MessageEvent;
      const payload = tryParse(me.data) as LaggedEvent;
      opts.onLagged?.(payload, me);
      if (payload?.resync === 'reconnect') {
        opts.onResync?.(payload, me);
        if (reconnect) scheduleReconnect();
      }
    });

    if (opts.events) {
      for (const [name, handler] of Object.entries(opts.events)) {
        if (name === 'lagged') continue;
        es.addEventListener(name, (ev) => {
          const me = ev as MessageEvent;
          handler(tryParse(me.data), me);
        });
      }
    }

    es.onopen = (ev) => {
      attempts = 0;
      opts.onOpen?.(ev);
    };

    es.onerror = (err) => {
      opts.onError?.(err);
      if (reconnect) scheduleReconnect();
    };
  }

  function scheduleReconnect(): void {
    if (closed || reconnectTimer || attempts >= maxAttempts) return;
    es?.close();
    es = null;
    const delay = Math.min(maxDelay, baseDelay * 2 ** attempts);
    attempts += 1;
    reconnectTimer = setTimeout(() => {
      reconnectTimer = null;
      open();
    }, delay);
  }

  open();

  return {
    close: () => {
      closed = true;
      clearReconnectTimer();
      es?.close();
      es = null;
    }
  };
}
