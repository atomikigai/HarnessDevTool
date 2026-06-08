---
id: frontend-shell/tauri-vs-app-server
title: "[Tombstone] Tauri vs App Server"
shard: 05-frontend-shell
tags: [tombstone, deprecated]
summary: Tauri descartado para la UI primaria F0-F6. Revisitado post-F6 como baseline desktop contra Slint performance-first.
related: [build-plan/decisions-locked, build-plan/tech-stack-locked, memory/decisions]
sources: []
---

# [Tombstone] Tauri vs App Server

> Esta comparación quedó **resuelta** durante planning: **WEB UI con PWA install** ganó.

> Revisitado 2026-06-08: la decisión original sigue vigente para la UI
> primaria/self-host F0-F6. Para una **app desktop post-F6**, se abre un track
> paralelo que no toca la web UI y usa la UI actual como referencia funcional.
> Tauri queda como baseline de migración rápida; Slint sigue como candidato
> performance-first. La decisión final se toma con métricas, no por preferencia.

## Resumen de la decisión

Razones para descartar Tauri:
- No permite acceso desde otra máquina en la LAN.
- Deploy es un instalador por OS (firmar, distribuir).
- Imposible self-host headless.
- xterm.js con WebGL2 cubre el "feel" de terminal sin Tauri.

Razones para medir Tauri como baseline si se abre una surface desktop:
- Reutiliza rutas, stores, componentes y estilos de `frontend/**`.
- Mantiene una sola implementación para web + desktop.
- Evita reescribir módulos completos en `.slint`.
- Conserva el contrato HTTP+SSE con `harness-server`.

Razones para mantener Slint como candidato serio:
- Prioridad explícita del usuario: performance desktop.
- UI nativa con menor overhead potencial que una WebView.
- Mejor candidato para medir startup, memoria, CPU/render y respuesta con listas
  grandes de Agents.
- Costo: requiere reconstruir la UI desktop tomando SvelteKit solo como
  referencia funcional, no como código compartido.

Ver entrada de memoria correspondiente cuando exista: `memory/decisions/2026-05-26-tauri-out.md` (template en [[memory/entry-format]]).

## Estado actual

- **Frontend**: SvelteKit con `adapter-node` en container `node:alpine`.
- **Backend**: `harness-server` (Axum) en container distroless.
- **Wire**: HTTP+SSE directo browser ↔ backend con CORS.
- **Desktop**: no es surface primaria; track paralelo. Mantener
  `experiments/slint-agents` como spike performance-first y comparar contra un
  spike Tauri mínimo antes de cerrar tecnología.

## Ver en su lugar

- [[build-plan/decisions-locked]] §"Arquitectura" — decisión bloqueada
- [[build-plan/tech-stack-locked]] — stack actual
- [[frontend-shell/tech-stack]] — detalle frontend
