#!/usr/bin/env bash
# SessionStart hook for HarnessDevTool — loads lightweight context into the session.
# Prints to stdout; Claude Code injects SessionStart stdout into the conversation context.
# Cheap by design: no paid CLIs are spawned. The dev team (Codex/Cursor/Sonnet) is invoked
# on demand by the Planner, never here. See CLAUDE.md §3.
set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root" || exit 0

backlog="docs/12-build-plan/pending-implementation-tasks.md"
board="docs/teamwork/BOARD.md"
improvement="docs/12-build-plan/improvement-plan.md"

echo "# Contexto de sesión — HarnessDevTool (equipo nativo)"
echo
echo "Eres el **Planner** del equipo de desarrollo (ver CLAUDE.md). No edites código: orquesta,"
echo "delega a Codex/Cursor/Sonnet y verifica. Estado actual del repo:"
echo

# --- Git ---
if git rev-parse --git-dir >/dev/null 2>&1; then
  echo "## Git"
  echo "- Rama: \`$(git branch --show-current 2>/dev/null || echo '?')\`"
  status="$(git status --porcelain 2>/dev/null)"
  if [ -n "$status" ]; then
    echo "- Árbol con cambios sin commitear:"
    echo '```'
    echo "$status"
    echo '```'
  else
    echo "- Árbol limpio."
  fi
  echo "- Últimos commits:"
  echo '```'
  git log --oneline -5 2>/dev/null
  echo '```'
  echo
fi

# --- Próxima tarea del backlog ---
if [ -f "$backlog" ]; then
  echo "## Próxima tarea (backlog)"
  next="$(grep -niE 'siguiente' "$backlog" 2>/dev/null | head -1)"
  if [ -n "$next" ]; then
    echo "$next" | sed 's/^/- /'
  else
    echo "- No encontré marcador \"siguiente\"; revisa \`$backlog\`."
  fi
  echo
  echo "Backlog completo: \`$backlog\`"
  echo
fi

# --- Board en curso ---
if [ -f "$board" ]; then
  echo "## Board — tarea en curso"
  echo "Ver \`$board\` para el detalle (objetivo, alcance, contrato, handoffs)."
  echo
fi

# --- Recordatorio de seguridad ---
if [ -f "$improvement" ]; then
  echo "## Recordatorio"
  echo "- Revisa residuales P1/P2 en \`$improvement\` antes de abrir trabajo grande."
  echo "- Gate histórico de dogfooding cerrado: P0 y rehidratación T4 están resueltos (CLAUDE.md §6)."
  echo
fi

exit 0
