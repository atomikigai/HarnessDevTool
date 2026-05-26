---
id: cross-cutting/profiles
title: Profiles — aislamiento entre contextos
shard: 09-cross-cutting
tags: [profiles, isolation, multi-context, auth]
summary: Concepto de profile: aislamiento de threads, memoria, credenciales y git por contexto.
related: [memory/overview, memory/layout, memory/git, cross-cutting/security-model]
sources: []
---

# Profiles

> Un **profile** es un namespace aislado del harness con sus propios threads, memoria, skills, credenciales y git. Permite separar contextos sin levantar instancias múltiples del backend.

## Por qué existen

Caso de uso real: una sola persona con **dos trabajos** que comparten stack pero tienen requisitos de aislamiento:
- Credenciales de `claude`/`codex` distintas (cuentas separadas).
- Push a remotes git distintos (GH personal vs GitLab empresarial).
- Threads / memoria que no deben mezclarse (privacidad de cliente).
- Configuración por contexto (modelo distinto, MCPs distintos, budgets distintos).

Sin profiles, el usuario tendría que correr dos instancias del backend con `HARNESS_HOME` distinto. Funcional pero pesado.

## Lo que aísla un profile

| Aislado | No aislado |
|---|---|
| Threads y tasks | `USER.md` global (sigue siendo tú) |
| Memoria (decisions, pending, in-flight, facts) | shadcn-svelte, código del harness, etc. |
| Skills privadas | `skills/bundled/` (read-only, viene con el harness) |
| Auth de `claude`/`codex` (`cli-state/`) | Telemetría global (si activada) |
| Config (modelo default, MCPs, sandbox) | Hooks de OS (atajos globales) |
| Git con su propio remote | |
| Budget tracking | |

## Estructura (resumen — full en [[memory/layout]])

```
~/.harness/
├── USER.md                       # global
├── config.toml                   # default_profile, telemetry
├── shared/                       # skills cross-profile (opt-in)
├── profiles/
│   ├── personal/...
│   └── work-acme/...
└── active_profile -> profiles/personal   # symlink
```

## Profile activo

Resolución (prioridad alta → baja):
1. Flag CLI `--profile <name>`.
2. Env `HARNESS_PROFILE=<name>`.
3. Symlink `~/.harness/active_profile`.
4. `~/.harness/config.toml :: default_profile`.
5. Profile literal `default` (creado automáticamente al primer boot).

## Cambio de profile

```bash
harness profile use work-acme
```

Efectos:
1. Symlink `active_profile` se actualiza.
2. `harness-server` recibe SIGHUP (o equivalente) → recarga config y rotates state.
3. Bind-mounts del container (`/root/.claude`, `/root/.codex`) se ajustan al nuevo `cli-state/`.
4. UI muestra badge con nuevo profile + recarga del dashboard.

**Threads activos del profile anterior**: persisten en disco. Al volver con `harness profile use personal`, siguen ahí.

## Comandos CLI

```bash
harness profile list                       # nombres + última actividad
harness profile current                    # nombre del activo
harness profile use <name>                 # cambia activo
harness profile create <name> [--from-template <template>]
harness profile delete <name>              # requiere confirmación + backup automático
harness profile rename <old> <new>
harness profile sync [--all]               # git pull/push del activo (o todos)
harness profile export <name> > backup.tar.zst
harness profile import backup.tar.zst [--as <new-name>]
```

## UI

- Sidebar fija muestra: `[badge profile] [dropdown switch]`.
- Switch desde dropdown lanza confirmación si hay spawns activos ("3 agentes corriendo se pausarán").
- Settings → "Profiles" muestra lista + acciones (rename, delete, export).

## Aislamiento de credenciales

`cli-state/.claude/` y `cli-state/.codex/` por profile. El backend bind-mountea estos directorios al container al cambiar profile:

```yaml
# docker-compose.yml usa una variable
volumes:
  - ${HARNESS_PROFILE_DIR}/cli-state/.claude:/root/.claude
  - ${HARNESS_PROFILE_DIR}/cli-state/.codex:/root/.codex
```

Alternativa más simple y operativamente preferida: **symlinks internos del container**. `/root/.claude` es un symlink a `/data/profiles/<active>/cli-state/.claude/`. Al cambiar profile, solo se actualiza el symlink interno (sin restart de docker-compose).

Auth inicial del CLI por profile:
```bash
harness profile use work-acme
harness shell                  # abre shell dentro del container
# inside container:
claude login                   # auth queda en /data/profiles/work-acme/cli-state/.claude/
exit
```

## Defaults al crear un profile

`profiles/<new>/config.toml`:
```toml
default_model = "claude-sonnet-4-5"            # heredable
approval_mode = "risky-only"
sandbox_level = "workspace"

[budget]
usd_max          = 10
wallclock_max_s  = 3600
turns_max        = 100

[mcp]
enabled = ["harness-bridge"]                   # solo harness-bridge por default

[git]
auto_sync = false
```

`PROFILE.md` template:
```markdown
# Profile: <name>

## Contexto laboral
[describe el rol, equipo, proyectos típicos]

## Estilo de comunicación esperado
[formal | casual | técnico estricto | etc.]

## No-goals (cosas que este profile NO hace)
[ej. "no toco infraestructura de producción"]

## Stakeholders y referencias
[links a wikis internos, docs]
```

## Profile "default"

Si el usuario nunca crea profiles, todo funciona en `profiles/default/`. Es transparente; las herramientas no exigen pensar en profiles hasta que el usuario quiera más de uno.

## Lo que NO es un profile

- ❌ No es un **proyecto**. Un proyecto es un directorio con código (cwd). Varios proyectos pueden coexistir dentro del mismo profile (cada thread guarda su `working_dir`).
- ❌ No es una **identidad del usuario** distinta. Eres la misma persona (USER.md es global). Un profile es contexto laboral, no identidad.
- ❌ No es **multi-tenancy**. Single-user local, dos contextos. Si quieres multi-user, son instancias separadas del backend.

## Anti-patrones

| Mal | Bien |
|---|---|
| Profile por proyecto | Profile por contexto (trabajo); proyectos coexisten dentro |
| Compartir auth entre profiles | `cli-state/` aislado |
| Push de memoria del trabajo a tu GH personal | Cada profile su remote; `work` → empresa, `personal` → GH personal |
| `USER.md` distinto por profile sin razón | Solo override si realmente cambia tu identidad |
| Skills duplicadas en cada profile | Promover genéricas a `shared/` |
