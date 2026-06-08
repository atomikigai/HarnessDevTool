import type { SftpListResult, SftpTransfer } from '$lib/api/ssh';

const ACTIVE_SSH_KEY = 'harness.ssh.activeHostId';

export interface SshWorkspaceSnapshot {
  path: string;
  result: SftpListResult | null;
  error: string | null;
  selectedRemotePath: string;
  downloadLocalPath: string;
  uploadLocalPath: string;
  uploadRemotePath: string;
  mkdirPath: string;
  renameToPath: string;
  lastTransfer: SftpTransfer | null;
  lastMutation: string;
}

function emptyWorkspace(): SshWorkspaceSnapshot {
  return {
    path: '.',
    result: null,
    error: null,
    selectedRemotePath: '',
    downloadLocalPath: '',
    uploadLocalPath: '',
    uploadRemotePath: '',
    mkdirPath: '',
    renameToPath: '',
    lastTransfer: null,
    lastMutation: ''
  };
}

function readActiveHostId(): string | null {
  if (typeof localStorage === 'undefined') return null;
  return localStorage.getItem(ACTIVE_SSH_KEY);
}

function writeActiveHostId(id: string | null): void {
  if (typeof localStorage === 'undefined') return;
  if (id) localStorage.setItem(ACTIVE_SSH_KEY, id);
  else localStorage.removeItem(ACTIVE_SSH_KEY);
}

class SshStore {
  activeHostId = $state<string | null>(readActiveHostId());
  workspaces = $state<Record<string, SshWorkspaceSnapshot>>({});

  workspace(hostId: string): SshWorkspaceSnapshot {
    return this.workspaces[hostId] ?? emptyWorkspace();
  }

  setActiveHost(hostId: string | null): void {
    this.activeHostId = hostId;
    writeActiveHostId(hostId);
  }

  saveWorkspace(hostId: string, snapshot: SshWorkspaceSnapshot): void {
    this.workspaces = { ...this.workspaces, [hostId]: { ...snapshot } };
  }
}

export const sshStore = new SshStore();
