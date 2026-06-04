import { apiRequest } from './client';
import type { Host } from './types/Host';
import type { HostInput } from './types/HostInput';
import type { HostTestResult } from './types/HostTestResult';
import type { SftpListResult } from './types/SftpListResult';
import type { SftpTransfer } from './types/SftpTransfer';
import type { SshExecResult } from './types/SshExecResult';
import type { SshSession } from './types/SshSession';

export type {
  Host,
  HostInput,
  HostTestResult,
  SftpListResult,
  SftpTransfer,
  SshExecResult,
  SshSession
};

export interface RemovedResponse {
  removed: boolean;
}

export const sshApi = {
  hosts: {
    list: (signal?: AbortSignal) => apiRequest<Host[]>('/ssh/hosts', { signal }),
    add: (body: HostInput, signal?: AbortSignal) =>
      apiRequest<Host>('/ssh/hosts', { method: 'POST', body, signal }),
    remove: (id: string, signal?: AbortSignal) =>
      apiRequest<RemovedResponse>(`/ssh/hosts/${id}`, { method: 'DELETE', signal }),
    test: (id: string, signal?: AbortSignal) =>
      apiRequest<HostTestResult>(`/ssh/hosts/${id}/test`, { method: 'POST', signal }),
    exec: (id: string, body: { cmd: string }, signal?: AbortSignal) =>
      apiRequest<SshExecResult>(`/ssh/hosts/${id}/exec`, { method: 'POST', body, signal }),
    mkdir: (id: string, body: { path: string }, signal?: AbortSignal) =>
      apiRequest<SshExecResult>(`/ssh/hosts/${id}/sftp/mkdir`, { method: 'POST', body, signal }),
    rmdir: (id: string, body: { path: string }, signal?: AbortSignal) =>
      apiRequest<SshExecResult>(`/ssh/hosts/${id}/sftp/rmdir`, { method: 'POST', body, signal }),
    unlink: (id: string, body: { path: string }, signal?: AbortSignal) =>
      apiRequest<SshExecResult>(`/ssh/hosts/${id}/sftp/unlink`, { method: 'POST', body, signal }),
    rename: (
      id: string,
      body: { from_path: string; to_path: string },
      signal?: AbortSignal
    ) => apiRequest<SshExecResult>(`/ssh/hosts/${id}/sftp/rename`, { method: 'POST', body, signal }),
    listRemote: (id: string, path: string = '.', signal?: AbortSignal) =>
      apiRequest<SftpListResult>(`/ssh/hosts/${id}/sftp?path=${encodeURIComponent(path)}`, {
        signal
      }),
    getRemote: (id: string, body: { remote_path: string; local_path: string }, signal?: AbortSignal) =>
      apiRequest<SftpTransfer>(`/ssh/hosts/${id}/sftp/get`, {
        method: 'POST',
        body,
        signal,
        timeoutMs: 600_000
      }),
    putRemote: (id: string, body: { local_path: string; remote_path: string }, signal?: AbortSignal) =>
      apiRequest<SftpTransfer>(`/ssh/hosts/${id}/sftp/put`, {
        method: 'POST',
        body,
        signal,
        timeoutMs: 600_000
      })
  },
  sessions: {
    open: (hostId: string, signal?: AbortSignal) =>
      apiRequest<SshSession>(`/ssh/hosts/${hostId}/sessions`, { method: 'POST', signal }),
    close: (sessionId: string, signal?: AbortSignal) =>
      apiRequest<RemovedResponse>(`/ssh/sessions/${sessionId}`, { method: 'DELETE', signal })
  }
};
