---
id: memory/continuity
title: CONTINUITY.md y resume de threads
shard: 14-memory
tags: [memory, continuity, resume, ui-banner, prompt-injection]
summary: Snapshot vivo de "qué hay en marcha"; UI banner + inyección selectiva al resume.
related: [memory/overview, memory/lifecycle, agents/orchestrator]
sources: []
---

# CONTINUITY.md

> **Archivo regenerado automáticamente**. Nunca se edita a mano. Vive en `profiles/<active>/memory/CONTINUITY.md`. Es la cara visible de "qué hay en marcha" — la primera cosa que tú y el harness consultan al volver a usar la herramienta.

## Cuándo se regenera

**On change** (cualquiera de estos):
- Transición de estado en alguna task de cualquier thread del profile.
- Apertura/cierre de un thread.
- Creación/promoción de una entrada `in_flight` en memoria.
- Cambio de `budget` que cruza un soft cap.

**Fallback periódico**: cada **1 hora** si nada disparó regeneración (mantiene la sección "Última actualización" relativamente fresca).

## Estructura

```markdown
# Continuidad — generado automáticamente

> Última actualización: 2026-05-26T19:30:00Z
> Profile activo: personal

## En marcha (across all threads del profile)

### Thread "Implementar paginación en /orders"
- Última actividad: hace 2h
- Working dir: ~/code/atomikigai/myapp
- T-0042 `in_progress` (frontend-agent paused, lease expired hace 30min)
- T-0043 `pending_verify` (esperando evaluator)
- T-0044 `blocked_by` T-0042, T-0043
- Budget: 4.20 / 10.00 USD consumido

### Thread "Setup CI básico"
- Última actividad: hace 1d
- T-0030 `done`
- T-0031 `in_progress` (devops-agent active hace 5min)
- Budget: 1.80 / 5.00 USD

## Pendientes editoriales (no son tasks)

- Decisión final sobre soporte Windows (ver memory/pending/2026-05-26-windows-support.md)
- Definir tasks-target reproducibles para GEPA (ver memory/pending/2026-05-22-gepa-targets.md)

## In-flight (discutiendo ahora)

- Arquitectura de memoria (ver memory/in-flight/2026-05-26-memory-design.md)

## Stats rápidas

- Threads activos: 2
- Threads archivados (último mes): 3
- Skills aprendidas (último mes): 7 promoted, 3 proposed pendientes review
- Costo último mes: ~$32 USD
```

## Quién lo escribe

El **harness scheduler** detecta cambios y dispara la regeneración. Implementación:

```rust
// pseudocódigo
async fn regenerate_continuity(profile: &Profile) -> Result<()> {
    let threads      = profile.threads.list_active().await?;
    let pendings     = profile.memory.list(kind = "pending").await?;
    let in_flights   = profile.memory.list(kind = "in_flight").await?;
    let recent_stats = profile.compute_recent_stats(30).await?;

    let md = render_continuity_template(threads, pendings, in_flights, recent_stats);
    fs::write(profile.memory_dir.join("CONTINUITY.md"), md).await?;
    profile.git.commit("memory: refresh CONTINUITY").await?;   // opcional: ruidoso en log
    Ok(())
}
```

Opción: omitir commit cada vez que se regenera y solo commit-ear cuando difiera **significativamente** del último (para no inflar el git log). Por defecto: commit cada cambio; si se vuelve ruidoso, switchear a `--amend` del commit "memory: refresh CONTINUITY" anterior.

## Dos consumidores

### 1. UI banner (dashboard)

Al abrir la UI o cambiar de profile:
```
GET /api/memory/continuity → CONTINUITY.md (parseado)
                          → render como card "Continuidad"
```

Card incluye:
- Threads en marcha (botones "Resume" / "Archive").
- Pendientes editoriales (links a la entrada en `/memory`).
- Stats rápidas.
- Un botón "Snapshot now" para forzar regeneración.

### 2. Inyección al prompt en **resume** de thread

**Solo al hacer resume de un thread existente** (decisión bloqueada).

Cuando el usuario clickea "Resume" en un thread:
1. Backend abre el thread.
2. El spawn del orchestrator recibe un preamble incluyendo:
   - Slice del CONTINUITY relevante **solo a ESTE thread**.
   - Lista de tasks del thread con su estado.
   - Última actividad y motivo de pausa si aplica.
3. NO se inyecta info de otros threads ni pendientes editoriales no relacionados.

Ejemplo del preamble:
```
## Continuidad del thread

Última actividad: hace 2h (paused)
Working dir: ~/code/atomikigai/myapp

Estado de tasks:
- T-0042 "Paginación frontend": in_progress, lease del frontend-agent expiró hace 30min
- T-0043 "Endpoint pagination": pending_verify, esperando evaluator
- T-0044 "Tests E2E": blocked_by T-0042, T-0043

Budget: 4.20 / 10.00 USD (42% consumido)

Spec.md no ha cambiado desde el cierre anterior.

Decisiones tomadas en este thread:
- (ninguna nueva desde la última sesión)

Continúa donde quedaste. Tu próxima acción típica sería reclamar T-0042 o
hacer plan para destrabar el lease.
```

## Por qué solo resume, no thread nuevo

(Decisión bloqueada). Si inyectáramos pendientes globales en cada thread nuevo:
- Polución de contexto: el agente ve cosas no relevantes.
- Tokens desperdiciados.
- Riesgo de que el agente se "salga del scope" del thread actual hacia un pendiente que vio mencionado.

Para threads nuevos, la memoria global se accede **bajo demanda** vía `memory.search(query)`. El orchestrator del thread nuevo, si su prompt humano menciona un tema conocido, hará la búsqueda y traerá lo relevante.

## API y CLI

```
GET  /api/memory/continuity         → CONTINUITY.md parseado
POST /api/memory/continuity/refresh → fuerza regeneración
GET  /api/memory/continuity/raw     → markdown crudo (para debug)

harness continuity                  → prints CONTINUITY.md en stdout
harness continuity refresh
```

## Tools para los agentes

`memory.continuity` (vía `harness-bridge`):
- Sin args.
- Devuelve `CONTINUITY.md` parseado a JSON.
- **Solo el orchestrator** debería llamarla en práctica. Otros agentes no necesitan ver el panorama global.
- Llamarla en un thread nuevo es "permitido pero raro"; el orchestrator lo hace para situarse al re-plan o al iniciar.

## Anti-patrones

| Mal | Bien |
|---|---|
| Editar `CONTINUITY.md` a mano | Es auto-generado; edita las fuentes (tasks, memoria) |
| Inyectar el contenido global en cada spawn | Solo al resume de thread + slice específico |
| Regenerar en cada update menor (commits ruidosos) | Throttle: max una regen por 10s; o amend |
| Snapshot estático que no envejece | Header con timestamp + refresh visible |
