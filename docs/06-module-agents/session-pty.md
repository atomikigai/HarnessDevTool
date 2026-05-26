---
id: module-agents/session-pty
title: Sesión PTY
shard: 06-module-agents
tags: [pty, portable-pty, terminal]
summary: Implementación PTY cross-OS con `portable-pty` y streaming a xterm.js.
related: [module-agents/claude-cli-bootstrap, module-agents/multi-agent]
sources: []
---

# Sesión PTY

## Crate
`portable-pty` — abstrae PTY en Unix y ConPTY en Windows.

## Estructura

```rust
pub struct AgentSession {
    pub id: SessionId,
    pty: Box<dyn MasterPty + Send>,
    child: Box<dyn Child + Send>,
    writer: Mutex<Box<dyn Write + Send>>,
    reader_task: JoinHandle<()>,
    exit_task: JoinHandle<()>,
}
```

## Reader task

```rust
let mut reader = pty.try_clone_reader()?;
let sink = sink.clone();
let id = session_id;
tokio::task::spawn_blocking(move || {
    let mut buf = [0u8; 4096];
    loop {
        let n = match reader.read(&mut buf) { Ok(0) | Err(_) => break, Ok(n) => n };
        sink.emit_blocking(ItemEvent::AgentOutput { id, data: buf[..n].to_vec() });
    }
});
```

`portable-pty` reader es sync → `spawn_blocking`.

## Resize
```rust
pty.resize(PtySize { rows, cols, ..Default::default() })?;
```

Llamado cada vez que la UI recalcula tamaño del xterm (debounce 100ms).

## Input
```rust
pub async fn input(&self, data: &[u8]) -> Result<()> {
    let mut w = self.writer.lock().await;
    w.write_all(data)?;
    w.flush()?;
    Ok(())
}
```

## Encoding del output
- Bytes crudos al cliente. xterm.js maneja UTF-8 y ANSI.
- En el `output.log` se guardan bytes raw también; un viewer reconstruye con un parser ANSI.

## Limpieza
- Drop de `AgentSession` cancela tasks y kill child.
- Verificar `child.wait()` no se quede colgado (timeout + force kill).
