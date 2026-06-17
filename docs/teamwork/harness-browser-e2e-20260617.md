# Harness Browser E2E Retest - 2026-06-17

Objetivo: validar el harness como usuario real desde ChatView, cubriendo
orquestacion, proveedores, subagentes, metricas, hard delete, UX y QA visual.

Backend/frontend usados durante la ronda:

- Backend: `http://127.0.0.1:55115`.
- Frontend: `http://localhost:46693`.
- Browser tool: `agent-browser` con skill `core` + `dogfood`.
- Evidencia: `docs/teamwork/browser-e2e-artifacts/`.

## Caso 1 - Codex scheduler con subagente medible

Objetivo:

- Crear desde la UI un flujo que provoque task assignment del scheduler.
- Confirmar que `agent:codex-1` se mantiene como assignee y actor MCP.
- Confirmar que ChatView muestra estado comprensible mientras el worker corre.
- Verificar si `/metrics` aporta datos conversacionales o solo metadata.

Criterios de aceptacion:

- Timeline/eventos muestran assignment a `agent:codex-1`.
- Spawn real usa `kind = codex`.
- UI no deja al usuario sin feedback.
- La session puede detenerse/pausarse sin dejar loop.

Resultado: fallido.

Evidencia browser:

- `screenshots/new-session-modal.png`
- `screenshots/case1-before-create.png`
- `screenshots/case1-after-create.png`
- `screenshots/case1-create-semantic-result.png`
- `videos/case1-send-prompt.webm`
- `screenshots/case1-after-send-10s.png`
- `screenshots/case1-after-send-30s.png`
- `screenshots/case1-terminal-tab.png`

Observado:

- El flujo `New session -> codex -> Create` cierra el modal, pero no crea
  sesion visible y no dispara `POST /api/threads/:tid/sessions`.
- Reintentado con selector semantico (`find role button click --name Create`)
  produjo el mismo resultado.
- La unica sesion nueva visible (`172bbc72-34e2-4035-8d77-12831bec52ef`) fue
  creada por `Restart`, no por `New session`.
- Enviar mensaje desde ChatView registra al menos un user message en backend
  (`user_message_count = 1`), pero tras 30s no aparece respuesta assistant.
- ChatView queda en `Working...`, con fallback `Terminal transcript available`,
  y el header muestra `backend down` aunque `/api/health` del backend seguia en
  `200`.
- `/metrics` de la sesion `172bbc72-34e2-4035-8d77-12831bec52ef`:
  `kind = codex`, `transcript_event_count = 8`, `user_message_count = 1`,
  `assistant_message_count = 0`, `tool_call_count = 0`.

Diagnostico:

- Como usuario real, el flujo basico de crear una sesion nueva desde la UI no
  es confiable.
- ChatView no da salida clara cuando el input llega al backend pero no hay
  respuesta visible.

## Caso 2 - Zeus root con matriz Codex y preparacion de delegacion

Objetivo:

- Crear desde ChatView una sesion Zeus con matriz donde orchestrator = Codex.
- Verificar que la UI expone el routing Zeus o al menos no lo oculta.
- Confirmar eventos `session.spawn.routing.resolved` y metricas del root.

Criterios de aceptacion:

- `requested_kind = zeus`.
- `resolved_provider = codex`.
- `source = zeus_matrix`.
- Transcript y metricas aparecen o existe fallback claro.

Resultado: parcialmente cubierto / bloqueado por UI.

Evidencia browser:

- `screenshots/initial.png`
- `screenshots/case5-before-restart.png`
- `screenshots/case5-after-restart.png`

Observado:

- La UI mostraba una sesion previa `8af257ec` con transcript Zeus/root:
  `spec_read {}` y respuesta del agente.
- El listado de sesiones no expone claramente `requested_kind=zeus`,
  `resolved_provider`, `source=zeus_matrix` ni la matriz de roles. Solo muestra
  `codex-cli planner` o, en otra sesion antigua, un badge `ZEUS`.
- `Restart` sobre la sesion seleccionada creo `172bbc72`, pero la nueva sesion
  aparece como `codex-cli`; desde UI no queda claro si conserva semantica Zeus,
  matriz ni rol de orchestrator.

Diagnostico:

- Backend Zeus ya habia sido validado por API, pero desde ChatView la
  trazabilidad Zeus -> provider -> matriz no es suficientemente visible para un
  usuario.

## Caso 3 - Zeus delegando child worker

Objetivo:

- Desde una sesion Zeus, pedir una tarea pequena que requiera delegar a un child.
- Comprobar que el child aparece como sesion hija en ChatView.
- Revisar si el usuario puede entender parent/child, estado, proveedor y salida.

Criterios de aceptacion:

- Existe child session real.
- Parent Zeus puede listar/resumir child.
- UI muestra la relacion sin confundir al usuario.
- Hay eventos/metricas por child.

Resultado: fallido / inconcluso por bloqueo de UI.

Evidencia browser:

- `screenshots/case3-agents-panel.png`
- `screenshots/case3-select-zeus-old.png`

Observado:

- El panel `Agents` de la sesion muestra `SUB-AGENTS · 0 SPAWNED`, pero tambien
  muestra `Failed to fetch`.
- Intentar seleccionar otra sesion Zeus antigua desde la lista no cambio el
  panel visible; el usuario queda mirando la sesion activa previa.
- No se pudo ejecutar una delegacion child real desde ChatView porque:
  - crear sesion Zeus nueva desde UI no funciono;
  - el envio de prompt en ChatView quedo sin respuesta visible;
  - el panel de subagentes falla fetch.

Diagnostico:

- La UI todavia no permite validar bien Zeus multiagente como usuario real.
- El backend puede tener la capacidad, pero ChatView no lo hace operable ni
  auditable en esta ronda.

## Caso 4 - Hard delete real desde experiencia de usuario

Objetivo:

- Crear una sesion desde la UI.
- Eliminarla desde la UI si existe control; si no existe, registrar brecha UX.
- Verificar backend/filesystem como evidencia.

Criterios de aceptacion:

- El usuario puede distinguir stop/archive/delete/hard-delete.
- Hard delete borra `sessions/:sid` del filesystem.
- La UI deja de mostrar la sesion sin referencias rotas.

Resultado: fallido.

Evidencia browser:

- `screenshots/case4-delete-click.png`
- `screenshots/case4-after-delete.png`

Observado:

- Antes del delete, existia el directorio:
  `~/.harness/profiles/default/sessions/172bbc72-34e2-4035-8d77-12831bec52ef`.
- La UI abre un dialogo `Delete session codex · 172bbc72?`, pero no explica si
  es hide/tombstone/stop/hard delete.
- Al confirmar `Delete`, la UI muestra toast/status `Delete failed: Failed to
  fetch`.
- Despues del intento, el directorio seguia existiendo.

Diagnostico:

- La UI no implementa ni comunica hard delete real.
- El backend ya tiene endpoint `POST /api/sessions/:sid/hard-delete`, pero no
  esta integrado en esta experiencia de usuario.

## Caso 5 - ChatView UX: restart, scroll, estados y legibilidad

Objetivo:

- Probar como usuario real el flujo comun: crear sesion, enviar prompt, esperar,
  restart/stop, navegar entre thread/session, revisar transcript y terminal.
- Evaluar si ChatView es suficientemente user friendly.

Criterios de aceptacion:

- Input, botones, tabs y estados son claros.
- No hay PTY crudo confuso cuando hay transcript estructurado.
- Auto-scroll funciona o hay CTA para volver al ultimo mensaje.
- Estados loading/working/idle/error son legibles.

Resultado: fallido con un punto positivo.

Evidencia browser:

- `videos/case5-restart.webm`
- `screenshots/case5-before-restart.png`
- `screenshots/case5-after-restart.png`
- `screenshots/case1-after-send-10s.png`
- `screenshots/case1-after-send-30s.png`
- `screenshots/case1-terminal-tab.png`

Observado positivo:

- `Restart` si funciona desde UI: creo `172bbc72`, activo input y mostro la
  sesion en el listado.

Problemas observados:

- Header muestra `backend down` aunque el backend respondia `/api/health 200`.
- El estado de la sesion muestra `idle` en la lista mientras ChatView muestra
  `Working...`.
- El chat queda con `Agent Working...` y un bloque `Terminal transcript
  available`, sin diagnostico claro ni timeout visible.
- El boton `View Terminal` dentro del bloque fallback no cambio de vista en el
  primer intento; el tab `Terminal` principal si mostro terminal.
- El frontend dejo de aceptar conexiones en `localhost:46693` durante la ronda,
  mientras backend seguia sano. `pnpm dev` seguia vivo, pero `curl` al frontend
  fallo con connection refused.

## Hallazgos

### ISSUE-001 - New session no crea sesion ni hace POST

Severidad: high.

Categoria: functional / ux.

Impacto:

- Bloquea el flujo principal de usuario para iniciar trabajo.
- Hace imposible probar proveedores/agentes desde UI sin usar API o sesiones
  previas.

Evidencia:

- `screenshots/new-session-modal.png`
- `screenshots/case1-after-create.png`
- `screenshots/case1-create-semantic-result.png`
- Network: no hubo `POST /api/threads/:tid/sessions`.

### ISSUE-002 - ChatView queda en Working sin respuesta ni diagnostico

Severidad: high.

Categoria: functional / ux.

Impacto:

- El usuario no sabe si el agente recibio el mensaje, si esta bloqueado, o si
  debe ir al terminal.

Evidencia:

- `videos/case1-send-prompt.webm`
- `screenshots/case1-after-send-10s.png`
- `screenshots/case1-after-send-30s.png`
- Backend `/metrics`: `user_message_count = 1`, `assistant_message_count = 0`.

### ISSUE-003 - Estado global erroneo: backend down con backend sano

Severidad: medium.

Categoria: ux / functional.

Impacto:

- Destruye confianza del usuario en el sistema y puede llevar a acciones
  incorrectas.

Evidencia:

- `screenshots/case1-after-send-10s.png`
- `screenshots/case1-after-send-30s.png`
- `curl /api/health` backend: `200 OK`.

### ISSUE-004 - Delete UI no es hard delete y falla fetch

Severidad: high.

Categoria: functional / data lifecycle.

Impacto:

- El usuario cree estar borrando, pero la sesion persiste en disco.
- No hay lenguaje que distinga hide/delete/hard-delete.

Evidencia:

- `screenshots/case4-delete-click.png`
- `screenshots/case4-after-delete.png`
- `dir_after=exists`.

### ISSUE-005 - Panel subagentes/Zeus no es auditable desde ChatView

Severidad: high.

Categoria: ux / orchestration observability.

Impacto:

- Zeus es el modo mas importante para el objetivo del harness, pero la UI no
  permite ver claramente matriz, provider real, parent/child ni estado de
  subagentes.

Evidencia:

- `screenshots/case3-agents-panel.png`
- `screenshots/case3-select-zeus-old.png`

### ISSUE-006 - Frontend dev dejo de responder mientras backend seguia sano

Severidad: medium.

Categoria: reliability / developer experience.

Impacto:

- La validacion browser se interrumpe y la UI muestra estados inconsistentes.

Evidencia:

- `agent-browser open http://localhost:46693/agents`: `ERR_CONNECTION_REFUSED`.
- `curl http://localhost:46693/`: connection refused.
- `curl http://127.0.0.1:55115/api/health`: `200 OK`.

## Decision tras esta ronda

- El backend routing Codex/Zeus corregido anteriormente esta en mejor estado que
  la experiencia ChatView.
- La prioridad debe moverse a frontend/contract:
  1. Arreglar `New session -> Create`.
  2. Arreglar estado health/frontend proxy para no mostrar `backend down` falso.
  3. Integrar hard delete real en UI con lenguaje explicito.
  4. Hacer que ChatView tenga timeout/diagnostico cuando hay user message sin
     assistant response.
  5. Exponer Zeus matrix/provider/children de forma visible y auditable.
