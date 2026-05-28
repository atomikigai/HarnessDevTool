/**
 * Lightweight fetch wrapper for the harness backend.
 * Reads the API base URL from the `PUBLIC_API_BASE` env var (defaults to `/api`,
 * which is proxied to http://localhost:7777 in dev).
 * Exposes the `X-Protocol-Version` header to callers.
 */

export const API_BASE: string = (import.meta.env.PUBLIC_API_BASE as string | undefined) ?? '/api';

export const PROTOCOL_VERSION_HEADER = 'X-Protocol-Version';

export class ApiError extends Error {
  status: number;
  body: unknown;
  constructor(status: number, message: string, body?: unknown) {
    super(message);
    this.name = 'ApiError';
    this.status = status;
    this.body = body;
  }
}

export { ApiError as ApiRequestError };

export class SpecEtagMismatchError extends ApiError {
  constructor(body?: unknown) {
    super(409, 'Spec etag mismatch', body);
    this.name = 'SpecEtagMismatchError';
  }
}

export interface ApiResponse<T> {
  data: T;
  protocolVersion: string | null;
  status: number;
}

export interface RequestOptions {
  method?: string;
  body?: unknown;
  headers?: Record<string, string>;
  signal?: AbortSignal;
}

function joinUrl(base: string, path: string): string {
  if (path.startsWith('http://') || path.startsWith('https://')) return path;
  const b = base.endsWith('/') ? base.slice(0, -1) : base;
  const p = path.startsWith('/') ? path : `/${path}`;
  return `${b}${p}`;
}

export async function apiRequest<T>(
  path: string,
  opts: RequestOptions = {}
): Promise<ApiResponse<T>> {
  const url = joinUrl(API_BASE, path);
  const headers: Record<string, string> = {
    Accept: 'application/json',
    ...(opts.headers ?? {})
  };
  let body: BodyInit | undefined;
  if (opts.body !== undefined) {
    headers['Content-Type'] = headers['Content-Type'] ?? 'application/json';
    body = typeof opts.body === 'string' ? opts.body : JSON.stringify(opts.body);
  }

  const res = await fetch(url, {
    method: opts.method ?? 'GET',
    headers,
    body,
    signal: opts.signal
  });

  const protocolVersion = res.headers.get(PROTOCOL_VERSION_HEADER);

  let parsed: unknown = null;
  const text = await res.text();
  if (text.length > 0) {
    try {
      parsed = JSON.parse(text);
    } catch {
      parsed = text;
    }
  }

  if (!res.ok) {
    throw new ApiError(res.status, `Request failed: ${res.status} ${res.statusText}`, parsed);
  }

  return {
    data: parsed as T,
    protocolVersion,
    status: res.status
  };
}

// Typed helpers for the F0/F1 endpoints.

export interface HealthResponse {
  version: string;
  uptime_s: number;
}

export interface ThreadSummary {
  id: string;
  title?: string | null;
  created_at?: string;
  sessions?: SessionMeta[];
}

export type SessionKind =
  | 'claude'
  | 'codex'
  | 'cursor'
  | 'antigravity'
  /**
   * Zeus is a virtual orchestrator (not a real CLI). Creating a Zeus session
   * runs a Claude PTY under the hood with a Zeus orchestrator system prompt;
   * full multi-CLI delegation lands with F3. See
   * docs/13-agents/zeus-orchestrator.md.
   */
  | 'zeus';
export type SessionStatus = 'running' | 'exited' | 'killed';

/** Heuristic interaction phase of the child CLI, derived from PTY scrollback. */
export type AgentState = 'working' | 'blocked' | 'idle' | 'unknown';

export interface SessionMeta {
  id: string;
  kind: SessionKind;
  thread_id: string;
  cwd?: string | null;
  pid: number;
  status: SessionStatus;
  started_at: string;
  exit_code?: number | null;
  /** Free-form role label — "zeus-orchestrator" for Zeus, "backend"/"qa"/...
   *  for spawned workers, null for plain user-created sessions. */
  role?: string | null;
  /** Parent session id when this session was spawned by an orchestrator. */
  parent_session_id?: string | null;
  /** Topmost ancestor in the session tree (equals `id` for root sessions). */
  root_session_id?: string;
  /** Heuristic detector — null until the first detection pass completes. */
  detected_state?: AgentState | null;
  /** Whether the harness is tailing a structured JSONL transcript for this
   *  session (= Chat view is available). True for Claude/Zeus today. */
  has_transcript?: boolean;
}

export type TranscriptKind =
  | 'message'
  | 'thinking'
  | 'tool_call'
  | 'tool_result'
  | 'system_note'
  | 'meta'
  | 'unknown';

export interface TranscriptUsage {
  input_tokens?: number;
  output_tokens?: number;
  cache_read_input_tokens?: number;
  cache_creation_input_tokens?: number;
  // Additional fields tolerated.
  [k: string]: unknown;
}

export interface TranscriptEvent {
  seq: number;
  session_id: string;
  ts: string;
  source: 'claude' | 'codex' | 'cursor' | 'antigravity';
  kind: TranscriptKind;
  role?: 'user' | 'assistant' | null;
  content?: string | null;
  tool_name?: string | null;
  tool_args?: unknown;
  tool_use_id?: string | null;
  tool_result?: unknown;
  is_error?: boolean | null;
  /** Model id that produced an assistant turn (e.g. "claude-opus-4-7"). */
  model?: string | null;
  /** Token usage for the assistant turn — attached only to its first event. */
  usage?: TranscriptUsage | null;
  /** User-facing label for `system_note` events ("init", "compact", ...). */
  subtype?: string | null;
  raw?: unknown;
}

export interface CreateSessionRequest {
  kind: SessionKind;
  cwd?: string;
  /**
   * Optional initial PTY size. When provided, the backend opens the PTY at
   * exactly this size instead of the 80x24 default, so the TUI's first frame
   * lands at the right dimensions. The SSE catch-up otherwise replays bytes
   * calibrated for 80 cols into a wider terminal and the user sees a mangled
   * first frame until they trigger a repaint.
   */
  cols?: number;
  rows?: number;
}

export interface CreateSessionResponse {
  session_id: string;
}

import type { BudgetView } from './types/BudgetView';
import type { SetBudgetRequest } from './types/SetBudgetRequest';
import type { ApprovalSummary } from './types/ApprovalSummary';
import type { Decision } from './types/Decision';
import type { RememberScope } from './types/RememberScope';
import type {
  Task,
  CreateTaskRequest,
  PatchTaskRequest,
  DeleteTaskRequest,
  TaskStatus,
  Agent,
  CreateAgentRequest
} from './models/task';

export interface ListTasksFilters {
  status?: TaskStatus;
  label?: string;
  assignee?: string;
}

export interface PauseAllState {
  paused: boolean;
}

export type { BudgetView } from './types/BudgetView';
export type { SetBudgetRequest } from './types/SetBudgetRequest';

function isEtagMismatch(body: unknown): boolean {
  if (!body || typeof body !== 'object') return false;
  const record = body as Record<string, unknown>;
  return (
    record.code === 'etag_mismatch' ||
    record.error === 'etag_mismatch' ||
    record.kind === 'etag_mismatch'
  );
}

export interface ProfileSummary {
  id: string;
  display_name: string;
  path: string | null;
  created_at: string;
  active: boolean;
}

export interface ActiveProfile {
  active: string;
  pending: string | null;
}

export interface CreateProfileRequest {
  id: string;
  display_name: string;
  path?: string;
}

export interface ActivateProfileResponse {
  pending: string;
  requires_restart: boolean;
}

export const api = {
  health: (signal?: AbortSignal) => apiRequest<HealthResponse>('/health', { signal }),
  approvals: {
    list: (signal?: AbortSignal) => apiRequest<ApprovalSummary[]>('/approvals', { signal }),
    decide: (id: string, decision: Decision, remember_scope?: RememberScope) =>
      apiRequest<null>(`/approvals/${id}/decide`, {
        method: 'POST',
        body: { decision, remember_scope }
      })
  },
  pauseAll: {
    get: (signal?: AbortSignal) => apiRequest<PauseAllState>('/pause-all', { signal }),
    pause: (signal?: AbortSignal) =>
      apiRequest<PauseAllState>('/pause-all', { method: 'POST', signal }),
    resume: (signal?: AbortSignal) =>
      apiRequest<PauseAllState>('/resume-all', { method: 'POST', signal })
  },
  agents: {
    list: (signal?: AbortSignal) => apiRequest<Agent[]>('/agents', { signal }),
    create: (body: CreateAgentRequest, signal?: AbortSignal) =>
      apiRequest<Agent>('/agents', { method: 'POST', body, signal })
  },
  tasks: {
    list: (threadId: string, filters: ListTasksFilters = {}, signal?: AbortSignal) => {
      const qs = new URLSearchParams();
      if (filters.status) qs.set('status', filters.status);
      if (filters.label) qs.set('label', filters.label);
      if (filters.assignee) qs.set('assignee', filters.assignee);
      const suffix = qs.toString() ? `?${qs.toString()}` : '';
      return apiRequest<Task[]>(`/threads/${threadId}/tasks${suffix}`, { signal });
    },
    get: (threadId: string, taskId: string, signal?: AbortSignal) =>
      apiRequest<Task>(`/threads/${threadId}/tasks/${taskId}`, { signal }),
    create: (threadId: string, body: CreateTaskRequest, signal?: AbortSignal) =>
      apiRequest<Task>(`/threads/${threadId}/tasks`, { method: 'POST', body, signal }),
    patch: (threadId: string, taskId: string, body: PatchTaskRequest, signal?: AbortSignal) =>
      apiRequest<Task>(`/threads/${threadId}/tasks/${taskId}`, {
        method: 'PATCH',
        body,
        signal
      }),
    remove: (threadId: string, taskId: string, body: DeleteTaskRequest, signal?: AbortSignal) =>
      apiRequest<null>(`/threads/${threadId}/tasks/${taskId}`, {
        method: 'DELETE',
        body,
        signal
      })
  },
  threads: {
    list: (signal?: AbortSignal) => apiRequest<ThreadSummary[]>('/threads', { signal }),
    create: (title?: string, signal?: AbortSignal) =>
      apiRequest<{ id: string }>('/threads', {
        method: 'POST',
        body: title ? { title } : undefined,
        signal
      })
  },
  spec: {
    get: (tid: string) => apiRequest<{ content: string; etag: string }>(`/threads/${tid}/spec`),
    put: async (tid: string, body: { content: string; etag?: string }) => {
      try {
        return await apiRequest<{ etag: string; bytes: number; created: boolean }>(
          `/threads/${tid}/spec`,
          { method: 'PUT', body }
        );
      } catch (err) {
        if (err instanceof ApiError && err.status === 409 && isEtagMismatch(err.body)) {
          throw new SpecEtagMismatchError(err.body);
        }
        throw err;
      }
    }
  },
  getBudget: (threadId: string, signal?: AbortSignal) =>
    apiRequest<BudgetView>(`/threads/${threadId}/budget`, { signal }),
  setBudget: (threadId: string, limitUsd: number, signal?: AbortSignal) =>
    apiRequest<BudgetView>(`/threads/${threadId}/budget`, {
      method: 'POST',
      body: { limit_usd: limitUsd } satisfies SetBudgetRequest,
      signal
    }),
  sessions: {
    create: (threadId: string, req: CreateSessionRequest, signal?: AbortSignal) =>
      apiRequest<CreateSessionResponse>(`/threads/${threadId}/sessions`, {
        method: 'POST',
        body: req,
        signal
      }),
    get: (sessionId: string, signal?: AbortSignal) =>
      apiRequest<SessionMeta>(`/sessions/${sessionId}`, { signal }),
    kill: (sessionId: string, signal?: AbortSignal) =>
      apiRequest<null>(`/sessions/${sessionId}`, { method: 'DELETE', signal }),
    resize: (sessionId: string, cols: number, rows: number, signal?: AbortSignal) =>
      apiRequest<null>(`/sessions/${sessionId}/resize`, {
        method: 'POST',
        body: { cols, rows },
        signal
      }),
    // input is sent as raw octet-stream; not typed via apiRequest because of binary body.
    input: async (sessionId: string, bytes: Uint8Array, signal?: AbortSignal) => {
      const url = `${API_BASE.endsWith('/') ? API_BASE.slice(0, -1) : API_BASE}/sessions/${sessionId}/input`;
      const res = await fetch(url, {
        method: 'POST',
        headers: { 'Content-Type': 'application/octet-stream' },
        body: bytes as BodyInit,
        signal
      });
      if (!res.ok) {
        const text = await res.text().catch(() => '');
        throw new ApiError(res.status, `input failed: ${res.status}`, text);
      }
    },
    /**
     * Attach files to a session (N5). Multipart upload; backend stores under
     * `$HARNESS_HOME/.runtime/attach/<sid>/<name>` and returns the saved
     * metadata. The MCP `attach.list`/`attach.read` tools that expose these
     * to the child CLI land in F3.
     */
    attach: async (
      sessionId: string,
      files: File[],
      signal?: AbortSignal
    ): Promise<AttachedFile[]> => {
      const url = `${API_BASE.endsWith('/') ? API_BASE.slice(0, -1) : API_BASE}/sessions/${sessionId}/attach`;
      const form = new FormData();
      for (const f of files) form.append('file', f, f.name);
      const res = await fetch(url, { method: 'POST', body: form, signal });
      if (!res.ok) {
        const text = await res.text().catch(() => '');
        throw new ApiError(res.status, `attach failed: ${res.status}`, text);
      }
      return (await res.json()) as AttachedFile[];
    },
    listAttachments: (sessionId: string, signal?: AbortSignal) =>
      apiRequest<AttachedFile[]>(`/sessions/${sessionId}/attach`, { signal })
  },
  profiles: {
    list: (signal?: AbortSignal) => apiRequest<ProfileSummary[]>('/profiles', { signal }),
    active: (signal?: AbortSignal) => apiRequest<ActiveProfile>('/profiles/active', { signal }),
    create: (body: CreateProfileRequest, signal?: AbortSignal) =>
      apiRequest<ProfileSummary>('/profiles', { method: 'POST', body, signal }),
    activate: (id: string, signal?: AbortSignal) =>
      apiRequest<ActivateProfileResponse>(`/profiles/${encodeURIComponent(id)}/activate`, {
        method: 'POST',
        signal
      })
  }
};

export interface AttachedFile {
  name: string;
  size: number;
  mime: string;
  path: string;
}
