#!/usr/bin/env bash
# graphify-mcp.sh — lanza el MCP stdio server de Graphify sirviendo el grafo del paquete.
#
# Lo invoca Claude Code vía .mcp.json:  bash scripts/graphify-mcp.sh [<path-a-graph.json>]
#   <path-a-graph.json>  default: graphify-out/graph.json (relativo al repo root).
#
# Por qué un wrapper y no "python3 -m graphify.serve" directo en el .mcp.json:
#   graphify suele vivir en un venv de pipx, así que "python3" del sistema NO puede importarlo.
#   Este wrapper resuelve el intérprete en runtime (misma lógica que githooks/post-commit) → el
#   .mcp.json queda portable y commiteable (sin paths absolutos ni "python3" hardcodeado), y cada
#   máquina resuelve su propio Python al lanzar el server.
#
# No bloqueante: si graphify no es importable o el grafo no existe, imprime una guía a stderr y sale
# != 0. Claude Code marca el server como no disponible y el flujo /opsx cae al fallback GRAPH_REPORT.md.
set -eu

# --- ubicarse en el repo root (donde Claude Code lanza el server y donde vive el .mcp.json) ---
REPO_ROOT=$(git rev-parse --show-toplevel 2>/dev/null || echo ".")
cd "$REPO_ROOT" || exit 1

GRAPH="${1:-graphify-out/graph.json}"

# --- resolver un python que pueda importar graphify (pipx/venv/system) ---
# (misma lógica que githooks/post-commit — mantener en sync) ---
GRAPHIFY_BIN=$(command -v graphify 2>/dev/null || true)
if [ -n "$GRAPHIFY_BIN" ]; then
    _SHEBANG=$(head -1 "$GRAPHIFY_BIN" | sed 's/^#![[:space:]]*//')
    case "$_SHEBANG" in
        */env\ *) GRAPHIFY_PYTHON="${_SHEBANG#*/env }" ;;
        *)        GRAPHIFY_PYTHON="$_SHEBANG" ;;
    esac
    case "$GRAPHIFY_PYTHON" in
        *[!a-zA-Z0-9/_.-]*) GRAPHIFY_PYTHON="python3" ;;
    esac
    if ! "$GRAPHIFY_PYTHON" -c "import graphify" 2>/dev/null; then
        GRAPHIFY_PYTHON="python3"
    fi
else
    GRAPHIFY_PYTHON="python3"
fi

if ! "$GRAPHIFY_PYTHON" -c "import graphify.serve" 2>/dev/null; then
    echo "[graphify-mcp] 'graphify' no está instalado/importable — no puedo iniciar el MCP server." >&2
    echo "               Instalá:  pipx install graphifyy   (mientras tanto /opsx usa el fallback GRAPH_REPORT.md)" >&2
    exit 1
fi

if [ ! -f "$GRAPH" ]; then
    PKG_DIR=$(dirname "$(dirname "$GRAPH")")
    HINT="/graphify"; [ "$PKG_DIR" != "." ] && HINT="/graphify $PKG_DIR"
    echo "[graphify-mcp] grafo no encontrado: $REPO_ROOT/$GRAPH" >&2
    echo "               Construilo:  $HINT   (o hacé un commit para disparar el hook post-commit)" >&2
    exit 1
fi

exec "$GRAPHIFY_PYTHON" -m graphify.serve "$GRAPH"
