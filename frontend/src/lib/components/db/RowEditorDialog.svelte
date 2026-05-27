<!--
  Generic insert/update/duplicate form generated from a table's columns.
  Renders date/datetime pickers, checkboxes, textarea for JSON, plain
  input otherwise. Type validation is best-effort client-side; the backend
  is the source of truth for coercion.
-->
<script lang="ts">
  import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogDescription,
    DialogFooter
  } from '$lib/components/ui/dialog';
  import { Button } from '$lib/components/ui/button';
  import { Label } from '$lib/components/ui/label';
  import { Input } from '$lib/components/ui/input';
  import { Loader2 } from '$lib/icons';
  import { dbApi, type TableMeta } from '$lib/api/db';
  import { ApiError } from '$lib/api/client';
  import { toast } from 'svelte-sonner';

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
      submitting = false;
    }
  });

  function fieldKind(dt: string): 'bool' | 'date' | 'datetime' | 'json' | 'number' | 'text' {
    const t = dt.toLowerCase();
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

  async function onSubmit(ev: SubmitEvent) {
    ev.preventDefault();
    if (submitting) return;
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

<Dialog bind:open>
  <DialogContent class="sm:max-w-2xl">
    <DialogHeader>
      <DialogTitle>
        {mode === 'insert' ? 'Insert row' : mode === 'update' ? 'Edit row' : 'Duplicate row'} —
        {table.name}
      </DialogTitle>
      <DialogDescription>
        {table.columns.length} column{table.columns.length === 1 ? '' : 's'}.
      </DialogDescription>
    </DialogHeader>

    <form class="mt-3 flex max-h-[60vh] flex-col gap-3 overflow-y-auto pr-1" onsubmit={onSubmit}>
      {#each table.columns as col (col.name)}
        {@const kind = fieldKind(col.data_type)}
        {@const v = values[col.name]}
        <div class="grid grid-cols-[180px_1fr] items-start gap-3">
          <div class="pt-1.5">
            <Label for={`f-${col.name}`} class="flex items-center gap-1.5">
              {#if col.pk}
                <span style="color: var(--accent);" title="primary key">🔑</span>
              {/if}
              <span class="font-mono">{col.name}</span>
            </Label>
            <p class="mt-0.5 text-[10px]" style="color: var(--fg-muted);">
              {col.data_type}{col.nullable ? ' · nullable' : ''}
            </p>
          </div>
          <div>
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
            {:else if kind === 'json'}
              <textarea
                id={`f-${col.name}`}
                rows="4"
                value={v == null ? '' : typeof v === 'string' ? v : JSON.stringify(v, null, 2)}
                oninput={(e) => setValue(col.name, (e.currentTarget as HTMLTextAreaElement).value)}
                class="w-full rounded-md border px-3 py-2 font-mono text-xs outline-none"
                style="border-color: var(--border-input); background: var(--surface-titlebar); color: var(--fg-default);"
              ></textarea>
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
          </div>
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

      <DialogFooter>
        <Button
          type="button"
          variant="outline"
          onclick={() => (open = false)}
          disabled={submitting}
        >
          Cancel
        </Button>
        <Button type="submit" disabled={submitting}>
          {#if submitting}<Loader2 class="h-4 w-4 animate-spin" />{/if}
          {mode === 'update' ? 'Save' : 'Insert'}
        </Button>
      </DialogFooter>
    </form>
  </DialogContent>
</Dialog>
