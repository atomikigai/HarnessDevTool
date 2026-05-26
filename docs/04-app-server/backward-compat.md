---
id: app-server/backward-compat
title: Compatibilidad hacia atrás
shard: 04-app-server
tags: [versioning, compat, protocol]
summary: Reglas para que clientes y servers desincronizados sigan trabajando.
related: [architecture/ipc-protocol, app-server/overview]
sources: [foundations/openai-codex-architecture]
---

# Backward compat

## Principio
Un App Server nuevo **debe** servir a clientes viejos. Un cliente nuevo **debe** degradar features faltantes al hablar con un server viejo.

## Mecanismos

### Negociación
`session.initialize` lleva `protocolVersion` y `clientFeatures`. La respuesta lleva `serverFeatures` (intersección publicada).

### Aditivos, no destructivos
- **Añadir campos opcionales**: OK.
- **Añadir métodos**: OK (cliente lo detecta por feature flag).
- **Renombrar/remover métodos**: ❌. Mantener alias por una versión major.
- **Cambiar semántica de un campo**: ❌. Crear campo nuevo.

### Items y kinds
- Kinds nuevos de item se ignoran como `unknown` por clientes viejos (UI los oculta).
- Nunca cambiar el shape de un kind existente; crear `assistant_message.v2` si hace falta.

### Errores
Códigos -320xx están reservados, nuevos no rompen clientes (deben mostrar `message` y seguir).

## Matriz de compatibilidad

| Server / Cliente | v1.0 client | v1.1 client | v2.0 client |
|---|---|---|---|
| v1.0 server | OK | OK (sin features 1.1) | reject (major mismatch) |
| v1.1 server | OK | OK | reject |
| v2.0 server | OK con shim "v1-compat" | OK con shim | OK |

Major bumps requieren shim explícito en el server por **al menos una versión**.

## Deprecaciones
- Anunciar deprecation en `serverFeatures.deprecated = ["thread.foo"]`.
- Cliente nuevo deja de usarlo, viejo sigue.
- Tras 2 minor → remover en major bump.

## Tests
Suite de "golden requests/responses" por versión del protocolo. CI corre contra todas las versiones soportadas para detectar regresión.
