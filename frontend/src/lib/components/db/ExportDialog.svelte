<!--
  Export dialog — drives the F4 `/api/db/connections/:id/export` endpoint.

  Two modes (driven by `target`):
    - `table`   single table; user picks format + scope + column subset
    - `schema`  whole schema; CSV disabled (backend rejects with 400)

  Browser download is triggered via a blob URL + anchor with the filename
  returned in `Content-Disposition`. Errors render inline so the dialog
  stays open (e.g., backend "csv export not supported for schema targets").
-->
<script lang="ts" module>
  import type { Column as _Column, TableMeta as _TableMeta } from '$lib/api/db';

  /** Public, dialog-friendly shape for the export target. */
  export type ExportDialogTarget =
    | { kind: 'table'; schema?: string; name: string; columns: _Column[] }
    | { kind: 'schema'; name: string; tables: _TableMeta[] };
</script>

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
  import { Loader2, Download, Key, FileJson, FileCode2, FileText } from '$lib/icons';
  import { toast } from 'svelte-sonner';
  import {
    dbApi,
    type Column,
    type ExportFormat,
    type ExportScope,
    type ExportRequest,
    type ExportTarget
  } from '$lib/api/db';

  interface Props {
    open: boolean;
    connectionId: string;
    database?: string | null;
    target: ExportDialogTarget | null;
  }

  let {
    open = $bindable(false),
    connectionId,
    database = null,
    target
  }: Props = $props();

  // ── Local form state ───────────────────────────────────────────────────────
  let format = $state<ExportFormat>('Json');
  let scope = $state<ExportScope>('SchemaAndData');
  let selectedCols = $state<Record<string, boolean>>({});
  let submitting = $state(false);
  let error = $state<string | null>(null);

  const isSchema = $derived(target?.kind === 'schema');
  const isTable = $derived(target?.kind === 'table');
  const tableColumns = $derived<Column[]>(
    target?.kind === 'table' ? target.columns : []
  );

  // ── Reset whenever the dialog re-opens or the target changes ───────────────
  let lastKey = '';
  $effect(() => {
    if (!open || !target) return;
    const key =
      target.kind === 'table'
        ? `t:${target.schema ?? ''}.${target.name}`
        : `s:${target.name}`;
    if (key === lastKey) return;
    lastKey = key;
    error = null;
    submitting = false;
    format = 'Json';
    scope = 'SchemaAndData';
    if (target.kind === 'table') {
      const next: Record<string, boolean> = {};
      for (const c of target.columns) next[c.name] = true;
      selectedCols = next;
    } else {
      selectedCols = {};
    }
  });
  $effect(() => {
    if (!open) lastKey = '';
  });

  // CSV constraints: schema → invalid; on table CSV scope is forced to DataOnly.
  $effect(() => {
    if (format === 'Csv') {
      if (scope !== 'DataOnly') scope = 'DataOnly';
    }
  });
  $effect(() => {
    if (isSchema && format === 'Csv') format = 'Json';
  });

  const selectedColCount = $derived(
    Object.values(selectedCols).filter(Boolean).length
  );
  const canSubmit = $derived.by(() => {
    if (!target || submitting) return false;
    if (isTable && selectedColCount === 0) return false;
    if (isSchema && format === 'Csv') return false;
    return true;
  });

  function selectAllCols(v: boolean) {
    if (target?.kind !== 'table') return;
    const next: Record<string, boolean> = {};
    for (const c of target.columns) next[c.name] = v;
    selectedCols = next;
  }

  function toggleCol(name: string) {
    selectedCols = { ...selectedCols, [name]: !selectedCols[name] };
  }

  // ── Submit ────────────────────────────────────────────────────────────────
  async function onSubmit() {
    if (!target || !canSubmit) return;
    submitting = true;
    error = null;

    let exportTarget: ExportTarget;
    if (target.kind === 'table') {
      const all = target.columns.map((c) => c.name);
      const picked = all.filter((n) => selectedCols[n]);
      // Omit `columns` when everything is selected — keeps payload tidy &
      // lets the backend treat it as "default = all".
      const subset = picked.length === all.length ? undefined : picked;
      exportTarget = {
        type: 'Table',
        schema: target.schema,
        name: target.name,
        columns: subset
      };
    } else {
      exportTarget = { type: 'Schema', name: target.name };
    }

    const body: ExportRequest = {
      database: database ?? undefined,
      target: exportTarget,
      format,
      scope
    };

    try {
      const { blob, filename } = await dbApi.export(connectionId, body);
      triggerDownload(blob, filename);
      toast.success(`Exported ${filename}`);
      open = false;
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      error = msg;
    } finally {
      submitting = false;
    }
  }

  function triggerDownload(blob: Blob, filename: string) {
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = filename;
    document.body.appendChild(a);
    a.click();
    a.remove();
    // Give the browser a moment to start the download before revoking.
    setTimeout(() => URL.revokeObjectURL(url), 1_000);
  }

  // ── UI helpers ────────────────────────────────────────────────────────────
  const FORMATS: { value: ExportFormat; label: string; icon: typeof FileJson }[] = [
    { value: 'Json', label: 'JSON', icon: FileJson },
    { value: 'SqlInsert', label: 'SQL', icon: FileCode2 },
    { value: 'Csv', label: 'CSV', icon: FileText }
  ];
  const SCOPES: { value: ExportScope; label: string }[] = [
    { value: 'SchemaOnly', label: 'Schema only' },
    { value: 'SchemaAndData', label: 'Schema + data' },
    { value: 'DataOnly', label: 'Data only' }
  ];

  function isScopeDisabled(s: ExportScope): boolean {
    if (format === 'Csv' && s !== 'DataOnly') return true;
    return false;
  }
  function isFormatDisabled(f: ExportFormat): boolean {
    if (isSchema && f === 'Csv') return true;
    return false;
  }

  const titleText = $derived.by(() => {
    if (!target) return 'Export';
    return target.kind === 'table'
      ? `Export table ${target.schema ? `${target.schema}.` : ''}${target.name}`
      : `Export schema ${target.name}`;
  });
</script>

<Dialog bind:open>
  <DialogContent class="sm:max-w-xl">
    <DialogHeader>
      <DialogTitle>{titleText}</DialogTitle>
      <DialogDescription>
        Download as JSON, SQL inserts, or CSV. The file is delivered as a download.
      </DialogDescription>
    </DialogHeader>

    {#if target}
      <div class="flex flex-col gap-5 py-2">
        <!-- Format -->
        <div class="flex flex-col gap-2">
          <Label class="text-[11px] uppercase tracking-wider">Format</Label>
          <div
            class="inline-flex rounded-md border p-0.5"
            style="border-color: var(--border-input); background: var(--surface-titlebar); width: fit-content;"
            role="radiogroup"
            aria-label="Export format"
          >
            {#each FORMATS as f (f.value)}
              {@const sel = format === f.value}
              {@const dis = isFormatDisabled(f.value)}
              <button
                type="button"
                role="radio"
                aria-checked={sel}
                disabled={dis}
                onclick={() => (format = f.value)}
                class="inline-flex h-7 items-center gap-1.5 rounded px-3 text-[12px] font-medium transition-colors disabled:cursor-not-allowed disabled:opacity-40"
                style={sel
                  ? 'background: var(--surface-window); color: var(--accent); box-shadow: 0 0 0 1px var(--border-subtle);'
                  : 'background: transparent; color: var(--fg-muted);'}
              >
                <f.icon class="h-3.5 w-3.5" />
                {f.label}
              </button>
            {/each}
          </div>
          {#if isSchema}
            <p class="text-[11px]" style="color: var(--fg-muted);">
              CSV is unavailable for whole-schema exports (table only).
            </p>
          {/if}
        </div>

        <!-- Scope -->
        <div class="flex flex-col gap-2">
          <Label class="text-[11px] uppercase tracking-wider">Scope</Label>
          <div
            class="inline-flex rounded-md border p-0.5"
            style="border-color: var(--border-input); background: var(--surface-titlebar); width: fit-content;"
            role="radiogroup"
            aria-label="Export scope"
          >
            {#each SCOPES as s (s.value)}
              {@const sel = scope === s.value}
              {@const dis = isScopeDisabled(s.value)}
              <button
                type="button"
                role="radio"
                aria-checked={sel}
                disabled={dis}
                onclick={() => (scope = s.value)}
                class="inline-flex h-7 items-center rounded px-3 text-[12px] font-medium transition-colors disabled:cursor-not-allowed disabled:opacity-40"
                style={sel
                  ? 'background: var(--surface-window); color: var(--accent); box-shadow: 0 0 0 1px var(--border-subtle);'
                  : 'background: transparent; color: var(--fg-muted);'}
              >
                {s.label}
              </button>
            {/each}
          </div>
          {#if format === 'Csv'}
            <p class="text-[11px]" style="color: var(--fg-muted);">
              CSV exports rows only — scope is fixed to data.
            </p>
          {/if}
        </div>

        <!-- Columns (table only) -->
        {#if isTable}
          <div class="flex flex-col gap-2">
            <div class="flex items-center justify-between">
              <Label class="text-[11px] uppercase tracking-wider">
                Columns <span class="font-mono normal-case" style="color: var(--fg-muted);">
                  ({selectedColCount}/{tableColumns.length})
                </span>
              </Label>
              <div class="flex items-center gap-1.5">
                <Button
                  variant="ghost"
                  size="sm"
                  onclick={() => selectAllCols(true)}
                  type="button"
                >
                  All
                </Button>
                <Button
                  variant="ghost"
                  size="sm"
                  onclick={() => selectAllCols(false)}
                  type="button"
                >
                  None
                </Button>
              </div>
            </div>
            <div
              class="max-h-56 overflow-y-auto rounded-md border px-2 py-1.5"
              style="border-color: var(--border-input); background: var(--surface-titlebar);"
            >
              {#each tableColumns as col (col.name)}
                <label
                  class="flex cursor-pointer items-center gap-2 rounded px-2 py-1 text-[12px] hover:bg-[var(--accent-soft)]"
                >
                  <input
                    type="checkbox"
                    checked={!!selectedCols[col.name]}
                    onchange={() => toggleCol(col.name)}
                    class="h-3.5 w-3.5"
                  />
                  <span class="truncate" style="color: var(--fg-default);">{col.name}</span>
                  {#if col.pk}
                    <Key class="h-3 w-3" style="color: var(--accent);" />
                  {/if}
                  <span
                    class="ml-auto truncate font-mono text-[10px]"
                    style="color: var(--fg-muted);"
                  >
                    {col.data_type}
                  </span>
                </label>
              {/each}
            </div>
            {#if selectedColCount === 0}
              <p class="text-[11px]" style="color: var(--dot-danger);">
                Select at least one column.
              </p>
            {/if}
          </div>
        {/if}

        {#if error}
          <div
            class="rounded-md border px-3 py-2 text-[12px]"
            style="border-color: color-mix(in srgb, var(--dot-danger) 35%, transparent); background: color-mix(in srgb, var(--dot-danger) 10%, transparent); color: var(--dot-danger);"
            role="alert"
          >
            {error}
          </div>
        {/if}
      </div>
    {/if}

    <DialogFooter>
      <Button variant="outline" onclick={() => (open = false)} disabled={submitting}>
        Cancel
      </Button>
      <Button onclick={onSubmit} disabled={!canSubmit}>
        {#if submitting}
          <Loader2 class="h-3.5 w-3.5 animate-spin" />
        {:else}
          <Download class="h-3.5 w-3.5" />
        {/if}
        Export
      </Button>
    </DialogFooter>
  </DialogContent>
</Dialog>
