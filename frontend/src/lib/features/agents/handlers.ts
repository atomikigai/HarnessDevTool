import axios, { AxiosError, type AxiosRequestConfig } from 'axios';
import {
  API_BASE,
  ApiError,
  DEFAULT_API_TIMEOUT_MS,
  PROTOCOL_VERSION_HEADER,
  apiHeaders,
  type ApiResponse
} from '$lib/api/client';
import type {
  CreateAgentSessionInput,
  CreateAgentSessionResult,
  CreateSessionResponse,
  CurrentRepoReport,
  NewSessionFormData,
  ReadinessReport
} from './entities';
import { validateNewSessionForm } from './schemas';

const http = axios.create({
  baseURL: API_BASE,
  timeout: DEFAULT_API_TIMEOUT_MS
});

function config(extra: AxiosRequestConfig = {}): AxiosRequestConfig {
  return {
    ...extra,
    headers: {
      ...apiHeaders(),
      ...(extra.headers ?? {})
    }
  };
}

function protocolVersion(headers: unknown): string | null {
  if (!headers || typeof headers !== 'object') return null;
  const h = headers as Record<string, string | undefined>;
  return h[PROTOCOL_VERSION_HEADER] ?? h[PROTOCOL_VERSION_HEADER.toLowerCase()] ?? null;
}

function toApiError(err: unknown): never {
  if (err instanceof ApiError) throw err;
  if (err instanceof AxiosError) {
    const status = err.response?.status ?? 0;
    const body = err.response?.data;
    const statusText = err.response?.statusText ?? err.message;
    throw new ApiError(status, `Request failed: ${status} ${statusText}`, body);
  }
  throw err;
}

async function request<T>(req: AxiosRequestConfig): Promise<ApiResponse<T>> {
  try {
    const res = await http.request<T>(config(req));
    return {
      data: res.data,
      protocolVersion: protocolVersion(res.headers),
      status: res.status
    };
  } catch (err) {
    toApiError(err);
  }
}

export function lookupCurrentRepo(cwd: string): Promise<ApiResponse<CurrentRepoReport>> {
  return request<CurrentRepoReport>({
    method: 'GET',
    url: '/repos/current',
    params: { cwd }
  });
}

function createThread(
  autonomyProfile: NewSessionFormData['autonomy'],
  cwd?: string
): Promise<ApiResponse<{ id: string; readiness: ReadinessReport }>> {
  return request<{ id: string; readiness: ReadinessReport }>({
    method: 'POST',
    url: '/threads',
    data: {
      autonomy_profile: autonomyProfile,
      cwd
    }
  });
}

function recalculateReadiness(threadId: string, cwd?: string): Promise<ApiResponse<ReadinessReport>> {
  return request<ReadinessReport>({
    method: 'POST',
    url: `/threads/${threadId}/readiness`,
    params: cwd ? { cwd } : undefined
  });
}

function createSession(
  threadId: string,
  form: NewSessionFormData
): Promise<ApiResponse<CreateSessionResponse>> {
  return request<CreateSessionResponse>({
    method: 'POST',
    url: `/threads/${threadId}/sessions`,
    data: {
      kind: form.kind,
      cwd: form.cwd,
      include_project_context: form.repoMode !== 'none',
      capability_profile: form.capabilityProfile,
      zeus_roles: form.kind === 'zeus' ? form.zeusRoles : [],
      cols: form.cols,
      rows: form.rows
    }
  });
}

export async function createAgentSession(
  input: CreateAgentSessionInput
): Promise<CreateAgentSessionResult> {
  const parsed = validateNewSessionForm(input.form);
  if (!parsed.ok) throw new ApiError(400, parsed.message);

  const form = parsed.data;
  let threadId = input.threadId ?? null;

  if (!threadId && form.repoMode === 'resume') {
    threadId =
      input.repoReport?.continuity?.recommended_thread_id ??
      input.repoReport?.repo?.last_thread_id ??
      null;
  }

  if (!threadId) {
    const thread = await createThread(form.autonomy, form.cwd);
    threadId = thread.data.id;
    if (form.cwd) {
      try {
        await recalculateReadiness(threadId, form.cwd);
      } catch {
        // Session creation can proceed; readiness is advisory UI metadata.
      }
    }
  }

  const session = await createSession(threadId, form);
  return { threadId, sessionId: session.data.session_id };
}
