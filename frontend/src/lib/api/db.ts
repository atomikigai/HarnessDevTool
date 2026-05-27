/**
 * F4 module-db — typed REST client.
 *
 * Hand-typed mirror of the backend contract (backend ts-rs export is
 * .gitignored, so we keep these here). Endpoints all live under
 * `/api/db/*` and degrade gracefully when the backend isn't ready —
 * callers should `try { await ... } catch (e) { ... }`.
 */

import { apiRequest } from './client';

// ────────────────────────────────────────────────────────────────────────────
// Types
// ────────────────────────────────────────────────────────────────────────────

export type DbEngine = 'sqlite' | 'postgres' | 'mysql';
export type SslMode = 'disable' | 'prefer' | 'require' | 'verify-ca' | 'verify-full';

export interface Connection {
  id: string;
  name: string;
  engine: DbEngine;
  host?: string | null;
  port?: number | null;
  database: string; // file path for sqlite
  username?: string | null;
  ssl_mode?: SslMode | null;
  params?: Record<string, string> | null;
  created_at?: string;
  updated_at?: string;
}

export interface ConnectionInput {
  name: string;
  engine: DbEngine;
  host?: string;
  port?: number;
  database: string;
  username?: string;
  password?: string;
  ssl_mode?: SslMode;
  params?: Record<string, string>;
}

export interface TestResult {
  ok: boolean;
  latency_ms?: number;
  server_version?: string;
  error?: string;
}

export interface Column {
  name: string;
  data_type: string;
  nullable: boolean;
  pk?: boolean;
  default?: string | null;
  comment?: string | null;
}

export interface Index {
  name: string;
  columns: string[];
  unique?: boolean;
}

export interface ForeignKey {
  name: string;
  columns: string[];
  ref_table: string;
  ref_columns: string[];
}

export interface TableMeta {
  name: string;
  kind: 'table' | 'view' | 'materialized_view';
  row_estimate?: number | null;
  columns: Column[];
  indexes: Index[];
  foreign_keys: ForeignKey[];
}

export interface SchemaNode {
  name: string;
  tables: TableMeta[];
}

export interface SchemaTree {
  schemas: SchemaNode[];
}

export interface QueryResult {
  columns: { name: string; data_type: string }[];
  rows: unknown[][];
  total_rows?: number | null;
  truncated: boolean;
  elapsed_ms: number;
  query_id: string;
}

export interface QueryRequest {
  database?: string;
  sql: string;
  params?: unknown[];
  page_size?: number;
  page?: number;
}

export interface RowMutation {
  database?: string;
  schema?: string;
  values?: Record<string, unknown>;
  pk?: Record<string, unknown>;
}

// ────────────────────────────────────────────────────────────────────────────
// Client
// ────────────────────────────────────────────────────────────────────────────

const base = '/db';

export const dbApi = {
  connections: {
    list: (signal?: AbortSignal) => apiRequest<Connection[]>(`${base}/connections`, { signal }),
    create: (body: ConnectionInput, signal?: AbortSignal) =>
      apiRequest<Connection>(`${base}/connections`, { method: 'POST', body, signal }),
    update: (id: string, body: ConnectionInput, signal?: AbortSignal) =>
      apiRequest<Connection>(`${base}/connections/${id}`, { method: 'PUT', body, signal }),
    remove: (id: string, signal?: AbortSignal) =>
      apiRequest<null>(`${base}/connections/${id}`, { method: 'DELETE', signal }),
    test: (id: string, signal?: AbortSignal) =>
      apiRequest<TestResult>(`${base}/connections/${id}/test`, { method: 'POST', signal })
  },
  /** Test an unsaved connection input (from the form). */
  test: (body: ConnectionInput, signal?: AbortSignal) =>
    apiRequest<TestResult>(`${base}/test`, { method: 'POST', body, signal }),
  databases: (id: string, signal?: AbortSignal) =>
    apiRequest<string[]>(`${base}/connections/${id}/databases`, { signal }),
  schema: (id: string, database?: string, signal?: AbortSignal) => {
    const qs = database ? `?database=${encodeURIComponent(database)}` : '';
    return apiRequest<SchemaTree>(`${base}/connections/${id}/schema${qs}`, { signal });
  },
  query: (id: string, req: QueryRequest, signal?: AbortSignal) =>
    apiRequest<QueryResult>(`${base}/connections/${id}/query`, {
      method: 'POST',
      body: req,
      signal
    }),
  cancel: (id: string, queryId: string, signal?: AbortSignal) =>
    apiRequest<null>(`${base}/connections/${id}/query/${queryId}/cancel`, {
      method: 'POST',
      signal
    }),
  rows: {
    insert: (id: string, table: string, body: RowMutation, signal?: AbortSignal) =>
      apiRequest<{ pk?: Record<string, unknown> }>(
        `${base}/connections/${id}/tables/${encodeURIComponent(table)}/rows`,
        { method: 'POST', body, signal }
      ),
    update: (id: string, table: string, body: RowMutation, signal?: AbortSignal) =>
      apiRequest<{ pk?: Record<string, unknown> }>(
        `${base}/connections/${id}/tables/${encodeURIComponent(table)}/rows`,
        { method: 'PUT', body, signal }
      ),
    remove: (id: string, table: string, body: RowMutation, signal?: AbortSignal) =>
      apiRequest<null>(`${base}/connections/${id}/tables/${encodeURIComponent(table)}/rows`, {
        method: 'DELETE',
        body,
        signal
      }),
    duplicate: (id: string, table: string, body: RowMutation, signal?: AbortSignal) =>
      apiRequest<{ pk?: Record<string, unknown> }>(
        `${base}/connections/${id}/tables/${encodeURIComponent(table)}/rows/duplicate`,
        { method: 'POST', body, signal }
      )
  }
};

// ────────────────────────────────────────────────────────────────────────────
// Engine helpers
// ────────────────────────────────────────────────────────────────────────────

export const defaultPort = (engine: DbEngine): number | undefined => {
  if (engine === 'postgres') return 5432;
  if (engine === 'mysql') return 3306;
  return undefined;
};

export const engineLabel = (engine: DbEngine): string => {
  if (engine === 'postgres') return 'PostgreSQL';
  if (engine === 'mysql') return 'MySQL';
  return 'SQLite';
};

export const needsHost = (engine: DbEngine): boolean => engine !== 'sqlite';
