/**
 * Health store — polls /api/health and exposes connection state for the
 * top-bar indicator and any other consumer that needs it. The dashboard
 * page used to own the polling; centralising avoids two refresh loops
 * and lets the rest of the shell read the live status without duplicating
 * fetch logic.
 */

import { api, type HealthResponse } from '$lib/api/client';

export type ConnState = 'idle' | 'connecting' | 'ok' | 'down';

class HealthStore {
  state = $state<ConnState>('idle');
  data = $state<HealthResponse | null>(null);
  protocolVersion = $state<string | null>(null);
  error = $state<string | null>(null);
  lastUpdated = $state<Date | null>(null);

  private controller: AbortController | null = null;
  private requestSeq = 0;

  async refresh(): Promise<void> {
    this.controller?.abort();
    this.controller = new AbortController();
    const seq = ++this.requestSeq;
    if (this.state !== 'ok') this.state = 'connecting';
    try {
      const res = await api.health(this.controller.signal);
      if (seq !== this.requestSeq) return;
      this.data = res.data;
      this.protocolVersion = res.protocolVersion;
      this.error = null;
      this.lastUpdated = new Date();
      this.state = 'ok';
    } catch (e) {
      if ((e as { name?: string }).name === 'AbortError') return;
      if (seq !== this.requestSeq) return;
      this.error = e instanceof Error ? e.message : String(e);
      if (!this.data) {
        this.protocolVersion = null;
      }
      this.state = 'down';
    }
  }
}

export const health = new HealthStore();
