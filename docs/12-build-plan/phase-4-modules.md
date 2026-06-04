---
id: build-plan/phase-4-modules
title: F4 — Módulos verticales (DB + SSH)
shard: 12-build-plan
tags: [phase, f4, modules, db, ssh]
summary: DB lite y SSH/SFTP usables por el humano y como tools por los agentes.
related: [build-plan/phase-3-team, module-db-manager/overview, module-ssh-manager/overview]
sources: []
---

# F4 — Módulos verticales

## Meta
Habilitar **dos módulos verticales** que el usuario usa directamente desde la UI y que también exponen **tools** a los agentes vía el MCP server. Esto evita gastar tokens en operaciones puramente humanas (browsear una DB) pero permite al equipo invocarlas cuando aplican.

## Entregables — Module DB

### Backend (`module-db`)
- [x] Crate `module-db` con `sqlx` features `sqlite` (default), `postgres`, `mysql` (opt-in). Pools per-engine (no `sqlx::Any`).
- [x] Schemas en `harness-core/schemas/db.connection.v1.json`.
- [x] Storage `~/.harness/profiles/<p>/modules/db/connections.db` (SQLite con conexiones guardadas; passwords en keyring).
- [x] `Manager` con pools per-engine (SQLite | Postgres | MySQL) + routing per-database.
- [x] Operaciones:
  - [x] `connection.list/add/remove/test` (+ `update`).
  - [x] `schema.tree` (introspección).
  - [x] `query.run` (paginado), `query.cancel`, `export` (JSON/SQL/CSV).
  - [x] `row.insert/update/delete/duplicate` (inline edit + RowEditor).
- [x] Tools MCP:
  - [x] `db_query { connection, sql, params?, limit? }` (gated por leading keyword).
  - [x] `db_schema { connection, scope? }`.
  - [x] `db_explain { connection, sql }`.

### Frontend
- [x] Ruta `/db` → lista de conexiones + botón Add.
- [x] Ruta `/db/[conn]` → árbol de schema (sidebar) + sub-tabs Data / Schema.
- [x] Componente `<SqlEditor>` (CodeMirror 6 + `@codemirror/lang-sql`).
- [x] `<ResultGrid>` con `@tanstack/svelte-virtual` (inline cell edit, pending-changes bar).
- [x] `<RowEditorPanel>` slide-out lateral.
- [x] `<ExportDialog>` para JSON/SQL/CSV (tablas + schemas via right-click).
- [x] Form "Add connection" con **valibot** (URL parsing, sslmode, etc.).

### Test de aceptación DB
1. Add SQLite local (`/data/test.db`).
2. Schema tree muestra tablas existentes.
3. Editor SQL: `SELECT * FROM users LIMIT 10` → 10 filas pintadas en <500 ms.
4. Cancelar una query lenta → backend confirma cancel.
5. Desde una sesión `claude` activa, el agente llama `db.query` y recibe resultado JSON.

## Entregables — Module SSH

### Backend (`module-ssh`)
- [x] Crate `module-ssh` integrado al workspace. Implementación actual usa el cliente `ssh` del sistema para el slice funcional porque `russh`/`russh-sftp` introdujeron conflictos de compilación en este workspace.
- [x] Storage privado `~/.harness/profiles/<p>/modules/ssh/hosts.toml` para hosts guardados; password auth soportado y redacted en respuestas REST.
- [x] Storage `~/.harness/profiles/<p>/modules/ssh/{identities.db, known_hosts}`.
- [ ] Operaciones:
  - [x] `host.list/add/remove/test`.
  - [x] `session.open/close`.
  - [x] `sftp.list`.
  - [x] `sftp.mkdir/rmdir/unlink/rename`.
  - [x] `sftp.put / sftp.get` básico síncrono para archivos.
  - [ ] `transfer.queue/pause/resume/cancel` con resume.
  - [x] `ssh.exec` (no interactivo).
- [ ] Verificación de host keys con TOFU + warning fuerte si cambia.
- [ ] Tools MCP:
  - [x] `ssh.exec { host, cmd, env? }`.
  - [x] `sftp.list { host, path }`.
  - [x] `sftp.put / sftp.get` básico síncrono.
  - [x] `sftp.mkdir/rmdir/unlink/rename`.

### Frontend
- [x] Ruta `/ssh` → lista de hosts + add/test/delete.
- [x] Ruta `/ssh/[host]` → panel remoto navegable para `sftp.list`.
- [ ] Ruta `/ssh/[host]` → dos paneles (local / remote) + cola de transferencias abajo.
- [ ] `<FilePane>` con virtual list.
- [ ] `<TransferQueue>` con progreso por archivo.
- [ ] Drag & drop entre paneles → encolar.

### Test de aceptación SSH
1. Add host con key file → test OK.
2. Listar carpeta remota → contenido visible <2s. Ejecutado 2026-06-04 contra `webadmin@20.51.242.62` vía REST `/api/ssh/hosts/:id/sftp?path=.`.
3. Subir y bajar archivo pequeño → completa y contenido coincide. Ejecutado 2026-06-04 vía REST `/sftp/put` + `/sftp/get`; cleanup remoto vía `/exec`.
3a. Crear/renombrar/borrar archivo/remover directorio remoto → ejecutado 2026-06-04 vía REST `sftp/mkdir`, `sftp/rename`, `sftp/unlink`, `sftp/rmdir`; verificación final por `/exec` devolvió `cleanup-ok`.
4. Subir archivo 100 MiB → progreso, velocidad, ETA → completa.
5. Cancelar mitad de transferencia → resume continúa desde offset correcto.
6. Desde `claude` en sesión: `sftp.list` devuelve entries; approval pedido para `ssh.exec`.

## Entregables — Approval-and-remember

- [x] Mecanismo base de [[harness-core/approval-flow]] §"Allow and remember" para approvals MCP sensibles.
- [x] Persistencia en `~/.harness/profiles/<p>/policy.toml` con reglas `<tool, args-pattern> → allow/deny`.
- [ ] UI: modal de approval con checkbox "Remember this decision for similar calls".
- [x] Auditoría de decisiones: `/api/approvals/check` escribe `$HARNESS_HOME/.runtime/audit/bridge.jsonl` con actor, rol, tool, recurso, decisión y hashes.
- [ ] Auditoría de reglas recordadas: cada regla guarda `created_at`, `created_by`, hash de args.

## Lo que NO está en F4
- Migraciones DDL desde UI (solo SELECT + ejecución SQL libre).
- Terminal SSH interactiva (xterm.js sobre SSH channel) — pospuesto.
- Diagrama ER, query builder visual.
- Sync entre DBs.
- Multi-hop SSH (ProxyJump) avanzado.

## Riesgos
- **DB drivers en distroless**: `sqlx-sqlite` necesita `libsqlite3` o `sqlite-bundled`. Compilar con bundled para evitar libs externas.
- **Cancelación de queries MySQL**: requiere conexión auxiliar; cuidado con pool starvation.
- **TLS de Postgres**: certs montados como volumen o env var. Documentar bien.
- **SSH key passphrase**: si la key está protegida, pedir passphrase la primera vez y guardar en keyring.
- **SSH host key changes**: comportamiento agresivo (block) vs permissivo (warn) — elegir block + UI clara.

## Decisiones a confirmar
- ¿Default DB engines compilados en imagen Docker? **Solo SQLite**; postgres/mysql como features → el usuario rebuilda si los necesita.
- ¿Aprobaciones de `ssh.exec`/`db.query` automáticas para queries `SELECT` y `LIST`? Recomiendo **sí** para read-only; **no** para write/exec.
