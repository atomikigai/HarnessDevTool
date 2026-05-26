<!--
  Inline SVG icon set adapted from `harness-icons.jsx` (design pack).
  Single component dispatches on `name`. All icons use currentColor and
  a consistent 1.2–1.4 stroke. Sized via the `size` prop so callers don't
  need to touch viewBox.
-->
<script lang="ts">
  type Props = {
    name:
      | 'threads'
      | 'agents'
      | 'sql'
      | 'ssh'
      | 'memory'
      | 'settings'
      | 'search'
      | 'plus'
      | 'command'
      | 'sun'
      | 'moon';
    size?: number;
    class?: string;
  };

  let { name, size = 16, class: className = '' }: Props = $props();
</script>

{#if name === 'threads'}
  <!-- speech bubbles — used for the Threads section in the rail. -->
  <svg
    width={size}
    height={size}
    viewBox="0 0 16 16"
    fill="none"
    class={className}
    aria-hidden="true"
  >
    <path
      d="M2 4a1.5 1.5 0 011.5-1.5h7A1.5 1.5 0 0112 4v4a1.5 1.5 0 01-1.5 1.5H6L3.5 11.5V9.5H3.5A1.5 1.5 0 012 8V4z"
      stroke="currentColor"
      stroke-width="1.2"
      stroke-linejoin="round"
    />
  </svg>
{:else if name === 'agents'}
  <svg
    width={size}
    height={size}
    viewBox="0 0 16 16"
    fill="none"
    class={className}
    aria-hidden="true"
  >
    <path
      d="M8 1.5l1.5 4 4 1.5-4 1.5L8 12.5 6.5 8.5l-4-1.5 4-1.5L8 1.5z"
      stroke="currentColor"
      stroke-width="1.2"
      stroke-linejoin="round"
    />
  </svg>
{:else if name === 'sql'}
  <svg
    width={size}
    height={size}
    viewBox="0 0 16 16"
    fill="none"
    class={className}
    aria-hidden="true"
  >
    <ellipse cx="8" cy="3.5" rx="5" ry="1.6" stroke="currentColor" stroke-width="1.2" />
    <path
      d="M3 3.5v9c0 .88 2.24 1.6 5 1.6s5-.72 5-1.6v-9"
      stroke="currentColor"
      stroke-width="1.2"
    />
    <path d="M3 8c0 .88 2.24 1.6 5 1.6s5-.72 5-1.6" stroke="currentColor" stroke-width="1.2" />
  </svg>
{:else if name === 'ssh'}
  <svg
    width={size}
    height={size}
    viewBox="0 0 16 16"
    fill="none"
    class={className}
    aria-hidden="true"
  >
    <rect x="1.5" y="3" width="13" height="10" rx="1.5" stroke="currentColor" stroke-width="1.2" />
    <path
      d="M4 7l2 1.5L4 10M8 10h4"
      stroke="currentColor"
      stroke-width="1.3"
      stroke-linecap="round"
      stroke-linejoin="round"
    />
  </svg>
{:else if name === 'memory'}
  <svg
    width={size}
    height={size}
    viewBox="0 0 16 16"
    fill="none"
    class={className}
    aria-hidden="true"
  >
    <path
      d="M3 3.5h6a2.5 2.5 0 010 5H3M3 3.5v9M3 8.5h6.5a2.5 2.5 0 010 5H3"
      stroke="currentColor"
      stroke-width="1.2"
      stroke-linejoin="round"
    />
  </svg>
{:else if name === 'settings'}
  <svg
    width={size}
    height={size}
    viewBox="0 0 16 16"
    fill="none"
    class={className}
    aria-hidden="true"
  >
    <circle cx="8" cy="8" r="2" stroke="currentColor" stroke-width="1.2" />
    <path
      d="M8 1v2M8 13v2M1 8h2M13 8h2M3 3l1.5 1.5M11.5 11.5L13 13M3 13l1.5-1.5M11.5 4.5L13 3"
      stroke="currentColor"
      stroke-width="1.2"
      stroke-linecap="round"
    />
  </svg>
{:else if name === 'search'}
  <svg
    width={size}
    height={size}
    viewBox="0 0 16 16"
    fill="none"
    class={className}
    aria-hidden="true"
  >
    <circle cx="7" cy="7" r="4.5" stroke="currentColor" stroke-width="1.4" />
    <path d="M10.5 10.5L14 14" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" />
  </svg>
{:else if name === 'plus'}
  <svg
    width={size}
    height={size}
    viewBox="0 0 16 16"
    fill="none"
    class={className}
    aria-hidden="true"
  >
    <path d="M8 3v10M3 8h10" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" />
  </svg>
{:else if name === 'command'}
  <svg
    width={size}
    height={size}
    viewBox="0 0 16 16"
    fill="none"
    class={className}
    aria-hidden="true"
  >
    <path
      d="M5 1.5a2 2 0 100 4h2v-2a2 2 0 00-2-2zM11 1.5a2 2 0 110 4H9v-2a2 2 0 012-2zM5 14.5a2 2 0 110-4h2v2a2 2 0 01-2 2zM11 14.5a2 2 0 100-4H9v2a2 2 0 002 2zM5 5.5h6v5H5z"
      stroke="currentColor"
      stroke-width="1.2"
      stroke-linejoin="round"
    />
  </svg>
{:else if name === 'sun'}
  <svg
    width={size}
    height={size}
    viewBox="0 0 16 16"
    fill="none"
    class={className}
    aria-hidden="true"
  >
    <circle cx="8" cy="8" r="3" stroke="currentColor" stroke-width="1.3" />
    <path
      d="M8 1v2M8 13v2M1 8h2M13 8h2M3 3l1.5 1.5M11.5 11.5L13 13M3 13l1.5-1.5M11.5 4.5L13 3"
      stroke="currentColor"
      stroke-width="1.3"
      stroke-linecap="round"
    />
  </svg>
{:else if name === 'moon'}
  <svg
    width={size}
    height={size}
    viewBox="0 0 16 16"
    fill="none"
    class={className}
    aria-hidden="true"
  >
    <path
      d="M13.5 9.5A5.5 5.5 0 016.5 2.5a5.5 5.5 0 107 7z"
      stroke="currentColor"
      stroke-width="1.3"
      stroke-linejoin="round"
    />
  </svg>
{/if}
