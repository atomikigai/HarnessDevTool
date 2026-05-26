---
id: module-ssh-manager/sveltekit-views
title: Vistas SvelteKit del módulo SSH
shard: 08-module-ssh-manager
tags: [sveltekit, ui, ssh, filezilla]
summary: Dos paneles tipo FileZilla + cola de transferencias abajo.
related: [module-ssh-manager/overview, module-ssh-manager/transfer-queue]
sources: []
---

# Vistas

## Layout `/ssh/[host]`

```
┌──────────────────────┬──────────────────────────┐
│ LOCAL                │ REMOTO                   │
│ /home/me/proj/       │ /var/www/app/            │
│ ┌──────────────────┐ │ ┌──────────────────────┐ │
│ │ name  size mtime │ │ │ name  size mtime     │ │
│ │ ...              │ │ │ ...                  │ │
│ └──────────────────┘ │ └──────────────────────┘ │
├──────────────────────┴──────────────────────────┤
│ Queue: ⬇ file.zip 45%  ⬆ assets/ ...            │
└─────────────────────────────────────────────────┘
```

## Componentes
- `<FilePane mode="local|remote">` — list virtualizada, breadcrumb, atajos.
- `<TransferQueue>` — table con direction, src, dst, progress, status.
- `<HostHeader>` — host, user, conectado/desconectado, latencia.

## Interacciones
- **Drag & drop** entre paneles → encolar transfer.
- Doble click directorio → entrar.
- `Backspace` → subir.
- `F2` → rename (in-place edit).
- `Del` → eliminar (con confirm).
- Shift+click / Ctrl+click → multi-select.

## Performance
- Listas remotas pueden ser grandes (10k entries) → virtual list.
- `stat` lazy: pedir size/mtime solo de filas visibles.
- Refresh manual + auto cada 30s.

## Drag from OS
- Tauri: aceptar `tauri://drag-drop` events → mapear a "upload to current remote dir".

## Estados
- Conectando: spinner.
- Desconectado: banner + botón Reconectar.
- Permission denied al listar: panel vacío + mensaje.

## Paths
- Mostrar siempre rutas absolutas.
- Breadcrumb clickeable para saltar.

## Comparación
Modo "diff dirs" v1.1: compara dos directorios, marca diferentes y solo-en-uno-de-los-dos.
