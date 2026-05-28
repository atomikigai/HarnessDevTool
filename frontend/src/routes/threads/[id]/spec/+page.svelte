<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { page } from '$app/stores';
  import SpecViewer from '$lib/components/spec/SpecViewer.svelte';
  import { specState } from '$lib/stores/spec.svelte';

  const threadId = $derived($page.params.id as string);

  onMount(() => {
    specState.start(threadId);
  });

  onDestroy(() => {
    specState.stop();
  });

  $effect(() => {
    if (threadId) specState.start(threadId);
  });
</script>

<SpecViewer {threadId} />
