# AGENTS.md

Guía corta para agentes (humanos y LLMs) que trabajen en este repo.

## Qué es esto

**HarnessDevTool** es un harness orquestador de agentes de codificación (Claude
Code, Codex, etc.) que los lanza como subprocesos a traves de un PTY, captura
su stream estructurado, persiste la conversacion en un log append-only y la
expone a una UI web (SvelteKit) via una API HTTP servida por un backend Rust
(`harness-server`).

## Arquitectura en una linea

`frontend (SvelteKit)` <-> `backend (harness-server)` <-> `PTY child agents`
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
- **Puertos locales dinámicos**: en dev, `just dev`/`just dev-raw`/`just docker-dev`
  eligen puertos altos libres si `BACKEND_PORT`/`FRONTEND_PORT` no están definidos.
  `HARNESS_CORS_ORIGIN` se deriva del puerto frontend elegido. En docker-compose
  los servicios se comunican internamente por nombre de servicio (`backend:7777`).
- **`HARNESS_HOME`**: variable de entorno para la raiz de estado. En container
  siempre vale `/data`; en host default `~/.harness`.
- **`.env` versionado**: el archivo `.env` se sube al repo deliberadamente. No
  lo ignores, elimines, renombres ni reemplaces por `.env.example` salvo pedido
  explicito.
- **QA frontend como usuario real**: cualquier cambio en `frontend/**`, o cambio
  backend que afecte contratos consumidos por frontend, requiere validacion con
  `agent-browser` como usuario real. El agente QA debe revisar pantalla, flujo,
  legibilidad de datos, estados visibles y que la interfaz sea user friendly.
  `pnpm check`, tests unitarios o Playwright no sustituyen esta validacion
  exploratoria; si el flujo cruza backend/frontend, levanta ambos servicios y
  prueba desde la UI.
- **`DESIGN.md` por repo frontend**: todo repo/app frontend trabajado por el
  harness debe tener un `DESIGN.md` como fuente de verdad visual (`frontend/DESIGN.md`
  si existe `frontend/`, si no `DESIGN.md` en la raiz). Si cambian tokens,
  estilos globales, patrones de layout o direccion visual, el agente
  correspondiente debe actualizarlo en la misma tarea.

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

Usa el `.env` versionado del repo y ajusta `HARNESS_HOME` si no quieres
`~/.harness`. `.env.example` queda como referencia de variables.

## Herramientas disponibles (skills)

El directorio `skills/bundled/` contiene guías de uso para las herramientas instaladas por `just setup`.
Cada skill describe cuándo usar la herramienta, patrones concretos y cuándo no usarla:

| Skill | Herramienta | Uso principal |
|---|---|---|
| `opensrc` | CLI npm | Leer source code de dependencias |
| `agent-browser` | CLI npm | Automatización browser compacta para agentes |
| `context7` | MCP/CLI npm | Documentación actual de librerías para agentes de coding |
| `crawl4ai-context` | MCP/CLI npm | Extracción/crawling de contexto web y docs externas |
| `excalidraw-board` | MCP HTTP | Diagramas, boards y wireframes editables |
| `excalidraw-diagram` | Skill harness | Diagramas Excalidraw técnicos con argumento visual |
| `skill-creator` | Skill harness | Crear, adaptar y evaluar skills del harness |
| `design-md` | Skill harness | Crear y mantener `DESIGN.md` como fuente visual |
| `frontend-design` | Skill harness | Diseño UI frontend productivo y pulido |
| `shadcn-svelte` | Skill harness/docs | Componentes shadcn-svelte, Bits UI y Tailwind v4 |
| `performance-optimization` | Skill harness | Medicion y optimizacion performance frontend/backend |
| `code-simplification` | Skill harness | Refactors de claridad sin cambiar comportamiento |
| `code-review-and-quality` | Skill harness | Revision multi-eje antes de merge/commit |
| `ast-grep` | CLI npm | Búsqueda estructural de código por AST |
| `difftastic` | CLI cargo | Diffs semánticos sin ruido |
| `cargo-nextest` | CLI cargo | Tests Rust más rápidos |
| `cargo-audit` | CLI cargo | Auditoría de CVEs en deps Rust |
| `pdf-oxide` | CLI cargo | Extracción de PDFs a markdown |
| `efficient-cli` | CLIs Rust/Go | Búsqueda, selección, HTTP, benchmarks, watchers y streams |
| `rust-tooling` | CLIs cargo | Calidad, deps, tamaño binario y profiling Rust |
| `security-tooling` | CLIs Rust/Go/sistema | Secret scanning, CVEs, containers, Dockerfiles y shell |
| `n8n-workflow-automation` | Skill harness | Crear, validar, importar y probar workflows de n8n desde agentes |

## Uso esperado de skills/capabilities

El harness debe tratar skills, MCPs y tools como un grafo de capacidades, no
como listas aisladas. Para cada task, carga lo minimo necesario segun paths,
keywords, rol y contrato afectado; registra lo cargado en `loaded_capabilities`.

- **Frontend/UI**: cargar `agent-browser`; si hay diseno/pulido, tambien
  `frontend-design`, `design-md` y `shadcn-svelte` cuando toque componentes UI.
  Leer `frontend/DESIGN.md`, correr `pnpm check`, validar como usuario real con
  `agent-browser`, y actualizar `DESIGN.md` si cambian estilos o direccion visual.
- **Frontend + backend contract**: levantar ambos servicios y probar desde la
  UI con `agent-browser`; API checks o tests estaticos no bastan.
- **Performance**: cargar `performance-optimization`; medir baseline, encontrar
  cuello de botella, corregir, medir despues y anotar numeros.
- **Refactor de claridad**: cargar `code-simplification`; preservar comportamiento,
  scope acotado, tests/checks enfocados y diff revisable.
- **Revision/calidad**: cargar `code-review-and-quality` antes de merge/commit
  relevante; revisar correctness, legibilidad, arquitectura, seguridad,
  performance y evidencia QA.
- **Diagramas**: usar `excalidraw-board` para la integracion editable/MCP y
  `excalidraw-diagram` para que el diagrama argumente visualmente con evidencia
  tecnica real.
- **n8n/workflows/automatizaciones**: cargar `n8n-workflow-automation`; usar
  el grupo MCP `n8n` para validar/guardar/importar/probar workflows. No guardar
  secretos crudos en JSON de workflows; usar credenciales de n8n o variables de
  entorno referenciadas por nombre.

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
