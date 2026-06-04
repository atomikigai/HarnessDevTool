<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { page } from '$app/stores';
  import { Button } from '$lib/components/ui/button';
  import { sshApi, type Host, type SftpListResult, type SftpTransfer } from '$lib/api/ssh';
  import {
    AlertTriangle,
    ArrowUp,
    Download,
    Edit3,
    FileText,
    Folder,
    FolderOpen,
    Home,
    Loader2,
    Plus,
    RefreshCw,
    Terminal,
    Trash2,
    Upload
  } from '$lib/icons';

  const hostId = $derived(($page.params.id ?? '') as string);

  let hosts = $state<Host[]>([]);
  let host = $state<Host | null>(null);
  let path = $state('.');
  let result = $state<SftpListResult | null>(null);
  let loadingHost = $state(false);
  let loadingRemote = $state(false);
  let transferring = $state(false);
  let error = $state<string | null>(null);
  let selectedRemotePath = $state('');
  let downloadLocalPath = $state('');
  let uploadLocalPath = $state('');
  let uploadRemotePath = $state('');
  let mkdirPath = $state('');
  let renameToPath = $state('');
  let lastTransfer = $state<SftpTransfer | null>(null);
  let lastMutation = $state('');

  const entries = $derived(result?.entries ?? []);
  const sortedEntries = $derived(
    [...entries].sort((a, b) => {
      const aDir = a.kind === 'directory' ? 0 : 1;
      const bDir = b.kind === 'directory' ? 0 : 1;
      return aDir - bDir || a.name.localeCompare(b.name);
    })
  );

  onMount(async () => {
    await loadHost();
    if (host) {
      await loadRemote('.');
    }
  });

  async function loadHost() {
    loadingHost = true;
    try {
      const res = await sshApi.hosts.list();
      hosts = res.data ?? [];
      host = hosts.find((h) => h.id === hostId) ?? null;
      error = host ? null : 'SSH host not found.';
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loadingHost = false;
    }
  }

  async function loadRemote(nextPath = path) {
    if (!hostId) return;
    loadingRemote = true;
    try {
      const res = await sshApi.hosts.listRemote(hostId, nextPath || '.');
      result = res.data;
      path = result.path || nextPath || '.';
      selectedRemotePath = '';
      error = result.error ?? null;
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loadingRemote = false;
    }
  }

  function parentPath(value: string): string {
    const clean = value.trim() || '.';
    if (clean === '.' || clean === '/' || clean === '~') return clean;
    const withoutTrailing = clean.replace(/\/+$/, '');
    const idx = withoutTrailing.lastIndexOf('/');
    if (idx <= 0) return '.';
    return withoutTrailing.slice(0, idx);
  }

  function openEntry(entryPath: string, kind: string) {
    if (kind !== 'directory') return;
    void loadRemote(entryPath);
  }

  async function downloadRemote() {
    const remotePath = selectedRemotePath.trim();
    const localPath = downloadLocalPath.trim();
    if (!remotePath || !localPath) {
      error = 'Select a remote file and provide a local destination path.';
      return;
    }
    transferring = true;
    try {
      const res = await sshApi.hosts.getRemote(hostId, {
        remote_path: remotePath,
        local_path: localPath
      });
      lastTransfer = res.data;
      error = res.data.error ?? null;
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      transferring = false;
    }
  }

  async function uploadRemote() {
    const localPath = uploadLocalPath.trim();
    const remotePath = uploadRemotePath.trim();
    if (!localPath || !remotePath) {
      error = 'Provide local source and remote destination paths.';
      return;
    }
    transferring = true;
    try {
      const res = await sshApi.hosts.putRemote(hostId, {
        local_path: localPath,
        remote_path: remotePath
      });
      lastTransfer = res.data;
      error = res.data.error ?? null;
      await loadRemote(path);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      transferring = false;
    }
  }

  async function mutateRemote(action: 'mkdir' | 'rmdir' | 'unlink' | 'rename') {
    try {
      if (action === 'mkdir') {
        const target = mkdirPath.trim();
        if (!target) {
          error = 'Provide a remote directory path to create.';
          return;
        }
        const res = await sshApi.hosts.mkdir(hostId, { path: target });
        lastMutation = mutationLabel('mkdir', res.data);
      } else if (action === 'rmdir') {
        const target = selectedRemotePath.trim();
        if (!target) {
          error = 'Select an empty remote directory to remove.';
          return;
        }
        const res = await sshApi.hosts.rmdir(hostId, { path: target });
        lastMutation = mutationLabel('rmdir', res.data);
      } else if (action === 'unlink') {
        const target = selectedRemotePath.trim();
        if (!target) {
          error = 'Select a remote file to remove.';
          return;
        }
        const res = await sshApi.hosts.unlink(hostId, { path: target });
        lastMutation = mutationLabel('unlink', res.data);
      } else {
        const fromPath = selectedRemotePath.trim();
        const toPath = renameToPath.trim();
        if (!fromPath || !toPath) {
          error = 'Select a remote path and provide the new path.';
          return;
        }
        const res = await sshApi.hosts.rename(hostId, {
          from_path: fromPath,
          to_path: toPath
        });
        lastMutation = mutationLabel('rename', res.data);
      }
      error = null;
      await loadRemote(path);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
  }

  function formatSize(raw: number | bigint): string {
    const bytes = Number(raw);
    if (!Number.isFinite(bytes)) return '-';
    if (bytes < 1024) return `${bytes} B`;
    const units = ['KiB', 'MiB', 'GiB', 'TiB'];
    let value = bytes / 1024;
    let idx = 0;
    while (value >= 1024 && idx < units.length - 1) {
      value /= 1024;
      idx += 1;
    }
    return `${value.toFixed(value >= 10 ? 0 : 1)} ${units[idx]}`;
  }

  function transferLabel(transfer: SftpTransfer): string {
    return `${transfer.status}: ${formatSize(transfer.bytes_done)} / ${formatSize(transfer.bytes_total)}`;
  }

  function mutationLabel(action: string, result: { ok: boolean; stderr: string; stdout: string }): string {
    if (result.ok) return `${action}: ok`;
    const detail = result.stderr.trim() || result.stdout.trim() || 'failed';
    return `${action}: ${detail}`;
  }
</script>

<div class="flex h-full min-h-0 flex-col">
  <header
    class="flex h-14 shrink-0 items-center justify-between gap-4 border-b px-6"
    style="background: var(--surface-window); border-color: var(--border-subtle);"
  >
    <div class="min-w-0">
      <div class="flex items-center gap-2">
        <Terminal class="h-4 w-4 shrink-0" />
        <h1
          class="truncate font-serif text-xl font-semibold tracking-tight"
          style="color: var(--fg-default);"
        >
          {host?.name ?? 'SSH'}
        </h1>
      </div>
      <p class="truncate text-xs" style="color: var(--fg-muted);">
        {#if host}
          {host.username}@{host.host}:{host.port}
        {:else if loadingHost}
          Loading host...
        {:else}
          Host unavailable
        {/if}
      </p>
    </div>
    <div class="flex shrink-0 items-center gap-2">
      <Button variant="outline" size="sm" onclick={() => goto('/ssh')}>Hosts</Button>
      <Button variant="outline" size="sm" onclick={() => loadRemote()} disabled={!host || loadingRemote}>
        {#if loadingRemote}<Loader2 class="h-3.5 w-3.5 animate-spin" />{:else}<RefreshCw class="h-3.5 w-3.5" />{/if}
        Refresh
      </Button>
    </div>
  </header>

  <main class="flex min-h-0 flex-1 flex-col p-5">
    <div class="mb-4 flex items-center gap-2">
      <Button variant="ghost" size="sm" onclick={() => loadRemote('.')} disabled={!host || loadingRemote}>
        <Home class="h-3.5 w-3.5" />
      </Button>
      <Button
        variant="ghost"
        size="sm"
        onclick={() => loadRemote(parentPath(path))}
        disabled={!host || loadingRemote}
      >
        <ArrowUp class="h-3.5 w-3.5" />
      </Button>
      <input
        class="min-w-0 flex-1 rounded-md border bg-transparent px-3 py-2 font-mono text-sm outline-none"
        style="border-color: var(--border-input);"
        bind:value={path}
        onkeydown={(e) => {
          if (e.key === 'Enter') void loadRemote(path);
        }}
      />
      <Button onclick={() => loadRemote(path)} disabled={!host || loadingRemote}>
        Open
      </Button>
    </div>

    {#if error}
      <div
        class="mb-4 flex items-start gap-2 rounded-md border px-4 py-3 text-sm"
        style="border-color: color-mix(in srgb, var(--dot-danger) 35%, transparent); color: var(--dot-danger);"
      >
        <AlertTriangle class="mt-0.5 h-4 w-4 shrink-0" />
        <span>{error}</span>
      </div>
    {/if}

    <section
      class="mb-4 grid gap-3 rounded-md border p-3 lg:grid-cols-2"
      style="border-color: var(--border-subtle); background: var(--surface-window);"
    >
      <div class="flex min-w-0 flex-col gap-2">
        <div class="flex items-center gap-2 text-xs font-medium" style="color: var(--fg-muted);">
          <Download class="h-3.5 w-3.5" />
          Download remote file
        </div>
        <input
          class="rounded-md border bg-transparent px-3 py-2 font-mono text-xs outline-none"
          style="border-color: var(--border-input);"
          bind:value={selectedRemotePath}
          placeholder="Remote path"
        />
        <div class="flex gap-2">
          <input
            class="min-w-0 flex-1 rounded-md border bg-transparent px-3 py-2 font-mono text-xs outline-none"
            style="border-color: var(--border-input);"
            bind:value={downloadLocalPath}
            placeholder="Local destination path on harness server"
          />
          <Button size="sm" onclick={downloadRemote} disabled={!host || transferring}>
            {#if transferring}<Loader2 class="h-3.5 w-3.5 animate-spin" />{:else}<Download class="h-3.5 w-3.5" />{/if}
          </Button>
        </div>
      </div>

      <div class="flex min-w-0 flex-col gap-2">
        <div class="flex items-center gap-2 text-xs font-medium" style="color: var(--fg-muted);">
          <Upload class="h-3.5 w-3.5" />
          Upload local file
        </div>
        <input
          class="rounded-md border bg-transparent px-3 py-2 font-mono text-xs outline-none"
          style="border-color: var(--border-input);"
          bind:value={uploadLocalPath}
          placeholder="Local source path on harness server"
        />
        <div class="flex gap-2">
          <input
            class="min-w-0 flex-1 rounded-md border bg-transparent px-3 py-2 font-mono text-xs outline-none"
            style="border-color: var(--border-input);"
            bind:value={uploadRemotePath}
            placeholder="Remote destination path"
          />
          <Button size="sm" onclick={uploadRemote} disabled={!host || transferring}>
            {#if transferring}<Loader2 class="h-3.5 w-3.5 animate-spin" />{:else}<Upload class="h-3.5 w-3.5" />{/if}
          </Button>
        </div>
      </div>
      {#if lastTransfer}
        <p class="lg:col-span-2 font-mono text-xs" style="color: var(--fg-muted);">
          {transferLabel(lastTransfer)}
        </p>
      {/if}
    </section>

    <section
      class="mb-4 grid gap-3 rounded-md border p-3 lg:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_auto]"
      style="border-color: var(--border-subtle); background: var(--surface-window);"
    >
      <div class="flex min-w-0 flex-col gap-2">
        <div class="flex items-center gap-2 text-xs font-medium" style="color: var(--fg-muted);">
          <Plus class="h-3.5 w-3.5" />
          Create directory
        </div>
        <input
          class="rounded-md border bg-transparent px-3 py-2 font-mono text-xs outline-none"
          style="border-color: var(--border-input);"
          bind:value={mkdirPath}
          placeholder="Remote directory path"
        />
      </div>

      <div class="flex min-w-0 flex-col gap-2">
        <div class="flex items-center gap-2 text-xs font-medium" style="color: var(--fg-muted);">
          <Edit3 class="h-3.5 w-3.5" />
          Rename selected path
        </div>
        <input
          class="rounded-md border bg-transparent px-3 py-2 font-mono text-xs outline-none"
          style="border-color: var(--border-input);"
          bind:value={renameToPath}
          placeholder="New remote path"
        />
      </div>

      <div class="grid grid-cols-2 gap-2 self-end lg:grid-cols-4">
        <Button size="sm" onclick={() => mutateRemote('mkdir')} disabled={!host}>
          <Plus class="h-3.5 w-3.5" />
        </Button>
        <Button size="sm" variant="outline" onclick={() => mutateRemote('rename')} disabled={!host}>
          <Edit3 class="h-3.5 w-3.5" />
        </Button>
        <Button size="sm" variant="outline" onclick={() => mutateRemote('rmdir')} disabled={!host}>
          <Folder class="h-3.5 w-3.5" />
        </Button>
        <Button size="sm" variant="outline" onclick={() => mutateRemote('unlink')} disabled={!host}>
          <Trash2 class="h-3.5 w-3.5" />
        </Button>
      </div>

      {#if lastMutation}
        <p class="lg:col-span-3 font-mono text-xs" style="color: var(--fg-muted);">
          {lastMutation}
        </p>
      {/if}
    </section>

    <section
      class="min-h-0 flex-1 overflow-hidden rounded-md border"
      style="border-color: var(--border-subtle); background: var(--surface-window);"
    >
      <div
        class="grid grid-cols-[minmax(0,1fr)_110px_130px] border-b px-3 py-2 text-[11px] font-medium uppercase"
        style="border-color: var(--border-subtle); color: var(--fg-muted);"
      >
        <span>Name</span>
        <span>Kind</span>
        <span class="text-right">Size</span>
      </div>

      {#if loadingRemote}
        <div class="flex h-52 items-center justify-center text-sm" style="color: var(--fg-muted);">
          <Loader2 class="mr-2 h-4 w-4 animate-spin" />
          Loading remote path
        </div>
      {:else if sortedEntries.length === 0}
        <div class="flex h-52 items-center justify-center text-sm" style="color: var(--fg-muted);">
          No entries.
        </div>
      {:else}
        <div class="max-h-full overflow-auto">
          {#each sortedEntries as entry (entry.path)}
            <button
              type="button"
              class="grid w-full grid-cols-[minmax(0,1fr)_110px_130px] items-center border-b px-3 py-2 text-left text-sm transition-colors hover:bg-[var(--surface-titlebar)]"
              style="border-color: var(--border-subtle);"
              ondblclick={() => openEntry(entry.path, entry.kind)}
              onclick={() => {
                selectedRemotePath = entry.path;
                if (entry.kind === 'directory') path = entry.path;
              }}
            >
              <span class="flex min-w-0 items-center gap-2">
                {#if entry.kind === 'directory'}
                  <Folder class="h-4 w-4 shrink-0" />
                {:else if entry.kind === 'symlink'}
                  <FolderOpen class="h-4 w-4 shrink-0" />
                {:else}
                  <FileText class="h-4 w-4 shrink-0" />
                {/if}
                <span class="truncate font-mono text-xs">{entry.name}</span>
              </span>
              <span class="text-xs" style="color: var(--fg-muted);">{entry.kind}</span>
              <span class="text-right font-mono text-xs" style="color: var(--fg-muted);">
                {entry.kind === 'directory' ? '-' : formatSize(entry.size)}
              </span>
            </button>
          {/each}
        </div>
      {/if}
    </section>
  </main>
</div>
