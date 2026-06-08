/**
 * F4 module-db — Svelte 5 runes-backed state for the DB module.
 *
 * Responsibilities:
 *   • cache the connections list (one fetch, refetch on mutation),
 *   • cache schema per (connection, database),
 *   • track open tabs per connection (SQL editors + table browsers) with
 *     their last result.
 *
 * Backend may not be live yet — every loader sets `error` and gracefully
 * yields empty state on failure so the UI can still render.
 */

import {
  dbApi,
  type Connection,
  type QueryResult,
  type SchemaTree,
  type TableMeta
} from '$lib/api/db';
import { ApiError, type SessionKind } from '$lib/api/client';

const ACTIVE_DB_KEY = 'harness.db.activeConnectionId';
const CONNECTIONS_SIDEBAR_COLLAPSED_KEY = 'harness.db.connectionsSidebarCollapsed';

/**
 * Comment-aware extraction of the first SQL keyword, uppercase. Mirrors
 * `module-db::query::leading_keyword` on the backend so the UI can decide
 * what to optimistically reflect (e.g. transaction state for the pin icon).
 */
function leadingSqlKeyword(sql: string): string {
  let s = sql.trimStart();
  while (true) {
    if (s.startsWith('--')) {
      const nl = s.indexOf('\n');
      if (nl === -1) return '';
      s = s.slice(nl + 1).trimStart();
      continue;
    }
    if (s.startsWith('/*')) {
      const end = s.indexOf('*/');
      if (end === -1) return '';
      s = s.slice(end + 2).trimStart();
      continue;
    }
    break;
  }
  const m = s.match(/^[A-Za-z_]+/);
  return m ? m[0].toUpperCase() : '';
}

export type DbTabKind = 'sql' | 'table';

export interface DbTab {
  id: string;
  kind: DbTabKind;
  title: string;
  // sql editor state
  sql?: string;
  // table browser state
  schema?: string;
  table?: string;
  tableMeta?: TableMeta;
  // shared
  database?: string;
  page: number;
  pageSize: number;
  loading: boolean;
  error: string | null;
  result: QueryResult | null;
  lastQueryId: string | null;
  /**
   * Whether this tab is pinned to a dedicated DB connection (Q13). Set by
   * the UI toggle, or automatically true after the backend auto-pinned on a
   * `BEGIN`. Visual-only: actual lease state lives on the backend.
   */
  pinned?: boolean;
}

export interface PendingRowEdit {
  pk: Record<string, unknown>;
  changes: Record<string, unknown>;
  original: Record<string, unknown>;
}

export interface PendingRowInsert {
  tempId: string;
  values: Record<string, unknown>;
  errors?: Record<string, string>;
}

/** Per-tab pending state: cell edits keyed by row index, plus inline inserts. */
export interface TabPending {
  edits: Record<number, PendingRowEdit>;
  inserts: PendingRowInsert[];
}

/** Per-workspace pending state, keyed by tabId. */
export type WorkspacePendingEdits = Record<string, TabPending>;

function emptyTabPending(): TabPending {
  return { edits: {}, inserts: [] };
}

function isTabBucketEmpty(b: TabPending): boolean {
  return Object.keys(b.edits).length === 0 && b.inserts.length === 0;
}

export interface DbAgentWorkspace {
  threadId: string | null;
  sessionId: string | null;
  kind: SessionKind;
  collapsed: boolean;
  fullscreen: boolean;
  size: number;
}

export interface ConnectionWorkspace {
  databases: string[];
  database: string | null;
  schema: SchemaTree | null;
  schemaLoading: boolean;
  schemaError: string | null;
  tabs: DbTab[];
  activeTabId: string | null;
  pendingEdits: WorkspacePendingEdits;
  schemaPanelCollapsed: boolean;
  tableSubTab: Record<string, 'data' | 'schema'>;
  agent: DbAgentWorkspace;
}

function emptyWorkspace(): ConnectionWorkspace {
  return {
    databases: [],
    database: null,
    schema: null,
    schemaLoading: false,
    schemaError: null,
    tabs: [],
    activeTabId: null,
    pendingEdits: {},
    schemaPanelCollapsed: false,
    tableSubTab: {},
    agent: {
      threadId: null,
      sessionId: null,
      kind: 'claude',
      collapsed: false,
      fullscreen: false,
      size: 30
    }
  };
}

function readActiveConnectionId(): string | null {
  if (typeof localStorage === 'undefined') return null;
  return localStorage.getItem(ACTIVE_DB_KEY);
}

function writeActiveConnectionId(id: string | null): void {
  if (typeof localStorage === 'undefined') return;
  if (id) localStorage.setItem(ACTIVE_DB_KEY, id);
  else localStorage.removeItem(ACTIVE_DB_KEY);
}

function readConnectionsSidebarCollapsed(): boolean {
  if (typeof localStorage === 'undefined') return false;
  return localStorage.getItem(CONNECTIONS_SIDEBAR_COLLAPSED_KEY) === 'true';
}

function writeConnectionsSidebarCollapsed(collapsed: boolean): void {
  if (typeof localStorage === 'undefined') return;
  localStorage.setItem(CONNECTIONS_SIDEBAR_COLLAPSED_KEY, String(collapsed));
}

let nextTabId = 1;

class DbStore {
  connections = $state<Connection[]>([]);
  loaded = $state(false);
  listLoading = $state(false);
  listError = $state<string | null>(null);
  activeConnectionId = $state<string | null>(readActiveConnectionId());
  connectionsSidebarCollapsed = $state(readConnectionsSidebarCollapsed());

  /** Keyed by connection id. */
  workspaces = $state<Record<string, ConnectionWorkspace>>({});

  workspace(connId: string): ConnectionWorkspace {
    return this.workspaces[connId] ?? emptyWorkspace();
  }

  #patchWorkspace(connId: string, patch: Partial<ConnectionWorkspace>) {
    const prev = this.workspaces[connId] ?? emptyWorkspace();
    this.workspaces = { ...this.workspaces, [connId]: { ...prev, ...patch } };
  }

  setActiveConnection(connId: string | null): void {
    this.activeConnectionId = connId;
    writeActiveConnectionId(connId);
  }

  setConnectionsSidebarCollapsed(collapsed: boolean): void {
    this.connectionsSidebarCollapsed = collapsed;
    writeConnectionsSidebarCollapsed(collapsed);
  }

  patchAgent(connId: string, patch: Partial<DbAgentWorkspace>): void {
    const ws = this.workspace(connId);
    this.#patchWorkspace(connId, { agent: { ...ws.agent, ...patch } });
  }

  setAgentSize(connId: string, size: number): void {
    this.patchAgent(connId, { size: Math.max(24, Math.min(60, Math.round(size))) });
  }

  resetAgent(connId: string): void {
    const current = this.workspace(connId).agent;
    this.#patchWorkspace(connId, {
      agent: { ...emptyWorkspace().agent, kind: current.kind, size: current.size }
    });
  }

  setSchemaPanelCollapsed(connId: string, collapsed: boolean): void {
    this.#patchWorkspace(connId, { schemaPanelCollapsed: collapsed });
  }

  setTableSubTab(connId: string, tabId: string, kind: 'data' | 'schema'): void {
    const ws = this.workspace(connId);
    this.#patchWorkspace(connId, { tableSubTab: { ...ws.tableSubTab, [tabId]: kind } });
  }

  // ── connections ──────────────────────────────────────────────────────────
  async refresh(signal?: AbortSignal): Promise<void> {
    this.listLoading = true;
    this.listError = null;
    try {
      const res = await dbApi.connections.list(signal);
      this.connections = res.data ?? [];
      if (
        this.activeConnectionId &&
        !this.connections.some((connection) => connection.id === this.activeConnectionId)
      ) {
        this.setActiveConnection(null);
      }
      this.loaded = true;
    } catch (err) {
      this.listError = err instanceof Error ? err.message : String(err);
      this.connections = [];
      this.loaded = true;
    } finally {
      this.listLoading = false;
    }
  }

  // ── databases & schema ───────────────────────────────────────────────────
  async loadDatabases(connId: string): Promise<void> {
    try {
      const res = await dbApi.databases(connId);
      const conn = this.connections.find((connection) => connection.id === connId);
      const savedDatabase = conn?.database.trim() || null;
      const fetched = res.data ?? [];
      const list =
        savedDatabase && !fetched.includes(savedDatabase) ? [savedDatabase, ...fetched] : fetched;
      const prev = this.workspace(connId);
      this.#patchWorkspace(connId, {
        databases: list,
        database: prev.database ?? savedDatabase ?? list[0] ?? null
      });
    } catch (err) {
      this.#patchWorkspace(connId, {
        databases: [],
        schemaError: err instanceof Error ? err.message : String(err)
      });
    }
  }

  async loadSchema(connId: string, database?: string): Promise<void> {
    this.#patchWorkspace(connId, { schemaLoading: true, schemaError: null });
    try {
      const res = await dbApi.schema(connId, database);
      this.#patchWorkspace(connId, {
        schema: res.data ?? { schemas: [] },
        schemaLoading: false
      });
    } catch (err) {
      this.#patchWorkspace(connId, {
        schema: { schemas: [] },
        schemaLoading: false,
        schemaError: err instanceof Error ? err.message : String(err)
      });
    }
  }

  setDatabase(connId: string, database: string): void {
    this.#patchWorkspace(connId, { database, schema: null });
    void this.loadSchema(connId, database);
  }

  // ── tabs ─────────────────────────────────────────────────────────────────
  openSqlTab(connId: string, initialSql = '-- new query\nSELECT 1;'): string {
    const id = `sql-${nextTabId++}`;
    const ws = this.workspace(connId);
    const tab: DbTab = {
      id,
      kind: 'sql',
      title: `Query ${ws.tabs.length + 1}`,
      sql: initialSql,
      database: ws.database ?? undefined,
      page: 0,
      pageSize: 100,
      loading: false,
      error: null,
      result: null,
      lastQueryId: null
    };
    this.#patchWorkspace(connId, { tabs: [...ws.tabs, tab], activeTabId: id });
    return id;
  }

  openTableTab(connId: string, schema: string, table: TableMeta): string {
    const ws = this.workspace(connId);
    const existing = ws.tabs.find(
      (t) => t.kind === 'table' && t.schema === schema && t.table === table.name
    );
    if (existing) {
      this.#patchWorkspace(connId, { activeTabId: existing.id });
      return existing.id;
    }
    const id = `tbl-${nextTabId++}`;
    const tab: DbTab = {
      id,
      kind: 'table',
      title: `${schema}.${table.name}`,
      schema,
      table: table.name,
      tableMeta: table,
      database: ws.database ?? undefined,
      page: 0,
      pageSize: 100,
      loading: false,
      error: null,
      result: null,
      lastQueryId: null
    };
    this.#patchWorkspace(connId, { tabs: [...ws.tabs, tab], activeTabId: id });
    return id;
  }

  closeTab(connId: string, tabId: string): void {
    const ws = this.workspace(connId);
    const remaining = ws.tabs.filter((t) => t.id !== tabId);
    const active =
      ws.activeTabId === tabId ? (remaining[remaining.length - 1]?.id ?? null) : ws.activeTabId;
    // Drop any pending edits attached to the closed tab.
    const { [tabId]: _drop, ...restEdits } = ws.pendingEdits;
    void _drop;
    this.#patchWorkspace(connId, {
      tabs: remaining,
      activeTabId: active,
      pendingEdits: restEdits
    });
  }

  // ── pending cell edits ───────────────────────────────────────────────────
  /** Read-only helper. Returns an empty bucket if the tab has none. */
  pendingFor(connId: string, tabId: string): TabPending {
    return this.workspace(connId).pendingEdits[tabId] ?? emptyTabPending();
  }

  #writeBucket(connId: string, tabId: string, next: TabPending): void {
    const ws = this.workspace(connId);
    const pendingEdits: WorkspacePendingEdits = { ...ws.pendingEdits };
    if (isTabBucketEmpty(next)) {
      delete pendingEdits[tabId];
    } else {
      pendingEdits[tabId] = next;
    }
    this.#patchWorkspace(connId, { pendingEdits });
  }

  #cloneBucket(connId: string, tabId: string): TabPending {
    const cur = this.workspace(connId).pendingEdits[tabId] ?? emptyTabPending();
    return {
      edits: { ...cur.edits },
      inserts: cur.inserts.map((i) => ({ ...i, values: { ...i.values }, errors: i.errors }))
    };
  }

  /**
   * Stage a single cell change for (tabId, rowIndex). If `newValue` equals the
   * original, that column's entry is removed; if `changes` becomes empty the
   * whole row entry is dropped.
   */
  stageCellEdit(
    connId: string,
    tabId: string,
    rowIndex: number,
    column: string,
    newValue: unknown,
    original: Record<string, unknown>,
    pk: Record<string, unknown>
  ): void {
    const bucket = this.#cloneBucket(connId, tabId);
    const rowEntry: PendingRowEdit = bucket.edits[rowIndex]
      ? {
          pk: bucket.edits[rowIndex].pk,
          original: bucket.edits[rowIndex].original,
          changes: { ...bucket.edits[rowIndex].changes }
        }
      : { pk, original, changes: {} };

    const originalValue = rowEntry.original[column];
    // Strict equality is good enough here — values are scalars or stringified blobs.
    if (newValue === originalValue) {
      delete rowEntry.changes[column];
    } else {
      rowEntry.changes[column] = newValue;
    }

    if (Object.keys(rowEntry.changes).length === 0) {
      delete bucket.edits[rowIndex];
    } else {
      bucket.edits[rowIndex] = rowEntry;
    }
    this.#writeBucket(connId, tabId, bucket);
  }

  // ── pending inline inserts ───────────────────────────────────────────────
  /** Append a new blank insert row to the top of the grid. Returns its tempId. */
  startInsert(
    connId: string,
    tabId: string,
    initialValues: Record<string, unknown> = {}
  ): string {
    const tempId =
      typeof crypto !== 'undefined' && 'randomUUID' in crypto
        ? crypto.randomUUID()
        : `ins-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
    const bucket = this.#cloneBucket(connId, tabId);
    bucket.inserts = [{ tempId, values: { ...initialValues } }, ...bucket.inserts];
    this.#writeBucket(connId, tabId, bucket);
    return tempId;
  }

  /** Update one cell on an in-progress insert row. */
  updateInsertCell(
    connId: string,
    tabId: string,
    tempId: string,
    column: string,
    value: unknown
  ): void {
    const bucket = this.#cloneBucket(connId, tabId);
    bucket.inserts = bucket.inserts.map((ins) => {
      if (ins.tempId !== tempId) return ins;
      const next: PendingRowInsert = {
        tempId: ins.tempId,
        values: { ...ins.values, [column]: value },
        errors: ins.errors ? { ...ins.errors } : undefined
      };
      // Clear a per-column error if user typed something new.
      if (next.errors && column in next.errors) {
        delete next.errors[column];
        if (Object.keys(next.errors).length === 0) next.errors = undefined;
      }
      return next;
    });
    this.#writeBucket(connId, tabId, bucket);
  }

  /** Remove an inline insert row. */
  removeInsert(connId: string, tabId: string, tempId: string): void {
    const bucket = this.#cloneBucket(connId, tabId);
    bucket.inserts = bucket.inserts.filter((i) => i.tempId !== tempId);
    this.#writeBucket(connId, tabId, bucket);
  }

  /** Attach validation errors to one insert (overwrites). */
  setInsertErrors(
    connId: string,
    tabId: string,
    tempId: string,
    errors: Record<string, string> | undefined
  ): void {
    const bucket = this.#cloneBucket(connId, tabId);
    bucket.inserts = bucket.inserts.map((i) =>
      i.tempId === tempId
        ? { ...i, errors: errors && Object.keys(errors).length > 0 ? errors : undefined }
        : i
    );
    this.#writeBucket(connId, tabId, bucket);
  }

  clearPendingForTab(connId: string, tabId: string): void {
    const ws = this.workspace(connId);
    if (!ws.pendingEdits[tabId]) return;
    const { [tabId]: _drop, ...rest } = ws.pendingEdits;
    void _drop;
    this.#patchWorkspace(connId, { pendingEdits: rest });
  }

  clearPendingAll(connId: string): void {
    this.#patchWorkspace(connId, { pendingEdits: {} });
  }

  setActiveTab(connId: string, tabId: string): void {
    this.#patchWorkspace(connId, { activeTabId: tabId });
  }

  patchTab(connId: string, tabId: string, patch: Partial<DbTab>): void {
    const ws = this.workspace(connId);
    const tabs = ws.tabs.map((t) => (t.id === tabId ? { ...t, ...patch } : t));
    this.#patchWorkspace(connId, { tabs });
  }

  // ── query execution ──────────────────────────────────────────────────────
  async runTab(connId: string, tabId: string): Promise<void> {
    const ws = this.workspace(connId);
    const tab = ws.tabs.find((t) => t.id === tabId);
    if (!tab) return;
    const sql =
      tab.kind === 'sql'
        ? (tab.sql ?? '')
        : `SELECT * FROM ${tab.schema ? `${tab.schema}.` : ''}${tab.table}`;
    if (!sql.trim()) return;

    this.patchTab(connId, tabId, { loading: true, error: null });
    try {
      // Namespace the tab id with the connection so the backend lease map
      // doesn't collide if two workspaces happen to mint the same local id.
      // The lease auto-pins on `BEGIN` and auto-unpins on `COMMIT`/`ROLLBACK`
      // — see Q13 in docs/12-build-plan/open-questions.md.
      const res = await dbApi.query(connId, {
        database: tab.database,
        sql,
        page: tab.page,
        page_size: tab.pageSize,
        tab_id: `${connId}:${tabId}`
      });
      // Mirror backend auto-pin/unpin transitions in the UI so the lock
      // icon reflects the actual lease state after BEGIN / COMMIT / ROLLBACK.
      const kw = leadingSqlKeyword(sql);
      const patch: Partial<DbTab> = {
        result: res.data,
        lastQueryId: res.data.query_id,
        loading: false
      };
      if (kw === 'BEGIN' || kw === 'START') patch.pinned = true;
      else if (kw === 'COMMIT' || kw === 'ROLLBACK' || kw === 'END') patch.pinned = false;
      this.patchTab(connId, tabId, patch);
    } catch (err) {
      this.patchTab(connId, tabId, {
        error: err instanceof Error ? err.message : String(err),
        loading: false
      });
    }
  }

  async cancelTab(connId: string, tabId: string): Promise<{ ok: boolean; message: string }> {
    const ws = this.workspace(connId);
    const tab = ws.tabs.find((t) => t.id === tabId);
    if (!tab?.lastQueryId) return { ok: false, message: 'No query is currently tracked.' };
    try {
      const res = await dbApi.cancel(connId, tab.lastQueryId);
      return res.data.ok
        ? { ok: true, message: 'Cancel requested.' }
        : { ok: false, message: 'Query already finished or was not found.' };
    } catch (err) {
      return {
        ok: false,
        message:
          err instanceof ApiError
            ? ((err.body as { error?: string } | undefined)?.error ?? err.message)
            : err instanceof Error
              ? err.message
              : String(err)
      };
    }
  }

  // ── lease pin/unpin (Q13) ────────────────────────────────────────────────
  async pinTab(connId: string, tabId: string): Promise<void> {
    const ws = this.workspace(connId);
    const tab = ws.tabs.find((t) => t.id === tabId);
    if (!tab) return;
    // Optimistic — revert on failure.
    this.patchTab(connId, tabId, { pinned: true });
    try {
      await dbApi.tabs.pin(`${connId}:${tabId}`, {
        connection_id: connId,
        database: tab.database
      });
    } catch (err) {
      this.patchTab(connId, tabId, { pinned: false });
      throw err;
    }
  }

  async unpinTab(connId: string, tabId: string): Promise<void> {
    this.patchTab(connId, tabId, { pinned: false });
    try {
      await dbApi.tabs.unpin(`${connId}:${tabId}`);
    } catch {
      // best-effort — server may already have released the lease.
    }
  }
}

export const dbStore = new DbStore();
