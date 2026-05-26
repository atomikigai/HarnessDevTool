---
id: harness-core/auth-and-config
title: Auth y configuración
shard: 03-harness-core
tags: [auth, config, secrets]
summary: Archivos de config, providers, keyring del SO para secretos.
related: [cross-cutting/config-files, cross-cutting/security-model]
sources: []
---

# Auth y config

## Archivos
- `~/.harness/config.toml` — preferencias del usuario.
- `~/.harness/credentials/` — refs cifradas (keyring nativo).
- `<project>/AGENTS.md` — instrucciones por proyecto.
- `<project>/.harness/project.toml` — overrides por proyecto.

## Esquema mínimo

```toml
[default]
model = "claude-opus-4-7"
approval_mode = "risky-only"
auto_compact_limit = 0.75

[provider.anthropic]
auth = "keyring:anthropic-api-key"
base_url = "https://api.anthropic.com"

[provider.openai]
auth = "keyring:openai-api-key"

[sandbox]
level = "workspace"
allow_net = ["github.com", "*.npmjs.org"]

[[mcp.servers]]
name = "playwright"
transport = "stdio"
command = "npx"
args = ["@modelcontextprotocol/server-playwright"]
```

## Resolución de credenciales
- `keyring:<name>` → consulta `keyring-rs` (Secret Service / Keychain / Credential Manager).
- `env:<NAME>` → variable de entorno.
- `file:<path>` → archivo plano (solo dev, warning).

Nunca persistir credenciales en `events.jsonl`. El prompt builder reemplaza por placeholders `{{secret:<ref>}}` antes de loggear; al enviar al provider, sustituye en memoria.

## Multi-perfil
`harness --profile work ...` selecciona `[profile.work]` que hereda de `[default]`.

## CLI de gestión
```
harness auth set anthropic --from-stdin
harness auth list
harness auth rotate openai
```

## Overrides por thread
Un thread puede fijar overrides en su `meta.json` (model, sandbox) — al hacer `resume` se respetan, ignorando cambios globales posteriores. Esto garantiza reproducibilidad.
