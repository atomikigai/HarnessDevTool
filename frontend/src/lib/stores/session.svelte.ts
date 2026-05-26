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

  async refresh(signal?: AbortSignal): Promise<void> {
    try {
      const res = await api.threads.list(signal);
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
      this.lastError = err instanceof Error ? err.message : String(err);
    }
  }
}

export const sessionsState = new SessionsState();
