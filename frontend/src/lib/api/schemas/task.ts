/**
 * valibot schemas for task forms / PATCH payloads.
 * Used in TaskCreateForm and TaskDetail before hitting the network.
 */
import * as v from 'valibot';

export const taskStatusSchema = v.picklist([
  'queued',
  'in_progress',
  'pending_verify',
  'done',
  'paused',
  'blocked',
  'abandoned'
]);

export const acceptanceCheckSchema = v.object({
  id: v.string(),
  text: v.pipe(v.string(), v.minLength(1, 'Check text required')),
  verified: v.boolean(),
  verified_by: v.optional(v.string())
});

export const createTaskSchema = v.object({
  title: v.pipe(
    v.string(),
    v.minLength(3, 'Title must be at least 3 characters'),
    v.maxLength(200, 'Title is too long')
  ),
  parent: v.optional(v.string()),
  depends_on: v.optional(v.array(v.string())),
  acceptance: v.optional(
    v.object({
      checks: v.pipe(
        v.array(v.object({ text: v.pipe(v.string(), v.minLength(1, 'Check text required')) })),
        v.maxLength(6, 'Maximum 6 acceptance checks recommended')
      )
    })
  ),
  labels: v.optional(v.array(v.string())),
  created_by: v.string()
});

export const patchTaskSchema = v.object({
  title: v.optional(v.pipe(v.string(), v.minLength(3))),
  status: v.optional(taskStatusSchema),
  assignee: v.optional(v.nullable(v.string())),
  labels: v.optional(v.array(v.string())),
  acceptance: v.optional(v.object({ checks: v.array(acceptanceCheckSchema) })),
  notes: v.optional(
    v.object({
      why_paused: v.optional(v.string()),
      why_abandoned: v.optional(v.string()),
      feedback: v.optional(v.array(v.unknown()))
    })
  ),
  by: v.string()
});

export const createAgentSchema = v.object({
  kind: v.pipe(v.string(), v.minLength(1, 'Kind required')),
  label: v.pipe(v.string(), v.minLength(1, 'Label required'))
});

export type ValidationResult<T> = { ok: true; value: T } | { ok: false; errors: string[] };

export function safeParse<T>(schema: v.GenericSchema<T>, input: unknown): ValidationResult<T> {
  const r = v.safeParse(schema, input);
  if (r.success) return { ok: true, value: r.output };
  return {
    ok: false,
    errors: r.issues.map((i) => i.message)
  };
}
