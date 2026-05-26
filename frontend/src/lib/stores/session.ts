/**
 * Session store — placeholder for F0.
 * Will hold user/session info once auth lands.
 */

import { writable, type Writable } from 'svelte/store';

export interface SessionState {
  userId: string | null;
  protocolVersion: string | null;
}

const initial: SessionState = {
  userId: null,
  protocolVersion: null
};

export const session: Writable<SessionState> = writable(initial);
