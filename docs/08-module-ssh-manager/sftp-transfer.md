---
id: module-ssh-manager/sftp-transfer
title: Transferencias SFTP
shard: 08-module-ssh-manager
tags: [sftp, transfer, resume]
summary: Subida/descarga con resumable, paralelismo limitado y verificación.
related: [module-ssh-manager/ssh-backend, module-ssh-manager/transfer-queue]
sources: []
---

# Transferencias SFTP

## Crate
`russh-sftp::client::SftpSession` sobre un channel `sftp` subsystem.

## Resumable
- Antes de transferir, `stat` el destino: si existe y tamaño < origen → resume desde offset.
- Si existe e igual tamaño + hash opcional → skip.
- Sino: empezar de cero.

## Lectura/escritura por chunks
- Chunk 32 KiB default.
- Pipeline de N requests en vuelo (`max_in_flight = 4`) por archivo para llenar BDP.

```rust
let mut remote = sftp.open_write_with_offset(&dst, offset).await?;
let mut local = tokio::fs::File::open(&src).await?;
local.seek(SeekFrom::Start(offset)).await?;

let mut buf = vec![0u8; 32 * 1024];
loop {
    let n = local.read(&mut buf).await?;
    if n == 0 { break; }
    remote.write_all(&buf[..n]).await?;
    progress.add(n as u64);
    if cancel.is_cancelled() { break; }
}
remote.flush().await?;
```

## Recursividad
- Carpeta: walk local con `walkdir`, generar lista de `(src, dst)`.
- Crear dirs remotos según se llega (mkdir si no existe).
- Mantener permisos básicos (chmod 0644 / 0755).

## Paralelismo
- Por batch: hasta 3 transferencias concurrentes.
- Archivos > 256 MiB en un solo channel para no saturar memoria.

## Verificación opcional
- Si user activa "verify after transfer": calcular SHA-256 local + remoto (via `ssh.exec sha256sum`) → comparar.
- Caro; default off.

## Cancelación / pausa
- Cancellation token propaga a las loops.
- Pausa: token "paused" detiene loop sin cerrar handle; resume continúa desde offset persistido en estado de la cola.
