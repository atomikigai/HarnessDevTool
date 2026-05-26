---
id: harness-core/sandbox
title: Sandbox
shard: 03-harness-core
tags: [sandbox, security, seccomp, fs-jail]
summary: Aislamiento de tool execution por OS (seccomp, sandbox-exec, AppContainer).
related: [harness-core/tool-execution, cross-cutting/security-model]
sources: []
---

# Sandbox

## Objetivos
- Limitar **escritura** del FS a rutas autorizadas (workspace + tmp).
- Bloquear acceso a credenciales del usuario fuera del workspace.
- Limitar red (allowlist de hosts).
- Limitar syscalls peligrosos (ptrace, mount, ...).

## Implementación por OS

| OS | Mecanismo | Crate |
|---|---|---|
| Linux | `seccomp-bpf` + namespaces + bind mounts | `seccompiler`, `nix` |
| macOS | `sandbox-exec` profile (.sb) | invoke via `Command` |
| Windows | AppContainer + Job Object | `windows-rs` |

## Niveles
```rust
pub enum SandboxLevel {
    None,             // dev only, requiere flag explícito
    Workspace,        // RW solo en project_root, lectura libre
    WorkspaceNet,     // + red bloqueada salvo allowlist
    Strict,           // lectura limitada a workspace + /usr + /etc/ssl
}
```

Default: `Workspace`.

## Modo "approval gate"
Si una tool necesita salir del sandbox, declara `requires_approval = true` y el ctx contiene un handle `escalate()` que crea un sub-sandbox temporal con permisos extra (auditado).

## Auditoría
Cada ejecución de tool sandboxed genera un evento `tool.executed` con:
- cmd / args (sanitizados)
- duración
- exit code
- bytes leídos / escritos
- syscalls denegados (si los hubo)

## Limitaciones honestas
- No reemplaza un container. Para tools que ejecutan código del usuario, recomendar Docker.
- En macOS, `sandbox-exec` está deprecado oficialmente pero sigue funcionando; tener fallback.
- Windows AppContainer es el mecanismo soportado a largo plazo.

## Testing
Suite de integración con tools maliciosas conocidas (escribe a `~/.ssh`, `curl evil.com`, `fork bomb`, ...) — todas deben fallar con causa clara.
