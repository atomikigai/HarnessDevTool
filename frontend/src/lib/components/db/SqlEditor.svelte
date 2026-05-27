<!--
  Thin wrapper around CodeMirror 6 with the SQL language. Two-way binds
  `value` and exposes a `run` callback wired to Cmd/Ctrl+Enter.
-->
<script lang="ts">
  import { onDestroy, onMount } from 'svelte';

  interface Props {
    value: string;
    onChange?: (v: string) => void;
    onRun?: () => void;
    class?: string;
  }

  let { value, onChange, onRun, class: className = '' }: Props = $props();

  let host: HTMLDivElement;
  // Keep ref to view so we can update when value changes externally.
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let view: any = null;

  onMount(async () => {
    const { EditorState } = await import('@codemirror/state');
    const { EditorView, keymap, lineNumbers, highlightActiveLine } =
      await import('@codemirror/view');
    const { defaultKeymap, history, historyKeymap, indentWithTab } =
      await import('@codemirror/commands');
    const { sql } = await import('@codemirror/lang-sql');
    const { autocompletion, completionKeymap } = await import('@codemirror/autocomplete');

    const runKey = {
      key: 'Mod-Enter',
      run: () => {
        onRun?.();
        return true;
      }
    };

    const state = EditorState.create({
      doc: value,
      extensions: [
        lineNumbers(),
        history(),
        highlightActiveLine(),
        autocompletion(),
        sql(),
        keymap.of([runKey, indentWithTab, ...defaultKeymap, ...historyKeymap, ...completionKeymap]),
        EditorView.theme({
          '&': { height: '100%', fontSize: '13px' },
          '.cm-scroller': { fontFamily: 'var(--font-mono)' },
          '.cm-content': { padding: '12px 0' },
          '.cm-gutters': {
            background: 'var(--surface-titlebar)',
            color: 'var(--fg-muted)',
            border: 'none'
          },
          '.cm-activeLine': { background: 'var(--accent-soft)' }
        }),
        EditorView.updateListener.of((u) => {
          if (u.docChanged) onChange?.(u.state.doc.toString());
        })
      ]
    });

    view = new EditorView({ state, parent: host });
  });

  // If parent updates `value` (tab switch), refresh editor.
  $effect(() => {
    if (view && view.state.doc.toString() !== value) {
      view.dispatch({
        changes: { from: 0, to: view.state.doc.length, insert: value }
      });
    }
  });

  onDestroy(() => view?.destroy?.());
</script>

<div bind:this={host} class={`h-full w-full overflow-hidden ${className}`}></div>
