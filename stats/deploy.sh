#!/usr/bin/env bash
# Deploy the Memento stats module to the Traction stats host (i167).
# rsync → /srv/stats/memento/app/, venv install, VERSION stamp, restart the
# stats-memento systemd unit, then verify /health reports the new version
# (a deploy that leaves the old version running must fail loudly).
#
# Usage:
#   REMOTE=max@158.160.163.167 ./deploy.sh
#   DRY_RUN=1 REMOTE=... ./deploy.sh
#
# Requires on the remote: passwordless sudo for rsync/chown/systemctl.
set -euo pipefail

# No default host on purpose: a stray ./deploy.sh must never land on the
# wrong box (see the MultiTool module's deploy.sh for the история).
REMOTE="${REMOTE:?set REMOTE=user@host (canonical: the stats box i167)}"
REMOTE_DIR="${REMOTE_DIR:-/srv/stats/memento/app}"
DRY_RUN="${DRY_RUN:-0}"

HERE="$(cd "$(dirname "$0")" && pwd)"

VERSION="$(git -C "$HERE" rev-parse --short HEAD 2>/dev/null || echo nogit)_$(date +%Y%m%d_%H%M%S)"
echo "$VERSION" > "$HERE/VERSION"
echo "[deploy] version $VERSION"

FLAGS=(-az --exclude=.venv --exclude=.DS_Store --exclude=__pycache__ --exclude='*.pyc'
  --exclude=data --exclude=.gitignore --exclude=deploy.sh)
[ "$DRY_RUN" = "1" ] && FLAGS+=(--dry-run -v)

echo "[deploy] $HERE -> $REMOTE:$REMOTE_DIR"
rsync "${FLAGS[@]}" --rsync-path="sudo rsync" "$HERE/" "$REMOTE:$REMOTE_DIR/"

if [ "$DRY_RUN" = "1" ]; then
  echo "[deploy] dry-run done; remote untouched"
  exit 0
fi

ssh "$REMOTE" "cd $REMOTE_DIR \
  && { [ -x .venv/bin/python ] || sudo python3 -m venv .venv; } \
  && sudo .venv/bin/pip install -q fastapi uvicorn \
  && sudo chown -R gigatool:gigatool /srv/stats/memento \
  && sudo systemctl restart stats-memento && sleep 2 \
  && systemctl is-active stats-memento \
  && DEPLOYED=\$(curl -sf http://127.0.0.1:9901/health | python3 -c 'import json,sys; print(json.load(sys.stdin)[\"version\"])') \
  && [ \"\$DEPLOYED\" = '$VERSION' ] \
  && curl -sf 'http://127.0.0.1:9901/summary?days=1' >/dev/null \
  && echo \"[remote] health + summary OK, version \$DEPLOYED\""
echo "[deploy] done"
