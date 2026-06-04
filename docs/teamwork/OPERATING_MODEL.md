# Modelo operativo del equipo

Objetivo: maximizar velocidad sin perder la señal de calidad del harness como
orquestador oficial.

## Autoridad

- **Claude / Planner** abre, prioriza y cierra tareas oficiales.
- **Claude / Planner** lanza reviewer, QA o evaluator oficiales desde el
  harness cuando el riesgo lo amerita.
- **Codex** implementa, corre checks locales, corrige findings y reporta
  evidencia.
- **Subagentes internos de Codex** pueden usarse como acelerador local, pero no
  cuentan como reviewer/QA oficial del harness.

## Rutas de ejecución

### Fast path

Usar para cambios pequenos, aislados y de bajo riesgo.

Flujo:
1. Codex implementa.
2. Codex corre tests/checks relevantes.
3. Codex hace self-review breve y reporta resultado.
4. Claude / Planner decide si pasa de `VERIFY` a cerrado.

### Review path

Usar para cambios medianos, con superficie frontend/backend o posible regresion.

Flujo:
1. Codex implementa y corre checks.
2. Codex puede pedir revision auxiliar interna para encontrar bugs rapido.
3. Los findings auxiliares se tratan como self-review asistida.
4. Claude / Planner decide si hace falta QA oficial.

### Official QA path

Usar para cambios de alto riesgo:

- auth, policy o permisos;
- sesiones, PTY, rehidratacion o procesos;
- persistencia append-only, replay, SSE o eventos;
- scheduler, budget o asignacion de agentes;
- migraciones de tipos compartidos;
- flujos multiagente o handoffs.

Flujo:
1. Claude / Planner abre la tarea y define criterios.
2. Codex u otro ejecutor implementa su slice.
3. Claude / Planner lanza reviewer/QA/evaluator oficial desde el harness.
4. Codex corrige findings.
5. Claude / Planner cierra con evidencia.

## Regla de reporte

Cuando Codex use subagentes internos, debe etiquetar el resultado como
**revision auxiliar**, no como QA oficial. El board solo debe marcar QA/review
oficial cuando provenga de una sesion/rol lanzado por el harness.

