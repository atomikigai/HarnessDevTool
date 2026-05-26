---
id: frontend-shell/tauri-vs-app-server
title: "[Tombstone] Tauri vs App Server"
shard: 05-frontend-shell
tags: [tombstone, deprecated]
summary: Tauri descartado tras priorizar WEB UI. App Server renombrado a harness-server.
related: [build-plan/decisions-locked, build-plan/tech-stack-locked, memory/decisions]
sources: []
---

# [Tombstone] Tauri vs App Server

> Esta comparación quedó **resuelta** durante planning: **WEB UI con PWA install** ganó.

## Resumen de la decisión

Razones para descartar Tauri:
- No permite acceso desde otra máquina en la LAN.
- Deploy es un instalador por OS (firmar, distribuir).
- Imposible self-host headless.
- xterm.js con WebGL2 cubre el "feel" de terminal sin Tauri.

Ver entrada de memoria correspondiente cuando exista: `memory/decisions/2026-05-26-tauri-out.md` (template en [[memory/entry-format]]).

## Estado actual

- **Frontend**: SvelteKit con `adapter-node` en container `node:alpine`.
- **Backend**: `harness-server` (Axum) en container distroless.
- **Wire**: HTTP+SSE directo browser ↔ backend con CORS.

## Ver en su lugar

- [[build-plan/decisions-locked]] §"Arquitectura" — decisión bloqueada
- [[build-plan/tech-stack-locked]] — stack actual
- [[frontend-shell/tech-stack]] — detalle frontend
