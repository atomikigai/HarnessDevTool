/**
 * Task and Agent types — mirrors the backend contracts for F2.
 * Hand-rolled until `ts-rs` exports land; once they do, this file can be
 * replaced with the generated bindings.
 */

import type { Artifact } from '../types/Artifact';
import type { ArtifactKind } from '../types/ArtifactKind';

export type { Artifact, ArtifactKind };

export type TaskStatus =
  | 'proposed'
  | 'queued'
  | 'in_progress'
  | 'pending_verify'
  | 'done'
  | 'paused'
  | 'blocked'
  | 'abandoned';

export interface AcceptanceCheck {
  id: string;
  text: string;
  verified: boolean;
  verified_by?: string;
}

export interface Lease {
  holder: string;
  until: string;
}

export interface TaskHistoryEvent {
  at: string;
  by: string;
  from: string;
  to: string;
}

export interface TaskArtifacts {
  files: string[];
  turns: string[];
  diff?: string;
  metadata?: Artifact[];
}

export interface TaskNotes {
  why_paused?: string;
  why_abandoned?: string;
  blocked_reason?: string;
  paused_reason?: string;
  rejected_reason?: string;
  last_failure?: string;
  needs_human?: boolean;
  feedback?: unknown[];
}

export interface TaskBrief {
  objective: string;
  context: string;
  tasks: string[];
  rules: string[];
  expected_result: string;
}

export interface SpecRef {
  section: string;
  version: number;
}

export interface Task {
  schema_version: number;
  id: string;
  title: string;
  status: TaskStatus;
  created_at: string;
  created_by: string;
  updated_at: string;
  updated_by: string;
  parent?: string;
  children: string[];
  blocked_by: string[];
  unblocks: string[];
  assignee?: string;
  claim_lease?: Lease;
  previous_assignees: string[];
  labels: string[];
  spec_refs: SpecRef[];
  brief?: TaskBrief;
  acceptance: { checks: AcceptanceCheck[] };
  artifacts: TaskArtifacts;
  notes: TaskNotes;
  history: { events: TaskHistoryEvent[] };
}

export interface CreateTaskRequest {
  title: string;
  status?: 'queued' | 'proposed';
  parent?: string;
  depends_on?: string[];
  brief?: TaskBrief;
  acceptance?: { checks: { text: string }[] };
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
  acceptance?: { checks: AcceptanceCheck[] };
  blocked_reason?: string;
  paused_reason?: string;
  rejected_reason?: string;
  last_failure?: string;
  needs_human?: boolean;
  notes?: TaskNotes;
  by: 'human' | string;
}

export interface DeleteTaskRequest {
  why: string;
  by: 'human' | string;
}

export type AgentKind = 'claude' | 'codex' | string;

export interface Agent {
  id: string;
  kind: AgentKind;
  label: string;
  created_at: string;
}

export interface CreateAgentRequest {
  kind: string;
  label: string;
}

/** Convenience: status → tone (color category) for badges. */
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
