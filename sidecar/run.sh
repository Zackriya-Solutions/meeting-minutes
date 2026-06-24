#!/usr/bin/env bash
# Start the Meetily ML sidecar (local-only). Creates a venv on first run.
set -euo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$DIR"

PY="${PYTHON:-python3}"
VENV="$DIR/.venv"

if [ ! -d "$VENV" ]; then
  echo "[sidecar] creating venv…"
  "$PY" -m venv "$VENV"
fi
# shellcheck disable=SC1091
source "$VENV/bin/activate"

python -m pip install --quiet --upgrade pip
python -m pip install --quiet -r "$DIR/requirements.txt"

export MEET_SIDECAR_PORT="${MEET_SIDECAR_PORT:-8178}"
export OLLAMA_URL="${OLLAMA_URL:-http://127.0.0.1:11434}"

echo "[sidecar] listening on http://127.0.0.1:${MEET_SIDECAR_PORT}"
exec python "$DIR/app.py"
