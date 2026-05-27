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
}

interface ConnectionWorkspace {
  databases: string[];
  database: string | null;
  schema: SchemaTree | null;
  schemaLoading: boolean;
  schemaError: string | null;
  tabs: DbTab[];
  activeTabId: string | null;
}

function emptyWorkspace(): ConnectionWorkspace {
  return {
    databases: [],
    database: null,
    schema: null,
    schemaLoading: false,
    schemaError: null,
    tabs: [],
    activeTabId: null
  };
}

let nextTabId = 1;

class DbStore {
  connections = $state<Connection[]>([]);
  loaded = $state(false);
  listLoading = $state(false);
  listError = $state<string | null>(null);

  /** Keyed by connection id. */
  workspaces = $state<Record<string, ConnectionWorkspace>>({});

  workspace(connId: string): ConnectionWorkspace {
    return this.workspaces[connId] ?? emptyWorkspace();
  }

  #patchWorkspace(connId: string, patch: Partial<ConnectionWorkspace>) {
    const prev = this.workspaces[connId] ?? emptyWorkspace();
    this.workspaces = { ...this.workspaces, [connId]: { ...prev, ...patch } };
  }

  // ── connections ──────────────────────────────────────────────────────────
  async refresh(signal?: AbortSignal): Promise<void> {
    this.listLoading = true;
    this.listError = null;
    try {
      const res = await dbApi.connections.list(signal);
      this.connections = res.data ?? [];
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
      const list = res.data ?? [];
      const prev = this.workspace(connId);
      this.#patchWorkspace(connId, {
        databases: list,
        database: prev.database ?? list[0] ?? null
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
    this.#patchWorkspace(connId, { tabs: remaining, activeTabId: active });
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
      const res = await dbApi.query(connId, {
        database: tab.database,
        sql,
        page: tab.page,
        page_size: tab.pageSize
      });
      this.patchTab(connId, tabId, {
        result: res.data,
        lastQueryId: res.data.query_id,
        loading: false
      });
    } catch (err) {
      this.patchTab(connId, tabId, {
        error: err instanceof Error ? err.message : String(err),
        loading: false
      });
    }
  }

  async cancelTab(connId: string, tabId: string): Promise<void> {
    const ws = this.workspace(connId);
    const tab = ws.tabs.find((t) => t.id === tabId);
    if (!tab?.lastQueryId) return;
    try {
      await dbApi.cancel(connId, tab.lastQueryId);
    } catch {
      // best-effort
    }
  }
}

export const dbStore = new DbStore();
