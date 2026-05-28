import { api, ApiRequestError } from '$lib/api/client';
import type { ApprovalSummary } from '$lib/api/types/ApprovalSummary';
import type { Decision } from '$lib/api/types/Decision';
import type { RememberScope } from '$lib/api/types/RememberScope';
import { subscribeSSE, type SSEHandle } from '$lib/api/sse';

class ApprovalsState {
  pending = $state<ApprovalSummary[]>([]);
  loading = $state(false);
  error = $state<string | null>(null);

  #handle: SSEHandle | null = null;

  async start(): Promise<void> {
    if (this.#handle) return;
    this.loading = true;
    try {
      const res = await api.approvals.list();
      this.pending = res.data ?? [];
      this.error = null;
    } catch (err) {
      this.error = err instanceof Error ? err.message : 'Failed to load approvals';
    } finally {
      this.loading = false;
    }

    this.#handle = subscribeSSE(
      '/events',
      () => {
        /* default channel ignored */
      },
      {
        events: {
          'approval.requested': (data) => {
            const summary = (data as { summary?: ApprovalSummary })?.summary;
            if (!summary) return;
            if (!this.pending.find((p) => p.id === summary.id)) {
              this.pending = [...this.pending, summary];
            }
          },
          'approval.resolved': (data) => {
            const id = (data as { id?: string })?.id;
            if (id) this.pending = this.pending.filter((p) => p.id !== id);
          }
        }
      }
    );
  }

  stop(): void {
    this.#handle?.close();
    this.#handle = null;
  }

  async decide(id: string, decision: Decision, remember_scope?: RememberScope): Promise<void> {
    const prev = this.pending;
    this.pending = this.pending.filter((p) => p.id !== id);
    try {
      await api.approvals.decide(id, decision, remember_scope);
    } catch (err) {
      if (err instanceof ApiRequestError && err.status === 404) {
        return;
      }
      this.pending = prev;
      throw err;
    }
  }
}

export const approvalsState = new ApprovalsState();
