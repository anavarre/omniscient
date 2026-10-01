#!/bin/bash
# UI smoke test: drives every page, dialog and shortcut off-screen (GTK
# Broadway backend, nothing appears on the desktop) and fails on any panic
# or GTK critical. Usage: scripts/smoke.sh [path/to/taskestra]
set -u
ulimit -c 0  # expected panics (e.g. a negative control) must not file system crash reports
cd "$(dirname "$0")/.."
BIN=${1:-target/release/taskestra}
TMP=$(mktemp -d)
trap 'kill $BW 2>/dev/null; rm -rf "$TMP"' EXIT

sqlite3 "$TMP/rails.sqlite3" < tests/rails_fixture.sql
"$BIN" --db "$TMP/app.sqlite3" --import "$TMP/rails.sqlite3" >/dev/null || { echo "import failed"; exit 1; }
sqlite3 "$TMP/app.sqlite3" "INSERT INTO settings VALUES ('identity', '1');
  INSERT INTO projects (name, created_at, updated_at) VALUES ('Empty', '2026-01-01', '2026-01-01');"

gtk4-broadwayd :37 --port 8137 >/dev/null 2>&1 & BW=$!
sleep 0.5

# Each scenario is one app run; keys are replayed 250 ms apart.
scenarios=(
  "j k j x x space J K l Left Return Escape"
  "g d g p g c g m g a g d b b G g g Home End"
  "question Escape s Escape C type:Smoke_Person S-Return Escape"
  "f type:rev Down Up Return Escape f type:zzz Return Escape"
  "t type:No_project S-Return Escape t Escape"
  "p j a a e Escape P Escape j Return t Escape e Escape Escape h"
  "p j j j Return g c g d Escape p a"
  "j Return focus:2 type:tom focus:2 type:nonsense S-Return Escape"
)
fail=0
for keys in "${scenarios[@]}"; do
  TASKESTRA_APP_ID=taskestra.smoke TASKESTRA_DEBUG_KEYS="$keys" GDK_BACKEND=broadway BROADWAY_DISPLAY=:37 \
    "$BIN" --db "$TMP/app.sqlite3" >"$TMP/log" 2>&1 &
  app=$!
  n=$(wc -w <<<"$keys")
  sleep "$(awk "BEGIN { print 1.5 + $n * 0.25 }")"
  if ! kill -0 $app 2>/dev/null; then
    echo "FAIL (exited) [$keys]"; cat "$TMP/log"; fail=1; continue
  fi
  kill $app; wait $app 2>/dev/null
  if grep -qiE "panicked|CRITICAL|WARNING" "$TMP/log"; then
    echo "FAIL [$keys]"; cat "$TMP/log"; fail=1
  else
    echo "ok   [$keys]"
  fi
done
exit $fail
