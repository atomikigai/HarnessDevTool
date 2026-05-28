<!--
  In-place right-side panel for insert/update/duplicate of a row.
  Sibling flex column of the main grid section — when `open` it takes
  its width slot and the main area auto-shrinks; when closed it
  renders nothing (parent gets full width back).
  Form-field types are inferred from data_type; backend remains the
  source of truth for coercion.
-->
<script lang="ts">
  import { Button } from '$lib/components/ui/button';
  import { Label } from '$lib/components/ui/label';
  import { Input } from '$lib/components/ui/input';
  import { Loader2, X } from '$lib/icons';
  import { dbApi, type TableMeta } from '$lib/api/db';
  import { ApiError } from '$lib/api/client';
  import { toast } from 'svelte-sonner';
  import JsonCellEditor from './JsonCellEditor.svelte';

  type Mode = 'insert' | 'update' | 'duplicate';

  interface Props {
    open: boolean;
    mode: Mode;
    connectionId: string;
    schema?: string;
    database?: string;
    table: TableMeta;
    initialValues?: Record<string, unknown>;
    onSaved?: () => void;
  }

  let {
    open = $bindable(false),
    mode,
    connectionId,
    schema,
    database,
    table,
    initialValues,
    onSaved
  }: Props = $props();

  let values = $state<Record<string, unknown>>({});
  let submitting = $state(false);
  let error = $state<string | null>(null);
  let errors = $state<Record<string, string>>({});

  $effect(() => {
    if (open) {
      const base: Record<string, unknown> = {};
      for (const col of table.columns) {
        const v = initialValues?.[col.name];
        // For duplicate, drop PK values so user can re-supply.
        if (mode === 'duplicate' && col.pk) {
          base[col.name] = null;
        } else {
          base[col.name] = v ?? null;
        }
      }
      values = base;
      error = null;
      errors = {};
      submitting = false;
    }
  });

  // ESC to close while open.
  $effect(() => {
    if (!open) return;
    function onKey(e: KeyboardEvent) {
      if (e.key === 'Escape') {
        e.preventDefault();
        open = false;
      }
    }
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  });

  function fieldKind(
    col: TableMeta['columns'][number]
  ): 'bool' | 'date' | 'datetime' | 'enum' | 'json' | 'number' | 'text' {
    if (col.kind?.kind === 'Enum') return 'enum';
    const t = col.data_type.toLowerCase();
    if (t.includes('bool')) return 'bool';
    if (t.includes('timestamp') || t.includes('datetime')) return 'datetime';
    if (t === 'date') return 'date';
    if (t.includes('json')) return 'json';
    if (
      t.includes('int') ||
      t.includes('float') ||
      t.includes('numeric') ||
      t.includes('decimal') ||
      t.includes('double') ||
      t.includes('real')
    )
      return 'number';
    return 'text';
  }

  function setValue(col: string, v: unknown) {
    values = { ...values, [col]: v };
  }

  function setFieldError(col: string, message: string | null) {
    const next = { ...errors };
    if (message) next[col] = message;
    else delete next[col];
    errors = next;
  }

  function jsonText(v: unknown): string {
    if (v == null) return '';
    return typeof v === 'string' ? v : JSON.stringify(v, null, 2);
  }

  const title = $derived(
    mode === 'insert' ? 'Insert row' : mode === 'update' ? 'Edit row' : 'Duplicate row'
  );
  const primaryLabel = $derived(
    mode === 'update' ? 'Save' : mode === 'duplicate' ? 'Insert copy' : 'Insert'
  );
  const hasFieldErrors = $derived(Object.keys(errors).length > 0);

  async function onSubmit(ev: SubmitEvent) {
    ev.preventDefault();
    if (submitting || hasFieldErrors) return;
    submitting = true;
    error = null;
    try {
      const payload = {
        database,
        schema,
        values
      };
      if (mode === 'insert' || mode === 'duplicate') {
        if (mode === 'duplicate') {
          await dbApi.rows.duplicate(connectionId, table.name, payload);
        } else {
          await dbApi.rows.insert(connectionId, table.name, payload);
        }
      } else {
        // update — need PK
        const pk: Record<string, unknown> = {};
        for (const col of table.columns) {
          if (col.pk) pk[col.name] = initialValues?.[col.name];
        }
        await dbApi.rows.update(connectionId, table.name, { ...payload, pk });
      }
      open = false;
      onSaved?.();
      toast.success(mode === 'update' ? 'Row updated' : 'Row inserted');
    } catch (err) {
      error =
        err instanceof ApiError
          ? `Backend ${err.status}: ${err.message}`
          : err instanceof Error
            ? err.message
            : String(err);
      toast.error(error);
    } finally {
      submitting = false;
    }
  }
</script>

{#if open}
  <aside
    class="flex h-full w-full shrink-0 flex-col sm:w-[480px]"
    style="background: var(--surface-panel); border-left: 1px solid var(--border-subtle);"
    aria-label={title}
  >
    <!-- Header (sticky) -->
    <header
      class="flex items-start justify-between gap-3 px-4 py-3"
      style="background: var(--surface-titlebar); border-bottom: 1px solid var(--border-subtle);"
    >
      <div class="min-w-0">
        <h2 class="truncate text-sm font-semibold" style="color: var(--fg-default);">{title}</h2>
        <p class="mt-0.5 truncate font-mono text-xs" style="color: var(--fg-muted);">
          {table.name} · {table.columns.length} column{table.columns.length === 1 ? '' : 's'}
        </p>
      </div>
      <button
        type="button"
        class="rounded-md p-1 transition-colors hover:bg-[var(--surface-hover,rgba(255,255,255,0.06))]"
        aria-label="Close"
        onclick={() => (open = false)}
        style="color: var(--fg-muted);"
      >
        <X class="h-4 w-4" />
      </button>
    </header>

    <form class="flex min-h-0 flex-1 flex-col" onsubmit={onSubmit}>
      <!-- Body (scrollable) -->
      <div class="flex-1 overflow-y-auto px-4 py-4">
        <div class="flex flex-col gap-3">
          {#each table.columns as col (col.name)}
            {@const kind = fieldKind(col)}
            {@const v = values[col.name]}
            <div class="flex flex-col gap-1">
              <div class="flex items-center gap-2">
                <Label for={`f-${col.name}`} class="flex items-center gap-1.5">
                  {#if col.pk}
                    <span style="color: var(--accent);" title="primary key">🔑</span>
                  {/if}
                  <span class="font-mono text-xs">{col.name}</span>
                </Label>
                <span
                  class="rounded px-1.5 py-0.5 font-mono text-[10px]"
                  style="background: var(--surface-titlebar); color: var(--fg-muted); border: 1px solid var(--border-subtle);"
                >
                  {col.data_type}
                </span>
              </div>

              {#if kind === 'bool'}
                <label class="inline-flex items-center gap-2 text-sm">
                  <input
                    id={`f-${col.name}`}
                    type="checkbox"
                    checked={v === true}
                    onchange={(e) =>
                      setValue(col.name, (e.currentTarget as HTMLInputElement).checked)}
                  />
                  <span style="color: var(--fg-muted);">{v ? 'true' : 'false'}</span>
                </label>
              {:else if kind === 'enum' && col.kind?.kind === 'Enum'}
                <select
                  id={`f-${col.name}`}
                  value={v == null ? '' : String(v)}
                  onchange={(e) => {
                    const raw = (e.currentTarget as HTMLSelectElement).value;
                    setValue(col.name, raw === '' && col.nullable ? null : raw);
                  }}
                  disabled={mode === 'update' && col.pk}
                  class="h-9 w-full rounded-md border px-3 text-sm outline-none disabled:cursor-not-allowed disabled:opacity-50"
                  style="border-color: var(--border-input); background: var(--surface-titlebar); color: var(--fg-default);"
                >
                  {#if col.nullable}<option value=""></option>{/if}
                  {#each col.kind.variants as variant (variant)}
                    <option value={variant}>{variant}</option>
                  {/each}
                </select>
              {:else if kind === 'json'}
                <JsonCellEditor
                  value={jsonText(v)}
                  nullable={col.nullable}
                  onCommit={(_parsed, raw) => {
                    setValue(col.name, raw.trim() === '' && col.nullable ? null : raw);
                    setFieldError(col.name, null);
                  }}
                  onCancel={() => setFieldError(col.name, null)}
                  onParseError={(msg) => setFieldError(col.name, msg)}
                />
              {:else if kind === 'date'}
                <Input
                  id={`f-${col.name}`}
                  type="date"
                  value={v == null ? '' : String(v)}
                  oninput={(e) => setValue(col.name, (e.currentTarget as HTMLInputElement).value)}
                />
              {:else if kind === 'datetime'}
                <Input
                  id={`f-${col.name}`}
                  type="datetime-local"
                  value={v == null ? '' : String(v).replace(' ', 'T').slice(0, 16)}
                  oninput={(e) => setValue(col.name, (e.currentTarget as HTMLInputElement).value)}
                />
              {:else if kind === 'number'}
                <Input
                  id={`f-${col.name}`}
                  type="number"
                  value={v == null ? '' : (v as number | string)}
                  oninput={(e) => {
                    const raw = (e.currentTarget as HTMLInputElement).value;
                    setValue(col.name, raw === '' ? null : Number(raw));
                  }}
                  disabled={mode === 'update' && col.pk}
                />
              {:else}
                <Input
                  id={`f-${col.name}`}
                  value={v == null ? '' : (v as string)}
                  oninput={(e) => setValue(col.name, (e.currentTarget as HTMLInputElement).value)}
                  disabled={mode === 'update' && col.pk}
                />
              {/if}

              {#if col.nullable || (mode === 'update' && col.pk)}
                <p class="text-[10px]" style="color: var(--fg-muted);">
                  {#if mode === 'update' && col.pk}primary key — not editable{:else if col.nullable}nullable{/if}
                </p>
              {/if}
              {#if errors[col.name]}
                <p class="text-[11px]" style="color: var(--dot-danger);">{errors[col.name]}</p>
              {/if}
            </div>
          {/each}

          {#if error}
            <p
              class="rounded-md border px-3 py-2 text-xs"
              style="border-color: color-mix(in srgb, var(--dot-danger) 35%, transparent); background: color-mix(in srgb, var(--dot-danger) 10%, transparent); color: var(--dot-danger);"
            >
              {error}
            </p>
          {/if}
        </div>
      </div>

      <!-- Footer (sticky) -->
      <footer
        class="flex items-center justify-end gap-2 px-4 py-3"
        style="background: var(--surface-titlebar); border-top: 1px solid var(--border-subtle);"
      >
        <Button
          type="button"
          variant="outline"
          onclick={() => (open = false)}
          disabled={submitting}
        >
          Cancel
        </Button>
        <Button type="submit" disabled={submitting || hasFieldErrors}>
          {#if submitting}<Loader2 class="h-4 w-4 animate-spin" />{/if}
          {primaryLabel}
        </Button>
      </footer>
    </form>
  </aside>
{/if}
