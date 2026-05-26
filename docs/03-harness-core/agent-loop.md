---
id: harness-core/agent-loop
title: Agent loop
shard: 03-harness-core
tags: [agent-loop, control-flow, core]
summary: El bucle central que orquesta prompt, modelo y tool calls.
related: [harness-core/turn-and-item-primitives, harness-core/tool-execution, harness-core/streaming-events]
sources: [foundations/openai-codex-architecture]
---

# Agent loop

## Pseudocódigo

```rust
pub async fn run_turn(&mut self, user_msg: UserMessage) -> Result<TurnOutcome> {
    self.history.push(Item::user(user_msg));
    self.persist_pending().await?;

    loop {
        let prompt = self.prompt_builder.build(&self.history, &self.config);
        let stream = self.llm.stream(prompt).await?;

        let mut tool_calls = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            match self.handle_chunk(chunk).await? {
                ChunkOutcome::TextDelta => { /* item.delta emitido */ }
                ChunkOutcome::ToolCall(tc) => tool_calls.push(tc),
                ChunkOutcome::Final => return Ok(TurnOutcome::Completed),
            }
        }

        if tool_calls.is_empty() { return Ok(TurnOutcome::Completed); }

        let results = self.tools.run_parallel_preserve_order(tool_calls).await?;
        for r in results { self.history.push(Item::tool_result(r)); }
        // continúa el loop → siguiente request al provider
    }
}
```

## Garantías
- **Append-only history**: nunca se reordena ni borra; ver [[harness-core/prompt-caching]].
- **Cancelable**: un `CancellationToken` propaga a stream y tools.
- **Idempotente al reanudar**: si el proceso muere antes de cerrar el turn, el resume reconstruye desde events y continúa.

## Terminación
Tres condiciones cierran el turn:
1. Modelo emite mensaje final sin tool calls.
2. Excede `max_iterations` (default 50) → emite `turn.aborted` con causa `iteration-limit`.
3. Cancelación externa → `turn.cancelled`.

## Métricas por turn
- duración, # iteraciones, # tool calls, tokens prompt/completion, costo estimado.
- Emitidas como atributos del span `tracing` (`turn.run`).

## Anti-patrones
- Insertar items "del sistema" entre el último user msg y la respuesta → invalida cache.
- Llamar a tools síncronamente; usar `FuturesOrdered` desde el inicio.
- Mutar el prompt antes de invalidación deliberada → cache miss silencioso.
