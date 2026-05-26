---
id: recipes/bootstrap-new-tool
title: Receta — añadir un módulo nuevo
shard: 10-recipes
tags: [recipe, module, howto]
summary: Pasos para crear `module-foo` con tools al core y vista SvelteKit.
related: [architecture/layered-architecture, harness-core/tool-execution, frontend-shell/routing-shell]
sources: []
---

# Añadir un módulo

Ejemplo: `module-git` (gestor de repos).

## 1. Crear crate
```bash
cargo new --lib crates/module-git
```

Editar `crates/module-git/Cargo.toml`:
```toml
[package]
name = "module-git"
version.workspace = true

[dependencies]
harness-core = { path = "../harness-core" }
tokio.workspace = true
async-trait.workspace = true
git2 = "0.20"
```

Añadir a `Cargo.toml` raíz `members`.

## 2. Implementar `HarnessTool`s

```rust
pub struct GitStatusTool;

#[async_trait::async_trait]
impl harness_core::HarnessTool for GitStatusTool {
    fn name(&self) -> &str { "git.status" }
    fn definition(&self) -> ToolDefinition { /* JSON schema */ }
    fn requires_approval(&self, _args: &Value) -> bool { false }
    async fn execute(&self, args: Value, ctx: ToolCtx<'_>) -> Result<ToolOutput, ToolError> {
        // ... usa git2 sobre ctx.sandbox.workspace_path
    }
}
```

## 3. Exponer namespace JSON-RPC

```rust
pub struct GitNamespace { /* state */ }

#[async_trait::async_trait]
impl NamespaceHandler for GitNamespace {
    fn name(&self) -> &str { "module.git" }
    async fn handle(&self, method: &str, params: Value, ctx: HandlerCtx) -> Result<Value, RpcError> {
        match method {
            "log" => self.log(params, ctx).await,
            "status" => self.status(params, ctx).await,
            // ...
            _ => Err(RpcError::method_not_found(method)),
        }
    }
}
```

## 4. Registrar en App Server

`crates/harness-app-server/src/main.rs`:
```rust
let modules = ModuleRegistry::new()
    .with(module_agents::register())
    .with(module_db::register())
    .with(module_ssh::register())
    .with(module_git::register());     // ← nuevo
```

`register()` devuelve `{ tools: Vec<Box<dyn HarnessTool>>, namespace: Box<dyn NamespaceHandler>, capabilities: ModuleCapabilities }`.

## 5. Schema y validación
Añadir `schemas/module-git.v1.json` para los params de cada method. CI valida que todos los params estén schematizados.

## 6. Frontend (SvelteKit)
- Nueva ruta `apps/desktop/src/routes/git/+page.svelte`.
- Componente `<GitPanel>` que consume `rpc.call("module.git.log", ...)`.
- Añadir al sidebar:
  ```ts
  // se publica via capabilities; sidebar lo recoge automáticamente
  ```

## 7. Tests
- Unit en `crates/module-git/tests/`.
- Integration: fixture con un repo git temporal, llamada via JSON-RPC.

## 8. Docs
Crear `docs/12-module-git/overview.md` (siguiendo formato [[meta/shard-format]]) y enlazar desde [[../README]].

## Checklist
- [ ] Crate creado y añadido al workspace
- [ ] `HarnessTool` implementadas con `requires_approval` razonable
- [ ] `NamespaceHandler` registrado
- [ ] Schemas JSON validados
- [ ] Vista SvelteKit
- [ ] Tests unit + integration
- [ ] Shards de docs
