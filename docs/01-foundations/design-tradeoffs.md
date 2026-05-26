---
id: foundations/design-tradeoffs
title: Trade-offs de diseño
shard: 01-foundations
tags: [tradeoffs, design, decisions]
summary: Decisiones recurrentes con su pro/contra y la elección por defecto del proyecto.
related: [foundations/harness-concept, foundations/openai-codex-architecture, architecture/system-overview]
sources: []
---

# Trade-offs

## 1. Core compartido vs lógica por surface
- **Compartido (elegido)**: paridad, una sola fuente de bugs. Costo: contrato JSON-RPC estable.
- Por surface: rápido inicio, divergencia rápida.

## 2. App Server externo vs in-process en cada surface
- **App Server externo (elegido)**: thread sobrevive al cliente; web/CLI/desktop con mismo backend.
- In-process: menos latencia, más simple inicialmente; obliga a re-implementar persistencia por surface.

## 3. JSON-RPC stdio vs HTTP local vs gRPC
- **Stdio (elegido)**: cero red, fácil de bundlear, child-process pattern.
- HTTP local: discoverable pero abre puertos.
- gRPC: más rápido, peor para streaming text-first y debug.

## 4. Stateless vs `previous_response_id`
- **Stateless (elegido)**: ZDR, multi-provider. Costo: prompt completo en cada request.
- `previous_response_id`: bytes menores, lock-in al provider.

## 5. Compaction vs context reset
- **Compaction**: barato, conserva semántica latente (con providers que lo soportan).
- **Reset + handoff**: combate context anxiety, requiere protocolo de hand-off.
- Política: compaction por defecto; reset cuando se detecta anxiety (heurística de tokens restantes).

## 6. Tools sandboxed vs trust-the-tool
- **Sandboxed (elegido para nativas)**: seguridad por defecto.
- MCP externos se auto-sandboxean (el harness no puede inspeccionar su código).

## 7. Una sola UI (mono-app) vs surfaces múltiples
- **Mono-app SvelteKit (elegido para v1)**: barra lateral con Agentes / DB / SSH. Costo de mantenimiento bajo.
- CLI y App Server se mantienen reutilizables si en v2 sumamos más surfaces.

## 8. Tauri vs Electron vs web puro
- **Tauri (elegido)**: binario chico, ya hablamos Rust, embebido del App Server natural.
- Electron: comunidad mayor, peso 10×.
- Web puro: requiere servidor remoto del App Server.

## Regla guía
Cada componente del harness **codifica una asunción sobre el modelo**. En cada release del modelo, audita: ¿esta asunción sigue viva? Si no, retira el componente. (Anthropic).
