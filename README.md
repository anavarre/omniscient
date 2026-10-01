# Taskestra (native)

A native Rust + GTK 4 recreation of the [Taskestra](../taskestra) Rails task
app, built for [Omarchy](https://omarchy.org). It paints itself entirely from
the active Omarchy theme and follows theme switches live.

## Omarchy integration

The app reads the same tokens as the Omarchy shell (`Commons/Color.qml` and
`Commons/Style.qml`):

| Source | Used for |
|---|---|
| `~/.local/state/omarchy/current/theme/colors.toml` | Palette, light/dark mode |
| `…/theme/shell.toml` (+ `~/.config/omarchy/shell.toml`) | `[controls]` hover/focus/selected fills and borders, `[font] base-size`, `[spacing] scale`, popup/menu/tooltip surfaces |
| fontconfig `monospace` | The Omarchy font (`omarchy font set`) |
| `hyprctl getoption decoration:rounding` | Corner radius |
| Nerd Font glyphs | Icons, as in the Omarchy bar |

`omarchy-theme-set` replaces the theme directory and rewrites `theme.name`.
The app watches for that and restyles within about 120 ms, with no restart.
It has no title bar because Hyprland tiles and decorates it. Omarchy's
window rules (opacity, borders) apply as they do to any other app.

## Install

```bash
make install              # → ~/.local/bin/taskestra + desktop entry + icon
```

It then appears in the Omarchy launcher (Super + Space). To give it a key,
add this to `~/.config/hypr/bindings.lua`:

```lua
o.bind("SUPER + SHIFT + T", "Taskestra", "omarchy-launch-or-focus io.github.anavarre.Taskestra taskestra")
```

The app is single-instance, so launching it again focuses the open window.

## Data

The data lives in SQLite at `~/.local/share/taskestra/taskestra.sqlite3`
(`--db <path>` to override). To bring over the data from the Rails app:

```bash
taskestra --import path/to/production.sqlite3   # only into an empty database
```

The import keeps ids, assignments, dependencies, sub-task order, completion
and auto-date state. ActionText notes become plain text.

## Using it

The app follows the web app's rules:

- **Sections:** Overdue, Today, Tomorrow and Later. Mine shows every task
  due today; the other sections show ten tasks each, plus a "Show N more"
  link.
- **Completing a task:** completion waits 10 seconds so you can undo it.
- **Blocking:** dependencies and sub-tasks block completion.
- **Auto-dates:** a completed task without a date gets today's date. Its
  unblocked dependents get one 30 seconds later.
- **Projects:** archiving waits 10 seconds so you can cancel it. A project
  can't be deleted or archived while it has open tasks.
- **Quitting:** a countdown still running when the app closes or receives
  SIGTERM (e.g. at logout) is committed, as the web app did when you left
  the page.

Press `?` in the app for every keyboard shortcut. They match the web app
(`j`/`k`, `x`, `t`, `g d`/`g p`/`g c`, `g m`/`g a`, `f`, `s`, `Shift+Enter`…),
plus a few additions:

| Key | Action |
|---|---|
| `b` | Toggle the sidebar (it hides by itself on narrow tiles) |
| `a` | Archive the selected project (press again to cancel) |
| `space`, `←`, `→` | Toggle sub-tasks |
| `J` / `K` | Move a sub-task down / up |
| `ctrl k` | Search |

The **When?** field takes typed dates: `today`, `tom`, `fri`, `next fri`,
`+3`, `2w`, `in 5 days`, `next week`, `oct 12`, `12 oct 2027`, `2026-10-12`.
A line under the field shows how it was read. The calendar button picks a
date with the mouse.

**Who?** and **Project** pickers can create a person or project on the fly
(choose “Add …”). New tasks are assigned to you once you've set your
identity (`s`).

## Layout and performance

- **Narrow tiles:** under about 1,100 px the sidebar hides, and under about
  760 px the Who column shows avatars only.
- **Rendering:** each change reloads the whole database (about 2 ms) and
  rebuilds the page. The first 40 rows are built at once and the rest are
  built in the background without blocking. Replaced pages are taken down
  in chunks after the new one is on screen. With 3,000 tasks the worst case
  is about 30 ms; typical data takes 2–5 ms.

## Development

```bash
make test        # unit tests: dates, list rules, DB rules, theme parsing
make smoke       # drives every page/dialog/shortcut off-screen (GTK Broadway); fails on panic or GTK critical
make run
```

Developer environment variables:

| Variable | Effect |
|---|---|
| `TASKESTRA_THEME_DIR=<theme dir>` | Preview another theme |
| `TASKESTRA_OMARCHY_STATE=<dir>` | Use a copy of the Omarchy state directory, to test theme switches |
| `TASKESTRA_DEBUG_TIMING=1` | Print render timings |
| `TASKESTRA_DEBUG_KEYS="j j x"` | Replay keys at startup |
| `TASKESTRA_APP_ID=<id>` | Run a second instance |

**State borrowing rule.** `App` state lives in `RefCell`s, and a panic
inside a GTK callback aborts the process. Never hold `st`/`snap`/`modals`
borrowed across a call back into `App` or a GTK call that can fire
handlers, such as focus or sensitivity changes or removing widgets.
Copy out what you need first. Watch for the hidden case: in
`match x.borrow().field { … }` and `if let … = x.borrow()…` the borrow lives
for the whole block. Release builds keep line tables, so a core dump or
panic names the exact line.

Code layout:

```
src/dates.rs   typed-date parser and formats
src/model.rs   snapshot + section/grouping/blocking rules
src/db.rs      SQLite schema, validation, completion rules, Rails import
src/theme.rs   Omarchy tokens → GTK CSS, live reload
src/ui/        window, pages, rows, editor, pickers, dialogs
```
