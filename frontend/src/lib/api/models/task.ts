/**
 * Public task model surface for the frontend.
 *
 * Canonical task documents come from Rust via `ts-rs` under `../types`.
 * Keep only frontend/API request helpers here.
 */

export type { AcceptanceBlock } from '../types/AcceptanceBlock';
export type { AcceptanceCheck } from '../types/AcceptanceCheck';
export type { Agent } from '../types/Agent';
export type { AgentKind } from '../types/AgentKind';
export type { Artifact } from '../types/Artifact';
export type { ArtifactKind } from '../types/ArtifactKind';
export type { Artifacts } from '../types/Artifacts';
export type { HistoryBlock } from '../types/HistoryBlock';
export type { HistoryEvent } from '../types/HistoryEvent';
export type { Lease } from '../types/Lease';
export type { Notes } from '../types/Notes';
export type { ReconcileEntity } from '../types/ReconcileEntity';
export type { ReconcileIssue } from '../types/ReconcileIssue';
export type { ReconcileReport } from '../types/ReconcileReport';
export type { ReconcileSeverity } from '../types/ReconcileSeverity';
export type { SchedulerDecisionKind } from '../types/SchedulerDecisionKind';
export type { SchedulerExplanation } from '../types/SchedulerExplanation';
export type { SpecRef } from '../types/SpecRef';
export type { Task } from '../types/Task';
export type { TaskBrief } from '../types/TaskBrief';
export type { TaskStatus } from '../types/TaskStatus';
export type { TimelineEntity } from '../types/TimelineEntity';
export type { TimelineItem } from '../types/TimelineItem';
export type { TimelineReport } from '../types/TimelineReport';

import type { AcceptanceCheck } from '../types/AcceptanceCheck';
import type { Notes } from '../types/Notes';
import type { SpecRef } from '../types/SpecRef';
import type { TaskBrief } from '../types/TaskBrief';
import type { TaskStatus } from '../types/TaskStatus';

export interface CreateTaskRequest {
  title: string;
  status?: 'queued' | 'proposed';
  parent?: string;
  depends_on?: string[];
  brief?: TaskBrief;
  acceptance?: { checks: { id?: string; text: string }[] };
  labels?: string[];
  spec_refs?: SpecRef[];
  created_by: string;
}

export interface PatchTaskRequest {
  title?: string;
  status?: TaskStatus;
  assignee?: string | null;
  labels?: string[];
  spec_refs?: SpecRef[];
  blocked_by?: string[];
  acceptance_checks?: AcceptanceCheck[];
  /**
   * Back-compat frontend alias. `api.tasks.patch` translates this to
   * `acceptance_checks` before sending to the Rust `TaskPatch`.
   */
  acceptance?: { checks: AcceptanceCheck[] };
  blocked_reason?: string;
  paused_reason?: string;
  rejected_reason?: string;
  last_failure?: string;
  needs_human?: boolean;
  notes?: Partial<Notes>;
  why_paused?: string;
  why_abandoned?: string;
  feedback?: string;
  by: 'human' | string;
}

export interface DeleteTaskRequest {
  why: string;
  by: 'human' | string;
}

export interface CreateAgentRequest {
  kind: string;
  label: string;
}

/** Convenience: status -> tone (color category) for badges. */
export function statusTone(s: TaskStatus): 'neutral' | 'accent' | 'warn' | 'success' | 'danger' {
  switch (s) {
    case 'proposed':
      return 'neutral';
    case 'queued':
      return 'neutral';
    case 'in_progress':
      return 'accent';
    case 'pending_verify':
      return 'warn';
    case 'done':
      return 'success';
    case 'paused':
      return 'neutral';
    case 'blocked':
      return 'warn';
    case 'abandoned':
      return 'danger';
  }
}
