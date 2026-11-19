# Omniscient

A native task app for [Omarchy](https://omarchy.org): a Rust core with a Qt Quick (QML) interface
via [cxx-qt](https://github.com/KDAB/cxx-qt), the same toolkit as the
Omarchy shell. Every colour, size, font and corner comes from the live
Omarchy theme, and theme switches apply while it runs.

## Omarchy integration

The app reads the same tokens as the Omarchy shell (`Commons/Color.qml` and
`Commons/Style.qml`):

| Source | Used for |
|---|---|
| `~/.local/state/omarchy/current/theme/colors.toml` | Palette and light/dark mode, with `omarchy-theme-color`'s fallbacks for keys a theme leaves out |
| `…/theme/shell.toml` (+ `~/.config/omarchy/shell.toml`) | `[controls]` fills and borders, `[font]` type scale, `[spacing]`, popup/menu/tooltip surfaces |
| fontconfig `monospace` (`fc-match`) | The Omarchy font (`omarchy-font-set`) |
| `hyprctl getoption decoration:rounding` | Corner radius |
| Nerd Font glyphs | Icons, as in the Omarchy bar |

`omarchy-theme-set` moves a new theme directory into place and rewrites
`theme.name` last. The app watches the state directory with inotify, settles
the burst of events (120 ms), re-resolves the tokens off the GUI thread, and
cross-fades to the new theme the way the shell does. Font and text-size
changes apply the same way. Every installed theme is checked for 4.5:1
contrast in the test suite.

The window's Wayland app id is `io.github.anavarre.Omniscient`, so
Hyprland's rules (opacity, borders, gaps) apply as they do to any other app.
It has no title bar; Hyprland tiles and decorates it.

## Install

```bash
make install              # → ~/.local/bin/omniscient + desktop entry + icon
```

It then appears in the Omarchy launcher (Super + Space). To give it a key,
add this to `~/.config/hypr/bindings.lua`:

```lua
o.bind("SUPER + SHIFT + T", "Omniscient", "omarchy-launch-or-focus io.github.anavarre.Omniscient omniscient")
```

Launching it again focuses the open window (one instance per database).
Build requirements: Rust ≥ 1.88, Qt ≥ 6.5 (base, declarative, wayland,
shadertools), a C++17 compiler.

## Data

SQLite, at `~/.local/share/omniscient/omniscient.sqlite3` unless you say
otherwise. To change it, use **Settings → Database location**: it copies the
database to the new folder or file, verifies the copy (integrity, foreign
keys, schema version, row counts of every table), saves the new location in
`~/.config/omniscient/config.toml` (`db_path = "…"`) and switches over. It
never overwrites an existing file, and the old copy is left in place for you
to delete. Any failure leaves the app on the old database.

Order of precedence: `--db <path>`, then `OMNISCIENT_DB`, then `db_path` in
the config file, then the default. The first two apply to one launch only and
disable the in-app move. To adopt an existing database (say, in a synced
folder), set `db_path` or `OMNISCIENT_DB`. Don't open one database from two
machines at once. The schema is versioned (`PRAGMA user_version`) and upgrades in
place. To bring over data from a compatible Rails database:

```bash
omniscient --import path/to/production.sqlite3   # only into an empty database
```

The import keeps ids, assignments, dependencies, sub-task order, completion
and auto-date state. ActionText notes become Markdown. Rows that can't be
carried over (no name, missing project) are skipped and listed; names that
differ only by case are merged.

## Using it

The rules:

- **Sections:** Overdue, Today, Tomorrow and Later. Mine shows every task
  due today or tomorrow; elsewhere each section shows ten tasks plus
  "Show N more".
- **Completing a task:** the row strikes through with a 10-second undo ring;
  untick to cancel. Dependencies and open sub-tasks block completion.
- **Auto-dates:** a completed task without a date gets today's date; its
  unblocked dependents get one 30 seconds later.
- **Projects:** archiving waits 10 seconds so you can cancel it. A project
  can't be archived or deleted while it has open tasks; a person can't be
  deleted while assigned to open tasks.
- **Quitting:** countdowns still running when the window closes or the app
  receives SIGTERM (e.g. at logout) are committed, auto-dating included.

Press `?` for every keyboard shortcut. Among them are `j`/`k`,
`x`, `t`, `g d`/`g p`/`g c`, `g m`/`g a`, `f`, `s`, `e`, `Esc` and
`Shift+Enter`, plus:

| Key | Action |
|---|---|
| `i` | Mark the selected task in progress |
| `a` | Archive the selected project (again to cancel) |
| `space`, `←`, `→` / `l` | Toggle sub-tasks |
| `J` / `K` | Move a sub-task down / up (or drag its handle) |
| `b` | Toggle the sidebar (a drawer on narrow tiles) |
| `Ctrl+K` | Search |
| `Ctrl+E` | Preview notes as Markdown |

The **When?** field takes typed dates: `today`, `tom`, `fri`, `next fri`,
`+3`, `2w`, `-1y`, `in 5 days`, `next week`, `oct 12`, `12 oct 2027`,
`2026-10-12`. The line under it shows how it was read.

**Who?**, **Project**, **Needs** and **Enables** are keyboard-first pickers:
type to filter, ↓/Enter to browse, Enter to pick, Backspace to remove. Each
can create what you typed ("Add …"). Notes are Markdown, shown rendered on
project pages and in the preview.

## Architecture

```
src/core/          plain Rust, no Qt — all of it unit-tested
  controller.rs    page state, row plans, undo countdowns, act()/query()
  rows.rs          keyed diff between two row lists (insert/remove/move/change)
  model.rs         snapshot + section/grouping/blocking rules
  db.rs            SQLite, migrations, validation, completion rules, import
  dates.rs         typed-date parser and formats
  theme.rs         Omarchy palette/shell tokens → JSON for QML
  json.rs          allocation-light JSON writer for row data
src/bridge/        cxx-qt: App (QAbstractListModel + act/query), Omarchy (tokens + watcher)
qml/               Main, Sidebar, PageHeader, rows/, dialogs/, components/
                   Theme/Store/Icons singletons
```

QML calls `App.act(name, json)` and `App.query(name, json)`. After an
action, the controller rebuilds the visible rows as `(key, json)` pairs and
the bridge sends QML only the difference, as fine-grained model
notifications. The `ListView` keeps its scroll position, selection and
running animations, and only creates delegates for rows on screen. Actions
that write nothing (navigation, starting a countdown) don't touch the
database. With 3,000 tasks an edit takes a few milliseconds.

## Development

```bash
make test        # core unit tests, incl. ported model specs and a contrast check of every installed theme
make smoke       # real key presses off-screen, then SQL assertions; any Qt/QML warning fails
make gallery     # every component on one page, for checking themes by eye
scripts/shot.sh out.png "j j Return" [theme] [WxH]   # screenshot after replaying keys
```

Developer environment variables:

| Variable | Effect |
|---|---|
| `OMNISCIENT_THEME_DIR=<theme dir>` | Preview another theme, e.g. `/usr/share/omarchy/themes/white` |
| `OMNISCIENT_OMARCHY_STATE=<dir>` | Use a copy of the Omarchy state directory, to test theme switches |
| `OMNISCIENT_DEBUG_KEYS="j j x"` | Replay keys as real key events, then quit |
| `OMNISCIENT_SCREENSHOT=<png>` | Save the window after the replay |
| `OMNISCIENT_GEOMETRY=640x760` | Window size (layout checks) |
| `OMNISCIENT_DEBUG_TIMING=1` | Print per-action timings |

Qt on this system logs to the journal; set `QT_FORCE_STDERR_LOGGING=1` to
see QML warnings in the terminal.

The GTK 4 prototype that preceded this port is kept at the `gtk-prototype`
tag.
