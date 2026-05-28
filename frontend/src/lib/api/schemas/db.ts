/**
 * valibot schemas for F4 module-db forms.
 * Used by ConnectionFormDialog before hitting /api/db/connections.
 *
 * Field-path mapping lets the form highlight the offending input
 * instead of just dumping an error string.
 */
import * as v from 'valibot';

const engineSchema = v.picklist(['sqlite', 'postgres', 'mysql'] as const);
const sslModeSchema = v.picklist(['disable', 'prefer', 'require'] as const);

const portSchema = v.pipe(
  v.number('Port must be a number'),
  v.integer('Port must be an integer'),
  v.minValue(1, 'Port must be ≥ 1'),
  v.maxValue(65535, 'Port must be ≤ 65535')
);

const baseFields = {
  name: v.pipe(
    v.string(),
    v.trim(),
    v.minLength(1, 'Required'),
    v.maxLength(80, 'Name too long')
  ),
  database: v.pipe(v.string(), v.trim(), v.minLength(1, 'Required')),
  username: v.optional(v.pipe(v.string(), v.trim())),
  password: v.optional(v.string()),
  params: v.optional(v.record(v.string(), v.string()))
};

const sqliteSchema = v.object({
  ...baseFields,
  engine: v.literal('sqlite'),
  database: v.pipe(v.string(), v.trim(), v.minLength(1, 'File path required'))
});

const postgresSchema = v.object({
  ...baseFields,
  engine: v.literal('postgres'),
  host: v.pipe(v.string(), v.trim(), v.minLength(1, 'Required')),
  port: v.optional(portSchema),
  ssl_mode: v.optional(sslModeSchema)
});

const mysqlSchema = v.object({
  ...baseFields,
  engine: v.literal('mysql'),
  host: v.pipe(v.string(), v.trim(), v.minLength(1, 'Required')),
  port: v.optional(portSchema)
});

export const connectionInputSchema = v.variant('engine', [
  sqliteSchema,
  postgresSchema,
  mysqlSchema
]);

export type ConnectionInputValidated = v.InferOutput<typeof connectionInputSchema>;

/**
 * Parse user-provided JSON for the "Advanced parameters" field.
 * Returns `{ ok: true, value }` on success (or `undefined` when empty),
 * or `{ ok: false, error }` on parse failure / wrong shape.
 */
export function parseParamsJson(
  text: string
): { ok: true; value: Record<string, string> | undefined } | { ok: false; error: string } {
  const trimmed = text.trim();
  if (!trimmed) return { ok: true, value: undefined };
  let parsed: unknown;
  try {
    parsed = JSON.parse(trimmed);
  } catch {
    return { ok: false, error: 'Invalid JSON' };
  }
  const check = v.safeParse(v.record(v.string(), v.string()), parsed);
  if (!check.success) return { ok: false, error: 'Expected object of string → string' };
  return { ok: true, value: check.output };
}

/**
 * Validate a connection input. Returns field-path → message map so the
 * form can pin errors to individual inputs. Empty record = valid.
 */
export function validateConnection(input: unknown): {
  ok: boolean;
  data?: ConnectionInputValidated;
  fieldErrors: Record<string, string>;
} {
  const result = v.safeParse(connectionInputSchema, input);
  if (result.success) return { ok: true, data: result.output, fieldErrors: {} };

  const fieldErrors: Record<string, string> = {};
  for (const issue of result.issues) {
    const path = issue.path?.map((p) => String(p.key)).join('.') ?? '_';
    if (!fieldErrors[path]) fieldErrors[path] = issue.message;
  }
  return { ok: false, fieldErrors };
}
