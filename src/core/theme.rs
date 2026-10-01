//! Omarchy theme → design tokens.
//!
//! Mirrors the Omarchy shell's `Commons/Color.qml` + `Commons/Style.qml` so
//! the app paints with the same tokens as the bar, menus and panels:
//!
//! - `~/.local/state/omarchy/current/theme/colors.toml`: the palette,
//!   resolved with `omarchy-theme-color`'s fallback rules
//! - `~/.local/state/omarchy/current/theme/shell.toml`: control states, type scale, spacing
//! - `~/.config/omarchy/shell.toml`: machine-level overrides (`omarchy display text size`)
//! - the fontconfig `monospace` alias: the Omarchy font
//! - `hyprctl getoption decoration:rounding`: corner radius
//!
//! `omarchy-theme-set` moves a freshly built theme directory over
//! `current/theme` and rewrites `current/theme.name` last, so watching the
//! `current` directory (never the theme dir itself) catches every switch.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}

pub const BLACK: Rgba = Rgba { r: 0.0, g: 0.0, b: 0.0, a: 1.0 };
pub const WHITE: Rgba = Rgba { r: 1.0, g: 1.0, b: 1.0, a: 1.0 };

impl Rgba {
    pub fn hex(s: &str) -> Option<Rgba> {
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
    pub fn token(s: &str) -> Option<Rgba> {
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

    /// Perceived brightness, 0–1 (cheap; used for picking contrasting ink).
    pub fn luminance(self) -> f64 {
        0.2126 * self.r + 0.7152 * self.g + 0.0722 * self.b
    }

    /// WCAG relative luminance.
    pub fn relative_luminance(self) -> f64 {
        let lin = |c: f64| if c <= 0.03928 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) };
        0.2126 * lin(self.r) + 0.7152 * lin(self.g) + 0.0722 * lin(self.b)
    }

    /// WCAG contrast ratio of `self` drawn over the opaque `bg`.
    pub fn contrast(self, bg: Rgba) -> f64 {
        let fg = bg.mix(Rgba { a: 1.0, ..self }, self.a);
        let (a, b) = (fg.relative_luminance(), bg.relative_luminance());
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    fn byte(c: f64) -> u8 {
        (c.clamp(0.0, 1.0) * 255.0).round() as u8
    }

    pub fn to_hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", Self::byte(self.r), Self::byte(self.g), Self::byte(self.b))
    }

    /// QML colour string, `#aarrggbb`.
    pub fn qml(self) -> String {
        format!("#{:02x}{:02x}{:02x}{:02x}", Self::byte(self.a), Self::byte(self.r), Self::byte(self.g), Self::byte(self.b))
    }
}

// ------------------------------------------------------------------ paths

fn home() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
}

/// `~/.local/state/omarchy/current`; `OMNISCIENT_OMARCHY_STATE` points at a
/// copy (dev aid for testing theme switches without touching the real one).
pub fn state_dir() -> PathBuf {
    std::env::var_os("OMNISCIENT_OMARCHY_STATE")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".local/state/omarchy/current"))
}

/// The active theme; `OMNISCIENT_THEME_DIR` previews another one (dev aid).
pub fn theme_dir() -> PathBuf {
    std::env::var_os("OMNISCIENT_THEME_DIR").map(PathBuf::from).unwrap_or_else(|| state_dir().join("theme"))
}

pub fn user_shell() -> PathBuf {
    home().join(".config/omarchy/shell.toml")
}

pub fn fontconfig_dir() -> PathBuf {
    home().join(".config/fontconfig")
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

// ------------------------------------------------------------------ parsing

/// `key = value` pairs of colors.toml, raw strings (quotes stripped).
pub fn parse_colors(raw: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for line in raw.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.starts_with('[') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else { continue };
        let k = k.trim().trim_matches(['"', '\'']);
        let v = v.trim();
        // Quoted values may contain '#'; bare ones end at whitespace.
        let v = match v.strip_prefix('"').or_else(|| v.strip_prefix('\'')) {
            Some(q) => q.split(['"', '\'']).next().unwrap_or(""),
            None => v.split_whitespace().next().unwrap_or(""),
        };
        if !k.is_empty() {
            out.insert(k.to_string(), v.to_string());
        }
    }
    out
}

/// The shell's TOML walker: `section.key → raw string`.
pub fn parse_shell(raw: &str, into: &mut HashMap<String, String>) {
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

/// The resolved palette, following `omarchy-theme-color`: legacy short and
/// ANSI names, derived shades for keys a theme leaves out, and the mode.
pub struct Palette {
    colors: HashMap<String, Rgba>,
    pub dark: bool,
}

impl Palette {
    pub fn get(&self, key: &str) -> Rgba {
        self.colors.get(key).copied().unwrap_or(BLACK)
    }

    pub fn resolve(raw: &HashMap<String, String>, light_mode_file: bool) -> Palette {
        let mut c: HashMap<String, Rgba> = raw.iter().filter_map(|(k, v)| Some((k.clone(), Rgba::token(v).or_else(|| Rgba::hex(v))?))).collect();
        fn alias(c: &mut HashMap<String, Rgba>, key: &str, from: &str) {
            if !c.contains_key(key)
                && let Some(v) = c.get(from).copied()
            {
                c.insert(key.into(), v);
            }
        }
        for (key, short) in [
            ("background", "bg"),
            ("dark_background", "dark_bg"),
            ("darker_background", "darker_bg"),
            ("lighter_background", "lighter_bg"),
            ("foreground", "fg"),
            ("dark_foreground", "dark_fg"),
            ("light_foreground", "light_fg"),
            ("bright_foreground", "bright_fg"),
        ] {
            alias(&mut c, key, short);
        }
        alias(&mut c, "background", "color0");
        alias(&mut c, "foreground", "color7");
        c.entry("background".into()).or_insert(Rgba::hex("#101315").unwrap());
        c.entry("foreground".into()).or_insert(Rgba::hex("#cacccc").unwrap());
        for (key, ansi) in [
            ("red", "color1"),
            ("green", "color2"),
            ("yellow", "color3"),
            ("blue", "color4"),
            ("magenta", "color5"),
            ("cyan", "color6"),
            ("bright_red", "color9"),
            ("bright_green", "color10"),
            ("bright_yellow", "color11"),
            ("bright_blue", "color12"),
            ("bright_magenta", "color13"),
            ("bright_cyan", "color14"),
        ] {
            alias(&mut c, key, ansi);
        }
        alias(&mut c, "magenta", "purple");
        alias(&mut c, "bright_magenta", "bright_purple");
        let bg = c["background"];
        let fg = c["foreground"];
        // Hues a theme might lack entirely: something legible beats black.
        for (key, hex) in [
            ("red", "#e06c75"),
            ("green", "#98c379"),
            ("yellow", "#e5c07b"),
            ("blue", "#61afef"),
            ("magenta", "#c678dd"),
            ("cyan", "#56b6c2"),
        ] {
            c.entry(key.into()).or_insert(Rgba::hex(hex).unwrap());
        }
        alias(&mut c, "light_foreground", "color7");
        alias(&mut c, "bright_foreground", "color15");
        alias(&mut c, "lighter_background", "color0");
        alias(&mut c, "dark_foreground", "color8");
        c.entry("light_foreground".into()).or_insert(fg);
        c.entry("bright_foreground".into()).or_insert(fg);
        c.entry("lighter_background".into()).or_insert(bg);
        c.entry("dark_foreground".into()).or_insert(fg);
        alias(&mut c, "muted", "color8");
        let dark_fg = c["dark_foreground"];
        c.entry("muted".into()).or_insert(dark_fg);
        alias(&mut c, "selection", "selection_background");
        alias(&mut c, "selection", "color8");
        c.entry("selection".into()).or_insert(bg);
        alias(&mut c, "orange", "yellow");
        let orange = c["orange"];
        c.entry("brown".into()).or_insert(orange.mix(BLACK, 0.5));
        c.entry("dark_background".into()).or_insert(bg.mix(BLACK, 0.25));
        c.entry("darker_background".into()).or_insert(bg.mix(BLACK, 0.5));
        for hue in ["red", "yellow", "green", "cyan", "blue", "magenta"] {
            let base = c[hue];
            c.entry(format!("bright_{hue}")).or_insert(base.mix(WHITE, 0.2));
        }
        alias(&mut c, "accent", "blue");

        let mode = raw.get("mode").or_else(|| raw.get("theme_type")).map(|m| m.trim().to_lowercase());
        let dark = match mode.as_deref() {
            Some("light") => false,
            Some("dark") => true,
            _ if light_mode_file => false,
            // The script's rule: R+G+B (0–255 each) above 382 is light.
            _ => (bg.r + bg.g + bg.b) * 255.0 <= 382.0,
        };
        Palette { colors: c, dark }
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
            && let Some(v) = self.shell.get(t)
        {
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

/// Every value the QML needs, resolved and ready to paint.
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
    pub on_accent: Rgba,
    pub red: Rgba,
    pub orange: Rgba,
    pub yellow: Rgba,
    pub green: Rgba,
    pub hairline: Rgba,
    pub card: Rgba,
    pub normal_fill: Rgba,
    pub hover_fill: Rgba,
    pub focus_fill: Rgba,
    pub selected_fill: Rgba,
    pub pressed_fill: Rgba,
    pub selection: Rgba,
    pub normal_border: Rgba,
    pub hover_border: Rgba,
    pub focus_border: Rgba,
    pub normal_border_width: i64,
    pub popup_bg: Rgba,
    pub popup_border: Rgba,
    pub tooltip_bg: Rgba,
    pub menu_selected_text: Rgba,
    pub scrim: Rgba,
    pub font_family: String,
    pub font_base: f64,
    pub spacing_scale: f64,
    pub radius: i64,
}

/// Reads the live theme. `radius` and `font_family` come from processes
/// (`hyprctl`, `fc-match`) the caller runs off the main thread.
pub fn load(radius: i64, font_family: &str) -> Tokens {
    let dir = theme_dir();
    let raw = parse_colors(&read(&dir.join("colors.toml")));
    let palette = Palette::resolve(&raw, dir.join("light.mode").exists());
    let mut shell = HashMap::new();
    parse_shell(&read(&dir.join("shell.toml")), &mut shell);
    parse_shell(&read(&user_shell()), &mut shell);
    let name = read(&state_dir().join("theme.name")).trim().to_string();
    tokens(name, &palette, &shell, radius, font_family)
}

pub fn tokens(name: String, p: &Palette, shell: &HashMap<String, String>, radius: i64, font_family: &str) -> Tokens {
    let dark = p.dark;
    let bg = p.get("background");
    let fg = p.get("foreground");
    let accent = p.get("accent");
    let red = p.get("red");
    let muted = p.get("muted");
    let r = Resolver { shell, fg, bg, accent, red, muted };
    let hover_fill_alpha = r.num("controls.hover-cursor-fill-alpha", 0.08);
    let hover_border_alpha = r.num("controls.hover-cursor-border-alpha", 0.25);
    let normal_width = r.num("controls.normal-border-width", 1.0).round().max(0.0) as i64;
    let scale_with_font = !matches!(r.get("spacing.scale-with-font"), Some("false" | "0" | "no" | "off"));
    let font_base = r.num("font.base-size", 12.0).max(1.0);

    // Secondary text: the theme's own dim ink when it is legible, else a mix.
    let fg_dim = {
        let theirs = p.get("dark_foreground");
        if theirs != fg && theirs.contrast(bg) >= 4.5 { theirs } else { legible(fg.mix(bg, 0.4), fg, bg) }
    };
    // Ink on accent-filled buttons: the theme's own bg or fg when legible.
    let on_accent = [bg, fg]
        .into_iter()
        .find(|c| c.contrast(accent) >= 4.5)
        .unwrap_or(if BLACK.contrast(accent) > WHITE.contrast(accent) { BLACK } else { WHITE });
    let selection = match p.get("selection") {
        s if s == bg => accent.alpha(0.35),
        s => s,
    };

    Tokens {
        name,
        dark,
        bg,
        bg_dark: if dark { p.get("dark_background") } else { bg.mix(BLACK, 0.035) },
        fg,
        fg_dim,
        muted,
        accent,
        on_accent,
        red: legible(red, fg, bg),
        orange: legible(p.get("orange"), fg, bg),
        yellow: legible(p.get("yellow"), fg, bg),
        green: legible(p.get("green"), fg, bg),
        hairline: bg.mix(fg, if dark { 0.12 } else { 0.14 }),
        card: if dark { bg.mix(fg, 0.015) } else { bg },
        normal_fill: r.state("normal", "fill", 0.04),
        hover_fill: r.state("hover-cursor", "fill", 0.08),
        focus_fill: r.state("focus", "fill", hover_fill_alpha),
        selected_fill: r.state("selected", "fill", 0.18),
        pressed_fill: r.state("pressed", "fill", 0.22),
        selection,
        normal_border: r.state("normal", "border", 0.4),
        hover_border: r.state("hover-cursor", "border", 0.25),
        focus_border: r.state("focus", "border", hover_border_alpha),
        normal_border_width: normal_width,
        popup_bg: r.surface("popups", "background", bg, 1.0).alpha(1.0),
        popup_border: r.surface("popups", "border", accent, 1.0),
        tooltip_bg: r.surface("tooltip", "background", bg, 0.97),
        menu_selected_text: r.get("menu.selected-text").map(|t| r.role(t, accent, 0)).unwrap_or(accent),
        scrim: r.surface("menu", "scrim", bg, 0.5),
        font_family: if font_family.is_empty() { "monospace".into() } else { font_family.into() },
        font_base,
        spacing_scale: r.num("spacing.scale", 1.0).max(0.0) * if scale_with_font { (font_base / 12.0).max(1.0 / 12.0) } else { 1.0 },
        radius,
    }
}

/// Nudges `c` towards `fg` until it reads at 4.5:1 on `bg` (status colours
/// in pale light themes otherwise wash out).
fn legible(c: Rgba, fg: Rgba, bg: Rgba) -> Rgba {
    let mut out = c;
    let mut t = 0.0;
    while out.contrast(bg) < 4.5 && t < 1.0 {
        t += 0.1;
        out = c.mix(fg, t);
    }
    out
}

impl Tokens {
    /// JSON for the QML `Theme` singleton. Colours are `#aarrggbb`.
    pub fn json(&self) -> String {
        let mut o = serde_json::Map::new();
        let mut col = |k: &str, c: Rgba| {
            o.insert(k.into(), c.qml().into());
        };
        col("bg", self.bg);
        col("bgDark", self.bg_dark);
        col("fg", self.fg);
        col("fgDim", self.fg_dim);
        col("muted", self.muted);
        col("accent", self.accent);
        col("onAccent", self.on_accent);
        col("red", self.red);
        col("orange", self.orange);
        col("yellow", self.yellow);
        col("green", self.green);
        col("hairline", self.hairline);
        col("card", self.card);
        col("normalFill", self.normal_fill);
        col("hoverFill", self.hover_fill);
        col("focusFill", self.focus_fill);
        col("selectedFill", self.selected_fill);
        col("pressedFill", self.pressed_fill);
        col("selection", self.selection);
        col("normalBorder", self.normal_border);
        col("hoverBorder", self.hover_border);
        col("hoverBorderStrong", self.hover_border.alpha((self.hover_border.a * 2.0).min(0.7)));
        col("focusBorder", self.focus_border);
        col("focusBorderStrong", self.accent.alpha(0.8));
        col("popupBg", self.popup_bg);
        col("popupBorder", self.popup_border);
        col("tooltipBg", self.tooltip_bg);
        col("menuSelectedText", self.menu_selected_text);
        col("menuSelectedFill", self.fg.alpha(self.selected_fill.a.min(0.12)));
        col("scrim", self.scrim);
        col("shadow", BLACK.alpha(if self.dark { 0.45 } else { 0.18 }));
        col("placeholder", self.fg.mix(self.bg, 0.55));
        col("doneText", self.fg.mix(self.bg, 0.45));
        col("redSoft", self.red.alpha(0.1));
        col("redBorder", self.red.alpha(0.5));
        col("yellowBorder", self.yellow.alpha(0.45));
        col("accentHover", self.accent.mix(self.fg, 0.18));
        col("accentPressed", self.accent.mix(self.bg, 0.15));
        col("subtaskFill", self.fg.alpha(0.02));
        col("scrollbar", self.fg.alpha(0.22));
        col("scrollbarHover", self.fg.alpha(0.4));
        o.insert("name".into(), self.name.clone().into());
        o.insert("dark".into(), self.dark.into());
        o.insert("fontFamily".into(), self.font_family.clone().into());
        o.insert("fontBase".into(), self.font_base.into());
        o.insert("spacingScale".into(), self.spacing_scale.into());
        o.insert("radius".into(), self.radius.into());
        o.insert("borderWidth".into(), self.normal_border_width.max(1).into());
        serde_json::Value::Object(o).to_string()
    }
}

/// `hyprctl -j getoption decoration:rounding` → radius. Blocking; call off
/// the main thread.
pub fn hyprland_rounding() -> Option<i64> {
    let out = std::process::Command::new("hyprctl").args(["-j", "getoption", "decoration:rounding"]).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let after = text.split("\"int\"").nth(1)?;
    let digits: String = after.chars().skip_while(|c| !c.is_ascii_digit()).take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// The family fontconfig resolves `monospace` to (what `omarchy-font-set`
/// changes). Blocking; call off the main thread.
pub fn monospace_family() -> Option<String> {
    let out = std::process::Command::new("fc-match").args(["-f", "%{family[0]}", "monospace"]).output().ok()?;
    let family = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!family.is_empty()).then_some(family)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_tokens() {
        assert_eq!(Rgba::hex("#ff0000").unwrap().qml(), "#ffff0000");
        assert_eq!(Rgba::token("rgba(33ccffee) rgba(00ff99ee) 45deg").unwrap().qml(), "#ee33ccff");
        assert!(Rgba::token("hyprland.active-border").is_none());
        assert!((WHITE.contrast(BLACK) - 21.0).abs() < 0.01);
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
    fn palette_fallbacks_follow_omarchy_theme_color() {
        let raw = parse_colors("bg = \"#202020\"\nfg = '#e0e0e0'\ncolor1 = \"#cc0000\"\ncolor3 = \"#ccaa00\"\n");
        let p = Palette::resolve(&raw, false);
        assert!(p.dark, "dark background means dark mode");
        assert_eq!(p.get("background").to_hex(), "#202020");
        assert_eq!(p.get("red").to_hex(), "#cc0000");
        assert_eq!(p.get("orange").to_hex(), "#ccaa00", "orange falls back to yellow");
        assert_eq!(p.get("brown").to_hex(), "#665500", "brown is orange 50% to black");
        assert_eq!(p.get("dark_background").to_hex(), "#181818");
        assert_eq!(p.get("bright_red").to_hex(), "#d63333");

        let light = Palette::resolve(&parse_colors("mode = \"light\"\nbackground = \"#000000\""), false);
        assert!(!light.dark, "explicit mode wins");
        let by_file = Palette::resolve(&parse_colors("background = \"#000000\""), true);
        assert!(!by_file.dark, "light.mode file");
        let bright = Palette::resolve(&parse_colors("background = \"#f0f0f0\""), false);
        assert!(!bright.dark, "bright background means light");
    }

    /// Every installed Omarchy theme must resolve to legible tokens.
    #[test]
    fn every_installed_theme_is_legible() {
        let Ok(dirs) = std::fs::read_dir("/usr/share/omarchy/themes") else { return };
        let mut checked = 0;
        for dir in dirs.flatten() {
            let path = dir.path();
            let raw = parse_colors(&read(&path.join("colors.toml")));
            if raw.is_empty() {
                continue;
            }
            let p = Palette::resolve(&raw, path.join("light.mode").exists());
            let mut shell = HashMap::new();
            parse_shell(include_str!("../../tests/shell_defaults.toml"), &mut shell);
            let t = tokens(path.display().to_string(), &p, &shell, 0, "");
            let name = path.file_name().unwrap().to_string_lossy();
            for (what, ink, paper) in [
                ("fg on bg", t.fg, t.bg),
                ("fg on card", t.fg, t.card),
                ("dim on bg", t.fg_dim, t.bg),
                ("on-accent", t.on_accent, t.accent),
                ("red on bg", t.red, t.bg),
                ("green on bg", t.green, t.bg),
                ("orange on bg", t.orange, t.bg),
                ("today badge", t.bg, t.fg),
            ] {
                let ratio = ink.contrast(paper);
                assert!(ratio >= 4.5, "{name}: {what} contrast {ratio:.2} < 4.5 ({} on {})", ink.to_hex(), paper.to_hex());
            }
            assert!(t.json().contains("\"bg\""));
            checked += 1;
        }
        assert!(checked > 0);
    }
}
