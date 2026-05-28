---
id: build-plan/pending-implementation-tasks
title: Tareas pendientes de implementación
shard: 12-build-plan
tags: [plan, backlog, f3, f4, implementation]
summary: Backlog secuencial de tareas pendientes para ejecutar F3/F4 con cambios mínimos.
related: [build-plan/phase-3-team, build-plan/phase-4-modules, build-plan/open-questions]
sources: []
---

# Tareas pendientes de implementación

Backlog ordenado para retomar el harness tarea por tarea. Cada bloque se puede
revisar, aprobar y ejecutar sin mezclar scopes.

## Orden recomendado

1. **Tab Agents con sesiones hijas reales** — ejecutada; corrige el bug observado y valida la base de sub-agentes.
2. **Smoke test backend de spawn child** — ejecutada; fija el contrato backend antes de extender UI.
3. **Tool MCP `task.create` con brief para orchestrator** — ejecutada; cierra el loop de creación de tasks por agentes.
4. **Validación valibot en Add DB Connection** — ejecutada; pendiente pequeño y aislado de DB.
5. **Mejorar visualización y edición de tipos especiales en DB tables** — ejecutada; fechas, bytes, boolean/null y arrays.
6. **Mejoras y bugs del DB Manager** — ejecutada; tarea creada desde la inspección de validación DB.
7. **Iconos lucide para schemas, tablas y vistas en DB** — ejecutada; mejora visual pequeña del árbol DB.
8. **Context menu avanzado para tablas/vistas DB** — ejecutada; exportar formatos y generar queries en nueva pestaña.
9. **Agente DB para conexión activa** — agente especializado con acceso controlado a la BD, backups y puente con Agents.
10. **Esqueleto mínimo del módulo SSH** — slice grande; arrancar después de cerrar pendientes chicos.
11. **Botón `+ task` en tab Tasks** — mejora secundaria para control manual.

## 1. Tab Agents con sesiones hijas reales

Objetivo:
Mostrar en vivo las sesiones hijas/sub-agentes de una sesión padre.

Contexto:
Frontend `SessionRightPanel.svelte`. Backend `routes/sessions.rs`.
Existe metadata `parent_session_id` / `root_session_id` y una ruta de hijos que
hay que auditar antes de tocar UI.

Tarea:
1. Auditar qué devuelve `GET /api/sessions/:id/children`.
2. Conectar el tab Agents a sesiones hijas reales.
3. Refrescar con el patrón existente de polling/store del panel.
4. Mostrar estados `running`, `exited` y `killed` con estilo consistente.

Reglas:
- No romper.
- Cambios mínimos.
- Seguir estilo existente.
- Agregar test.

Resultado esperado:
Cuando una sesión spawnea un sub-agente, el tab Agents lo muestra sin refrescar
la página y permite abrir la sesión hija.

## 2. Smoke test backend de spawn child

Objetivo:
Fijar por test que una sesión hija queda enlazada correctamente a su padre.

Contexto:
Backend `routes/sessions.rs` y MCP/session tools en `harness-mcp-server/src/tools/session.rs`.
Este test protege el contrato que consume el tab Agents.

Tarea:
1. Identificar o crear el punto de test para sesiones.
2. Crear una sesión padre y una hija con `parent_session_id`.
3. Verificar `parent_session_id`, `root_session_id` y listado de children.
4. Cubrir hija activa y finalizada si el harness de test lo permite.

Reglas:
- No romper.
- Cambios mínimos.
- Seguir estilo existente.
- Agregar test.

Resultado esperado:
`GET /api/sessions/:id/children` devuelve hijas correctas y estables para la UI.

## 3. Tool MCP `task.create` con brief para orchestrator

Objetivo:
Permitir que una sesión/orchestrator cree tasks vía MCP usando el formato
estándar de brief.

Contexto:
Backend MCP `harness-mcp-server/src/tools/tasks.rs`.
Core task store `harness-core/src/tasks/store.rs`.
F3 permite creación directa por planner/orchestrator; workers usan propuestas después.

Tarea:
1. Auditar tools MCP actuales de tasks y sus tests.
2. Analizar la implementación actual del harness para adaptar el formato de brief
   a tasks, memoria y continuidad entre sesiones sin migraciones grandes.
3. Agregar soporte de `brief` en `task_create` usando el store existente.
4. Convertir el brief al formato textual estándar y persistirlo de forma recuperable.
5. Respetar validaciones y state machine actuales.
6. Persistir/emitir eventos con el flujo existente para que SSE/UI lo vea.

Reglas:
- No romper.
- Cambios mínimos.
- Seguir estilo existente.
- Agregar test.

Resultado esperado:
Un agente autorizado llama `task_create` con `brief`; la task queda persistida,
la UI la refleja por el flujo normal y un worker puede recuperar el contrato
con `task_get`.

## 4. Validación valibot en Add DB Connection

Objetivo:
Cerrar el pendiente menor del módulo DB validando el formulario de conexión.

Contexto:
Frontend `ConnectionFormDialog.svelte` y `api/schemas/db.ts`.
SQL ya está operativo; falta validación cliente para entradas inválidas.

Tarea:
1. Revisar campos actuales del dialog y shape esperado por el API.
2. Analizar e inspeccionar el gestor de BD actual en busca de bugs, deuda y
   posibles mejoras; crear una tarea separada con esos hallazgos antes de
   implementar cambios fuera de la validación.
3. Crear o extender un schema valibot para URL, engine y opciones.
4. Mostrar errores por campo sin cambiar el flujo exitoso.
5. Mantener compatibilidad con conexiones SQLite locales.

Reglas:
- No romper.
- Cambios mínimos.
- Seguir estilo existente.
- Agregar test.

Resultado esperado:
El formulario rechaza datos inválidos antes de llamar al backend y conserva el
flujo actual para conexiones válidas.

## 5. Mejorar visualización y edición de tipos especiales en DB tables

Objetivo:
Mejorar cómo el gestor de BD muestra y edita valores especiales en las tablas.

Contexto:
Frontend `/db/[id]`, `ResultGrid.svelte`, `RowEditorPanel.svelte` y helpers de
serialización/edición de valores.
Backend `module-db` devuelve valores tipados como JSON, por ejemplo:
`{ "_t": "date_time", "v": "2025-06-27T15:26:02.651197" }`,
`{ "_t": "bytes", "v": "QUy2uHsMT8T+L68+YobBso4ZZOEhpXLzlzlU/XfMJW0dOCOhUvzFP9P6auyaL/85" }`
y actualmente algunos arrays aparecen como `<unsupported:TEXT[]>`.

Tarea:
1. Auditar cómo `ResultGrid` y `RowEditorPanel` renderizan valores tipados (`date_time`, `bytes`, boolean, null, arrays).
2. Mostrar fechas de forma legible en celdas, conservando el valor original para edición/envío.
3. Mostrar bytes como valor compacto con affordance de inspección/copia, evitando pintar el base64 completo por defecto.
4. Cambiar la edición inline de booleanos a selector `TRUE` / `FALSE`; si la columna acepta `NULL`, incluir opción `NULL`.
5. Mejorar visualización de arrays (`TEXT[]` y equivalentes) para no mostrar `<unsupported:...>` cuando se pueda representar como lista/JSON editable.
6. Agregar tests o checks focalizados para los helpers de render/parse si existen; si no, cubrir con el test disponible del frontend.

Reglas:
- No romper.
- Cambios mínimos.
- Seguir estilo existente.
- Agregar test.

Resultado esperado:
Las tablas DB muestran fechas, bytes, booleanos/null y arrays de forma legible;
la edición inline de booleanos usa selector seguro; y los arrays dejan de verse
como `<unsupported:TEXT[]>` cuando el backend provee datos representables.

## 6. Mejoras y bugs del DB Manager

Objetivo:
Resolver bugs y mejoras detectadas durante la inspección del gestor de BD.

Contexto:
Frontend `/db`, `/db/[id]`, `ConnectionFormDialog.svelte`, `dbStore`.
Backend `module-db` y `routes/db.rs`.
No mezclar con la validación valibot; ejecutar como tarea aparte.

Tarea:
1. Mostrar errores inline para todos los campos validados, no solo name/database/host/params.
2. Revisar UX de password en edición: aclarar que vacío conserva el password actual.
3. Revisar validación backend de `ConnectionInput`: hoy solo valida name/database.
4. Revisar si el selector de SQLite debería tener picker/path helper o mejor copy clara.
5. Auditar estados de query larga/cancelación para asegurar feedback consistente en UI.
6. Auditar export filename parsing y errores de export para mejorar mensajes.

Reglas:
- No romper.
- Cambios mínimos.
- Seguir estilo existente.
- Agregar test.

Resultado esperado:
El DB Manager queda con validaciones y mensajes más consistentes, y los bugs
detectados se cierran sin cambiar el alcance funcional del módulo.

## 7. Iconos lucide para schemas, tablas y vistas en DB

Objetivo:
Mejorar visualmente la representación de schemas, tablas y vistas en el árbol
del gestor de BD usando iconos adecuados de `lucide-svelte`.

Contexto:
Frontend `/db/[id]`, componente `frontend/src/lib/components/db/SchemaTree.svelte`
y re-export central `frontend/src/lib/icons.ts`.
El árbol actualmente usa símbolos manuales para tablas/vistas y texto plano para
schemas. El proyecto ya importa iconos desde `$lib/icons`, que re-exporta
`lucide-svelte`.

Tarea:
1. Auditar cómo `SchemaTree.svelte` representa schemas, tablas y vistas hoy.
2. Seleccionar iconos lucide consistentes para schema/database, table, view y
   materialized view si aplica.
3. Agregar los iconos necesarios al re-export central `$lib/icons` si no existen.
4. Reemplazar símbolos manuales por iconos lucide manteniendo tamaño, color,
   alineación, estado activo y hover actuales.
5. Verificar que el árbol siga siendo legible con filtros, schemas colapsados y
   tablas con `row_estimate`.

Reglas:
- No romper.
- Cambios mínimos.
- Seguir estilo existente.
- Agregar test.

Resultado esperado:
En el gestor de BD, schemas, tablas y vistas se distinguen visualmente con
iconos lucide consistentes, sin cambiar el comportamiento de navegación,
filtro, menú contextual ni apertura de tablas.

## 8. Context menu avanzado para tablas/vistas DB

Objetivo:
Agregar un context menu para tablas y vistas que permita exportar datos en varios
formatos y generar queries base en una pestaña SQL nueva.

Contexto:
Frontend `/db/[id]`, `SchemaTree.svelte`, `ExportDialog.svelte`, `dbStore` y
tabs SQL/table del workspace DB.
Backend `module-db` y rutas `/api/db/*` ya tienen export parcial para JSON, CSV
y SQL inserts; XLSX y Markdown pueden requerir ampliar contrato o implementar
generación frontend según alcance.
El menú contextual actual solo expone export básico para schema/table.

Tarea:
1. Auditar el context menu actual de `SchemaTree.svelte` y el flujo existente de
   `ExportDialog`.
2. Definir acciones para tablas y vistas: exportar `JSON`, `CSV`, `XLSX` y
   `Markdown`.
3. Definir acciones para generar queries `SELECT`, `INSERT`, `UPDATE` y `DELETE`
   usando metadata de columnas y primary keys cuando existan.
4. Al generar una query, abrir una pestaña SQL nueva con el texto preparado para
   copiar o ejecutar, sin modificar datos automáticamente.
5. Validar restricciones por tipo: vistas pueden generar `SELECT` y exportar,
   pero `INSERT`/`UPDATE`/`DELETE` deben ocultarse o quedar deshabilitados si no
   son seguros.
6. Ampliar export backend o helper frontend solo lo mínimo necesario para soportar
   los formatos faltantes.
7. Agregar tests/checks para generación de queries y validación de formatos.

Reglas:
- No romper.
- Cambios mínimos.
- Seguir estilo existente.
- Agregar test.

Resultado esperado:
Al hacer click derecho sobre una tabla o vista, el usuario puede exportarla en
`JSON`, `CSV`, `XLSX` o `Markdown`; también puede generar queries base que se
abren en una nueva pestaña SQL listas para copiar, revisar o ejecutar.

## 9. Agente DB para conexión activa

Objetivo:
Inicializar un agente especializado dentro del gestor DB para la conexión activa,
capaz de consultar, analizar, documentar y asistir con cambios de base de datos
con permisos controlados y coordinación con los agentes del panel Agents.

Contexto:
Frontend DB `/db/[id]`, panel Agents (`SessionRightPanel.svelte`), backend
`harness-server`, `module-db`, sesiones/PTY de agentes y MCP tools.
La conexión activa ya existe en el workspace DB. El agente debe partir en modo
solo lectura y no debe ejecutar modificaciones sin solicitud explícita,
backup previo y trazabilidad. Respetar append-only, `X-Protocol-Version` y tipos
generados desde Rust cuando haya contrato compartido.

Tarea:
1. Auditar la implementación actual de sesiones/agentes, MCP tools y DB Manager
   para definir el mínimo contrato entre un agente DB y la conexión activa.
2. Agregar un botón en el gestor DB para iniciar un agente asociado a la conexión
   y base de datos actualmente seleccionadas.
3. Crear el contexto inicial del agente con metadata de conexión segura, schema
   introspectado, restricciones de permisos y modo inicial de solo lectura.
4. Exponer tools DB controladas para el agente: listar schema, ejecutar queries
   de lectura, documentar estructura y proponer acciones sin modificar.
5. Diseñar el flujo de elevación para escrituras: el agente solo puede modificar
   cuando el usuario lo solicita explícitamente y el sistema valida que no está
   en modo solo lectura.
6. Antes de cualquier modificación, ejecutar backup obligatorio. Crear un helper
   Rust reutilizable para backup por engine (`sqlite`, `postgres`, `mysql`) o
   una estrategia equivalente mínima y testeable.
7. Persistir la documentación/análisis del agente DB como contexto recuperable
   para la sesión y visible/usable por el harness.
8. Definir el puente de comunicación entre el agente DB y los agentes del panel
   Agents: compartir hallazgos, schema docs, riesgos y propuestas de migración
   sin romper el modelo append-only.
9. Para cambios de estructura o código, priorizar que el agente DB proponga una
   migración o task al agente de coding en vez de modificar directamente cuando
   el contexto sea desarrollo.
10. Agregar tests backend para permisos read-only, bloqueo de escrituras sin
    backup, creación de backup y contrato de contexto compartido.
11. Agregar checks frontend para el botón/estado de agente DB y probar el flujo
    completo con una conexión SQLite local.

Reglas:
- No romper.
- Seguir arquitectura existente.
- Mantener modo inicial solo lectura.
- No exponer secretos de conexión al frontend ni al log de conversación.
- Agregar test y probar.

Resultado esperado:
Desde el gestor DB puedo iniciar un agente ligado a la conexión activa. El
agente puede responder preguntas sobre la BD, ejecutar consultas, analizar
estado, documentar estructura, proponer mejoras y coordinar información con el
panel Agents. No modifica datos ni schema salvo solicitud explícita; antes de
cualquier modificación crea backup obligatorio y deja trazabilidad. Cuando el
contexto sea desarrollo, prefiere proponer una migración/task para un agente de
coding antes de tocar la BD directamente.

## 10. Esqueleto mínimo del módulo SSH

Objetivo:
Arrancar el SSH Manager con un slice mínimo y usable.

Contexto:
No existe crate `module-ssh`. Frontend SSH está pendiente y `IconRail` lo mantiene
deshabilitado. Diseño objetivo en [[build-plan/phase-4-modules]].

Tarea:
1. Crear crate `module-ssh` con tipos base `Host`, `HostConfig` y `TestConnection`.
2. Agregar storage mínimo para `host.list`, `host.add`, `host.remove` y `host.test`.
3. Exponer endpoints REST pequeños desde `harness-server`.
4. Crear ruta frontend `/ssh` con lista de hosts y dialog Add Host.
5. Dejar SFTP, transfer queue y `ssh.exec` fuera de este primer slice.

Reglas:
- No romper.
- Cambios mínimos.
- Seguir estilo existente.
- Agregar test.

Resultado esperado:
Se pueden guardar hosts SSH, listarlos y ejecutar un test de conexión básico.

## 11. Botón `+ task` en tab Tasks

Objetivo:
Permitir crear una task manual desde el tab Tasks del panel derecho.

Contexto:
Frontend `SessionRightPanel.svelte` y `stores/tasks.svelte.ts`.
Backend REST de tasks en `routes/tasks.rs`.
Mejora secundaria: no bloquea la autonomía de agentes.

Tarea:
1. Agregar un botón pequeño `+ task` en el tab Tasks.
2. Reusar el endpoint REST existente de creación de task.
3. Crear la task con autor humano usando el shape actual del API.
4. Refrescar el listado/store para que aparezca inmediatamente.

Reglas:
- No romper.
- Cambios mínimos.
- Seguir estilo existente.
- Agregar test.

Resultado esperado:
Desde una sesión abierta se crea una task asociada al thread y se ve
inmediatamente en el panel.
