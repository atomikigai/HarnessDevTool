/**
 * Lightweight fetch wrapper for the harness backend.
 * Reads the API base URL from the `PUBLIC_API_BASE` env var (defaults to `/api`,
 * which is proxied to http://localhost:7777 in dev).
 * Exposes the `X-Protocol-Version` header to callers.
 */

export const API_BASE: string = (import.meta.env.PUBLIC_API_BASE as string | undefined) ?? '/api';

export const PROTOCOL_VERSION_HEADER = 'X-Protocol-Version';

export class ApiError extends Error {
  status: number;
  body: unknown;
  constructor(status: number, message: string, body?: unknown) {
    super(message);
    this.name = 'ApiError';
    this.status = status;
    this.body = body;
  }
}

export interface ApiResponse<T> {
  data: T;
  protocolVersion: string | null;
  status: number;
}

export interface RequestOptions {
  method?: string;
  body?: unknown;
  headers?: Record<string, string>;
  signal?: AbortSignal;
}

function joinUrl(base: string, path: string): string {
  if (path.startsWith('http://') || path.startsWith('https://')) return path;
  const b = base.endsWith('/') ? base.slice(0, -1) : base;
  const p = path.startsWith('/') ? path : `/${path}`;
  return `${b}${p}`;
}

export async function apiRequest<T>(
  path: string,
  opts: RequestOptions = {}
): Promise<ApiResponse<T>> {
  const url = joinUrl(API_BASE, path);
  const headers: Record<string, string> = {
    Accept: 'application/json',
    ...(opts.headers ?? {})
  };
  let body: BodyInit | undefined;
  if (opts.body !== undefined) {
    headers['Content-Type'] = headers['Content-Type'] ?? 'application/json';
    body = typeof opts.body === 'string' ? opts.body : JSON.stringify(opts.body);
  }

  const res = await fetch(url, {
    method: opts.method ?? 'GET',
    headers,
    body,
    signal: opts.signal
  });

  const protocolVersion = res.headers.get(PROTOCOL_VERSION_HEADER);

  let parsed: unknown = null;
  const text = await res.text();
  if (text.length > 0) {
    try {
      parsed = JSON.parse(text);
    } catch {
      parsed = text;
    }
  }

  if (!res.ok) {
    throw new ApiError(res.status, `Request failed: ${res.status} ${res.statusText}`, parsed);
  }

  return {
    data: parsed as T,
    protocolVersion,
    status: res.status
  };
}

// Typed helpers for the F0 endpoints.

export interface HealthResponse {
  version: string;
  uptime_s: number;
}

export const api = {
  health: (signal?: AbortSignal) => apiRequest<HealthResponse>('/health', { signal }),
  threads: {
    list: (signal?: AbortSignal) => apiRequest<unknown[]>('/threads', { signal }),
    create: (signal?: AbortSignal) =>
      apiRequest<{ id: string }>('/threads', { method: 'POST', signal })
  }
};
