---
id: module-db-manager/sveltekit-views
title: Vistas SvelteKit del módulo DB
shard: 07-module-db-manager
tags: [sveltekit, ui, db]
summary: Layout: árbol a la izquierda, tabs (editor + browser) a la derecha.
related: [module-db-manager/overview, frontend-shell/tech-stack]
sources: []
---

# Vistas

## Layout `/db/[connection]`

```
┌────────────┬──────────────────────────────────────┐
│ Schema     │  ┌──── tabs ──────────────────┐      │
│  tree      │  │  Editor SQL │ Browser orders │   │
│ (orders,   │  └────────────────────────────┘      │
│  users,    │  ┌────────────────────────────┐      │
│  …)        │  │  Editor / Tabla virtual    │      │
│            │  └────────────────────────────┘      │
└────────────┴──────────────────────────────────────┘
```

## Componentes
- `<SchemaTree>` — virtual tree, expand lazy, drag para insertar nombre al editor.
- `<SqlEditor>` — CodeMirror 6 con `@codemirror/lang-sql`, completion alimentado por el schema tree.
- `<ResultTable>` — TanStack Virtual + resize de columnas + sticky header. Render: 1 row = 1 div.
- `<QueryStatusBar>` — elapsed, filas leídas, botón cancel.

## Atajos
- `Cmd/Ctrl+Enter` — ejecutar query.
- `Cmd/Ctrl+/` — toggle comentario.
- `Alt+Up/Down` — mover líneas.
- `Cmd/Ctrl+Shift+F` — formatear (sql-formatter).

## Browser de tabla (sin SQL)
- Click en `orders` → abre tab "Browser orders" → ejecuta `SELECT * FROM orders LIMIT 500 OFFSET 0`.
- Filtros por columna (input por col → WHERE).
- Ordenar por columna (ORDER BY).
- Paginar (next/prev).

## Estados de UI
- Loading: skeleton de columnas.
- Empty: mensaje + sugerencia.
- Error: card roja con SQL y mensaje del servidor.
- Cancelled: badge amarillo.

## Persistencia local
- Pestañas abiertas por conexión en `localStorage`.
- Historial de queries (últimas 100) por conexión.
