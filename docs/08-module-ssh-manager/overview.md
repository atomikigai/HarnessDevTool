---
id: module-ssh-manager/overview
title: SSH Manager — overview
shard: 08-module-ssh-manager
tags: [module, ssh, sftp, filezilla]
summary: SSH + SFTP estilo FileZilla con backend Rust (russh) y UI SvelteKit.
related: [module-ssh-manager/ssh-backend, module-ssh-manager/sveltekit-views]
sources: []
---

# SSH Manager

## Alcance v1
- Hosts guardados (host, port, user, auth).
- Auth: password (vía keyring), key file, ssh-agent.
- Verificación de host keys (known_hosts).
- Sesión SFTP: navegar, descargar, subir, mover, eliminar.
- Cola de transferencias con progreso, pausa y resume parcial.
- Terminal SSH interactiva (opcional v1.1).
- Tools para el agente: `ssh.exec`, `sftp.list`, `sftp.transfer`.

## Stack Rust
- `russh` — cliente SSH puro Rust.
- `russh-sftp` — extensión SFTP.
- `russh-keys` — parser de claves OpenSSH.
- `keyring-rs` — passwords.

## Por qué no `libssh2`
- `russh` es puro Rust, sin C bindings → cross-compile más limpio.
- Más rápido en algunas benchmarks; activo desarrollo.

## API JSON-RPC

```
module.ssh.host.list / add / remove / test
module.ssh.session.open  { host_id } → session_id
module.ssh.session.close { session_id }

module.ssh.sftp.list   { session_id, path } → entries
module.ssh.sftp.mkdir / rmdir / unlink / rename

module.ssh.transfer.queue { session_id, items: [{ direction, src, dst, recursive? }] } → batch_id
module.ssh.transfer.pause / resume / cancel { batch_id }
module.ssh.transfer.progress { batch_id, ... }  # notification

module.ssh.exec { session_id, cmd, env? } → { exit, stdout, stderr }   # not interactive
```

## Tools para el agente
- `ssh.exec(host, cmd)` — para diagnóstico remoto, deploys.
- `sftp.list(host, path)` — para inspección.
- `sftp.put(host, src, dst)` / `sftp.get(host, src, dst)` — transferencia.

Requieren approval por default (acción de red + escritura remota).
