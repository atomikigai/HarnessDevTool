import * as v from 'valibot';

export const sessionKindSchema = v.picklist(['claude', 'codex', 'cursor', 'antigravity', 'zeus']);
export const autonomyProfileSchema = v.picklist(['manual', 'assisted', 'autonomous', 'ci']);
export const repoModeSchema = v.picklist(['resume', 'context', 'none']);
export const capabilityProfileSchema = v.picklist(['auto', 'none']);

export const zeusRoleSelectionSchema = v.object({
  role: v.pipe(v.string(), v.trim(), v.minLength(1, 'Role required')),
  provider: v.picklist(['claude', 'codex', 'cursor', 'antigravity', 'zeus']),
  model: v.optional(v.pipe(v.string(), v.trim())),
  effort: v.optional(v.pipe(v.string(), v.trim()))
});

export const newSessionFormSchema = v.object({
  kind: sessionKindSchema,
  autonomy: autonomyProfileSchema,
  cwd: v.optional(v.pipe(v.string(), v.trim())),
  repoMode: repoModeSchema,
  capabilityProfile: capabilityProfileSchema,
  zeusRoles: v.array(zeusRoleSelectionSchema),
  cols: v.pipe(v.number(), v.integer(), v.minValue(40), v.maxValue(300)),
  rows: v.pipe(v.number(), v.integer(), v.minValue(10), v.maxValue(120))
});

export type NewSessionFormValidated = v.InferOutput<typeof newSessionFormSchema>;

export function validateNewSessionForm(input: unknown):
  | { ok: true; data: NewSessionFormValidated }
  | { ok: false; message: string } {
  const result = v.safeParse(newSessionFormSchema, input);
  if (result.success) return { ok: true, data: result.output };
  return {
    ok: false,
    message: result.issues[0]?.message ?? 'Invalid session form'
  };
}
