<!--
  Add/Edit connection dialog.

  Engine-aware field gating: sqlite collapses to {name, file path};
  postgres exposes ssl_mode; mysql/postgres expose host/port/user/pass.
  Inline "Test connection" hits the backend without saving.
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
  import { Input } from '$lib/components/ui/input';
  import { Label } from '$lib/components/ui/label';
  import {
    dbApi,
    defaultPort,
    needsHost,
    type Connection,
    type ConnectionInput,
    type DbEngine,
    type SslMode
  } from '$lib/api/db';
  import { parseParamsJson, validateConnection } from '$lib/api/schemas/db';
  import { ApiError } from '$lib/api/client';
  import { Loader2 } from '$lib/icons';
  import { toast } from 'svelte-sonner';

  interface Props {
    open: boolean;
    existing?: Connection | null;
    onSaved?: (conn: Connection) => void;
  }

  let { open = $bindable(false), existing = null, onSaved }: Props = $props();

  // Form state
  let name = $state('');
  let engine = $state<DbEngine>('postgres');
  let host = $state('');
  let port = $state<string>('');
  let database = $state('');
  let username = $state('');
  let password = $state('');
  let sslMode = $state<SslMode>('prefer');
  let paramsText = $state('');
  let showAdvanced = $state(false);

  let submitting = $state(false);
  let testing = $state(false);
  let testResult = $state<string | null>(null);
  let testOk = $state<boolean | null>(null);
  let error = $state<string | null>(null);
  let fieldErrors = $state<Record<string, string>>({});

  // Reset when dialog opens with a different `existing`.
  $effect(() => {
    if (open) {
      if (existing) {
        name = existing.name;
        engine = existing.engine;
        host = existing.host ?? '';
        port = existing.port ? String(existing.port) : '';
        database = existing.database ?? '';
        username = existing.username ?? '';
        password = '';
        sslMode = (existing.ssl_mode as SslMode) ?? 'prefer';
        paramsText = existing.params ? JSON.stringify(existing.params, null, 2) : '';
      } else {
        name = '';
        engine = 'postgres';
        host = 'localhost';
        port = String(defaultPort('postgres'));
        database = '';
        username = '';
        password = '';
        sslMode = 'prefer';
        paramsText = '';
      }
      showAdvanced = false;
      testResult = null;
      testOk = null;
      error = null;
      fieldErrors = {};
    }
  });

  function onEngineChange(next: DbEngine) {
    engine = next;
    const p = defaultPort(next);
    port = p ? String(p) : '';
    if (next === 'sqlite') {
      host = '';
      username = '';
      password = '';
    } else if (!host) {
      host = 'localhost';
    }
  }

  /**
   * Build the candidate input from form state. Parses the advanced-JSON field
   * and surfaces a `params` field error if it's malformed.
   */
  function buildInput(): ConnectionInput | null {
    const paramsResult = parseParamsJson(paramsText);
    if (!paramsResult.ok) {
      fieldErrors = { ...fieldErrors, params: paramsResult.error };
      return null;
    }
    const body: ConnectionInput = {
      name: name.trim(),
      engine,
      database: database.trim()
    };
    if (needsHost(engine)) {
      body.host = host.trim();
      if (port.trim()) body.port = Number(port);
      if (username.trim()) body.username = username.trim();
      if (password) body.password = password;
    }
    if (engine === 'postgres') body.ssl_mode = sslMode;
    if (paramsResult.value) body.params = paramsResult.value;
    return body;
  }

  /**
   * Validate via valibot. Returns the validated body when ok, null otherwise.
   * Field errors are mirrored to local state for inline rendering.
   */
  function validateInput(): ConnectionInput | null {
    const candidate = buildInput();
    if (!candidate) return null;
    const result = validateConnection(candidate);
    fieldErrors = result.fieldErrors;
    return result.ok ? candidate : null;
  }

  async function onTest() {
    const body = validateInput();
    if (!body) return;
    testing = true;
    testResult = null;
    testOk = null;
    try {
      const res = await dbApi.test(body);
      testOk = res.data.ok;
      if (res.data.ok) {
        testResult = `Connected · ${res.data.latency_ms ?? '?'}ms${
          res.data.server_version ? ` · ${res.data.server_version}` : ''
        }`;
      } else {
        testResult = res.data.error ?? 'Test failed';
      }
    } catch (err) {
      testOk = false;
      testResult =
        err instanceof ApiError
          ? `Backend ${err.status}: ${err.message}`
          : err instanceof Error
            ? err.message
            : String(err);
    } finally {
      testing = false;
    }
  }

  async function onSubmit(ev: SubmitEvent) {
    ev.preventDefault();
    if (submitting) return;
    const body = validateInput();
    if (!body) return;

    submitting = true;
    error = null;
    try {
      const res = existing
        ? await dbApi.connections.update(existing.id, body)
        : await dbApi.connections.create(body);
      open = false;
      onSaved?.(res.data);
      toast.success(existing ? 'Connection updated' : 'Connection added');
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
  <DialogContent class="sm:max-w-xl">
    <DialogHeader>
      <DialogTitle>{existing ? 'Edit connection' : 'New connection'}</DialogTitle>
      <DialogDescription>
        Saved credentials are kept on the backend. Passwords are not returned by the API.
      </DialogDescription>
    </DialogHeader>

    <form class="mt-4 flex flex-col gap-4" onsubmit={onSubmit}>
      <!-- Engine selector — segmented control -->
      <div class="flex flex-col gap-2">
        <Label>Engine</Label>
        <div class="flex gap-2" role="radiogroup">
          {#each ['sqlite', 'postgres', 'mysql'] as const as opt (opt)}
            <button
              type="button"
              role="radio"
              aria-checked={engine === opt}
              class="flex-1 rounded-md border px-3 py-2 text-sm font-medium capitalize transition-colors {engine ===
              opt
                ? 'border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)]'
                : 'border-[var(--border-input)] bg-[var(--surface-titlebar)] text-[var(--fg-muted)] hover:text-[var(--fg-default)]'}"
              onclick={() => onEngineChange(opt)}
            >
              {opt}
            </button>
          {/each}
        </div>
      </div>

      <!-- Name -->
      <div class="flex flex-col gap-1.5">
        <Label for="db-name">Name</Label>
        <Input id="db-name" bind:value={name} placeholder="e.g. local sara" autocomplete="off" />
        {#if fieldErrors.name}
          <p class="text-[11px] text-[var(--dot-danger)]">{fieldErrors.name}</p>
        {/if}
      </div>

      <!-- Database / file path -->
      <div class="flex flex-col gap-1.5">
        <Label for="db-database">
          {engine === 'sqlite' ? 'File path' : 'Database'}
        </Label>
        <Input
          id="db-database"
          bind:value={database}
          placeholder={engine === 'sqlite' ? '/path/to/db.sqlite' : 'mydb'}
          autocomplete="off"
        />
        {#if fieldErrors.database}
          <p class="text-[11px] text-[var(--dot-danger)]">{fieldErrors.database}</p>
        {/if}
      </div>

      {#if needsHost(engine)}
        <div class="grid grid-cols-[1fr_120px] gap-3">
          <div class="flex flex-col gap-1.5">
            <Label for="db-host">Host</Label>
            <Input id="db-host" bind:value={host} placeholder="localhost" autocomplete="off" />
            {#if fieldErrors.host}
              <p class="text-[11px] text-[var(--dot-danger)]">{fieldErrors.host}</p>
            {/if}
          </div>
          <div class="flex flex-col gap-1.5">
            <Label for="db-port">Port</Label>
            <Input id="db-port" type="number" bind:value={port} />
          </div>
        </div>

        <div class="grid grid-cols-2 gap-3">
          <div class="flex flex-col gap-1.5">
            <Label for="db-user">Username</Label>
            <Input id="db-user" bind:value={username} autocomplete="off" />
          </div>
          <div class="flex flex-col gap-1.5">
            <Label for="db-pass">Password</Label>
            <Input
              id="db-pass"
              type="password"
              bind:value={password}
              placeholder={existing ? '(unchanged)' : '••••••••'}
              autocomplete="new-password"
            />
          </div>
        </div>

        {#if engine === 'postgres'}
          <div class="flex flex-col gap-1.5">
            <Label for="db-ssl">SSL mode</Label>
            <select
              id="db-ssl"
              bind:value={sslMode}
              class="h-9 rounded-md border border-[var(--border-input)] bg-[var(--surface-titlebar)] px-3 text-sm text-[var(--fg-default)]"
            >
              {#each ['disable', 'prefer', 'require', 'verify-ca', 'verify-full'] as m (m)}
                <option value={m}>{m}</option>
              {/each}
            </select>
          </div>
        {/if}
      {/if}

      <!-- Advanced -->
      <details
        bind:open={showAdvanced}
        class="rounded-md border border-[var(--border-subtle)] px-3 py-2 text-sm"
      >
        <summary class="cursor-pointer text-[var(--fg-muted)]">Advanced parameters (JSON)</summary>
        <textarea
          bind:value={paramsText}
          rows="4"
          placeholder={'{ "application_name": "harness" }'}
          class="mt-2 w-full rounded-md border border-[var(--border-input)] bg-[var(--surface-titlebar)] px-3 py-2 font-mono text-xs text-[var(--fg-default)] outline-none focus:border-[var(--accent)]"
        ></textarea>
        {#if fieldErrors.params}
          <p class="text-[11px] text-[var(--dot-danger)]">{fieldErrors.params}</p>
        {/if}
      </details>

      <!-- Test result + form error -->
      {#if testResult}
        <p
          class="rounded-md border px-3 py-2 text-xs"
          style={testOk
            ? 'border-color: color-mix(in srgb, var(--dot-success) 35%, transparent); background: color-mix(in srgb, var(--dot-success) 10%, transparent); color: var(--dot-success);'
            : 'border-color: color-mix(in srgb, var(--dot-danger) 35%, transparent); background: color-mix(in srgb, var(--dot-danger) 10%, transparent); color: var(--dot-danger);'}
        >
          {testResult}
        </p>
      {/if}
      {#if error}
        <p
          class="rounded-md border px-3 py-2 text-xs"
          style="border-color: color-mix(in srgb, var(--dot-danger) 35%, transparent); background: color-mix(in srgb, var(--dot-danger) 10%, transparent); color: var(--dot-danger);"
        >
          {error}
        </p>
      {/if}

      <DialogFooter class="flex items-center gap-2 sm:justify-between">
        <Button type="button" variant="outline" onclick={onTest} disabled={testing || submitting}>
          {#if testing}<Loader2 class="h-4 w-4 animate-spin" />{/if}
          Test connection
        </Button>
        <div class="flex gap-2">
          <Button
            type="button"
            variant="ghost"
            onclick={() => (open = false)}
            disabled={submitting}
          >
            Cancel
          </Button>
          <Button type="submit" disabled={submitting}>
            {#if submitting}<Loader2 class="h-4 w-4 animate-spin" />{/if}
            {existing ? 'Save' : 'Create'}
          </Button>
        </div>
      </DialogFooter>
    </form>
  </DialogContent>
</Dialog>
