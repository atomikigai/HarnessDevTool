---
id: cross-cutting/config-files
title: Archivos de configuración
shard: 09-cross-cutting
tags: [config, toml, files]
summary: Layout y precedencia de configs.
related: [harness-core/auth-and-config, architecture/state-persistence, agents/autonomy-protocol]
sources: []
---

# Configuración

## Precedencia (mayor → menor)
1. Flags CLI (`--model claude-opus-4-7`)
2. Env vars (`HARNESS_MODEL=...`)
3. `<project>/.harness/project.toml`
4. `~/.harness/config.toml` (perfil activo)
5. Defaults compilados

## Archivos

### `~/.harness/config.toml`
```toml
default_profile = "personal"

[profiles.personal]
model = "claude-opus-4-7"
approval_mode = "risky-only"
autonomy_profile = "assisted"

[provider.anthropic]
auth = "keyring:anthropic-api-key"

[sandbox]
level = "workspace"

[[mcp.servers]]
name = "playwright"
transport = "stdio"
command = "npx"
args = ["@modelcontextprotocol/server-playwright"]
```

### `<project>/.harness/project.toml`
Overrides específicos del proyecto:
```toml
[overrides]
sandbox.allow_net = ["github.com", "internal.example.com"]
sandbox.level = "workspace-net"

[budget]
usd_max = 50

[autonomy]
profile = "autonomous"     # manual | assisted | autonomous | ci
allow_install = true
allow_network = true
```

### `<project>/AGENTS.md`
Prosa para el modelo (no para el harness). Cargado en cada thread del proyecto. Convención: 1-2 KB max para no inflar el prompt.

## Schemas
Todos los TOML/JSON tienen JSON Schema en `crates/harness-core/schemas/`. CLI provee:
```
harness config validate
harness config edit                 # abre $EDITOR
harness config show --resolved      # config tras todas las precedencias
```

## Hot reload
- Cambios en `config.toml` requieren restart del `harness-server` (`docker compose restart backend`).
- Cambios en `project.toml` se aplican al siguiente thread (no afectan threads activos).
- Cambios en `AGENTS.md` se aplican al siguiente turn (re-snapshotea como append).
