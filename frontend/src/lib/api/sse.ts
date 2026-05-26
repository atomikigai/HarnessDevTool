import { API_BASE } from './client';

export interface SSEHandle {
  close: () => void;
}

export interface SubscribeOptions {
  onError?: (err: Event) => void;
  onOpen?: (ev: Event) => void;
  /**
   * Map from SSE `event:` name → handler. Data is parsed as JSON when possible.
   * Anonymous (default) messages still flow through `onMessage`.
   */
  events?: Record<string, (data: unknown, raw: MessageEvent) => void>;
}

function buildUrl(path: string): string {
  return path.startsWith('http')
    ? path
    : `${API_BASE.endsWith('/') ? API_BASE.slice(0, -1) : API_BASE}${path.startsWith('/') ? path : `/${path}`}`;
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
  const es = new EventSource(buildUrl(path));

  es.onmessage = (ev) => {
    onMessage(tryParse(ev.data) as T, ev);
  };

  if (opts.events) {
    for (const [name, handler] of Object.entries(opts.events)) {
      es.addEventListener(name, (ev) => {
        const me = ev as MessageEvent;
        handler(tryParse(me.data), me);
      });
    }
  }

  if (opts.onOpen) es.onopen = opts.onOpen;
  if (opts.onError) es.onerror = opts.onError;

  return {
    close: () => es.close()
  };
}
