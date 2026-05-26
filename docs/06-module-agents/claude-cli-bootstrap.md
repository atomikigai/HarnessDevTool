---
id: module-agents/claude-cli-bootstrap
title: Bootstrap del CLI `claude`
shard: 06-module-agents
tags: [claude-cli, bootstrap, installer]
summary: Detectar, sugerir instalación, validar versión y arrancar.
related: [module-agents/overview, module-agents/session-pty]
sources: []
---

# Bootstrap

## Detección
1. `which claude` (o `where.exe claude`).
2. Si no, buscar en paths típicos: `~/.local/bin`, `/usr/local/bin`, `~/.bun/bin`, `~/.npm-global/bin`.
3. Validar versión: `claude --version` → parsear semver.

## Versión mínima
Definida por el módulo (p.ej. `>= 2.0.0`). Si está por debajo: warning en UI con link al método de update.

## Instalación asistida (no auto)
Mostrar comandos según el SO; nunca ejecutar sin consentimiento:
- macOS: `brew install anthropics/tap/claude` o `curl ... | sh`
- Linux: `curl -fsSL https://claude.ai/install.sh | sh`
- Windows: instalador `.msi`.

## Spawn

```rust
use portable_pty::{native_pty_system, CommandBuilder, PtySize};

let pty = native_pty_system().openpty(PtySize { rows, cols, ..Default::default() })?;
let mut cmd = CommandBuilder::new(claude_path);
cmd.cwd(cwd);
cmd.env("FORCE_COLOR", "1");
for a in args { cmd.arg(a); }
let child = pty.slave.spawn_command(cmd)?;
```

Streams:
- `pty.master.try_clone_reader()` → task que empuja bytes como `module.agents.session.output`.
- `pty.master.take_writer()` → recibe input desde `session.input`.

## Variables de entorno seguras
- Filtrar `HARNESS_*` para no fugar config del shell.
- Preservar `PATH`, `HOME`, `LANG`, `TERM` (=`xterm-256color`).
- No exportar credenciales del provider; `claude` ya tiene su propia auth.

## Salida
- `child.wait()` en task aparte → emite `session.exited`.
- Si el usuario kill via UI: enviar `SIGINT`, esperar 3s, luego `SIGKILL`.
