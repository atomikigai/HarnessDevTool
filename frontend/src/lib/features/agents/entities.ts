import type {
  AutonomyProfile,
  CapabilityProfile,
  CreateSessionRequest,
  CreateSessionResponse,
  CurrentRepoReport,
  ReadinessReport,
  SessionKind,
  ZeusRoleSelection
} from '$lib/api/client';

export type {
  AutonomyProfile,
  CapabilityProfile,
  CreateSessionRequest,
  CreateSessionResponse,
  CurrentRepoReport,
  ReadinessReport,
  SessionKind,
  ZeusRoleSelection
};

export type RepoMode = 'resume' | 'context' | 'none';

export interface NewSessionFormData {
  kind: SessionKind;
  autonomy: AutonomyProfile;
  cwd?: string;
  repoMode: RepoMode;
  capabilityProfile: Extract<CapabilityProfile, 'auto' | 'none'>;
  zeusRoles: ZeusRoleSelection[];
  cols: number;
  rows: number;
}

export interface CreateAgentSessionInput {
  threadId?: string | null;
  form: NewSessionFormData;
  repoReport?: CurrentRepoReport | null;
}

export interface CreateAgentSessionResult {
  threadId: string;
  sessionId: string;
}
