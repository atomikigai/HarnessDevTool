import { API_BASE } from './client';

export interface SSEHandle {
  close: () => void;
}

export interface SubscribeOptions {
  onError?: (err: Event) => void;
  onOpen?: (ev: Event) => void;
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
  const url = path.startsWith('http')
    ? path
    : `${API_BASE.endsWith('/') ? API_BASE.slice(0, -1) : API_BASE}${path.startsWith('/') ? path : `/${path}`}`;

  const es = new EventSource(url);

  es.onmessage = (ev) => {
    let parsed: unknown = ev.data;
    try {
      parsed = JSON.parse(ev.data);
    } catch {
      // keep raw string
    }
    onMessage(parsed as T, ev);
  };

  if (opts.onOpen) es.onopen = opts.onOpen;
  if (opts.onError) es.onerror = opts.onError;

  return {
    close: () => es.close()
  };
}
