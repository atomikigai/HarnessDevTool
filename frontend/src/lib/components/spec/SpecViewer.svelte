<script lang="ts">
  import { formatDistanceToNow } from 'date-fns';
  import { specState } from '$lib/stores/spec.svelte';
  import { Button } from '$lib/components/ui/button';
  import { Edit3, RefreshCw, Save, X, RotateCcw } from '$lib/icons';

  let { threadId }: { threadId: string } = $props();

  let editing = $state(false);
  let draft = $state('');

  const etagShort = $derived(specState.etag ? specState.etag.slice(0, 8) : 'none');
  const updatedText = $derived(
    specState.updatedAt
      ? formatDistanceToNow(new Date(specState.updatedAt), { addSuffix: true })
      : 'never'
  );

  function beginEdit() {
    draft = specState.content;
    editing = true;
  }

  function cancelEdit() {
    draft = specState.content;
    editing = false;
  }

  async function save() {
    await specState.save(draft);
    if (!specState.staleEtag && !specState.error) editing = false;
  }

  function reload() {
    specState.start(threadId);
    if (!editing) draft = specState.content;
  }
</script>

<div class="flex h-full min-h-0 flex-col gap-3 p-4">
  <section
    class="flex min-h-0 flex-1 flex-col overflow-hidden rounded-md border"
    style="background: var(--surface-panel); border-color: var(--border-subtle);"
  >
    <header
      class="flex items-center gap-3 border-b px-4 py-2"
      style="background: var(--surface-titlebar); border-color: var(--border-subtle);"
    >
      <h1 class="text-sm font-semibold" style="color: var(--fg-default);">Spec</h1>
      <div class="ml-auto flex items-center gap-2">
        <Button size="sm" variant="outline" onclick={reload}>
          <RefreshCw class="h-3.5 w-3.5" /> Refresh
        </Button>
        {#if editing}
          <Button size="sm" variant="ghost" onclick={cancelEdit}>
            <X class="h-3.5 w-3.5" /> Cancel
          </Button>
        {:else}
          <Button size="sm" onclick={beginEdit}>
            <Edit3 class="h-3.5 w-3.5" /> Edit
          </Button>
        {/if}
      </div>
    </header>

    {#if specState.staleEtag}
      <div
        class="flex items-center gap-3 border-b px-4 py-2 text-xs"
        style="border-color: color-mix(in srgb, var(--dot-warn) 35%, transparent); background: color-mix(in srgb, var(--dot-warn) 12%, transparent); color: var(--dot-warn);"
      >
        <span class="font-medium">Spec changed elsewhere - reload to merge</span>
        <Button size="sm" variant="outline" onclick={reload}>
          <RotateCcw class="h-3.5 w-3.5" /> Reload
        </Button>
      </div>
    {/if}

    <div class="min-h-0 flex-1 overflow-hidden">
      {#if editing}
        <textarea
          bind:value={draft}
          class="h-full w-full resize-none border-0 bg-transparent p-4 font-mono text-sm outline-none"
          style="color: var(--fg-default);"
          spellcheck="false"
        ></textarea>
      {:else}
        <pre
          class="h-full overflow-auto p-4 font-mono text-sm"
          style="color: var(--fg-default); white-space: pre-wrap; word-wrap: break-word;">{specState.content ||
            'No spec saved yet.'}</pre>
      {/if}
    </div>

    <footer
      class="flex min-h-10 items-center gap-3 border-t px-4 py-2 text-xs"
      style="background: var(--surface-statusbar); border-color: var(--border-subtle); color: var(--fg-muted);"
    >
      {#if specState.loading}
        <span>Loading...</span>
      {:else if specState.error}
        <span style="color: var(--dot-danger);">{specState.error}</span>
      {:else}
        <span>Updated {updatedText} - etag <span class="font-mono">{etagShort}</span></span>
      {/if}
      {#if editing}
        <div class="ml-auto flex items-center gap-2">
          <Button size="sm" variant="ghost" onclick={cancelEdit}>Cancel</Button>
          <Button size="sm" onclick={save} disabled={specState.loading}>
            <Save class="h-3.5 w-3.5" /> Save
          </Button>
        </div>
      {/if}
    </footer>
  </section>

  {#if specState.artifacts.length > 0}
    <section
      class="rounded-md border"
      style="background: var(--surface-panel); border-color: var(--border-subtle);"
    >
      <div
        class="border-b px-4 py-2 text-xs font-medium"
        style="border-color: var(--border-subtle); color: var(--fg-muted);"
      >
        Recent artifacts
      </div>
      <ul class="max-h-36 overflow-auto">
        {#each specState.artifacts as artifact (`${artifact.at}:${artifact.path}`)}
          <li
            class="flex items-center gap-3 border-b px-4 py-2 text-xs last:border-b-0"
            style="border-color: var(--row-divider);"
          >
            <span class="min-w-0 flex-1 truncate font-mono" title={artifact.path}
              >{artifact.path}</span
            >
            <span
              class="rounded px-1.5 py-0.5"
              style="background: var(--accent-soft); color: var(--accent);"
            >
              {artifact.kind}
            </span>
          </li>
        {/each}
      </ul>
    </section>
  {/if}
</div>
