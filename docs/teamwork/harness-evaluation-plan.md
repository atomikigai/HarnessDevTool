# Plan de evaluacion integral del harness

Fecha: 2026-06-17

## Objetivo

Medir, probar y mejorar HarnessDevTool de punta a punta: orquestacion de
agentes, ejecucion por PTY, persistencia append-only, metricas, contratos
backend/frontend, ChatView, QA en browser, reviewer y cierre de tareas.

La prueba debe responder con evidencia:

- Que tan agnostico al agente es el harness.
- Que tan eficientes son las combinaciones agente/proveedor/modelo.
- Que tan bien resuelve problemas reales y escribe codigo mantenible.
- Que tan claro y usable es ChatView para un usuario real.
- Que partes del flujo requieren mejora antes de considerarlo productivo.

## Preparacion

1. Usar un `HARNESS_HOME` limpio o limpiar el perfil activo antes de empezar.
2. Levantar backend y frontend con el `.env` versionado.
3. Ejecutar cada caso desde la UI para cubrir el flujo real.
4. Registrar ids de thread, session y task de cada caso.
5. Validar con browser como usuario real cuando el caso toque frontend o ChatView.
6. Pasar reviewer antes de cerrar cada tarea.

## Decision tecnica a evaluar: filesystem vs SQLite

Durante las pruebas se debe analizar si el estado de sesiones debe seguir
repartido en directorios (`threads/`, `sessions/`, `budgets`, `events.jsonl`,
indices derivados) o migrar total/parcialmente a SQLite.

La evaluacion debe comparar:

- Invariante append-only: facilidad para conservar log inmutable y auditable.
- Hard delete: facilidad para borrar por thread/session sin dejar restos.
- Archive/restore: mover, exportar, importar y recuperar sesiones.
- Performance: lectura de listas, busqueda, paginacion, metricas y ChatView.
- Concurrencia: escrituras simultaneas de agentes, watchers e indexadores.
- Robustez: corrupcion parcial, crashes, fsync, locks y recuperacion.
- Portabilidad: copiar `HARNESS_HOME`, montar en Docker, backups y diff/debug.
- Observabilidad: inspeccion manual por humanos y herramientas CLI.
- Migraciones: versionado de esquema, compatibilidad hacia atras y costo de
  cambiar el modelo actual.
- Privacidad: borrado verificable, compactacion/vacuum y secretos en logs.

Opciones a comparar:

- Filesystem actual: JSON/JSONL por thread/session con indices derivados.
- SQLite primario: tablas normalizadas para threads, sessions, events, budgets
  y context index.
- Modelo hibrido: JSONL append-only como fuente canonica y SQLite como indice
  reconstruible.

Criterio inicial recomendado para discutir: mantener JSONL append-only como
fuente de verdad auditable y tratar SQLite como indice reconstruible, salvo que
las pruebas demuestren que el coste de limpieza, consulta o consistencia vuelve
mejor un SQLite primario.

## Metricas a capturar por sesion

- Routing: `requested_kind`, `resolved_provider`, `underlying_cli`, `model`,
  `source`, `role`.
- Identidad de subagente: `agent_id`, `session_id`, `task_id`, `role`,
  assignee esperado, CLI spawneada e identidad MCP efectiva. Deben coincidir o
  quedar auditadas las razones de fallback.
- Duracion: tiempo de thread creado a finalizado, `conversation_duration_ms`,
  `max_gap_ms`, `max_gap_after_seq`.
- Coste y tokens: input, output, cache read/write y `cost_usd`.
- Uso de herramientas: `tool_call_count`, `tool_call_breakdown`,
  `tool_error_count`, payload maximo de tool args/results.
- Capacidades: `loaded_capabilities` y si coinciden con lo que pedia la tarea.
- Observabilidad por subagente: `output.log`, transcript estructurado,
  `transcript_index.sqlite`, eventos `capability.decided`, handoffs, mailbox y
  estado final del proceso.
- Calidad: tests/checks ejecutados, findings del reviewer, defectos corregidos.
- UX: legibilidad de ChatView, estados visibles, streaming, errores, navegacion,
  tool calls y comportamiento en desktop/mobile.

## Matriz minima de agentes

Cada caso debe ejecutarse con una combinacion principal y, cuando el coste lo
permita, con una comparativa:

- Zeus orchestrator con proveedor resuelto automaticamente.
- Claude directo.
- Codex directo.
- Cursor agent directo.
- Antigravity directo.

Si una CLI no esta autenticada o falla, el resultado se registra como evidencia
del harness: mensaje visible, evento persistido, recuperacion posible y claridad
en ChatView.

## Caso 1: mejora backend

Tarea: mejorar una pieza real del backend sin cambiar contrato publico, por
ejemplo metricas de conversacion, rehidratacion de eventos o manejo de gaps.

Objetivos:

- Ver si el agente respeta append-only, `HARNESS_HOME` y versionado de protocolo.
- Medir calidad Rust, cobertura y tamano del diff.
- Confirmar que las metricas de sesion reflejan tools, errores y duraciones.
- Pasar reviewer enfocado en concurrencia, persistencia y regresiones.

Cierre:

- Tests backend enfocados pasan.
- No cambia el contrato frontend.
- Reviewer aprueba o sus findings quedan corregidos.
- La sesion queda auditable en logs y endpoint de metricas.

## Caso 2: feature end-to-end

Tarea: agregar una feature transversal pequena, por ejemplo resumen visible de
metricas por sesion en la UI.

Objetivos:

- Probar backend, tipos `ts-rs`, frontend y contrato HTTP.
- Verificar que no se editan a mano tipos generados en
  `frontend/src/lib/api/types/`.
- Ejecutar `just gen-types` si cambian tipos compartidos.
- Validar flujo real con browser.

Cierre:

- Feature visible y usable desde la UI.
- `pnpm check` y checks backend relevantes pasan.
- `frontend/DESIGN.md` se actualiza si cambia direccion visual.
- QA browser aprueba estados loading, error, empty y datos reales.

## Caso 3: bug real o sembrado

Tarea: resolver un bug reproducible en streaming, ChatView, eventos parciales,
errores de tool result o protocolo.

Subtarea obligatoria: implementar o corregir hard delete real de sesiones.
Hoy la eliminacion visible puede dejar restos en `sessions/`, `budgets`,
`context.sqlite` u otros indices derivados. El objetivo es que la accion de
hard delete borre el estado asociado de forma completa, verificable y segura.

Objetivos:

- Medir si el agente reproduce antes de corregir.
- Exigir test regresivo cuando sea razonable.
- Confirmar que la solucion no reescribe logs append-only.
- Evaluar claridad del error para el usuario.
- Definir semantica explicita entre soft delete, archive y hard delete.
- Implementar hard delete limpiando thread, sesiones hijas, budgets, indices,
  contexto indexado y artefactos derivados asociados.
- Asegurar que la UI no prometa "delete" si solo esta ocultando o archivando.

Cierre:

- Bug reproducido con pasos claros.
- Test falla antes y pasa despues, cuando aplique.
- QA browser confirma que el usuario entiende el estado.
- Las metricas registran gaps o tool errors si ocurrieron.
- Despues del hard delete, no quedan entradas del thread/session en:
  `threads/`, `sessions/`, `budgets`, `context.sqlite` ni indices derivados.
- El hard delete es intencional: requiere confirmacion clara en UI y no se
  confunde con archive/soft delete.

## Caso 4: refactor de mantenibilidad

Tarea: simplificar una zona compleja sin cambiar comportamiento, por ejemplo
render de ChatView, parsing de markdown/tools o calculo de metricas.

Objetivos:

- Medir si el agente reduce complejidad sin sobreingenieria.
- Comparar tamano del diff entre agentes.
- Mantener comportamiento observable.
- Pasar reviewer estricto de claridad, performance y seguridad.

Cierre:

- Tests existentes pasan.
- Diff es pequeno y revisable.
- No hay cambios visuales o contractuales no declarados.
- Reviewer no deja findings abiertos.

## Caso 5: flujo multiagente completo

Tarea: ejecutar una mejora de ChatView como flujo de producto completo:
planificacion, implementacion, QA browser, reviewer, correcciones y cierre.

Objetivos:

- Probar el harness como orquestador, no solo como launcher de CLI.
- Comparar Zeus contra agentes directos.
- Validar handoff entre implementer, QA y reviewer.
- Medir si ChatView es suficientemente user friendly.

Checklist ChatView:

- Streaming legible, sin saltos bruscos.
- Tool calls compactos pero inspeccionables.
- Errores visibles y accionables.
- Mensajes largos no rompen layout.
- Estados claros: running, waiting, failed, completed.
- Capacidades cargadas auditables.
- Navegacion entre thread y sesion conserva contexto.
- Desktop y mobile sin solapamientos.

Cierre:

- QA browser aprueba el flujo real.
- Reviewer aprueba despues de correcciones.
- La sesion queda persistida y auditable.
- Las metricas permiten comparar agente/proveedor con precision.

## Resultado esperado

Al terminar debe existir una tabla comparativa por agente/proveedor/modelo con:

- Exito o fallo.
- Tiempo total.
- Coste y tokens.
- Errores de tool o gaps.
- Calidad del codigo.
- Findings del reviewer.
- Observaciones UX de ChatView.
- Mejoras priorizadas para el harness.

La segunda ronda debe repetir una muestra pequena despues de aplicar mejoras
para confirmar si bajaron errores, coste, tiempo o friccion de usuario.
