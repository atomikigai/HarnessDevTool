---
id: frontend-shell/tech-stack
title: Stack del frontend
shard: 05-frontend-shell
tags: [frontend, sveltekit, tauri, tailwind]
summary: SvelteKit + Tauri + Tailwind + virtual lists para tablas grandes.
related: [frontend-shell/sveltekit-integration, frontend-shell/tauri-vs-app-server]
sources: []
---

# Stack frontend

## Elección
- **SvelteKit** — DX, runtime liviano, stores nativos para streaming.
- **Tauri v2** — empaqueta el binario `harness-app-server` como sidecar; bridge a comandos.
- **TailwindCSS** + tokens propios (sin design system pesado).
- **CodeMirror 6** — editores (SQL, Markdown, JSON).
- **xterm.js** — terminal para sesiones Claude CLI y SSH.
- **TanStack Virtual** — listas virtualizadas (tablas DB grandes).
- **lucide-svelte** — iconos.

## Por qué no Electron
- 10× menos peso (~10 MiB vs ~120 MiB).
- Ya hablamos Rust → bridge Tauri es natural.
- WebView nativo → mejor batería.

## Por qué SvelteKit sobre Svelte vanilla
- Routing built-in (sidebar → vistas).
- SSR opt-in si en el futuro publicamos modo web.
- Convenciones (`+page.svelte`, `+layout.svelte`) facilitan que un agente añada vistas (ver [[recipes/add-frontend-route]]).

## Layout SvelteKit

```
apps/desktop/
├── src-tauri/            # crate Rust de Tauri
│   ├── tauri.conf.json
│   └── src/main.rs       # sidecar spawn, IPC con app-server
├── src/
│   ├── lib/
│   │   ├── rpc/          # cliente JSON-RPC tipado
│   │   ├── stores/       # threads, items, modules
│   │   └── components/   # Sidebar, ChatStream, TableVirtual
│   ├── routes/
│   │   ├── +layout.svelte
│   │   ├── agents/+page.svelte
│   │   ├── db/[connection]/+page.svelte
│   │   └── ssh/[host]/+page.svelte
└── static/
```

## Theming
Dos temas (dark/light) con variables CSS. Mismo árbol de tokens que `docs/architecture.html` para consistencia visual al onboardear devs.
