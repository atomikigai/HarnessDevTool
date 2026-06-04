/**
 * Per-thread budget store. Keyed by thread id so multiple meters can be
 * mounted in parallel without trampling each other. Backed by the
 * `GET /api/threads/:id/budget` endpoint and refreshed on demand —
 * the parent component drives `loadBudget` on mount and on
 * `budget.warning` SSE events filtered by `thread_id`.
 */

import { api, type BudgetView } from '$lib/api/client';

interface BudgetEntry {
  view: BudgetView | null;
  loading: boolean;
  saving: boolean;
  error: string | null;
}

function emptyEntry(): BudgetEntry {
  return { view: null, loading: false, saving: false, error: null };
}

class BudgetStore {
  /**
   * Reactive map of threadId → entry. Using a plain object so reads from
   * components are O(1); mutations replace the wrapping object so
   * `$state` proxies fan out updates.
   */
  byThread = $state<Record<string, BudgetEntry>>({});

  get(threadId: string): BudgetEntry {
    return this.byThread[threadId] ?? emptyEntry();
  }

  #patch(threadId: string, patch: Partial<BudgetEntry>) {
    const prev = this.byThread[threadId] ?? emptyEntry();
    this.byThread = { ...this.byThread, [threadId]: { ...prev, ...patch } };
  }

  async loadBudget(threadId: string): Promise<void> {
    this.#patch(threadId, { loading: true, error: null });
    try {
      const res = await api.getBudget(threadId);
      this.#patch(threadId, { view: res.data, loading: false, error: null });
    } catch (err) {
      this.#patch(threadId, {
        loading: false,
        error: err instanceof Error ? err.message : String(err)
      });
    }
  }

  async setLimit(
    threadId: string,
    limitUsd: number,
    maxConcurrentWorkers?: number | null
  ): Promise<void> {
    if (!(Number.isFinite(limitUsd) && limitUsd > 0)) {
      this.#patch(threadId, { error: 'Limit must be a positive number' });
      return;
    }
    if (
      maxConcurrentWorkers !== undefined &&
      maxConcurrentWorkers !== null &&
      !(Number.isInteger(maxConcurrentWorkers) && maxConcurrentWorkers >= 1)
    ) {
      this.#patch(threadId, { error: 'Max workers must be a positive integer' });
      return;
    }
    this.#patch(threadId, { saving: true, error: null });
    try {
      const res = await api.setBudget(threadId, limitUsd, maxConcurrentWorkers);
      this.#patch(threadId, { view: res.data, saving: false, error: null });
    } catch (err) {
      this.#patch(threadId, {
        saving: false,
        error: err instanceof Error ? err.message : String(err)
      });
    }
  }

  /** Replace the cached view for a thread (used by SSE handlers). */
  applyView(view: BudgetView): void {
    this.#patch(view.thread_id, { view, error: null });
  }
}

export const budgetStore = new BudgetStore();
