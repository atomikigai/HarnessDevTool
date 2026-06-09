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

## Criterio de regresión

- TerminalView no debe bloquear input durante salida grande.
- ChatView no debe quedarse en estado vacío si hay transcript en disco.
- La búsqueda de contexto debe responder en menos de 50 ms para sesiones normales.
