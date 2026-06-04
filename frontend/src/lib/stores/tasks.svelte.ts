/**
 * Tasks store — runes-backed per-thread cache + live SSE wiring.
 * Owns the canonical list for the currently-viewed thread; the page is
 * expected to call `start(threadId)` on mount and `stop()` on destroy.
 */
import { api, type ListTasksFilters } from '$lib/api/client';
import type { Task } from '$lib/api/models/task';
import { subscribeSSE, type SSEHandle } from '$lib/api/sse';

class TasksState {
  threadId = $state<string | null>(null);
  items = $state<Task[]>([]);
  loading = $state<boolean>(false);
  error = $state<string | null>(null);
  filters = $state<ListTasksFilters>({});

  #sse: SSEHandle | null = null;
  #controller: AbortController | null = null;

  byId(id: string): Task | undefined {
    return this.items.find((t) => t.id === id);
  }

  #matchesFilters(task: Task): boolean {
    if (this.filters.status && task.status !== this.filters.status) return false;
    if (this.filters.assignee && task.assignee !== this.filters.assignee) return false;
    if (this.filters.label && !task.labels.includes(this.filters.label)) return false;
    return true;
  }

  async refresh(): Promise<void> {
    if (!this.threadId) return;
    this.#controller?.abort();
    this.#controller = new AbortController();
    this.loading = true;
    try {
      const res = await api.tasks.list(this.threadId, this.filters, this.#controller.signal);
      this.items = res.data ?? [];
      this.error = null;
    } catch (err) {
      if ((err as { name?: string }).name === 'AbortError') return;
      this.error = err instanceof Error ? err.message : String(err);
    } finally {
      this.loading = false;
    }
  }

  async refreshOne(taskId: string): Promise<void> {
    if (!this.threadId) return;
    try {
      const res = await api.tasks.get(this.threadId, taskId);
      const idx = this.items.findIndex((t) => t.id === taskId);
      if (!this.#matchesFilters(res.data)) {
        if (idx >= 0) {
          const next = [...this.items];
          next.splice(idx, 1);
          this.items = next;
        }
        return;
      }
      if (idx >= 0) {
        const next = [...this.items];
        next[idx] = res.data;
        this.items = next;
      } else {
        this.items = [...this.items, res.data];
      }
    } catch (err) {
      // Soft-fail; SSE may have arrived before the task exists yet.
      console.warn('[tasks] refreshOne failed', err);
    }
  }

  setFilters(f: ListTasksFilters) {
    this.filters = f;
    void this.refresh();
  }

  start(threadId: string): void {
    if (this.threadId === threadId && this.#sse) return;
    this.stop();
    this.threadId = threadId;
    void this.refresh();
    this.#sse = subscribeSSE(
      `/events?thread=${encodeURIComponent(threadId)}`,
      () => {
        /* default channel ignored */
      },
      {
        events: {
          'task.created': (data) => {
            const tid = (data as { task_id?: string })?.task_id;
            if (tid) void this.refreshOne(tid);
          },
          'task.changed': (data) => {
            const tid = (data as { task_id?: string })?.task_id;
            if (tid) void this.refreshOne(tid);
          },
          'task.updated': (data) => {
            const tid = (data as { task_id?: string })?.task_id;
            if (tid) void this.refreshOne(tid);
          },
          'task.ready': (data) => {
            const tid = (data as { task_id?: string })?.task_id;
            if (tid) void this.refreshOne(tid);
          },
          'task.lease-expired': (data) => {
            const tid = (data as { task_id?: string })?.task_id;
            if (tid) void this.refreshOne(tid);
          },
          'artifact.added': (data) => {
            const tid = (data as { task_id?: string })?.task_id;
            if (tid) void this.refreshOne(tid);
          }
        },
        onError: () => {
          // EventSource auto-reconnects; surface only persistent errors.
        }
      }
    );
  }

  stop(): void {
    this.#sse?.close();
    this.#sse = null;
    this.#controller?.abort();
    this.#controller = null;
    this.threadId = null;
    this.items = [];
    this.error = null;
    this.loading = false;
  }
}

export const tasksState = new TasksState();
