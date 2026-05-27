/**
 * Global pause-all store.
 * Mirrors the backend's POST /api/pause-all and POST /api/resume-all
 * endpoints, with GET /api/pause-all used for initial hydration.
 *
 * If the backend endpoints are unavailable (404/500), `supported` is set
 * to `false` so the UI can hide the control rather than expose a broken
 * one.
 */

import { api } from '$lib/api/client';

class PauseAllStore {
  paused = $state(false);
  supported = $state(false);
  loading = $state(false);
  error = $state<string | null>(null);

  async refresh(): Promise<void> {
    try {
      const res = await api.pauseAll.get();
      this.paused = !!res.data?.paused;
      this.supported = true;
      this.error = null;
    } catch (e) {
      this.supported = false;
      this.error = e instanceof Error ? e.message : String(e);
    }
  }

  async toggle(): Promise<void> {
    if (!this.supported || this.loading) return;
    this.loading = true;
    try {
      const res = this.paused ? await api.pauseAll.resume() : await api.pauseAll.pause();
      this.paused = !!res.data?.paused;
      this.error = null;
    } catch (e) {
      this.error = e instanceof Error ? e.message : String(e);
      // On failure, re-sync from the server so the UI never lies.
      await this.refresh();
    } finally {
      this.loading = false;
    }
  }
}

export const pauseAll = new PauseAllStore();
