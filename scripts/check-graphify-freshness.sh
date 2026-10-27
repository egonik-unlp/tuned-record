#!/usr/bin/env bash
# Avisa si graphify-out/graph.json es más viejo que el último commit que toca src/.
# Vive en <package>/scripts/. Invocación manual o desde el post-commit hook. Exit 0 siempre (solo advierte).

set -eu

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
PKG_NAME="$(basename "$PROJECT_ROOT")"
GRAPH="$PROJECT_ROOT/graphify-out/graph.json"

if [ ! -f "$GRAPH" ]; then
  echo "⚠️  $PKG_NAME/graphify-out/graph.json no existe. Correr rebuild de Graphify." >&2
  exit 0
fi

GRAPH_TS=$(stat -c %Y "$GRAPH" 2>/dev/null || stat -f %m "$GRAPH")
cd "$PROJECT_ROOT"
LAST_SRC_COMMIT_TS=$(git log -1 --format=%ct -- 'src/' 2>/dev/null || echo 0)

if [ "$LAST_SRC_COMMIT_TS" -gt "$GRAPH_TS" ]; then
  DELTA=$(( (LAST_SRC_COMMIT_TS - GRAPH_TS) / 3600 ))
  echo "⚠️  Graphify stale: $PKG_NAME/graph.json tiene ${DELTA}h menos que el último commit de src/." >&2
  echo "   Rebuild: python3 -c \"from graphify.watch import _rebuild_code; from pathlib import Path; _rebuild_code(Path('$PKG_NAME'))\"" >&2
fi
