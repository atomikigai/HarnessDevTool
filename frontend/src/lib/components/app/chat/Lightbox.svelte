<script lang="ts">
  import { X } from '$lib/icons';

  interface Props {
    src: string | null;
    onClose: () => void;
  }

  let { src, onClose }: Props = $props();
</script>

{#if src}
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
  <div
    class="lightbox-overlay"
    onclick={onClose}
    role="dialog"
    tabindex="-1"
    aria-modal="true"
    aria-label="Image preview"
  >
    <button type="button" class="lightbox-close" onclick={onClose} aria-label="Close lightbox">
      <X size={20} />
    </button>
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <img {src} alt="Preview" class="lightbox-img" onclick={(e) => e.stopPropagation()} />
  </div>
{/if}

<style>
  .lightbox-overlay {
    align-items: center;
    background: rgba(0, 0, 0, 0.92);
    bottom: 0;
    cursor: pointer;
    display: flex;
    justify-content: center;
    left: 0;
    position: fixed;
    right: 0;
    top: 0;
    z-index: 9999;
  }

  .lightbox-close {
    align-items: center;
    background: rgba(255, 255, 255, 0.1);
    border: 1px solid rgba(255, 255, 255, 0.18);
    border-radius: 999px;
    color: #fff;
    cursor: pointer;
    display: flex;
    height: 2.25rem;
    justify-content: center;
    position: fixed;
    right: 1.25rem;
    top: 1.25rem;
    transition: background 150ms ease;
    width: 2.25rem;
    z-index: 10000;
  }

  .lightbox-close:hover {
    background: rgba(255, 255, 255, 0.2);
  }

  .lightbox-img {
    border-radius: 0.5rem;
    cursor: default;
    max-height: 90vh;
    max-width: 90vw;
    object-fit: contain;
  }
</style>
