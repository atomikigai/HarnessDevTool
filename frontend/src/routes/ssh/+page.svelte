<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { Button } from '$lib/components/ui/button';
  import { sshApi, type Host, type HostInput } from '$lib/api/ssh';
  import { ApiError } from '$lib/api/client';
  import { Loader2, Plus, RefreshCw, Trash2, Activity, Terminal } from '$lib/icons';
  import { sshStore } from '$lib/stores/ssh.svelte';
  import { toast } from 'svelte-sonner';

  let hosts = $state<Host[]>([]);
  let loading = $state(false);
  let saving = $state(false);
  let error = $state<string | null>(null);
  let testing = $state<Record<string, boolean>>({});
  let testResults = $state<Record<string, string>>({});

  let draft = $state<HostInput>({
    name: '',
    host: '',
    port: 22,
    username: '',
    auth_method: 'key_file',
    key_path: '',
    password: '',
    host_key_policy: 'tofu'
  });

  async function refresh() {
    loading = true;
    try {
      const res = await sshApi.hosts.list();
      hosts = res.data ?? [];
      if (sshStore.activeHostId && hosts.some((host) => host.id === sshStore.activeHostId)) {
        goto(`/ssh/${sshStore.activeHostId}`, { replaceState: true });
      }
      error = null;
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    void refresh();
  });

  function cleanDraft(): HostInput {
    return {
      ...draft,
      name: draft.name.trim(),
      host: draft.host.trim(),
      username: draft.username.trim(),
      key_path: draft.auth_method === 'key_file' ? draft.key_path?.trim() || null : null,
      password: draft.auth_method === 'password' ? draft.password?.trim() || null : null
    };
  }

  async function addHost() {
    saving = true;
    try {
      await sshApi.hosts.add(cleanDraft());
      draft = {
        name: '',
        host: '',
        port: 22,
        username: '',
        auth_method: 'key_file',
        key_path: '',
        password: '',
        host_key_policy: 'tofu'
      };
      await refresh();
      toast.success('SSH host saved');
    } catch (err) {
      const msg =
        err instanceof ApiError
          ? ((err.body as { error?: string } | undefined)?.error ?? err.message)
          : err instanceof Error
            ? err.message
            : 'Save failed';
      toast.error(msg);
    } finally {
      saving = false;
    }
  }

  async function testHost(host: Host) {
    testing = { ...testing, [host.id]: true };
    try {
      const res = await sshApi.hosts.test(host.id);
      testResults = { ...testResults, [host.id]: res.data.message };
    } catch (err) {
      testResults = {
        ...testResults,
        [host.id]: err instanceof Error ? err.message : 'Test failed'
      };
    } finally {
      testing = { ...testing, [host.id]: false };
    }
  }

  async function removeHost(host: Host) {
    try {
      await sshApi.hosts.remove(host.id);
      await refresh();
      toast.success('SSH host deleted');
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Delete failed');
    }
  }
</script>

<div class="flex h-full min-h-0 flex-col">
  <header
    class="flex h-14 shrink-0 items-center justify-between gap-4 border-b px-6"
    style="background: var(--surface-window); border-color: var(--border-subtle);"
  >
    <div>
      <h1 class="font-serif text-xl font-semibold tracking-tight" style="color: var(--fg-default);">
        SSH
      </h1>
      <p class="text-xs" style="color: var(--fg-muted);">Manage SSH hosts and SFTP sessions.</p>
    </div>
    <Button variant="outline" size="sm" onclick={refresh} disabled={loading}>
      {#if loading}<Loader2 class="h-3.5 w-3.5 animate-spin" />{:else}<RefreshCw class="h-3.5 w-3.5" />{/if}
      Refresh
    </Button>
  </header>

  <div class="grid min-h-0 flex-1 grid-cols-[360px_minmax(0,1fr)] overflow-hidden">
    <section class="border-r p-5" style="border-color: var(--border-subtle);">
      <div class="flex flex-col gap-3">
        <input class="rounded-md border bg-transparent px-3 py-2 text-sm outline-none" style="border-color: var(--border-input);" bind:value={draft.name} placeholder="Name" />
        <input class="rounded-md border bg-transparent px-3 py-2 text-sm outline-none" style="border-color: var(--border-input);" bind:value={draft.host} placeholder="Host" />
        <div class="grid grid-cols-[1fr_110px] gap-2">
          <input class="rounded-md border bg-transparent px-3 py-2 text-sm outline-none" style="border-color: var(--border-input);" bind:value={draft.username} placeholder="Username" />
          <input class="rounded-md border bg-transparent px-3 py-2 text-sm outline-none" style="border-color: var(--border-input);" bind:value={draft.port} type="number" min="1" />
        </div>
        <select class="rounded-md border bg-transparent px-3 py-2 text-sm outline-none" style="border-color: var(--border-input);" bind:value={draft.auth_method}>
          <option value="key_file">Key file</option>
          <option value="agent">Agent</option>
          <option value="password">Password</option>
        </select>
        {#if draft.auth_method === 'key_file'}
          <input class="rounded-md border bg-transparent px-3 py-2 text-sm outline-none" style="border-color: var(--border-input);" bind:value={draft.key_path} placeholder="Key path" />
        {:else if draft.auth_method === 'password'}
          <input class="rounded-md border bg-transparent px-3 py-2 text-sm outline-none" style="border-color: var(--border-input);" bind:value={draft.password} placeholder="Password" type="password" autocomplete="new-password" />
        {/if}
        <Button onclick={addHost} disabled={saving}>
          {#if saving}<Loader2 class="h-3.5 w-3.5 animate-spin" />{:else}<Plus class="h-3.5 w-3.5" />{/if}
          Add host
        </Button>
      </div>
    </section>

    <section class="min-h-0 overflow-auto p-6">
      {#if error}
        <div class="rounded-md border px-4 py-3 text-sm" style="border-color: color-mix(in srgb, var(--dot-danger) 35%, transparent); color: var(--dot-danger);">
          {error}
        </div>
      {/if}

      {#if hosts.length === 0 && !loading}
        <div class="flex h-full items-center justify-center text-sm" style="color: var(--fg-muted);">
          No SSH hosts saved.
        </div>
      {:else}
        <div class="grid gap-3">
          {#each hosts as host (host.id)}
            <article class="rounded-md border p-4" style="border-color: var(--border-subtle); background: var(--surface-window);">
              <div class="flex items-start justify-between gap-4">
                <div class="min-w-0">
                  <div class="flex items-center gap-2">
                    <Terminal class="h-4 w-4" />
                    <h2 class="truncate text-sm font-semibold">{host.name}</h2>
                  </div>
                  <p class="mt-1 text-xs" style="color: var(--fg-muted);">{host.username}@{host.host}:{host.port}</p>
                  {#if testResults[host.id]}
                    <p class="mt-2 text-xs" style="color: var(--fg-muted);">{testResults[host.id]}</p>
                  {/if}
                </div>
                <div class="flex shrink-0 items-center gap-2">
                  <Button
                    variant="outline"
                    size="sm"
                    onclick={() => {
                      sshStore.setActiveHost(host.id);
                      goto(`/ssh/${host.id}`);
                    }}
                  >
                    Open
                  </Button>
                  <Button variant="outline" size="sm" onclick={() => testHost(host)} disabled={testing[host.id]}>
                    {#if testing[host.id]}<Loader2 class="h-3.5 w-3.5 animate-spin" />{:else}<Activity class="h-3.5 w-3.5" />{/if}
                    Test
                  </Button>
                  <Button variant="ghost" size="sm" onclick={() => removeHost(host)}>
                    <Trash2 class="h-3.5 w-3.5" />
                  </Button>
                </div>
              </div>
            </article>
          {/each}
        </div>
      {/if}
    </section>
  </div>
</div>
