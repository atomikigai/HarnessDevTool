<script lang="ts">
  import { onDestroy, onMount } from 'svelte';

  interface Props {
    value: string;
    nullable: boolean;
    onCommit: (parsed: unknown, raw: string) => void;
    onCancel: () => void;
    onParseError?: (error: string | null) => void;
  }

  let { value, nullable, onCommit, onCancel, onParseError }: Props = $props();

  let host = $state<HTMLDivElement | null>(null);
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let view: any = null;
  let text = $state('');
  let parseError = $state<string | null>(null);
  let plainText = $state(false);

  function validate(raw: string) {
    text = raw;
    if (nullable && raw.trim() === '') {
      parseError = null;
      onParseError?.(null);
      return;
    }
    try {
      JSON.parse(raw);
      parseError = null;
      onParseError?.(null);
    } catch (err) {
      parseError = err instanceof Error ? err.message : String(err);
      onParseError?.(parseError);
    }
  }

  function save() {
    if (parseError) return;
    if (nullable && text.trim() === '') {
      onCommit(null, '');
      return;
    }
    onCommit(JSON.parse(text), text);
  }

  onMount(async () => {
    text = value;
    validate(value);
    if (!host) return;

    const { EditorState } = await import('@codemirror/state');
    const { EditorView, keymap } = await import('@codemirror/view');
    const { defaultKeymap } = await import('@codemirror/commands');
    const { json } = await import('@codemirror/lang-json');

    const state = EditorState.create({
      doc: text,
      extensions: [
        json(),
        keymap.of(defaultKeymap),
        EditorView.lineWrapping,
        EditorView.theme({
          '&': {
            minHeight: '92px',
            maxHeight: '220px',
            fontSize: '12px',
            background: 'var(--surface-titlebar)',
            color: 'var(--fg-default)'
          },
          '.cm-scroller': { fontFamily: 'var(--font-mono)', overflow: 'auto' },
          '.cm-content': { padding: '8px 0' },
          '.cm-line': { padding: '0 8px' }
        }),
        EditorView.updateListener.of((u) => {
          if (u.docChanged) validate(u.state.doc.toString());
        })
      ]
    });

    view = new EditorView({ state, parent: host });
  });

  $effect(() => {
    if (view && view.state.doc.toString() !== text) {
      view.dispatch({
        changes: { from: 0, to: view.state.doc.length, insert: text }
      });
    }
  });

  onDestroy(() => view?.destroy?.());
</script>

<div
  class="json-cell-editor flex min-w-[260px] flex-col gap-2 rounded-md border p-2"
  style="background: var(--surface-panel); border-color: {parseError
    ? 'var(--dot-danger)'
    : 'var(--border-subtle)'};"
>
  <div class="flex items-center justify-between gap-2">
    <button
      type="button"
      class="rounded px-2 py-1 text-[11px]"
      style="background: var(--surface-titlebar); color: var(--fg-muted); border: 1px solid var(--border-subtle);"
      onclick={() => (plainText = !plainText)}
    >
      {plainText ? 'Use JSON editor' : 'Edit as text'}
    </button>
    <div class="flex items-center gap-1.5">
      <button
        type="button"
        class="rounded px-2 py-1 text-[11px]"
        style="background: transparent; color: var(--fg-muted); border: 1px solid var(--border-subtle);"
        onclick={onCancel}
      >
        Cancel
      </button>
      <button
        type="button"
        class="rounded px-2 py-1 text-[11px] disabled:cursor-not-allowed disabled:opacity-50"
        style="background: var(--accent); color: var(--surface-window); border: 1px solid var(--accent);"
        disabled={parseError !== null}
        onclick={save}
      >
        Save
      </button>
    </div>
  </div>

  {#if plainText}
    <textarea
      rows="5"
      class="w-full resize-y rounded border px-2 py-1.5 font-mono text-xs outline-none"
      style="background: var(--surface-titlebar); border-color: var(--border-subtle); color: var(--fg-default);"
      value={text}
      oninput={(e) => validate((e.currentTarget as HTMLTextAreaElement).value)}
    ></textarea>
  {/if}
  <div
    bind:this={host}
    class:hidden={plainText}
    class="overflow-hidden rounded border"
    style="border-color: var(--border-subtle);"
  ></div>

  {#if parseError}
    <p class="text-[11px]" style="color: var(--dot-danger);">{parseError}</p>
  {/if}
</div>
