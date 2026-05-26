---
id: frontend-shell/tech-stack
title: Stack del frontend
shard: 05-frontend-shell
tags: [frontend, sveltekit, tailwind, shadcn]
summary: SvelteKit + adapter-node + Tailwind + shadcn-svelte + lucide + xterm.js + valibot.
related: [build-plan/tech-stack-locked, frontend-shell/sveltekit-integration, frontend-shell/routing-shell]
sources: []
---

# Stack frontend

## Elección final

| Capa | Tecnología | Razón |
|---|---|---|
| Framework | **SvelteKit (Svelte 5)** | DX, runtime liviano, stores nativos |
| Adapter | **`@sveltejs/adapter-node`** | corre como server Node dentro del container; soporta SSR futuro si hace falta |
| Estilos | **TailwindCSS** + tokens shadcn | utility-first, sin design system pesado |
| Componentes UI | **shadcn-svelte** | añadidos selectivamente con CLI; copiamos código, no dependencia opaca |
| Iconos | **lucide-svelte** | tree-shake amigable; re-export centralizado |
| Terminal | **xterm.js** + addon-fit + addon-web-links + addon-unicode11 + (opcional) addon-webgl | render PTY perfomante en browser |
| Editor de código | **CodeMirror 6** | `@codemirror/lang-sql`, `@codemirror/lang-markdown` |
| Listas grandes | **TanStack Virtual** | tablas DB, listas SFTP |
| Markdown streaming | **`marked`** + sanitizer | render incremental de items |
| Validación runtime | **valibot** | tree-shake, bundle chico |
| Estado | **stores Svelte nativos** | sin Redux/Pinia |
| Package manager | **pnpm** | |
| Testing | **Vitest** + **Playwright** (F2+) | |
| Lint/format | **eslint** + **prettier** + **svelte-check** | |

## Lo que NO usamos

- ❌ **Tauri** — descartado (acceso remoto, deploy simple, self-host triunfan).
- ❌ **Electron** — peso 10×.
- ❌ **adapter-static** — vamos con `adapter-node` para tener server flexible.
- ❌ **zod** — preferimos valibot por bundle.
- ❌ **Redux / Pinia / writable-store-libs** — stores Svelte nativos bastan.
- ❌ **tRPC / GraphQL** — HTTP+SSE basta.
- ❌ **Turborepo / Nx** — monorepo simple con Justfile.

## Layout del proyecto

Ver [[build-plan/repo-layout]] §"frontend/". Resumen:
```
frontend/
├── Dockerfile                     # node:alpine + pnpm + adapter-node runtime
├── package.json
├── pnpm-lock.yaml
├── svelte.config.js               # adapter-node
├── vite.config.ts                 # proxy dev :7777
├── tailwind.config.ts
├── tsconfig.json
├── components.json                # shadcn-svelte
├── eslint.config.js
└── src/
    ├── app.html
    ├── app.css                    # tailwind + tokens shadcn
    ├── lib/
    │   ├── api/                   # cliente HTTP + SSE; types/ ← ts-rs
    │   ├── components/
    │   │   ├── ui/                # shadcn-svelte (copiados con CLI)
    │   │   └── app/               # nuestros (Sidebar, ChatStream, etc.)
    │   ├── stores/                # threads, tasks, sessions, profiles
    │   ├── hooks/
    │   ├── utils/
    │   ├── validators/            # valibot schemas
    │   └── icons.ts               # re-export lucide
    └── routes/
        ├── +layout.svelte
        ├── +layout.ts             # ssr=false, csr=true
        ├── +page.svelte           # dashboard con CONTINUITY banner
        └── ...
```

## Tema

Dark por defecto. Light togglable. Tokens shadcn en `app.css`; los mismos colores que `docs/architecture.html`.

## PWA install

Manifest.json + service worker básico para "Install app" en Chrome/Edge. Cubre 80% de "feel" nativo sin Tauri.
- Ícono en dock/menu.
- Ventana sin barra de browser.
- Shortcuts del SO (Cmd+Tab).

Implementado en F1 (cuando la UI ya tenga contenido real).

## Render del PTY

xterm.js con WebGL2 renderer hace 60fps sostenidos con output de 1+ MB/s. Igual o mejor que terminales nativos en perf.

Configuración:
```ts
import { Terminal } from "xterm";
import { FitAddon } from "xterm-addon-fit";
import { WebLinksAddon } from "xterm-addon-web-links";
import { Unicode11Addon } from "xterm-addon-unicode11";
import { WebglAddon } from "xterm-addon-webgl";

const term = new Terminal({
  fontFamily: "JetBrains Mono, monospace",
  fontSize: 14,
  cursorBlink: true,
  scrollback: 10000,
  allowProposedApi: true,
});
term.loadAddon(new FitAddon());
term.loadAddon(new WebLinksAddon());
term.loadAddon(new Unicode11Addon());
try { term.loadAddon(new WebglAddon()); } catch { /* fallback canvas */ }
term.unicode.activeVersion = "11";
```

## Anti-patrones

| Mal | Bien |
|---|---|
| Inventar componentes UI sin usar shadcn-svelte | Añadir con CLI; copiar código |
| Importar `lucide-svelte` directo en cada archivo | Re-export en `lib/icons.ts` para tree-shake |
| Estado global pesado (Pinia clone) | Stores Svelte + derived |
| Markdown render no incremental | Reactivo a deltas SSE |
| Validar inputs solo en backend | valibot en boundary; backend valida igual |
