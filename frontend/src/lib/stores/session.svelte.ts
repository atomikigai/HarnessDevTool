/**
 * Session store — holds basic user/protocol info and a runes-backed list of
 * active sessions across all threads. Refreshed by polling `/api/threads`.
 */

import { writable, type Writable } from 'svelte/store';
import { api, type SessionMeta, type ThreadSummary } from '$lib/api/client';

export interface SessionState {
  userId: string | null;
  protocolVersion: string | null;
}

const initial: SessionState = {
  userId: null,
  protocolVersion: null
};

export const session: Writable<SessionState> = writable(initial);

/**
 * Rune-backed reactive state for active sessions and threads.
 * Access via `sessionsState.active`, `sessionsState.threads`, etc.
 */
class SessionsState {
  active = $state<SessionMeta[]>([]);
  threads = $state<ThreadSummary[]>([]);
  loaded = $state<boolean>(false);
  lastError = $state<string | null>(null);

  #pollMs = 5_000;
  #startRefs = 0;
  #timer: ReturnType<typeof setInterval> | null = null;
  #controller: AbortController | null = null;
  #requestSeq = 0;

  start(): void {
    this.#startRefs += 1;
    if (this.#timer) return;
    void this.refresh();
    this.#timer = setInterval(() => {
      void this.refresh();
    }, this.#pollMs);
  }

  stop(): void {
    this.#startRefs = Math.max(0, this.#startRefs - 1);
    if (this.#startRefs > 0) return;
    if (this.#timer) {
      clearInterval(this.#timer);
      this.#timer = null;
    }
    this.#controller?.abort();
    this.#controller = null;
  }

  async refresh(signal?: AbortSignal): Promise<void> {
    const seq = ++this.#requestSeq;
    let controller: AbortController | null = null;
    if (!signal) {
      this.#controller?.abort();
      controller = new AbortController();
      this.#controller = controller;
      signal = controller.signal;
    }
    try {
      const res = await api.threads.list(signal);
      if (seq !== this.#requestSeq) return;
      const threads = res.data ?? [];
      this.threads = threads;
      const active: SessionMeta[] = [];
      for (const t of threads) {
        if (Array.isArray(t.sessions)) {
          for (const s of t.sessions) {
            if (s.status === 'running') active.push(s);
          }
        }
      }
      this.active = active;
      this.loaded = true;
      this.lastError = null;
    } catch (err) {
      if ((err as { name?: string }).name === 'AbortError') return;
      if (seq !== this.#requestSeq) return;
      this.lastError = err instanceof Error ? err.message : String(err);
    } finally {
      if (controller && this.#controller === controller) {
        this.#controller = null;
      }
    }
  }
}

export const sessionsState = new SessionsState();
