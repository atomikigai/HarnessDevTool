---
id: module-ssh-manager/sessions-and-keys
title: Sesiones, identidades y host keys
shard: 08-module-ssh-manager
tags: [ssh, keys, identities, known-hosts]
summary: Gestión de identidades, conexión a ssh-agent y verificación de host.
related: [module-ssh-manager/ssh-backend, cross-cutting/security-model]
sources: []
---

# Identidades y host keys

## Identidad
Una **identidad** es un material auth: key file, password, o agente.

```rust
pub enum AuthMaterial {
    Password(SecretString),
    KeyFile { path: PathBuf, passphrase: Option<SecretString> },
    Agent,
}
```

Persiste en `~/.harness/modules/ssh/identities.db` (SQLite):
- `id`, `name` (humano), `kind`, `key_path|password_ref`, `created_at`.

`password_ref` apunta al keyring; nunca la password al disco.

## Asociación host ↔ identidad
- Un host puede tener una identidad por defecto.
- En el modal "Add host" se elige una identidad existente o se crea nueva.

## ssh-agent
- Detección: existe `$SSH_AUTH_SOCK`.
- `russh-keys::agent::client::AgentClient::connect_env()`.
- Sin agent: deshabilitar opción en UI con tooltip.

## known_hosts
- Archivo `~/.harness/modules/ssh/known_hosts`.
- Formato compatible con OpenSSH (`hash` opcional).
- Al conectar:
  - Si host no está → mostrar fingerprint + algoritmo + ask user.
  - Si está y coincide → OK.
  - Si está y NO coincide → **bloquear** con warning fuerte (man-in-the-middle).

## TOFU (Trust On First Use)
- Por defecto: pedir confirmación.
- Opción "Auto-add" solo en modo dev (warning en UI).

## Cambios de host key
- Si una key cambia legítimamente: usuario edita known_hosts vía UI (botón "Forget host key").
- Auditado en logs (`tracing`).

## Permisos de archivos
- Key files cargados → memoria, jamás copiados.
- En Unix, advertir si key file tiene permisos > 0600.
