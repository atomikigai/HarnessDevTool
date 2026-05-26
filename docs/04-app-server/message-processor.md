---
id: app-server/message-processor
title: Message processor
shard: 04-app-server
tags: [routing, processor, dispatch]
summary: Routing por namespace, validación de params, traducción a ops del core.
related: [architecture/ipc-protocol, app-server/jsonrpc-transport]
sources: []
---

# Message processor

## Diseño
Un dispatcher por **namespace** (`thread`, `turn`, `tool`, `module.db`, ...). Cada handler implementa:

```rust
#[async_trait]
pub trait NamespaceHandler: Send + Sync {
    fn name(&self) -> &str;                 // "thread", "module.db", ...
    async fn handle(&self, method: &str, params: Value, ctx: HandlerCtx) -> Result<Value, RpcError>;
}
```

## Pipeline por request
1. Parse → `Message`.
2. Split `method` → `(namespace, action)`.
3. Lookup handler. Si no hay → error -32601.
4. Validar `params` contra JSON schema del action.
5. Adquirir contexto (thread mgr, sandbox, sinks).
6. Llamar handler con timeout configurable.
7. Serializar `result` o `error`.
8. Enviar al writer.

## Notifications
- Sin `id` → no se responde.
- Procesadas in-task; errores se loggean pero no responden.

## Concurrencia
- Requests del **mismo cliente** se procesan en paralelo. El cliente es responsable de no asumir orden entre requests independientes.
- Ops sobre el mismo `thread` se serializan en su task root (un mailbox por thread).

## Contexto inyectado
```rust
pub struct HandlerCtx {
    pub thread_mgr: Arc<ThreadManager>,
    pub sink: Arc<dyn EventSink>,
    pub auth: Arc<AuthStore>,
    pub modules: Arc<ModuleRegistry>,
    pub cancel: CancellationToken,
}
```

## Tests
Cada handler tiene tests con `InMemorySink` que verifican:
- params válidos → result esperado.
- params inválidos → -32602 con mensaje útil.
- cancelación → handler cierra recursos.
