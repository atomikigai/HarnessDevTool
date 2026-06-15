---
name: n8n-workflow-automation
description: Build, validate, import, activate, and test n8n workflow automations through Harness MCP tools. Use when the user asks for n8n workflows, workflow JSON, automation design, webhook/API/database/scheduled/AI-agent automations, n8n credentials, node configuration, n8n expressions, Code nodes, importing workflows into n8n, launching a local n8n instance, or testing an automation end to end.
---

# n8n Workflow Automation

Use this skill to turn a plain-language automation request into an importable and testable n8n workflow. Prefer Harness MCP tools for operational steps and references here for n8n-specific workflow craft.

## Harness Workflow

1. Load the n8n tool group with `tools_load({"groups":["n8n"]})` if `n8n_*` tools are not visible.
2. Clarify the automation boundary only when missing information blocks a correct workflow: trigger, inputs, external systems, credentials needed, expected outputs, and test payload.
3. Design the workflow before emitting JSON: trigger -> transform/branch -> actions -> response/error path.
4. Generate workflow JSON without raw secrets. Reference credentials by real n8n credential id/name only when known; otherwise omit the `credentials` block so the n8n UI remains configurable.
5. Validate with `n8n_validate_workflow`, fix errors/warnings, then save with `n8n_save_workflow`.
6. Import or execute only after explicit user approval. Use `n8n_import_workflow`, `n8n_activate_workflow`, `n8n_deactivate_workflow`, `n8n_webhook_request`, `n8n_local_start`, and `n8n_local_stop` with `approved:true` only after approval.
7. Report artifacts: saved workflow name/path, validation status, n8n instance URL if started/configured, required credentials, and safe test steps.

## Harness n8n Tools

Read [harness-tools.md](references/harness-tools.md) before calling operational `n8n_*` tools. Important constraints:

- API keys are read from an environment variable such as `N8N_API_KEY`; never ask the model to paste or store the key in workflow JSON.
- Local n8n state lives under `HARNESS_HOME/profiles/<profile>/modules/n8n/`.
- Mutating operations require explicit approval and `approved:true`.
- The built-in validator catches minimal JSON shape and raw-secret smells; it does not replace n8n runtime validation.

## Workflow Design References

Load only the reference needed for the current task:

- General pattern selection: [workflow-patterns.md](references/workflow-patterns.md)
- Webhooks: [pattern-webhook-processing.md](references/pattern-webhook-processing.md)
- REST/API integrations: [pattern-http-api-integration.md](references/pattern-http-api-integration.md)
- Database automations: [pattern-database-operations.md](references/pattern-database-operations.md)
- AI-agent workflows: [pattern-ai-agent-workflow.md](references/pattern-ai-agent-workflow.md)
- Scheduled jobs: [pattern-scheduled-tasks.md](references/pattern-scheduled-tasks.md)

## Node And Expression References

- Node parameter setup and operation-specific required fields: [node-configuration.md](references/node-configuration.md)
- n8n expressions and cross-node data access: [expression-syntax.md](references/expression-syntax.md)
- Common expression mistakes: [expression-common-mistakes.md](references/expression-common-mistakes.md)
- Validation errors and fix loops: [validation-expert.md](references/validation-expert.md)
- Detailed validation error catalog: [validation-error-catalog.md](references/validation-error-catalog.md)

## Code Nodes

Use JavaScript for most n8n Code nodes unless the user explicitly needs Python.

- JavaScript Code node guide: [code-javascript.md](references/code-javascript.md)
- Common JS Code node patterns: [code-javascript-common-patterns.md](references/code-javascript-common-patterns.md)
- n8n Code node data access: [code-javascript-data-access.md](references/code-javascript-data-access.md)
- AI-agent Custom Code Tool, not regular Code node: [code-tool.md](references/code-tool.md)

## JSON Hygiene

- Generate UUID-like stable node `id` values, not descriptive strings.
- Use full workflow node types such as `n8n-nodes-base.webhook`, `n8n-nodes-base.httpRequest`, and `n8n-nodes-base.code`.
- Include `position` arrays for every node so the imported workflow is readable.
- Keep `connections` explicit, even for one-node workflows.
- Add error-handling branches for production workflows that call external APIs or mutate data.
- Never embed tokens, passwords, authorization headers, private keys, cookies, OAuth refresh tokens, or personal data samples in workflow JSON.

## Attribution

Reference material in this skill is adapted from `czlonkowski/n8n-skills` under the MIT license. See [upstream-n8n-skills-license.txt](references/upstream-n8n-skills-license.txt).
