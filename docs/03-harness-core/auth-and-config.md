---
id: harness-core/auth-and-config
title: Configuración del harness (sin provider auth)
shard: 03-harness-core
tags: [config, profiles, no-provider-auth]
summary: Archivos TOML por nivel; auth de claude/codex la maneja el propio CLI.
related: [cross-cutting/config-files, cross-cutting/profiles, build-plan/decisions-locked]
sources: []
---

# Configuración

> Cambio importante: **no manejamos auth de provider** (Anthropic/OpenAI). El `claude`/`codex` tiene su propia. Nuestra config cubre solo preferencias del harness.

## Archivos

```
~/.harness/
├── config.toml                          # global
└── profiles/<active>/
    ├── config.toml                       # override por profile
    ├── PROFILE.md                        # contexto laboral
    ├── cli-state/                        # auth del CLI hijo (gestionada por claude/codex)
    │   ├── .claude/                      # → bind-mount al container
    │   └── .codex/
    └── policy.toml                       # allow-and-remember rules
```

## `~/.harness/config.toml` (global)

```toml
default_profile = "personal"

[ui]
theme = "dark"               # dark | light | auto
port = 8080

[server]
listen = "127.0.0.1:7777"
cors_origins = ["http://localhost:8080"]

[telemetry]
enabled = false              # opt-in, ver cross-cutting/telemetry
```

## `profiles/<active>/config.toml` (override)

```toml
[ui]
# hereda global

[sessions]
default_cli = "claude"       # claude | codex
default_model_hint = "sonnet"  # passed to CLI as --model hint

[budget]
default_usd_max = 10
default_wallclock_s = 3600
default_max_concurrent_spawns = 3

[sandbox]
level = "workspace"          # none | workspace | workspace-net | strict
allow_net = ["github.com", "*.npmjs.org"]

[approval]
mode = "risky-only"          # auto | risky-only | every-call

[git]
auto_sync = false
sync_interval_min = 30

[mcp]
# MCP servers externos disponibles para los CLIs hijos
enabled = ["context7"]
[[mcp.servers]]
name = "context7"
transport = "stdio"
command = "npx"
args = ["@context7/mcp"]
```

## Lo que NO está en config

- ❌ `[provider.anthropic] auth = "keyring:..."`. **No aplica**: el CLI maneja eso.
- ❌ API keys del provider.
- ❌ Auth tokens del modelo.

Si el usuario quiere cambiar de cuenta de `claude`:
```bash
docker compose exec backend claude logout
docker compose exec backend claude login
```

La auth queda en `cli-state/.claude/` del profile activo. Cambiar profile → cambia auth automáticamente.

## Precedencia (mayor → menor)

1. Flag/header del request (raro).
2. Env vars (`HARNESS_LISTEN`, `RUST_LOG`, ...).
3. `profiles/<active>/config.toml`.
4. `~/.harness/config.toml`.
5. Defaults compilados.

## Validación

JSON Schemas en `backend/crates/harness-core/schemas/`:
- `config.global.v1.json`
- `config.profile.v1.json`

Al boot, schema validation. Drift → error claro con campo + línea.

## Hot reload

- Cambios en `config.toml` requieren restart suave del backend (`docker compose restart backend`).
- Cambios en `policy.toml` (allow-and-remember) → re-leído por cada approval; cambio aplicable sin restart.
- Cambios en `PROFILE.md` → aplicable al siguiente spawn (no re-injecta a spawns vivos).

## Cambiar profile (CLI futuro o API)

```bash
# vía API (siempre disponible)
curl -X POST localhost:7777/api/profile/use -d '{"name":"work-acme"}'

# vía CLI (post-F6)
harness profile use work-acme
```

Efectos:
- Symlink `~/.harness/active_profile` actualizado.
- Symlink interno del container actualizado (`/root/.claude` → nuevo cli-state).
- Backend recarga config del nuevo profile.
- UI muestra nuevo badge de profile activo.

## Anti-patrones

| Mal | Bien |
|---|---|
| Auth del provider en `~/.harness/config.toml` | El CLI maneja su auth; nosotros no la tocamos |
| Cambiar `cli-state/` a mano sin pasar por profile use | Usa el comando; el switch coordina symlinks |
| Hard-code de paths | Usa `HARNESS_HOME` env var |
| Sin schema validation | JSON Schema obligatorio |
| Mezclar config global con profile-specific | Global = comportamiento del backend; profile = preferencias contextuales |
