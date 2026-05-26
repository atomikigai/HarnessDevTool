---
id: build-plan/phase-1-sessions
title: F1 — Sesiones PTY (claude/codex desde la UI)
shard: 12-build-plan
tags: [phase, f1, pty, sessions, claude, codex]
summary: Spawn de un CLI agéntico desde la UI con terminal en vivo bidireccional.
related: [build-plan/phase-0-skeleton, build-plan/phase-2-tasks-mcp, module-agents/session-pty]
sources: []
---

# F1 — Sesiones

## Meta
Que el usuario haga clic en "New session" en la UI, elija `claude` o `codex`, vea el binario arrancando en un xterm.js, pueda escribir input y vea la respuesta del modelo en streaming. **Una sola sesión a la vez**, sin tareas, sin MCP.

## Entregables

### Backend
- [ ] Crate **`harness-session`**:
  - [ ] Wrapper sobre `portable-pty`.
  - [ ] `Manager { sessions: DashMap<SessionId, AgentSession> }`.
  - [ ] `spawn(kind: Claude | Codex, cwd, args[])` → `SessionId`.
  - [ ] `input(id, bytes)`, `resize(id, cols, rows)`, `kill(id, signal)`.
  - [ ] Reader task que empuja bytes del PTY a un `EventSink`.
  - [ ] Cleanup: drop de sesión kill el child + cancela tasks.
- [ ] `harness-server`:
  - [ ] `POST /api/threads/:id/sessions { kind, cwd? }` → spawn + devuelve `session_id`.
  - [ ] `GET /api/sessions/:sid` → metadata.
  - [ ] `POST /api/sessions/:sid/input` body raw bytes.
  - [ ] `POST /api/sessions/:sid/resize { cols, rows }`.
  - [ ] `DELETE /api/sessions/:sid`.
  - [ ] SSE `/api/events?thread=:id&session=:sid` emite `session.output { seq, b64data }`.
- [ ] Detección de binarios:
  - [ ] `which claude` / `which codex` al arrancar; cachear path.
  - [ ] Si no existen, endpoint devuelve error con mensaje claro y comandos de instalación sugeridos.
- [ ] Persistencia del output:
  - [ ] `~/.harness/profiles/<p>/sessions/<sid>/output.log` (raw bytes, rotación a 50 MiB).

### Docker
- [ ] Backend Dockerfile: **bind-mounts** documentados en `docker-compose.yml`:
  ```yaml
  volumes:
    - /usr/local/bin/claude:/usr/local/bin/claude:ro
    - /usr/local/bin/codex:/usr/local/bin/codex:ro
    - ${HOME}/.claude:/root/.claude:rw
    - ${HOME}/.codex:/root/.codex:rw
    - ./.harness-data:/data
  ```
- [ ] Verificar que el binario monto se ejecuta dentro del container distroless. Si requiere libs dinámicas → switchear backend a `debian:slim`.

### Frontend
- [ ] Página `/threads/[id]/sessions/[sid]/+page.svelte`:
  - [ ] Componente `<TerminalView>` con xterm.js + `addon-fit` + `addon-web-links` + `addon-unicode11`.
  - [ ] Conexión SSE al endpoint de eventos filtrado por session.
  - [ ] Input local → POST a `/sessions/:sid/input`.
  - [ ] Resize observer → debounce 100ms → POST `/sessions/:sid/resize`.
  - [ ] Botón "Kill".
- [ ] Sidebar muestra sesiones activas (badge contador).
- [ ] Modal "New session" desde dashboard / thread view: select `claude` | `codex`, optional cwd.

## Test de aceptación

1. Spawn `claude` desde UI → terminal aparece, prompt del CLI visible en <2s.
2. Escribir un mensaje y enter → se ve respuesta streaming en la terminal.
3. Resize ventana del browser → la terminal se ajusta, el CLI respeta el nuevo width (ej. tablas re-formateadas).
4. Click "Kill" → child process termina (verificar con `ps`), terminal muestra "[session ended]".
5. Cerrar tab del browser → el child sigue vivo; reabrir el tab → reconecta vía SSE y la terminal **catch-up** desde el `output.log`.
6. `claude` / `codex` no instalados → error claro en UI con instrucciones.

## Lo que NO está en F1

- Múltiples sesiones simultáneas en una sola UI (lista sí, vista activa una).
- Coordinación entre sesiones (F3).
- MCP bridge (F2).
- Persistencia de "qué hizo el modelo" como Items estructurados — solo se guarda el PTY raw.

## Riesgos
- **`claude`/`codex` con libs dinámicas** que no estén en distroless → ya identificado, fallback a `debian:slim`. Decidir al validar el bind-mount.
- **Codificación**: el PTY emite bytes; xterm.js espera string-like. Usar base64 sobre SSE para no corromper UTF-8 partido.
- **ANSI escape codes** raros: `claude` usa colores + cursor positioning; xterm.js los maneja, pero validar redimensiones repetidas.
- **Permisos en bind-mount de `~/.claude/`**: el container corre como `root` (distroless); el dir del host puede tener permisos `0700`. Solución: mount RW con `:Z` en SELinux o cambiar usuario en el container (después).

## Decisiones a confirmar
- ¿Soportar Windows en F1 o esperar a F6? Recomiendo **esperar**: ConPTY + bind-mount cross-OS es scope creep.
- ¿Permitir cambiar `cwd` al spawn? Sí, default a `$HOME` dentro del container; usuario puede sobre-escribir.
