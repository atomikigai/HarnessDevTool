/**
 * Pure helpers shared by the Agents-panel UI components.
 *
 * F2 only exposes minimal session metadata (kind/status/cwd/pid/started_at) —
 * the design calls for a richer card (model variant, uptime, tokens, task
 * progress). We derive what we can from the data we have and use stable
 * placeholders for everything else. F3 will replace these with real values
 * once the orchestrator emits cost/token stats and sub-agent telemetry.
 */
import type { SessionMeta as GeneratedSessionMeta } from './api/types/SessionMeta';
import type { SessionKind, SessionMeta, SessionStatus } from '$lib/api/client';
import type { Task } from '$lib/api/models/task';

/** UI status — wider than backend SessionStatus to cover idle/empty cases. */
export type UiStatus = 'active' | 'idle' | 'stopped' | 'killed' | 'untitled';

export interface KindChip {
  /** Display label (e.g. "claude-code", "codex-cli"). */
  label: string;
  /** CSS color for the chip text/border. */
  color: string;
  /** Soft background color (rgba/hex with low opacity). */
  bg: string;
}

/**
 * For F2 we hard-map known kinds to the labels shown in the reference
 * design. Unknown kinds pass through verbatim with the neutral palette.
 */
export function kindChip(kind: SessionKind | string | undefined): KindChip {
  switch (kind) {
    case 'claude':
      return {
        label: 'claude-code',
        color: 'var(--accent)',
        bg: 'var(--accent-soft)'
      };
    case 'codex':
      return {
        label: 'codex-cli',
        color: '#0f52c8',
        bg: 'rgba(15, 82, 200, 0.10)'
      };
    default:
      return {
        label: kind ?? 'unknown',
        color: 'var(--fg-muted)',
        bg: 'var(--surface-titlebar)'
      };
  }
}

/** Map backend status → UI status. `running` becomes active; missing → untitled. */
export function uiStatus(s: SessionMeta | null | undefined): UiStatus {
  if (!s) return 'untitled';
  switch (s.status) {
    case 'running':
      return 'active';
    case 'killed':
      return 'killed';
    case 'exited':
      return 'stopped';
    default:
      return 'idle';
  }
}

/** Color for the status dot keyed by UI status. */
export function statusColor(s: UiStatus): string {
  switch (s) {
    case 'active':
      return 'var(--dot-success)';
    case 'idle':
      return 'var(--dot-warn)';
    case 'killed':
      return 'var(--dot-danger)';
    case 'stopped':
      return 'var(--fg-label)';
    case 'untitled':
    default:
      return 'var(--fg-muted)';
  }
}

/** Human label for the status (mirrors STATUS_CFG.label in the ref). */
export function statusLabel(s: UiStatus): string {
  switch (s) {
    case 'active':
      return 'Active';
    case 'idle':
      return 'Idle';
    case 'stopped':
      return 'Stopped';
    case 'killed':
      return 'Killed';
    case 'untitled':
      return 'Untitled';
  }
}

function displayRole(role: string | null | undefined): 'planner' | 'generator' | 'evaluator' | 'other' {
  if (role === 'planner' || role === 'generator' || role === 'evaluator') return role;
  return 'other';
}

export function groupSessionsByRole(
  sessions: GeneratedSessionMeta[]
): Array<{ role: string; items: GeneratedSessionMeta[] }>;
export function groupSessionsByRole<T extends object>(
  sessions: T[]
): Array<{ role: string; items: T[] }>;
export function groupSessionsByRole<T extends object>(
  sessions: T[]
): Array<{ role: string; items: T[] }> {
  // Only exact role template names are surfaced as groups. Missing roles,
  // agent-prefixed metadata, and unknown values are folded into "other".
  const grouped: Record<'planner' | 'generator' | 'evaluator' | 'other', T[]> = {
    planner: [],
    generator: [],
    evaluator: [],
    other: []
  };

  for (const session of sessions) {
    const role = 'role' in session ? (session.role as string | null | undefined) : null;
    grouped[displayRole(role)].push(session);
  }

  return (['planner', 'generator', 'evaluator', 'other'] as const)
    .map((role) => ({ role, items: grouped[role] }))
    .filter((group) => group.items.length > 0);
}

type TimeInput = string | number | bigint | undefined;

function timeMs(value: TimeInput): number {
  if (value == null) return Number.NaN;
  if (typeof value === 'bigint') return Number(value);
  if (typeof value === 'number') return value;
  return new Date(value).getTime();
}

/** Coarse "X min ago" / "just now" relative time. */
export function relTime(iso: TimeInput): string {
  const then = timeMs(iso);
  if (Number.isNaN(then)) return '';
  const s = Math.max(0, Math.round((Date.now() - then) / 1000));
  if (s < 30) return 'just now';
  if (s < 60) return `${s}s ago`;
  if (s < 3600) return `${Math.floor(s / 60)} min ago`;
  if (s < 86400) return `${Math.floor(s / 3600)}h ago`;
  return `${Math.floor(s / 86400)}d ago`;
}

/** Compact "uptime" duration since `iso` (e.g. "14m", "1h 8m"). */
export function uptime(iso: TimeInput): string {
  const then = timeMs(iso);
  if (Number.isNaN(then)) return '—';
  const s = Math.max(0, Math.round((Date.now() - then) / 1000));
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m`;
  const h = Math.floor(m / 60);
  const rm = m - h * 60;
  return rm > 0 ? `${h}h ${rm}m` : `${h}h`;
}

/** Compact thousands formatter for token counts (F3 will pipe real numbers). */
export function tokensLabel(n: number | null | undefined): string {
  if (n == null) return '0 tok';
  if (n < 1000) return `${n} tok`;
  return `${(n / 1000).toFixed(1)}k tok`;
}

export interface TaskProgress {
  done: number;
  total: number;
  pct: number;
}

/** Count `done` and `pending_verify` (verified) as completed. */
export function taskProgress(tasks: Task[] | undefined): TaskProgress {
  if (!tasks || tasks.length === 0) return { done: 0, total: 0, pct: 0 };
  const total = tasks.length;
  const done = tasks.filter((t) => t.status === 'done').length;
  const pct = total === 0 ? 0 : Math.round((done / total) * 100);
  return { done, total, pct };
}

/** True when the backend status maps to a destructive end-state. */
export function isTerminal(s: SessionStatus | undefined): boolean {
  return s === 'exited' || s === 'killed';
}
