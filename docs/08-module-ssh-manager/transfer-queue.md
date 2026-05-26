---
id: module-ssh-manager/transfer-queue
title: Cola de transferencias
shard: 08-module-ssh-manager
tags: [transfer, queue, progress, retry]
summary: Estados, persistencia y retry de transferencias SFTP.
related: [module-ssh-manager/sftp-transfer, module-ssh-manager/sveltekit-views]
sources: []
---

# Cola de transferencias

## Modelo

```rust
pub struct TransferBatch {
    pub id: BatchId,
    pub host_id: HostId,
    pub items: Vec<TransferItem>,
    pub status: BatchStatus,
}

pub struct TransferItem {
    pub id: ItemId,
    pub direction: Direction,        // Upload | Download
    pub src: PathBuf,
    pub dst: PathBuf,
    pub size: u64,
    pub transferred: u64,
    pub status: ItemStatus,          // Queued | Active | Paused | Done | Failed | Skipped
    pub error: Option<String>,
    pub retries: u8,
}
```

## Estados de batch
```
Queued → Active → (Paused) → Active → Done
                                    → Cancelled
                                    → Failed (si todos fallan)
```

## Persistencia
- Cola persistida en `~/.harness/modules/ssh/queue.db` (SQLite).
- Sobrevive a kill del proceso: al re-abrir, los `Active` pasan a `Paused` y se ofrece retomar.

## Concurrencia
- Hasta 3 items `Active` por batch.
- Hasta 1 batch `Active` por host (evita saturar la sesión SSH).

## Retry
- Errores transitorios (timeout, conn reset) → reintentar hasta 3 veces con backoff (1s, 4s, 16s).
- Errores permanentes (permission denied, no space) → marcar Failed inmediato.

## Progress
Notification cada 250 ms (throttle):
```jsonc
{
  "method": "module.ssh.transfer.progress",
  "params": {
    "batch_id": "...",
    "items": [
      { "id": "...", "transferred": 12345678, "size": 234567890, "rate_bps": 5242880, "eta_s": 42 }
    ]
  }
}
```

## Conflictos
- Destino ya existe → política por batch: `skip` | `overwrite` | `resume` | `ask`.
- `ask`: emite un `approval.request` por archivo (riesgo de fatiga; usar con cuidado).

## Cancelación
- Cancelar batch → cancel token; items `Active` se interrumpen, parciales se conservan para resume.
- Cancelar item individual → solo ese item se interrumpe.

## UI
- Tabla con: dirección icon, archivo, tamaño, % barra, velocidad, ETA, estado.
- Acciones por fila: pause, resume, retry, remove, "open destination".
