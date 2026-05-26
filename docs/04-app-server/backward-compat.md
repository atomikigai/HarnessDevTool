---
id: app-server/backward-compat
title: harness-server — versionado de API
shard: 04-app-server
tags: [server, versioning, api]
summary: Header `X-Protocol-Version`, reglas aditivas, deprecación gradual.
related: [app-server/overview, architecture/ipc-protocol]
sources: []
---

# Versionado de API

## Principio

Un `harness-server` nuevo **debe** servir a clientes (frontend) viejos. Un frontend nuevo **debe** degradar features faltantes al hablar con un server viejo.

Esto importa especialmente cuando:
- Distribuyes binarios pre-compilados de server y frontend independientemente.
- El usuario hace `docker compose pull` parcial.
- Tienes dos máquinas con versiones distintas (frontend en LAN remoto).

## Mecanismos

### Header `X-Protocol-Version`
Toda response del backend incluye `X-Protocol-Version: 1.0` (o el actual). Toda request del frontend incluye su propio header.

Negociación implícita:
- Mismo major → compatible.
- Server major mayor → ofrece compat shim al cliente viejo.
- Cliente major mayor que server → cliente degrada (skip features 1.x).

### Endpoint `/api/capabilities`
```jsonc
GET /api/capabilities
{
  "protocol_version": "1.2",
  "server_version": "0.4.0",
  "features": [
    "skills.search",
    "skills.manage",
    "memory.note",
    "module.db",
    "module.ssh"
  ],
  "deprecated": []
}
```

El frontend lo consulta al `initialize` y oculta UI de features no presentes.

### Reglas para evolucionar
- **Añadir campos opcionales**: OK siempre.
- **Añadir endpoints**: OK.
- **Añadir features a `capabilities`**: OK.
- **Renombrar campos**: ❌. Crear campo nuevo, mantener viejo por una minor.
- **Cambiar semántica de un campo**: ❌. Crear endpoint nuevo o campo nuevo.
- **Remover endpoint**: anunciar en `deprecated` por al menos una minor, retirar en major.

## Tipos compartidos (ts-rs)

Cuando una struct expuesta cambia:
1. **Aditivo**: campo nuevo opcional. Regenera tipos. Frontend viejo lo ignora.
2. **Breaking**: rename o cambio de tipo. Bump major, marca el shape viejo como `deprecated`.

`just gen-types` debe ejecutarse antes de commit que toca structs expuestas. CI valida que `frontend/src/lib/api/types/` está al día.

## Estrategia de release

- Patch (0.4.0 → 0.4.1): bug fixes, no API changes.
- Minor (0.4 → 0.5): nuevos endpoints / features. Aditivo.
- Major (1.x → 2.x): breaking. Requiere shim "v1-compat" en el server por al menos una minor.

## Testing

- Golden requests/responses por versión en `backend/tests/golden/`.
- CI corre el suite golden contra todas las versiones soportadas.
- Si el cambio es breaking sin shim → CI falla.

## Anti-patrones

| Mal | Bien |
|---|---|
| Cambiar shape de un endpoint sin bump | Bump minor + nuevo campo |
| Borrar endpoint sin deprecation | Marcar `deprecated`, retirar en major |
| Cliente asume features sin checkear `capabilities` | Pregunta `capabilities` al `initialize` |
| Header de versión solo en `/health` | En todas las responses (header) |
