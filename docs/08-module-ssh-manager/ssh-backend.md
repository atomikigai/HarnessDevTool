---
id: module-ssh-manager/ssh-backend
title: Backend SSH (russh)
shard: 08-module-ssh-manager
tags: [ssh, russh, backend]
summary: Sesión `russh::client::Handle` con handler propio; multiplex de canales.
related: [module-ssh-manager/sessions-and-keys, module-ssh-manager/sftp-transfer]
sources: []
---

# Backend SSH

## Cliente

```rust
use russh::*;
use russh::client::{Config, Handle, Handler};

struct Client { /* known_hosts, fingerprints */ }

#[async_trait::async_trait]
impl Handler for Client {
    type Error = russh::Error;
    async fn check_server_key(&mut self, key: &PublicKey) -> Result<bool, Self::Error> {
        Ok(self.verify_against_known_hosts(key).await?)
    }
}

pub async fn connect(spec: &HostSpec, auth: AuthMaterial) -> Result<Handle<Client>> {
    let config = Arc::new(Config::default());
    let mut handle = client::connect(config, (spec.host.as_str(), spec.port), Client::new(spec)).await?;
    auth.apply(&mut handle, &spec.user).await?;
    Ok(handle)
}
```

## Auth
- Password → `handle.authenticate_password(user, pwd)`.
- Key file (PEM/OpenSSH) → `russh_keys::load_secret_key(path, passphrase?)` → `authenticate_publickey`.
- ssh-agent → conectar al socket `$SSH_AUTH_SOCK` via `russh-keys::agent`.

## Multiplex
Una sesión SSH puede tener varios **channels** concurrentes:
- Uno o más `exec` (comandos one-shot).
- Uno SFTP (subsystem `sftp`).
- Uno shell interactivo (futuro).

`Handle` se clona; cada channel = `handle.channel_open_session()`.

## Keep-alive
- `Config::keepalive_interval = Some(30s)`.
- Si el server no responde 3 keepalives → marcar disconnected → reconnect opcional.

## TLS / Proxy
- ProxyCommand vía `Command` y pipe stdio → `russh` puede tomar un `AsyncRead+Write` arbitrario.
- ProxyJump: encadenar dos sesiones (`session1.direct_tcpip → session2.connect`).

## Logging
- `tracing::info` al conectar; nunca loggear secretos.
- Detalles de handshake en `debug`.
