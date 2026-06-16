import { api } from '$lib/api/client';
import type { PrevTurn } from './types';

export async function fetchPreviousTurns(
  sessionId: string,
  signal: AbortSignal
): Promise<PrevTurn[]> {
  const turns: PrevTurn[] = [];
  const res = await api.sessions.transcriptQuery(sessionId, { kind: 'message' }, signal);

  for (const ev of res.data.events) {
    if (ev.content == null) continue;
    const last = turns[turns.length - 1];
    if (ev.role === 'user') {
      turns.push({ id: `prev-${ev.seq}`, role: 'user', content: ev.content });
    } else if (ev.role === 'assistant') {
      if (last?.role === 'assistant') {
        last.content += ev.content;
      } else {
        turns.push({ id: `prev-${ev.seq}`, role: 'assistant', content: ev.content });
      }
    }
  }

  return turns;
}
