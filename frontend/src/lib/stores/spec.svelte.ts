import { api, SpecEtagMismatchError } from '$lib/api/client';
import { subscribeSSE, type SSEHandle } from '$lib/api/sse';

// TODO: replace with ts-rs type after backend slice lands
type SpecChangedEvent = {
  thread_id: string;
  etag: string;
  bytes: number;
  at: string;
  version?: number;
  section?: string | null;
  section_version?: number | null;
};
type ArtifactAddedEvent = { thread_id: string; path: string; kind: string; at: string };

export interface SpecArtifact {
  path: string;
  kind: string;
  at: string;
}

class SpecState {
  threadId = $state<string | null>(null);
  content = $state('');
  etag = $state('');
  version = $state(0);
  artifacts = $state<SpecArtifact[]>([]);
  loading = $state(false);
  error = $state<string | null>(null);
  staleEtag = $state(false);
  updatedAt = $state<string | null>(null);

  #sse: SSEHandle | null = null;
  #controller: AbortController | null = null;

  async refresh(): Promise<void> {
    if (!this.threadId) return;
    this.#controller?.abort();
    this.#controller = new AbortController();
    this.loading = true;
    try {
      const res = await api.spec.get(this.threadId);
      this.content = res.data.content;
      this.etag = res.data.etag;
      this.version = res.data.version ?? 0;
      this.error = null;
      this.staleEtag = false;
      this.updatedAt = new Date().toISOString();
    } catch (err) {
      if ((err as { name?: string }).name === 'AbortError') return;
      this.error = err instanceof Error ? err.message : String(err);
    } finally {
      this.loading = false;
    }
  }

  start(tid: string): void {
    if (this.threadId === tid && this.#sse) {
      void this.refresh();
      return;
    }
    this.stop();
    this.threadId = tid;
    void this.refresh();
    this.#sse = subscribeSSE(
      `/events?thread=${encodeURIComponent(tid)}`,
      () => {
        /* default channel ignored */
      },
      {
        reconnect: true,
        onResync: () => {
          void this.refresh();
        },
        events: {
          'spec.changed': (data) => {
            const ev = data as SpecChangedEvent;
            if (ev.thread_id === this.threadId) void this.refresh();
          },
          'artifact.added': (data) => {
            const ev = data as ArtifactAddedEvent;
            if (ev.thread_id !== this.threadId) return;
            this.artifacts = [{ path: ev.path, kind: ev.kind, at: ev.at }, ...this.artifacts];
          }
        },
        onError: () => {}
      }
    );
  }

  async save(content: string): Promise<void> {
    if (!this.threadId) return;
    this.loading = true;
    try {
      const res = await api.spec.put(this.threadId, { content, etag: this.etag || undefined });
      this.content = content;
      this.etag = res.data.etag;
      this.version = res.data.version ?? this.version;
      this.error = null;
      this.staleEtag = false;
      this.updatedAt = new Date().toISOString();
    } catch (err) {
      if (err instanceof SpecEtagMismatchError) {
        this.staleEtag = true;
        this.error = null;
        return;
      }
      this.error = err instanceof Error ? err.message : String(err);
    } finally {
      this.loading = false;
    }
  }

  stop(): void {
    this.#sse?.close();
    this.#sse = null;
    this.#controller?.abort();
    this.#controller = null;
    this.threadId = null;
    this.content = '';
    this.etag = '';
    this.version = 0;
    this.artifacts = [];
    this.loading = false;
    this.error = null;
    this.staleEtag = false;
    this.updatedAt = null;
  }
}

export const specState = new SpecState();
