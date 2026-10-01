#!/bin/bash
# Screenshot the app off-screen after replaying keys:
#   scripts/shot.sh out.png "j j Return" [omarchy-theme-name] [WxH]
# Uses a throwaway database seeded from tests/rails_fixture.sql.
set -eu
cd "$(dirname "$0")/.."
OUT=$1; KEYS=${2:-wait}; THEME=${3:-}; export OMNISCIENT_GEOMETRY=${4:-}
BIN=${BIN:-target/debug/omniscient}
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
sqlite3 "$TMP/rails.sqlite3" < tests/rails_fixture.sql
"$BIN" --db "$TMP/app.sqlite3" --import "$TMP/rails.sqlite3" >/dev/null
sqlite3 "$TMP/app.sqlite3" "INSERT INTO settings VALUES ('identity', '1');"
export OMNISCIENT_THEME_DIR=${THEME:+/usr/share/omarchy/themes/$THEME}
[ -z "$THEME" ] && unset OMNISCIENT_THEME_DIR
QT_FORCE_STDERR_LOGGING=1 QT_QPA_PLATFORM=offscreen \
  OMNISCIENT_DEBUG_KEYS="$KEYS" OMNISCIENT_SCREENSHOT="$(realpath -m "$OUT")" \
  timeout 30 "$BIN" --db "$TMP/app.sqlite3"
