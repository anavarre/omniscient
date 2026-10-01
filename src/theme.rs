//! Omarchy theme → GTK stylesheet.
//!
//! Mirrors the Omarchy shell's `Commons/Color.qml` + `Commons/Style.qml` so
//! the app paints with the same tokens as the bar, menus and panels:
//!
//! - `~/.local/state/omarchy/current/theme/colors.toml`: the palette
//! - `~/.local/state/omarchy/current/theme/shell.toml`: control states, type scale, spacing
//! - `~/.config/omarchy/shell.toml`: machine-level overrides (`omarchy display text size`)
//! - the fontconfig `monospace` alias: the Omarchy font
//! - `hyprctl getoption decoration:rounding`: corner radius
//!
//! `omarchy-theme-set` moves a freshly built theme directory over
//! `current/theme` and rewrites `current/theme.name`, so watching the
//! `current` directory catches every switch.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use gtk::{gdk, gio, glib, prelude::*};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba {
    r: f64,
    g: f64,
    b: f64,
    a: f64,
}

impl Rgba {
    fn hex(s: &str) -> Option<Rgba> {
        let s = s.trim().trim_start_matches('#');
        let v = |i: usize| u8::from_str_radix(s.get(i..i + 2)?, 16).ok().map(|n| n as f64 / 255.0);
        match s.len() {
            6 => Some(Rgba { r: v(0)?, g: v(2)?, b: v(4)?, a: 1.0 }),
            8 => Some(Rgba { r: v(0)?, g: v(2)?, b: v(4)?, a: v(6)? }),
            3 => {
                let d = |i: usize| u8::from_str_radix(&s[i..i + 1].repeat(2), 16).ok().map(|n| n as f64 / 255.0);
                Some(Rgba { r: d(0)?, g: d(1)?, b: d(2)?, a: 1.0 })
            }
            _ => None,
        }
    }

    /// Hyprland-style tokens: `#rrggbb`, `rgba(rrggbbaa)`, or a gradient
    /// `rgba(..) rgba(..) 45deg` whose first stop is used.
    fn token(s: &str) -> Option<Rgba> {
        let first = s.split_whitespace().find(|p| !p.ends_with("deg"))?;
        if let Some(inner) = first.strip_prefix("rgba(").or_else(|| first.strip_prefix("rgb(")) {
            return Rgba::hex(inner.trim_end_matches(')'));
        }
        if first.starts_with('#') { Rgba::hex(first) } else { None }
    }

    pub fn alpha(self, a: f64) -> Rgba {
        Rgba { a: a.clamp(0.0, 1.0), ..self }
    }

    pub fn mix(self, o: Rgba, t: f64) -> Rgba {
        let t = t.clamp(0.0, 1.0);
        Rgba {
            r: self.r + (o.r - self.r) * t,
            g: self.g + (o.g - self.g) * t,
            b: self.b + (o.b - self.b) * t,
            a: self.a + (o.a - self.a) * t,
        }
    }

    fn luminance(self) -> f64 {
        0.2126 * self.r + 0.7152 * self.g + 0.0722 * self.b
    }

    pub fn to_hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", (self.r * 255.0).round() as u8, (self.g * 255.0).round() as u8, (self.b * 255.0).round() as u8)
    }

    pub fn css(self) -> String {
        format!(
            "rgba({},{},{},{:.3})",
            (self.r * 255.0).round(),
            (self.g * 255.0).round(),
            (self.b * 255.0).round(),
            self.a
        )
    }

    pub fn gdk(self) -> gdk::RGBA {
        gdk::RGBA::new(self.r as f32, self.g as f32, self.b as f32, self.a as f32)
    }
}

const BLACK: Rgba = Rgba { r: 0.0, g: 0.0, b: 0.0, a: 1.0 };

/// Every value the stylesheet and the custom-drawn widgets need.
#[derive(Clone, Debug)]
pub struct Tokens {
    pub name: String,
    pub dark: bool,
    pub bg: Rgba,
    pub bg_dark: Rgba,
    pub fg: Rgba,
    pub fg_dim: Rgba,
    pub muted: Rgba,
    pub accent: Rgba,
    pub red: Rgba,
    pub yellow: Rgba,
    pub green: Rgba,
    pub hairline: Rgba,
    pub normal_fill: Rgba,
    pub hover_fill: Rgba,
    pub focus_fill: Rgba,
    pub selected_fill: Rgba,
    pub pressed_fill: Rgba,
    pub selection_fill: Rgba,
    pub normal_border: Rgba,
    pub hover_border: Rgba,
    pub focus_border: Rgba,
    pub normal_border_width: i64,
    pub popup_bg: Rgba,
    pub popup_border: Rgba,
    pub tooltip_bg: Rgba,
    pub menu_selected_text: Rgba,
    pub scrim: Rgba,
    pub font_base: f64,
    pub spacing_scale: f64,
    pub radius: i64,
}

fn home() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
}

fn state_dir() -> PathBuf {
    // TASKESTRA_OMARCHY_STATE points at a copy of the state dir (dev aid for
    // testing theme switches without touching the real theme).
    std::env::var_os("TASKESTRA_OMARCHY_STATE")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".local/state/omarchy/current"))
}

/// The active theme; `TASKESTRA_THEME_DIR` previews another one (dev aid).
fn theme_dir() -> PathBuf {
    std::env::var_os("TASKESTRA_THEME_DIR").map(PathBuf::from).unwrap_or_else(|| state_dir().join("theme"))
}

fn user_shell() -> PathBuf {
    home().join(".config/omarchy/shell.toml")
}

fn read(path: PathBuf) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

fn parse_colors(raw: &str) -> (HashMap<String, Rgba>, Option<bool>) {
    let mut out = HashMap::new();
    let mut dark = None;
    for line in raw.lines() {
        let Some((k, v)) = line.split_once('=') else { continue };
        let k = k.trim();
        let v = v.trim();
        // Quoted values may contain '#'; bare ones end at whitespace.
        let v = match v.strip_prefix('"').or_else(|| v.strip_prefix('\'')) {
            Some(q) => q.split(['"', '\'']).next().unwrap_or(""),
            None => v.split_whitespace().next().unwrap_or(""),
        };
        if k == "mode" {
            dark = Some(v != "light");
        } else if let Some(c) = Rgba::hex(v) {
            out.insert(k.to_string(), c);
        }
    }
    (out, dark)
}

/// The shell's TOML walker: `section.key → raw string`.
fn parse_shell(raw: &str, into: &mut HashMap<String, String>) {
    let mut section = String::new();
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(s) = line.strip_prefix('[').and_then(|l| l.split(']').next()) {
            section = s.trim().to_string();
            continue;
        }
        let Some((k, v)) = line.split_once('=') else { continue };
        if section.is_empty() {
            continue;
        }
        let v = v.trim();
        let v = if let Some(q) = v.strip_prefix('"').or_else(|| v.strip_prefix('\'')) {
            q.split(['"', '\'']).next().unwrap_or("")
        } else {
            v.split('#').next().unwrap_or("").trim()
        };
        into.insert(format!("{section}.{}", k.trim()), v.to_string());
    }
}

struct Resolver<'a> {
    shell: &'a HashMap<String, String>,
    fg: Rgba,
    bg: Rgba,
    accent: Rgba,
    red: Rgba,
    muted: Rgba,
}

impl Resolver<'_> {
    fn role(&self, token: &str, fallback: Rgba, depth: u8) -> Rgba {
        let t = token.trim();
        if t.is_empty() || depth > 4 {
            return fallback;
        }
        if t.contains('.')
            && let Some(v) = self.shell.get(t) {
                return self.role(v, fallback, depth + 1);
            }
        match t.to_lowercase().as_str() {
            "foreground" | "text" => self.fg,
            "accent" => self.accent,
            "urgent" => self.red,
            "muted" => self.muted,
            "background" => self.bg,
            "transparent" => self.bg.alpha(0.0),
            _ => Rgba::token(t).unwrap_or(fallback),
        }
    }

    fn num(&self, key: &str, fallback: f64) -> f64 {
        self.shell.get(key).and_then(|v| v.parse::<f64>().ok()).unwrap_or(fallback)
    }

    fn get(&self, key: &str) -> Option<&str> {
        self.shell.get(key).map(String::as_str)
    }

    /// `[controls] <state>-color` × `<state>-fill-alpha` / `-border-alpha`.
    fn state(&self, state: &str, kind: &str, alpha_fallback: f64) -> Rgba {
        let color_key = format!("controls.{state}-color");
        let token = self.get(&color_key).or_else(|| if state != "normal" { self.get("controls.hover-cursor-color") } else { None });
        let color = token.map(|t| self.role(t, self.fg, 0)).unwrap_or(self.fg);
        color.alpha(self.num(&format!("controls.{state}-{kind}-alpha"), alpha_fallback))
    }

    fn surface(&self, section: &str, key: &str, fallback: Rgba, alpha_fallback: f64) -> Rgba {
        let color = self.get(&format!("{section}.{key}")).map(|t| self.role(t, fallback, 0)).unwrap_or(fallback);
        color.alpha(self.num(&format!("{section}.{key}-alpha"), alpha_fallback))
    }
}

pub fn load(radius: i64) -> Tokens {
    let dir = state_dir();
    let (p, mode) = parse_colors(&read(theme_dir().join("colors.toml")));
    let mut shell = HashMap::new();
    parse_shell(&read(theme_dir().join("shell.toml")), &mut shell);
    parse_shell(&read(user_shell()), &mut shell);

    let pick = |keys: &[&str], fallback: Rgba| keys.iter().find_map(|k| p.get(*k).copied()).unwrap_or(fallback);
    let bg = pick(&["background", "color0"], Rgba::hex("#101315").unwrap());
    let fg = pick(&["foreground", "color7"], Rgba::hex("#cacccc").unwrap());
    let dark = mode.unwrap_or(bg.luminance() < 0.5);
    let accent = pick(&["accent", "color4", "blue"], fg);
    let red = pick(&["red", "color1"], Rgba::hex("#e06c75").unwrap());
    let muted = pick(&["muted", "color8"], bg.mix(fg, 0.45));

    let r = Resolver { shell: &shell, fg, bg, accent, red, muted };
    let hover_fill_alpha = r.num("controls.hover-cursor-fill-alpha", 0.08);
    let hover_border_alpha = r.num("controls.hover-cursor-border-alpha", 0.25);
    let normal_width = r.num("controls.normal-border-width", 1.0).round().max(0.0) as i64;
    let scale_with_font = !matches!(r.get("spacing.scale-with-font"), Some("false" | "0" | "no" | "off"));
    let font_base = r.num("font.base-size", 12.0).max(1.0);

    Tokens {
        name: read(dir.join("theme.name")).trim().to_string(),
        dark,
        bg,
        bg_dark: pick(&["dark_background"], bg.mix(BLACK, if dark { 0.25 } else { 0.04 })),
        fg,
        fg_dim: pick(&["dark_foreground"], fg.mix(bg, 0.45)),
        muted,
        accent,
        red,
        yellow: pick(&["yellow", "color3"], Rgba::hex("#e5c07b").unwrap()),
        green: pick(&["green", "color2"], Rgba::hex("#98c379").unwrap()),
        hairline: bg.mix(fg, if dark { 0.12 } else { 0.14 }),
        normal_fill: r.state("normal", "fill", 0.04),
        hover_fill: r.state("hover-cursor", "fill", 0.08),
        focus_fill: r.state("focus", "fill", hover_fill_alpha),
        selected_fill: r.state("selected", "fill", 0.18),
        pressed_fill: r.state("pressed", "fill", 0.22),
        selection_fill: r.state("selection", "fill", 0.35),
        normal_border: r.state("normal", "border", 0.4),
        hover_border: r.state("hover-cursor", "border", 0.25),
        focus_border: r.state("focus", "border", hover_border_alpha),
        normal_border_width: normal_width,
        popup_bg: r.surface("popups", "background", bg, 1.0),
        popup_border: r.surface("popups", "border", accent, 1.0),
        tooltip_bg: r.surface("tooltip", "background", bg, 0.97),
        menu_selected_text: r.get("menu.selected-text").map(|t| r.role(t, accent, 0)).unwrap_or(accent),
        scrim: r.surface("menu", "scrim", bg, 0.5),
        font_base,
        spacing_scale: r.num("spacing.scale", 1.0).max(0.0) * if scale_with_font { (font_base / 12.0).max(1.0 / 12.0) } else { 1.0 },
        radius,
    }
}

impl Tokens {
    /// The shell's `Style.space(px)`.
    pub fn sp(&self, px: f64) -> i64 {
        ((px * self.spacing_scale).round() as i64).max(1)
    }

    /// The shell's type scale (`Style.font.<token>` multipliers).
    pub fn font(&self, mult: f64) -> f64 {
        (self.font_base * mult).round().max(1.0)
    }

    pub fn css(&self) -> String {
        let t = self;
        let c = |c: Rgba| c.css();
        let r = t.radius;
        let small_r = r.min(4);
        // App body sits a step above the shell's 12px bar text: task titles
        // are read, not glanced at.
        let body = t.font(1.167);
        let small = t.font(1.0);
        let caption = t.font(0.917);
        let heading = t.font(1.333);
        let display = t.font(2.0);
        let primary_fg = if t.accent.luminance() > 0.55 { t.bg.mix(BLACK, 0.2) } else { t.bg.mix(t.fg, 0.0) };
        let primary_fg = if (primary_fg.luminance() - t.accent.luminance()).abs() < 0.35 {
            if t.accent.luminance() > 0.5 { BLACK.mix(t.bg, 0.1) } else { Rgba { r: 1.0, g: 1.0, b: 1.0, a: 1.0 } }
        } else {
            primary_fg
        };
        let today_bg = t.fg;
        let today_fg = t.bg;
        let sp = |px: f64| t.sp(px);
        format!(
            r#"
/* Generated from Omarchy theme "{name}" */
window.taskestra, window.taskestra * {{
  font-family: monospace;
  -gtk-icon-shadow: none;
  text-shadow: none;
}}
window.taskestra {{
  background-color: {bg};
  color: {fg};
  font-size: {body}px;
}}
window.taskestra * {{
  transition: background-color 180ms ease-out, border-color 180ms ease-out, opacity 160ms ease-out;
}}
selection, text selection, label selection {{ background-color: {selection_fill}; color: {fg}; }}
tooltip {{ background-color: {tooltip_bg}; color: {fg}; border: 1px solid {popup_border}; border-radius: {r}px; box-shadow: none; }}
tooltip > box {{ padding: {s3}px {s6}px; }}
tooltip label {{ font-size: {caption}px; }}

scrollbar {{ background: none; border: none; }}
scrollbar slider {{ background-color: {scroll}; min-width: 4px; min-height: 4px; border-radius: {small_r}px; border: none; }}
scrollbar slider:hover {{ background-color: {scroll_hover}; }}
undershoot, overshoot {{ background: none; }}

/* ---------------------------------------------------------------- chrome */
.sidebar {{ background-color: {bg_dark}; border-right: 1px solid {hairline}; padding: {s14}px {s10}px; }}
.brand {{ font-weight: bold; font-size: {heading}px; padding: {s4}px {s8}px {s14}px {s8}px; }}
.brand .glyph {{ color: {accent}; }}
.nav-heading {{ color: {fg_dim}; font-size: {caption}px; font-weight: bold; letter-spacing: 1px; padding: {s14}px {s8}px {s4}px {s8}px; }}
button.nav {{ padding: {s6}px {s8}px; color: {fg}; }}
button.nav .count {{ color: {fg_dim}; font-size: {small}px; }}
button.nav .glyph {{ color: {fg_dim}; min-width: {glyph_w}px; }}
button.nav.active {{ background-color: {selected_fill}; color: {menu_selected}; font-weight: bold; }}
button.nav.active .glyph, button.nav.active .count {{ color: {menu_selected}; }}
.identity {{ color: {fg_dim}; font-size: {small}px; }}

.page {{ padding: {s22}px {s30}px {s40}px {s30}px; }}
.page-title {{ font-size: {display}px; font-weight: bold; }}
.page-subtitle {{ color: {fg_dim}; }}
.page-meta {{ color: {fg_dim}; font-size: {small}px; }}
.page-meta.link {{ color: {accent}; }}
.notes-block {{ color: {fg_dim}; border-left: 2px solid {hairline}; padding: {s2}px 0 {s2}px {s10}px; }}
.section-title {{ font-size: {heading}px; font-weight: bold; margin-top: {s18}px; }}
.section-title.link:hover {{ color: {accent}; }}
.section-count {{ color: {fg_dim}; font-size: {small}px; }}
.hint {{ color: {fg_dim}; font-size: {small}px; }}
.hint b {{ color: {fg}; }}
.count-badge {{ color: {fg_dim}; font-size: {small}px; padding: {s3}px {s8}px; border: 1px solid {hairline}; border-radius: {r}px; }}

/* ---------------------------------------------------------------- controls */
button {{
  background: none; background-image: none; box-shadow: none; outline: none;
  border: {nbw}px solid transparent; border-radius: {r}px;
  min-height: 0; min-width: 0; padding: {s4}px {s8}px; color: {fg};
}}
button:hover {{ background-color: {hover_fill}; border-color: {hover_border}; }}
button:active {{ background-color: {pressed_fill}; }}
button:focus-visible {{ background-color: {focus_fill}; border-color: {focus_border}; }}
button.flat:not(:hover):not(:focus-visible) {{ border-color: transparent; }}
button.bordered, button.btn {{ border-color: {normal_border}; background-color: {normal_fill}; padding: {s6}px {s12}px; }}
button.btn:hover {{ border-color: {hover_border_strong}; background-color: {hover_fill}; }}
button.primary {{ background-color: {accent}; border-color: {accent}; color: {primary_fg}; font-weight: bold; }}
button.primary:hover {{ background-color: {accent_hover}; border-color: {accent_hover}; }}
button.primary:active {{ background-color: {accent_press}; }}
button.danger:hover {{ color: {red}; border-color: {red}; background-color: {red_soft}; }}
button.danger.primary {{ background-color: {red}; border-color: {red}; color: {bg}; }}
button.icon {{ padding: {s2}px {s6}px; color: {fg_dim}; }}
button.icon:hover {{ color: {fg}; }}
button .key {{ color: {fg_dim}; font-size: {caption}px; margin-left: {s6}px; }}
button.primary .key {{ color: {primary_fg}; opacity: 0.7; }}

.segmented {{ border: 1px solid {normal_border}; border-radius: {r}px; padding: 1px; background-color: {normal_fill}; }}
.segmented button {{ padding: {s3}px {s10}px; font-size: {small}px; color: {fg_dim}; border-width: 0; }}
.segmented button.active {{ background-color: {selected_fill}; color: {fg}; font-weight: bold; }}

entry, textview.field, .field-box {{
  background-color: {normal_fill}; color: {fg};
  border: {nbw}px solid {normal_border}; border-radius: {r}px;
  box-shadow: none; outline: none; min-height: 0;
}}
entry {{ padding: {s6}px {s10}px; caret-color: {accent}; }}
entry:hover, .field-box:hover {{ border-color: {hover_border_strong}; }}
entry:focus-within, .field-box:focus-within, textview.field:focus-within {{ border-color: {focus_border_strong}; background-color: {focus_fill}; }}
entry > text > placeholder, textview.field text placeholder {{ color: {placeholder}; }}
entry.bare {{ border: none; background: none; padding: {s4}px {s6}px; }}
entry.bare:focus-within {{ background: none; }}
entry.invalid {{ border-color: {red}; }}
textview.field {{ padding: {s8}px {s10}px; }}
textview.field text {{ background: none; color: {fg}; caret-color: {accent}; }}
.field-label {{ color: {fg_dim}; font-size: {caption}px; font-weight: bold; letter-spacing: 1px; }}
.field-hint {{ color: {fg_dim}; font-size: {caption}px; }}
.field-hint.bad {{ color: {red}; }}
.field-box {{ padding: {s3}px {s4}px; }}

.chip {{
  background-color: {normal_fill}; border: 1px solid {hairline}; border-radius: {small_r}px;
  padding: 0 {s6}px; font-size: {small}px; color: {fg};
}}
button.chip {{ padding: 0 {s6}px; }}
button.chip:hover {{ border-color: {hover_border_strong}; background-color: {hover_fill}; }}
.chip.done {{ color: {muted_text}; }}
.chip.done label {{ text-decoration-line: line-through; font-style: italic; }}
.chip button.flat, .chip > button:not(.x) {{ padding: 0; border: none; background: none; min-height: 0; }}
.chip > button:not(.x):hover {{ color: {accent}; }}
menubutton.btn > button {{ border: {nbw}px solid {normal_border}; background-color: {normal_fill}; padding: {s6}px {s10}px; }}
menubutton.btn > button:hover {{ border-color: {hover_border_strong}; background-color: {hover_fill}; }}
menubutton.btn > button:checked {{ background-color: {pressed_fill}; }}
button:disabled {{ opacity: 0.45; }}
.blocked button:disabled {{ opacity: 1; }}
.chip .x {{ color: {fg_dim}; padding: 0 0 0 {s4}px; border: none; }}
.chip .x:hover {{ color: {red}; background: none; }}
.chip.project {{ color: {fg_dim}; }}

popover > contents {{
  background-color: {popup_bg}; color: {fg};
  border: {pbw}px solid {popup_border}; border-radius: {r}px; box-shadow: 0 6px 24px {shadow}; padding: {s4}px;
}}
popover > arrow {{ background: none; border: none; }}
popover listview, popover list, popover listbox {{ background: none; }}
row.suggestion {{ padding: {s6}px {s10}px; border-radius: {r}px; }}
row.suggestion:hover {{ background-color: {hover_fill}; }}
row.suggestion:selected {{ background-color: {selected_menu}; color: {menu_selected}; }}
row.suggestion .detail {{ color: {fg_dim}; font-size: {small}px; }}
row.suggestion:selected .detail {{ color: {menu_selected}; }}
row.suggestion.create .glyph {{ color: {accent}; }}

calendar {{ background: none; border: none; color: {fg}; padding: {s4}px; }}
calendar > header {{ border: none; }}
calendar > header > button {{ color: {fg}; }}
calendar > grid > label.day-name {{ color: {fg_dim}; }}
calendar > grid > label.day-number {{ padding: {s4}px; border-radius: {r}px; }}
calendar > grid > label.day-number:hover {{ background-color: {hover_fill}; }}
calendar > grid > label.day-number.other-month {{ color: {placeholder}; }}
calendar > grid > label.today {{ box-shadow: inset 0 -2px {accent}; }}
calendar > grid > label.day-number:selected {{ background-color: {accent}; color: {primary_fg}; }}

/* ---------------------------------------------------------------- task lists */
.card {{ border: 1px solid {hairline}; border-radius: {r}px; background-color: {card}; }}
.card.faded {{ opacity: 0.75; }}
.col-header {{ background-color: {normal_fill}; border-bottom: 1px solid {hairline}; padding: {s6}px {s12}px; }}
.col-header label {{ color: {fg_dim}; font-size: {caption}px; font-weight: bold; letter-spacing: 1px; }}
.task-row {{ padding: {s8}px {s12}px; }}
.rows > .task-block {{ border-bottom: 1px solid {hairline}; }}
.rows > .task-block:last-child {{ border-bottom: none; }}
.task-row.subtask {{ border-top: 1px solid {hairline}; }}
.task-row:hover {{ background-color: {hover_fill}; }}
.task-row.completed {{ background-color: {normal_fill}; }}
.task-row.selected {{ background-color: {focus_fill}; box-shadow: inset 3px 0 {accent}; }}
.task-row.subtask {{ padding-left: {s40}px; background-color: {sub_bg}; }}
.task-row.subtask:hover {{ background-color: {hover_fill}; }}
.task-row.subtask.selected {{ background-color: {focus_fill}; }}
.task-row.drop-before {{ box-shadow: inset 0 2px {accent}; }}
.task-row.drop-after {{ box-shadow: inset 0 -2px {accent}; }}
.task-row.dragging {{ opacity: 0.5; }}
.task-row.leaving {{ opacity: 0; }}
.task-row .title {{ font-weight: 500; }}
.task-row.pending .title, .task-row.completed .title {{ text-decoration-line: line-through; font-style: italic; color: {muted_text}; }}
.task-row.pending .meta, .task-row.completed .meta {{ opacity: 0.6; }}
.linkline {{ font-size: {small}px; color: {fg_dim}; }}
.linkline link {{ color: {fg_dim}; text-decoration-line: none; }}
.linkline link:hover {{ color: {accent}; }}
.linkline.deps link {{ color: {fg}; text-decoration-line: none; }}
.linkline.deps link:hover {{ color: {accent}; }}
.task-row .notes {{ color: {fg_dim}; font-size: {small}px; }}
.task-row .glyph-dim {{ color: {fg_dim}; font-size: {small}px; }}
.chip.progress {{ color: {yellow}; border-color: {yellow_border}; }}
button.chip.linkchip {{ color: {accent}; }}
.task-row .reveal {{ opacity: 0; }}
.task-row:hover .reveal, .task-row.selected .reveal {{ opacity: 1; }}
.task-row .conflict, .task-row .blocked {{ color: {red}; font-size: {small}px; }}
.task-row .handle {{ color: {fg_dim}; opacity: 0; }}
.task-row:hover .handle, .task-row.selected .handle {{ opacity: 1; }}

button.check {{
  min-width: {check}px; min-height: {check}px; padding: 0;
  border: {check_bw}px solid {fg_dim}; border-radius: {small_r}px; background-color: transparent;
  color: transparent; font-size: {check_glyph}px;
}}
button.check:hover {{ border-color: {fg}; background-color: {hover_fill}; color: {fg_dim}; }}
button.check.done {{ background-color: {fg}; border-color: {fg}; color: {bg}; }}
button.check.done:hover {{ background-color: {fg_dim}; border-color: {fg_dim}; color: {bg}; }}
button.chevron {{ padding: 0; min-width: {check}px; color: {fg_dim}; border: none; }}
button.chevron:hover {{ color: {fg}; background: none; }}

.avatar {{
  min-width: {avatar}px; min-height: {avatar}px; border-radius: {small_r}px;
  background-color: {fg}; color: {bg}; font-weight: bold; font-size: {caption}px;
}}
.avatar.none, .task-row.completed .avatar {{ background-color: {hover_fill}; color: {fg_dim}; }}
.who-name {{ font-size: {small}px; }}
.who-none {{ color: {fg_dim}; font-size: {small}px; }}

.date {{ border: 1px solid {hairline}; border-radius: {small_r}px; padding: {s1}px {s6}px; font-size: {small}px; color: {fg_dim}; background-color: {normal_fill}; }}
.date.overdue {{ color: {red}; border-color: {red_border}; background-color: {red_soft}; }}
.date.today {{ color: {today_fg}; background-color: {today_bg}; border-color: {today_bg}; font-weight: bold; }}
.date.done {{ color: {placeholder}; background: none; }}
.date-none {{ color: {placeholder}; }}

.empty {{ padding: {s40}px {s20}px; }}
.empty .glyph {{ font-size: {display}px; color: {accent}; }}
.empty .title {{ font-size: {heading}px; font-weight: bold; }}
.empty .text {{ color: {fg_dim}; }}

button.disclosure {{ color: {fg_dim}; font-weight: bold; padding: {s6}px {s4}px; margin-top: {s18}px; }}
button.disclosure:hover {{ color: {fg}; background: none; border-color: transparent; }}
button.more {{ color: {fg_dim}; font-size: {small}px; padding: {s6}px {s12}px; border-radius: 0; border-width: 0; border-top: 1px solid {hairline}; }}
button.more:hover {{ color: {fg}; border-color: {hairline}; }}

/* ---------------------------------------------------------------- projects */
.project-row {{ padding: {s10}px {s12}px; border-bottom: 1px solid {hairline}; }}
.project-row:last-child {{ border-bottom: none; }}
.project-row:hover {{ background-color: {hover_fill}; }}
.project-row.selected {{ background-color: {focus_fill}; box-shadow: inset 3px 0 {accent}; }}
.project-row .bullet {{ min-width: {bullet}px; min-height: {bullet}px; background-color: {accent}; border-radius: {small_r}px; }}
.project-row.archived .bullet {{ background-color: {muted}; }}
.project-row.archived .name {{ color: {fg_dim}; }}
.project-row.pending .name, .project-row.pending .count {{ color: {muted_text}; }}
.project-row .name {{ font-weight: bold; }}
.project-row .count {{ color: {fg_dim}; font-size: {small}px; }}
.project-row .count.zero {{ color: {green}; }}
.project-row .link {{ color: {accent}; font-size: {small}px; }}
.project-row .actions {{ opacity: 0; }}
.project-row:hover .actions, .project-row.selected .actions, .project-row.pending .actions {{ opacity: 1; }}
.project-row .error, .person-row .error {{ color: {red}; font-size: {small}px; }}

/* ---------------------------------------------------------------- overlays */
.scrim {{ background-color: {scrim}; }}
.modal {{
  background-color: {popup_bg}; color: {fg};
  border: {pbw}px solid {popup_border}; border-radius: {r}px;
  box-shadow: 0 12px 48px {shadow};
  animation: modal-in 160ms cubic-bezier(0.2, 0.9, 0.3, 1);
}}
@keyframes modal-in {{ from {{ opacity: 0; transform: translateY(8px) scale(0.985); }} to {{ opacity: 1; transform: none; }} }}
.scrim {{ animation: fade-in 140ms ease-out; }}
@keyframes fade-in {{ from {{ opacity: 0; }} to {{ opacity: 1; }} }}
.modal-header {{ padding: {s14}px {s18}px; border-bottom: 1px solid {hairline}; }}
.modal-title {{ font-size: {heading}px; font-weight: bold; }}
.modal-title-glyph {{ color: {accent}; }}
.modal-body {{ padding: {s18}px; }}
.modal-footer {{ padding: {s12}px {s18}px; border-top: 1px solid {hairline}; background-color: {normal_fill}; }}
.form-errors {{ color: {red}; border: 1px solid {red_border}; background-color: {red_soft}; padding: {s8}px {s12}px; border-radius: {r}px; }}

.palette-entry {{ font-size: {heading}px; padding: {s10}px {s14}px; border: none; border-bottom: 1px solid {hairline}; border-radius: 0; background: none; }}
.palette-entry:focus-within {{ background: none; }}
.palette-row {{ padding: {s8}px {s14}px; }}
.palette-row.selected {{ background-color: {selected_menu}; }}
.palette-row.selected .title {{ color: {menu_selected}; }}
.palette-row .detail {{ color: {fg_dim}; font-size: {small}px; }}
.palette-row .glyph {{ color: {fg_dim}; }}
.palette-empty {{ color: {fg_dim}; padding: {s14}px; }}
.palette-footer {{ border-top: 1px solid {hairline}; padding: {s6}px {s14}px; color: {fg_dim}; font-size: {caption}px; }}

.kbd {{ border: 1px solid {normal_border}; border-bottom-width: 2px; border-radius: {small_r}px; padding: 0 {s6}px; font-size: {small}px; background-color: {normal_fill}; min-width: {kbd}px; }}
.shortcut-group {{ color: {accent}; font-weight: bold; font-size: {small}px; letter-spacing: 1px; margin-top: {s10}px; }}

.person-row {{ padding: {s6}px {s8}px; border-radius: {r}px; }}
.person-row:hover {{ background-color: {hover_fill}; }}
.person-row .count {{ color: {fg_dim}; font-size: {small}px; }}
.person-row .count.zero {{ color: {green}; }}
.person-row .actions {{ opacity: 0; }}
.person-row:hover .actions, .person-row:focus-within .actions {{ opacity: 1; }}
.person-row.me .name {{ color: {menu_selected}; font-weight: bold; }}

.toast {{
  background-color: {popup_bg}; color: {fg}; border: 1px solid {popup_border}; border-radius: {r}px;
  padding: {s8}px {s14}px; margin-bottom: {s18}px; box-shadow: 0 6px 24px {shadow};
  animation: modal-in 160ms ease-out;
}}
.toast.error {{ border-color: {red}; color: {red}; }}
"#,
            name = t.name,
            bg = c(t.bg),
            bg_dark = c(t.bg_dark),
            fg = c(t.fg),
            fg_dim = c(t.fg_dim),
            muted = c(t.muted),
            muted_text = c(t.fg.mix(t.bg, 0.5)),
            placeholder = c(t.fg.mix(t.bg, 0.6)),
            accent = c(t.accent),
            accent_hover = c(t.accent.mix(t.fg, 0.18)),
            accent_press = c(t.accent.mix(t.bg, 0.15)),
            primary_fg = c(primary_fg),
            red = c(t.red),
            red_soft = c(t.red.alpha(0.1)),
            red_border = c(t.red.alpha(0.5)),
            yellow = c(t.yellow),
            yellow_border = c(t.yellow.alpha(0.45)),
            green = c(t.green),
            hairline = c(t.hairline),
            card = c(if t.dark { t.bg.mix(t.fg, 0.015) } else { t.bg }),
            sub_bg = c(t.fg.alpha(0.02)),
            normal_fill = c(t.normal_fill),
            hover_fill = c(t.hover_fill),
            focus_fill = c(t.focus_fill),
            selected_fill = c(t.selected_fill),
            selected_menu = c(t.fg.alpha(t.selected_fill.a.min(0.12))),
            pressed_fill = c(t.pressed_fill),
            selection_fill = c(t.selection_fill),
            normal_border = c(t.normal_border),
            hover_border = c(t.hover_border),
            hover_border_strong = c(t.hover_border.alpha((t.hover_border.a * 2.0).min(0.7))),
            focus_border = c(t.focus_border),
            focus_border_strong = c(t.accent.alpha(0.8)),
            nbw = t.normal_border_width.max(1),
            pbw = t.sp(2.0).clamp(1, 2),
            popup_bg = c(t.popup_bg.alpha(1.0)),
            popup_border = c(t.popup_border),
            tooltip_bg = c(t.tooltip_bg),
            menu_selected = c(t.menu_selected_text),
            scrim = c(t.scrim),
            shadow = c(BLACK.alpha(if t.dark { 0.45 } else { 0.18 })),
            scroll = c(t.fg.alpha(0.22)),
            scroll_hover = c(t.fg.alpha(0.4)),
            today_bg = c(today_bg),
            today_fg = c(today_fg),
            body = body,
            small = small,
            caption = caption,
            heading = heading,
            display = display,
            r = r,
            small_r = small_r,
            check = sp(16.0),
            check_bw = if t.font_base >= 14.0 { 2 } else { 1 },
            check_glyph = caption,
            avatar = sp(20.0),
            bullet = sp(8.0),
            glyph_w = sp(18.0),
            kbd = sp(14.0),
            s1 = sp(1.0),
            s2 = sp(2.0),
            s3 = sp(3.0),
            s4 = sp(4.0),
            s6 = sp(6.0),
            s8 = sp(8.0),
            s10 = sp(10.0),
            s12 = sp(12.0),
            s14 = sp(14.0),
            s18 = sp(18.0),
            s20 = sp(20.0),
            s22 = sp(22.0),
            s30 = sp(30.0),
            s40 = sp(40.0),
        )
    }
}

fn hyprland_rounding() -> Option<i64> {
    let out = std::process::Command::new("hyprctl").args(["-j", "getoption", "decoration:rounding"]).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let after = text.split("\"int\"").nth(1)?;
    let digits: String = after.chars().skip_while(|c| !c.is_ascii_digit()).take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// Owns the CSS provider and the file monitors; calls `on_change` after
/// every re-theme so custom-drawn widgets can repaint.
pub struct Theme {
    provider: gtk::CssProvider,
    pub tokens: RefCell<Tokens>,
    monitors: RefCell<Vec<gio::FileMonitor>>,
    pending: RefCell<Option<glib::SourceId>>,
    listeners: RefCell<Vec<Box<dyn Fn()>>>,
}

impl Theme {
    pub fn install(display: &gdk::Display) -> Rc<Theme> {
        let provider = gtk::CssProvider::new();
        gtk::style_context_add_provider_for_display(display, &provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
        let theme = Rc::new(Theme {
            provider,
            tokens: RefCell::new(load(0)),
            monitors: RefCell::new(vec![]),
            pending: RefCell::new(None),
            listeners: RefCell::new(vec![]),
        });
        theme.apply();
        theme.watch();
        theme.refresh_rounding();
        theme
    }

    pub fn on_change(&self, f: impl Fn() + 'static) {
        self.listeners.borrow_mut().push(Box::new(f));
    }

    fn apply(&self) {
        let tokens = self.tokens.borrow();
        if let Some(settings) = gtk::Settings::default() {
            // Lets GTK's own bits (text handles, IM popups) pick the right variant.
            settings.set_gtk_application_prefer_dark_theme(tokens.dark);
        }
        self.provider.load_from_string(&tokens.css());
    }

    fn reload(self: &Rc<Self>) {
        let radius = self.tokens.borrow().radius;
        *self.tokens.borrow_mut() = load(radius);
        self.apply();
        for f in self.listeners.borrow().iter() {
            f();
        }
        // Hyprland re-reads its config asynchronously after a theme switch.
        let this = self.clone();
        glib::timeout_add_local_once(Duration::from_millis(250), move || this.refresh_rounding());
    }

    fn refresh_rounding(self: &Rc<Self>) {
        let this = self.clone();
        glib::spawn_future_local(async move {
            let Ok(Some(radius)) = gio::spawn_blocking(hyprland_rounding).await else { return };
            if radius != this.tokens.borrow().radius {
                this.tokens.borrow_mut().radius = radius;
                this.apply();
            }
        });
    }

    fn schedule(self: &Rc<Self>) {
        if let Some(id) = self.pending.borrow_mut().take() {
            id.remove();
        }
        let this = self.clone();
        let id = glib::timeout_add_local_once(Duration::from_millis(120), move || {
            this.pending.borrow_mut().take();
            this.reload();
        });
        *self.pending.borrow_mut() = Some(id);
    }

    fn watch(self: &Rc<Self>) {
        let dirs = [
            state_dir(),
            state_dir().join("theme"),
            home().join(".config/omarchy"),
            home().join(".config/fontconfig"),
        ];
        let mut monitors = self.monitors.borrow_mut();
        for dir in dirs {
            let file = gio::File::for_path(&dir);
            let Ok(monitor) = file.monitor_directory(gio::FileMonitorFlags::WATCH_MOVES, gio::Cancellable::NONE) else {
                continue;
            };
            let weak = Rc::downgrade(self);
            let watched = dir.clone();
            monitor.connect_changed(move |m, file, other, _event| {
                let Some(this) = weak.upgrade() else { return };
                let relevant = |f: &gio::File| {
                    let name = f.basename().map(|b| b.to_string_lossy().into_owned()).unwrap_or_default();
                    name == "theme" || name == "theme.name" || name.ends_with(".toml") || name == "fonts.conf"
                };
                if relevant(file) || other.is_some_and(relevant) {
                    this.schedule();
                    // The theme dir itself was replaced: re-arm its monitor.
                    if watched.ends_with("theme") {
                        m.cancel();
                        let this = this.clone();
                        glib::timeout_add_local_once(Duration::from_millis(300), move || this.rewatch_theme_dir());
                    }
                }
            });
            monitors.push(monitor);
        }
    }

    fn rewatch_theme_dir(self: &Rc<Self>) {
        let dir = state_dir().join("theme");
        let mut monitors = self.monitors.borrow_mut();
        monitors.retain(|m| !m.is_cancelled());
        if let Ok(monitor) = gio::File::for_path(&dir).monitor_directory(gio::FileMonitorFlags::WATCH_MOVES, gio::Cancellable::NONE) {
            let weak = Rc::downgrade(self);
            monitor.connect_changed(move |_, file, _, _| {
                let Some(this) = weak.upgrade() else { return };
                if file.basename().is_some_and(|b| b.to_string_lossy().ends_with(".toml")) {
                    this.schedule();
                }
            });
            monitors.push(monitor);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_tokens() {
        assert_eq!(Rgba::hex("#ff0000").unwrap().css(), "rgba(255,0,0,1.000)");
        assert_eq!(Rgba::token("rgba(33ccffee) rgba(00ff99ee) 45deg").unwrap().css(), "rgba(51,204,255,0.933)");
        assert!(Rgba::token("hyprland.active-border").is_none());
    }

    #[test]
    fn shell_toml_parsing() {
        let mut m = HashMap::new();
        parse_shell("[controls]\nnormal-color = \"#d4be98\" # c\nnormal-border-width = 1\n[popups]\nborder = hyprland.active-border\n", &mut m);
        assert_eq!(m["controls.normal-color"], "#d4be98");
        assert_eq!(m["controls.normal-border-width"], "1");
        assert_eq!(m["popups.border"], "hyprland.active-border");
    }

    #[test]
    fn colors_toml_parsing() {
        let (p, dark) = parse_colors("mode = \"light\"\naccent = \"#205EA6\"\n");
        assert_eq!(dark, Some(false));
        assert_eq!(p["accent"].css(), "rgba(32,94,166,1.000)");
    }
}
