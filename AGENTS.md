# AGENTS.md

Guía corta para agentes (humanos y LLMs) que trabajen en este repo.

## Qué es esto

**HarnessDevTool** es un harness orquestador de agentes de codificación (Claude
Code, Codex, etc.) que los lanza como subprocesos a traves de un PTY, captura
su stream estructurado, persiste la conversacion en un log append-only y la
expone a una UI web (SvelteKit) via una API HTTP servida por un backend Rust
(`harness-server`).

## Arquitectura en una linea

`frontend (SvelteKit, :8080)` <-> `backend (harness-server, :7777)` <-> `PTY child agents`
con estado en `HARNESS_HOME` (default `~/.harness`, montado como `/data` en
container).

## Convenciones criticas (no negociables)

- **Append-only**: el log de conversacion nunca se reescribe; solo se anaden
  eventos. Cualquier "edicion" es un evento nuevo que referencia al anterior.
- **`X-Protocol-Version`**: todo request/response HTTP entre frontend y backend
  declara la version del protocolo via este header. Mismatch => error explicito.
- **`ts-rs` como fuente de verdad de tipos**: los tipos compartidos se definen
  en Rust y se exportan a TypeScript via `just gen-types`. Nunca editar a mano
  los `.ts` generados en `frontend/src/lib/api/types/`.
- **Puertos fijos**: backend `7777`, frontend `8080`. En docker-compose se
  comunican por nombre de servicio (`backend:7777`).
- **`HARNESS_HOME`**: variable de entorno para la raiz de estado. En container
  siempre vale `/data`; en host default `~/.harness`.

## Propiedad por dominio (no cruzar paths)

| Dominio    | Paths                                     |
| ---------- | ----------------------------------------- |
| backend    | `backend/**` (incluye `backend/Dockerfile`) |
| frontend   | `frontend/**` (incluye `frontend/Dockerfile`) |
| infra/raiz | `Justfile`, `docker-compose*.yml`, `.env.example`, `.gitignore`, `.editorconfig`, `AGENTS.md` |
| docs       | `docs/**`                                 |

Si trabajas en un dominio, no modifiques los otros.

## Como correr

```bash
just dev-backend     # solo backend (cargo run)
just dev-frontend    # solo frontend (pnpm dev)
just dev             # ambos en paralelo, sin docker
just docker-up       # stack de produccion via compose
just docker-dev      # stack con hot-reload (cargo-watch + vite)
just gen-types       # regenera tipos TS desde Rust
just test            # tests de ambos stacks
```

Copia `.env.example` a `.env` y ajusta `HARNESS_HOME` si no quieres
`~/.harness`.

## CLIs soportados y auth compartida (importante)

El harness sabe spawnear 4 CLIs: `claude`, `codex`, `cursor` (binario
`cursor-agent`) y `antigravity` (binario `agy`). Cada uno guarda su token de
auth en su propio directorio del home: `~/.claude/`, `~/.codex/`,
`~/.cursor/`, `~/.antigravity/`.

En docker, esos directorios se **bind-mountean RW** al container — el harness
y el host comparten literalmente el token store. Esto permite que el refresh
del token sobreviva a destruir el container.

**Restriccion**: no corras el mismo CLI con otra cuenta en el host mientras
hay sesion activa en el harness. Podria confundir el token store del CLI.
Ver [[agents/supported-clis]] y la decision N4 en
[[build-plan/decisions-locked]].

## Referencia

El indice completo de documentacion vive en
[`docs/README.md`](./docs/README.md): shards numerados por dominio
(`00-meta` ... `14-memory`).
