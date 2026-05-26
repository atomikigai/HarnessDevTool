---
id: cross-cutting/security-model
title: Modelo de seguridad
shard: 09-cross-cutting
tags: [security, sandbox, secrets, approval]
summary: Capas: sandbox OS, approval-gate, secret store, host-key verification.
related: [harness-core/sandbox, harness-core/approval-flow, module-ssh-manager/sessions-and-keys]
sources: []
---

# Modelo de seguridad

## Trust boundaries

```
[user] ─trusted──► [UI]
[UI]   ─trusted──► [harness-server]
[harness-server] ─trusted──► [harness-core]
[harness-core] ─UNTRUSTED──► [model output]
[harness-core] ─UNTRUSTED──► [tool execution]
[harness-core] ─UNTRUSTED──► [MCP servers]
```

El output del modelo es **untrusted**: puede contener instrucciones envenenadas (prompt injection desde contenido leído). Por eso:
- Tool calls peligrosas pasan por approval (`risky-only` default).
- Sandbox limita el daño aunque el modelo intente algo.

## Capas de defensa

### 1. Sandbox del SO
Ver [[harness-core/sandbox]]. FS jail, seccomp/AppContainer, red allowlist.

### 2. Approval gate
Ver [[harness-core/approval-flow]]. Aprobación humana para acciones destructivas.

### 3. Whitelist de comandos peligrosos
Estos requieren approval **siempre**, no overrideable:
- `rm -rf` (cualquier path)
- `git push --force`
- `npm publish`, `cargo publish`
- `kubectl apply`, `terraform apply`
- `curl ... | sh`
- Cualquier escritura fuera de `project_root`

### 4. Secret store
- Keyring del SO: macOS Keychain, Linux Secret Service, Windows Credential Manager.
- Crate: `keyring-rs`.
- Fallback dev: archivo `~/.harness/credentials.toml` con permisos 0600 + warning.
- Nunca en `events.jsonl`, nunca en logs (sustitución por `{{secret:<ref>}}`).

### 5. Host key verification (SSH)
TOFU con confirmación, `known_hosts` propio del módulo. Cambio de key → bloqueo. Ver [[module-ssh-manager/sessions-and-keys]].

### 6. MCP servers
Trust boundary explícito. Recomendaciones:
- Ejecutar el child MCP local bajo sandbox del SO.
- Lista blanca de servers MCP por proyecto.
- Tools MCP aprobadas individualmente al primer uso.

## Datos en disco
- Default sin cifrado en `~/.harness/`. Reposa sobre cifrado de disco del SO.
- Opción `--encrypt` para cifrar threads con `age` (clave en keyring).

## Datos al provider
- Stateless (sin `previous_response_id`) → ZDR-friendly.
- Configurable: provider, base_url, header opt-in `X-No-Retain: true`.

## Telemetría
Off por defecto. Si se activa, anonymizada y solo agregados. Ver [[cross-cutting/telemetry]].
