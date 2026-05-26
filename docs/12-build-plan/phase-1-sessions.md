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
Que el usuario haga clic en "New session" en la UI, elija `claude`, `codex` o `cursor`, vea el binario arrancando en un xterm.js, pueda escribir input y vea la respuesta del modelo en streaming. **Múltiples sesiones simultáneas permitidas** (multi-tab), sin tareas, sin MCP.

## Entregables

### Backend
- [ ] Crate **`harness-session`**:
  - [ ] Wrapper sobre `portable-pty`.
  - [ ] `Manager { sessions: DashMap<SessionId, AgentSession> }`.
  - [ ] `spawn(kind: Claude | Codex | Cursor, cwd, args[])` → `SessionId`.
  - [ ] Args por kind incluyen el **bypass del approval interno** del CLI (ej. `claude --dangerously-skip-permissions`, equivalentes en `codex`/`cursor`). Centralizado en `spawn_args_for(kind)`.
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
  - [ ] `which claude` / `which codex` / `which cursor` al arrancar; cachear path.
  - [ ] Si no existen, endpoint devuelve error con mensaje claro y comandos de instalación sugeridos.
- [ ] **AGENTS.md del proyecto** (Q2 resuelta):
  - [ ] Endpoint `POST /api/threads/:id/workdirs { paths: [...] }` para pasar explícitamente las carpetas locales a usar.
  - [ ] Endpoint `POST /api/threads/:id/agents-config/spawn` que arranca un agente dedicado "config-AGENTS" para que ayude al usuario a armar/actualizar el `AGENTS.md`.
  - [ ] Fallback secundario: si `working_dir` está dentro de un git root con `AGENTS.md`, hacer snapshot.
- [ ] Persistencia del output:
  - [ ] `~/.harness/profiles/<p>/sessions/<sid>/output.log` (raw bytes, rotación a 50 MiB).

### Docker
- [ ] Backend Dockerfile: **bind-mounts** documentados en `docker-compose.yml`:
  ```yaml
  volumes:
    - /usr/local/bin/claude:/usr/local/bin/claude:ro
    - /usr/local/bin/codex:/usr/local/bin/codex:ro
    - /usr/local/bin/cursor:/usr/local/bin/cursor:ro
    - ${HOME}/.claude:/root/.claude:rw
    - ${HOME}/.codex:/root/.codex:rw
    - ${HOME}/.cursor:/root/.cursor:rw
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
- [ ] **Multi-tab**: cada sesión activa abre una pestaña; el usuario puede tener varias visibles/conmutables.
- [ ] Modal "New session" desde dashboard / thread view: select `claude` | `codex` | `cursor`, optional cwd, optional rutas locales adicionales (Q2).

## Test de aceptación

1. Spawn `claude` desde UI → terminal aparece, prompt del CLI visible en <2s.
2. Escribir un mensaje y enter → se ve respuesta streaming en la terminal.
3. Resize ventana del browser → la terminal se ajusta, el CLI respeta el nuevo width (ej. tablas re-formateadas).
4. Click "Kill" → child process termina (verificar con `ps`), terminal muestra "[session ended]".
5. Cerrar tab del browser → el child sigue vivo; reabrir el tab → reconecta vía SSE y la terminal **catch-up** desde el `output.log`.
6. `claude` / `codex` no instalados → error claro en UI con instrucciones.

## Lo que NO está en F1

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

## Decisiones ya tomadas (ref `decisions-locked`)
- CLIs soportados: `claude`, `codex`, `cursor` (cerrado).
- Múltiples sesiones simultáneas: **sí desde F1**.
- Bypass del approval interno del CLI hijo: **sí**, control de seguridad vive en el harness.
- `AGENTS.md`: agente "config-AGENTS" + API explícita de rutas locales.
