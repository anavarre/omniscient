#!/bin/bash
# End-to-end smoke test: replays real key presses into the app off-screen
# (Qt offscreen platform, nothing appears on the desktop), then checks the
# database. Any QML/Qt warning is fatal (QT_FATAL_WARNINGS), as is a panic.
#   scripts/smoke.sh [path/to/omniscient]
set -u
ulimit -c 0
cd "$(dirname "$0")/.."
BIN=${1:-target/release/omniscient}
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
fail=0

fresh() {
  rm -f "$TMP"/app.sqlite3*
  "$BIN" --db "$TMP/app.sqlite3" --import "$TMP/rails.sqlite3" >/dev/null || { echo "import failed"; exit 1; }
  sqlite3 "$TMP/app.sqlite3" "INSERT INTO settings VALUES ('identity', '1');"
}

# run "<name>" "<keys>" ["<sql>" "<expected>"]
run() {
  local name=$1 keys=$2 sql=${3:-} want=${4:-}
  QT_FORCE_STDERR_LOGGING=1 QT_QPA_PLATFORM=offscreen QT_FATAL_WARNINGS=1 \
    OMNISCIENT_OMARCHY_STATE="$TMP/state" OMNISCIENT_DEBUG_KEYS="$keys" \
    timeout 60 "$BIN" --db "$TMP/app.sqlite3" >"$TMP/log" 2>&1
  local code=$?
  if [ $code -ne 0 ] || grep -qiE "panicked|warning|error:" "$TMP/log"; then
    echo "FAIL $name (exit $code)"; sed 's/^/     /' "$TMP/log" | head -20; fail=1; return
  fi
  if [ -n "$sql" ]; then
    local got; got=$(sqlite3 "$TMP/app.sqlite3" "$sql")
    if [ "$got" != "$want" ]; then
      echo "FAIL $name: $sql → '$got', want '$want'"; fail=1; return
    fi
  fi
  echo "ok   $name"
}

sqlite3 "$TMP/rails.sqlite3" < tests/rails_fixture.sql
mkdir -p "$TMP/state/theme"
cp /usr/share/omarchy/themes/gruvbox/colors.toml "$TMP/state/theme/" 2>/dev/null || echo 'background = "#282828"' >"$TMP/state/theme/colors.toml"
echo gruvbox >"$TMP/state/theme.name"

fresh
run "navigate every page and grouping" \
  "j k j j g d g p g c g m g a G g g h p j j Return g c g d Escape p h b b"
run "dialogs open and close" \
  "question Escape s Escape C Escape f type:rev Down Up Escape t Escape P Escape"

run "complete commits when the app quits" "x" \
  "SELECT count(*) FROM tasks WHERE what LIKE 'Audit current%' AND completed_at IS NOT NULL" 1
run "unticking inside the undo window cancels" "j k x x" \
  "SELECT count(*) FROM tasks WHERE what LIKE 'Draft homepage%' AND completed_at IS NOT NULL" 0
run "blocked tasks can't be ticked" "j j x" \
  "SELECT count(*) FROM tasks WHERE what LIKE 'Draft homepage%' AND completed_at IS NOT NULL" 0

fresh
run "create a task on a project page" \
  "p j Return t type:Smoke_task Tab Tab type:tom S-Return" \
  "SELECT count(*) FROM tasks WHERE what = 'Smoke task' AND due_on = date('now', 'localtime', '+1 day')" 1
run "creating without a project shows an error and keeps the dialog" \
  "h t type:Orphan S-Return wait Escape" \
  "SELECT count(*) FROM tasks WHERE what = 'Orphan'" 0
run "edit a task's date by typing" \
  "h Return Tab Tab C-a type:2030-01-02 Return" \
  "SELECT due_on FROM tasks WHERE what LIKE 'Audit current%'" 2030-01-02
run "notes preview toggles" \
  "h Return Tab Tab Tab Tab Tab Tab Tab type:**bold** C-e wait C-e S-Return" \
  "SELECT count(*) FROM tasks WHERE notes LIKE '%**bold**%'" 1
run "pick a different identity in Settings" \
  "s BackSpace Down Down Return Escape g m" \
  "SELECT value <> '1' FROM settings WHERE key = 'identity'" 1
run "add a collaborator" \
  "C type:Smoke_Person Return Escape" \
  "SELECT count(*) FROM people WHERE name = 'Smoke Person'" 1
run "search opens a task" \
  "f type:budget Return wait Escape"
run "projects with open tasks can't be archived" \
  "p j a" \
  "SELECT count(*) FROM projects WHERE archived_at IS NOT NULL" 1

# Live theme switch mid-run: swap the theme the way omarchy-theme-set does.
( sleep 1.5
  mkdir -p "$TMP/state/next-theme"
  cp /usr/share/omarchy/themes/catppuccin-latte/colors.toml "$TMP/state/next-theme/" 2>/dev/null
  rm -rf "$TMP/state/theme"; mv "$TMP/state/next-theme" "$TMP/state/theme"
  echo catppuccin-latte >"$TMP/state/theme.name" ) &
run "live theme switch" "$(printf 'wait %.0s' {1..30}) j"
wait

exit $fail
