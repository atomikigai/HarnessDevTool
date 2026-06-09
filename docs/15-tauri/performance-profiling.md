---
id: tauri/performance-profiling
title: Performance profiling Tauri
shard: 15-tauri
tags: [tauri, performance, pty, chatview, profiling]
summary: Comandos y métricas para comparar PTY, ChatView y crecimiento de contexto.
related: [memory/search-and-index]
sources: []
---

# Performance profiling Tauri

## Preparación

```bash
just dev-tauri
```

`dev-tauri` recompila el sidecar antes de abrir la app. Para release:

```bash
just tauri-build
```

`tauri-build` recompila el sidecar, genera el bundle y ejecuta
`scripts/check-tauri-bundle.sh`. Ese check falla si ningún paquete inspeccionable
contiene `harness-server`.

En Linux el default local es `deb,rpm` para evitar que un entorno sin
`linuxdeploy` bloquee el release verificable. Para pedir AppImage explícitamente:

```bash
HARNESS_TAURI_BUNDLES=deb,rpm,appimage just tauri-build
```

Para verificar paquetes ya generados sin reconstruir:

```bash
just tauri-bundle-check
```

## Métricas mínimas

| Área | Qué medir |
|---|---|
| PTY throughput | bytes/s, chunks/s, dropped frames, tiempo hasta primer frame |
| TerminalView | FPS percibido, latencia input→render, scrollback grande |
| ChatView | tiempo de replay, tiempo de markdown render, memoria con transcript grande |
| Context governor | latencia de indexación FTS, tiempo de búsqueda, eventos al 35/40% |

## Checks rápidos

1. Abrir una sesión y ejecutar salida grande:

```bash
python - <<'PY'
for i in range(20000):
    print(f"{i:05d} lorem ipsum dolor sit amet")
PY
```

2. Cambiar a Chat y validar replay con markdown largo.

3. En Info → Context buscar términos de checkpoint.

4. En DevTools medir memoria antes/después de:

- 20k líneas PTY.
- 100 turns de ChatView.
- 20 búsquedas FTS.

## Benchmark reproducible de UI

```bash
just perf-tauri-ui
```

Este benchmark levanta Vite con Playwright y mocks grandes:

- 120 turns de transcript con markdown.
- 20 resultados de búsqueda Context/FTS.
- Montaje de TerminalView.

La salida incluye una línea `PERF {...}` con:

| Métrica | Significado |
|---|---|
| `dashboardReadyMs` | Tiempo hasta que el dashboard puede interactuar. |
| `chatReplayMs` | Tiempo de replay/render markdown de ChatView. |
| `contextSearchMs` | Tiempo de abrir Info, buscar y pintar resultados. |
| `terminalMountMs` | Tiempo de montar la pestaña Terminal. |
| `domNodes` | Tamaño DOM final aproximado. |
| `usedJSHeapMB` | Heap JS si Chromium expone `performance.memory`. |

## Criterio de regresión

- TerminalView no debe bloquear input durante salida grande.
- ChatView no debe quedarse en estado vacío si hay transcript en disco.
- La búsqueda de contexto debe responder en menos de 50 ms para sesiones normales.
