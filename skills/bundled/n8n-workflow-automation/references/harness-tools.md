# Harness n8n MCP Tools

This reference describes the n8n tools exposed by Harness agents. These tools complement n8n-specific workflow knowledge: they operate an n8n instance and persist generated workflow artifacts.

## Standard Flow

1. `tools_load({"groups":["n8n"]})`
2. `n8n_validate_workflow({"workflow": ...})`
3. `n8n_save_workflow({"name":"safe-name","workflow": ...})`
4. Optional, after approval:
   - `n8n_local_start({"approved":true})`
   - `n8n_configure({"base_url":"http://127.0.0.1:5678","api_key_env":"N8N_API_KEY"})`
   - `n8n_import_workflow({"name":"safe-name","approved":true})`
   - `n8n_activate_workflow({"id":"<workflow-id>","approved":true})`
   - `n8n_webhook_request({"path":"/webhook-test/example","body":{...},"approved":true})`

## Tool Summary

- `n8n_configure`: Store default `base_url`, `api_key_env`, and optional local Docker container name for this profile. It stores the env var name only, not the key value.
- `n8n_status`: Check configured API reachability and Docker container state. Does not expose secret values.
- `n8n_local_start`: Start a local Docker n8n instance. Requires explicit approval and `approved:true`.
- `n8n_local_stop`: Remove the local Docker n8n container. Requires explicit approval and `approved:true`.
- `n8n_save_workflow`: Save generated workflow JSON under `HARNESS_HOME/profiles/<profile>/modules/n8n/workflows/`.
- `n8n_list_saved_workflows`: List saved local workflow JSON files.
- `n8n_read_workflow`: Read a saved workflow and return validation diagnostics.
- `n8n_validate_workflow`: Validate minimal workflow shape and warn about likely raw secrets.
- `n8n_import_workflow`: POST workflow JSON to n8n's public API. Requires API key env var, explicit approval, and `approved:true`.
- `n8n_list_remote_workflows`: List workflows via n8n public API.
- `n8n_activate_workflow`: Activate a workflow by id. Requires explicit approval and `approved:true`.
- `n8n_deactivate_workflow`: Deactivate a workflow by id. Requires explicit approval and `approved:true`.
- `n8n_webhook_request`: Call a GET or POST webhook path on the configured n8n base URL. Requires explicit approval and `approved:true`.

## Secrets And Credentials

Use n8n credentials, n8n environment variables, or Harness/user-provided runtime env vars. Do not place secrets directly in workflow JSON. If the credential id is unknown, omit the node's `credentials` block so the user can select credentials in the n8n UI.

API access uses `X-N8N-API-KEY`; the key value must be available to the harness process through `N8N_API_KEY` or the configured `api_key_env`.

## Local Instance Notes

`n8n_local_start` runs Docker with:

- localhost-only port binding
- persistent n8n data under the Harness profile
- a generated `N8N_ENCRYPTION_KEY` stored under the profile
- `N8N_PUBLIC_API_DISABLED=false`

After first start, the user may need to open the n8n UI, complete owner setup, and create an API key in Settings -> n8n API. Export that key for the harness process before using API import/list/activate tools.

## Validation Limits

`n8n_validate_workflow` checks required top-level fields (`name`, non-empty `nodes`, `connections`) and basic node fields (`name`, `type`, `position`). It does not know every node schema. For complex nodes, use an external n8n schema source or `n8n-mcp` if configured, then run n8n import/runtime validation.
