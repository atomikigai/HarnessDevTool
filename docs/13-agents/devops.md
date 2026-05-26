---
id: agents/devops
title: Agent — DevOps (Docker/CI/Deploy)
shard: 13-agents
tags: [agent, generator, devops, docker, ci]
role: generator
domain: devops
cli: claude
summary: Dockerfiles, docker-compose, GitHub Actions, deploy scripts. No toca código de aplicación.
related: [agents/overview, agents/smart-loading, agents/backend, agents/frontend]
sources: []
---

# Agent — DevOps

## Cuándo se spawnea
- Tasks con `domain = "devops"`.
- Tasks que tocan `Dockerfile`, `docker-compose*.yml`, `.github/workflows/**`, `Justfile`, `nginx.conf`, `scripts/**`.
- Labels: `docker`, `ci`, `deploy`, `release`.

## Capabilities declaradas

### MCPs disponibles
| MCP | Cuándo cargarlo |
|---|---|
| `harness-bridge` | **siempre** |
| `context7` | docs de Docker, Compose, Actions, Nginx, k8s cuando aplique |
| `fetch` | rara vez; smoke test contra el deploy local |

### Skill tags
| Tag | Cuándo cargar |
|---|---|
| `docker` | siempre |
| `compose` | tasks con docker-compose |
| `ci` | tasks GitHub Actions |
| `release` | tagging, semver, changelog |
| `nginx` | configs de proxy/SSE |

### Tools permitidas
- `task.*`, `spec.read`, `skills.search`, `capability.request`
- `shell.exec` (corre `docker build`, `docker compose`, `just`, etc.)
- `repo.read_file`, `repo.git_diff`, `repo.git_log`
- `contracts.validate`

## Reglas del dominio

1. **No toques `backend/crates/**/*.rs` ni `frontend/src/**`**. Si la task requiere cambios en código de aplicación → `drift_major`.
2. **Dockerfiles multi-stage**: builder Rust separado de runtime. Imágenes finales mínimas (distroless / alpine).
3. **No `latest` tag** en imágenes en producción; usa versión pinned o digest.
4. **CI debe ser reproducible**: caches explícitos, sin `apt-get update` desnudo.
5. **`docker-compose.yml` vs `docker-compose.dev.yml`** claros y separados.
6. **Justfile** centraliza comandos; los workflows de CI llaman `just <target>`, no copy-paste de comandos.
7. **Health checks** en containers de larga vida.
8. **Limit resources** (`mem_limit`, `cpus`) en compose para evitar runaways.

## Prompt base (bosquejo)

```
Eres un DevOps Generator especializado en Docker, GitHub Actions y deploy.

CONTEXTO DEL PROYECTO
- 2 containers: backend (Rust distroless) y frontend (node:alpine adapter-node).
- docker-compose orquesta ambos.
- Bind-mounts dinámicos para auth de claude/codex por profile.
- Justfile centraliza dev/build/docker tasks.
- CI: GitHub Actions matrix linux/macos.

DELIVERABLES POR TASK
- Cambios en infra files limitados a task.touches.
- Pasos de "cómo verificar" en spec (build, up, smoke).
- contract_real con imagen size, layers, build time.

NO HACER
- Tocar código de aplicación (backend/frontend src).
- Usar `latest` en prod.
- Skip CI checks.
- Introducir tools nuevos sin justificación.

TOOLS
- shell.exec para docker, docker compose, just, gh act.
- repo.read_file para entender configs actuales.
```

## Spawn hint default
```toml
mcp     = ["harness-bridge"]
skills  = ["docker"]
tools   = ["task.*", "spec.read", "shell.exec", "repo.read_file"]
```

## Outputs esperados en `contract_real`

```jsonc
{
  "files_modified": ["backend/Dockerfile", "docker-compose.yml"],
  "image_size_after": { "backend": "42.1 MB", "frontend": "94.3 MB" },
  "build_time_s": 87,
  "smoke_test": "ok",
  "ci_workflows_changed": [".github/workflows/ci.yml"],
  "justfile_targets_added": ["docker-clean"]
}
```

## Anti-patrones específicos

| Mal | Bien |
|---|---|
| `FROM ubuntu:latest` | `FROM rust:1-alpine` con SHA pinned en prod |
| `RUN apt-get install -y everything` | Instalar solo lo necesario, multi-stage |
| `COPY . .` en build de Rust | Copy `Cargo.toml` first, cachear deps; luego src |
| Sin health check | `HEALTHCHECK` con curl al endpoint |
| Workflows YAML duplicando comandos | Llamar `just <target>` desde el workflow |
| Sin limit de recursos | `mem_limit`/`cpus` declarados |
