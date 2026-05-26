---
id: foundations/anthropic-principles
title: Principios de Anthropic (harness para apps largas)
shard: 01-foundations
tags: [anthropic, principles, multi-agent, evaluation]
summary: Tri-agente (planner/generator/evaluator), context anxiety y bias de auto-evaluación.
related: [foundations/harness-concept, foundations/design-tradeoffs, harness-core/context-compaction]
sources: [references/sources]
---

# Principios extraídos de Anthropic

Fuente: "Harness Design for Long-Running Application Development" (Anthropic Engineering).

## 1. Arquitectura tri-agente inspirada en GAN

| Rol | Función | Salida |
|---|---|---|
| **Planner** | Expande prompt → especificación con 10–16 features | Spec en archivo |
| **Generator** | Implementa iterativamente (sprint v1 / continuo v2) | Código + diffs |
| **Evaluator** | QA interactivo, califica contra criterios | Reportes + grados |

La separación generator/evaluator es **adversarial** y rompe el sesgo de auto-elogio.

## 2. Problemas que mitiga el harness

### Context anxiety
El modelo "cierra trabajo prematuramente" al acercarse al límite. Mitigaciones:
- **Context reset** + handoff estructurado (preferido para Sonnet 4.5).
- **Compaction** (resumen in-place) — insuficiente por sí solo.
- Opus 4.6 reduce mucho este problema → menos necesidad de resets.

### Self-evaluation bias
El generator alaba su propio output. Sin evaluator separado, el harness no converge a calidad.

## 3. Criterios de evaluación (frontend)
Ponderar **calidad subjetiva**:
- **Design quality** — coherencia, identidad.
- **Originality** — evitar "telltale AI": gradientes morados sobre cards blancas.
- **Craft** — tipografía, espaciado, contraste.
- **Functionality** — usabilidad independiente del look.

Evaluator usa **Playwright MCP** para interactuar con la página real. Few-shot calibration alinea con preferencias humanas.

## 4. Sprint contract (v1)
Antes de implementar, generator y evaluator negocian:
- Qué significa "done" para el sprint.
- Criterios testables (27+ por sprint complejo).
- Evita sobre-especificación pero mantiene alineación.

Comunicación **por archivos**, no instrucción directa.

## 5. v1 vs v2 — simplificar al subir el modelo

| | v1 (Sonnet 4.5) | v2 (Opus 4.6) |
|---|---|---|
| Sprints | sí, decompuestos | no, build continuo |
| QA | siempre | opcional según dificultad |
| Costo (DAW) | ~$200 / 6h | $124.70 / 3h 50min |

Lección: cada release re-evalúa qué componente es **load-bearing**.

## 6. Failure modes ↔ remedios

| Problema | Causa raíz | Remedio |
|---|---|---|
| Incoherencia en tareas largas | context anxiety | reset + handoff |
| Output bland | auto-elogio | evaluator separado |
| Features incompletas | generator no testea | QA interactivo (Playwright MCP) |
| Spec under-scoped | el modelo se salta planificación | planner dedicado |
| Layout/UX pobre | poca intuición de producto | spec con principios + evaluator |

## 7. Qué **no** funcionó
- Simplificación radical sin método (pierdes baseline de performance).
- Decomposición de sprints fija entre versiones del modelo.
- Confiar en auto-evaluación del generator.
