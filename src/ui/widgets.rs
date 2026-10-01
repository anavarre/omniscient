//! Small widget helpers. Icons are Nerd Font glyphs from the Omarchy font,
//! the same way the Omarchy bar draws its icons, so they follow the theme's
//! colors and the user's font choice.

use std::rc::Rc;
use std::time::Instant;

use gtk::{glib, pango, prelude::*};

use crate::theme::Theme;

pub mod glyph {
    pub const CHECK: &str = "\u{f00c}";
    pub const PLUS: &str = "\u{f067}";
    pub const CALENDAR: &str = "\u{f073}";
    pub const LINK: &str = "\u{f0c1}";
    pub const UNLOCK: &str = "\u{f09c}";
    pub const SEARCH: &str = "\u{f002}";
    pub const COG: &str = "\u{f013}";
    pub const USER: &str = "\u{f007}";
    pub const FOLDER: &str = "\u{f07b}";
    pub const NOTE: &str = "\u{f0f6}";
    pub const CHEVRON_RIGHT: &str = "\u{f054}";
    pub const CHEVRON_DOWN: &str = "\u{f078}";
    pub const TRASH: &str = "\u{f1f8}";
    pub const EDIT: &str = "\u{f040}";
    pub const ARCHIVE: &str = "\u{f187}";
    pub const CLOSE: &str = "\u{f00d}";
    pub const HOURGLASS: &str = "\u{f252}";
    pub const LIST: &str = "\u{f03a}";
    pub const EXTERNAL: &str = "\u{f08e}";
    pub const KEYBOARD: &str = "\u{f11c}";
    pub const TASKS: &str = "\u{f0ae}";
    pub const UNDO: &str = "\u{f0e2}";
    pub const PLAY: &str = "\u{f04b}";
    pub const STOP: &str = "\u{f04d}";
    pub const STAR: &str = "\u{f005}";
    pub const DRAG: &str = "⠿";
    pub const BACK: &str = "\u{f060}";
}

pub fn label(text: &str, classes: &[&str]) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    l.set_xalign(0.0);
    for c in classes {
        l.add_css_class(c);
    }
    l
}

pub fn wrapping(text: &str, classes: &[&str]) -> gtk::Label {
    let l = label(text, classes);
    l.set_wrap(true);
    l.set_wrap_mode(pango::WrapMode::WordChar);
    l
}

pub fn ellipsized(text: &str, classes: &[&str]) -> gtk::Label {
    let l = label(text, classes);
    l.set_ellipsize(pango::EllipsizeMode::End);
    l.set_single_line_mode(true);
    l
}

pub fn glyph(g: &str, classes: &[&str]) -> gtk::Label {
    let l = gtk::Label::new(Some(g));
    l.add_css_class("glyph");
    for c in classes {
        l.add_css_class(c);
    }
    l
}

pub fn hbox(spacing: i32) -> gtk::Box {
    gtk::Box::new(gtk::Orientation::Horizontal, spacing)
}

pub fn vbox(spacing: i32) -> gtk::Box {
    gtk::Box::new(gtk::Orientation::Vertical, spacing)
}

pub fn spacer() -> gtk::Box {
    let b = hbox(0);
    b.set_hexpand(true);
    b
}

/// A button with an optional leading glyph, a label and a trailing key hint.
pub fn button(g: Option<&str>, text: &str, key: Option<&str>, classes: &[&str]) -> gtk::Button {
    let content = hbox(6);
    if let Some(g) = g {
        content.append(&glyph(g, &[]));
    }
    if !text.is_empty() {
        // Ellipsizing keeps the minimum width small so narrow tiles never clip.
        let l = gtk::Label::new(Some(text));
        l.set_ellipsize(pango::EllipsizeMode::End);
        l.set_xalign(0.0);
        content.append(&l);
    }
    if let Some(k) = key {
        content.append(&label(k, &["key"]));
    }
    let b = gtk::Button::new();
    b.set_child(Some(&content));
    for c in classes {
        b.add_css_class(c);
    }
    b
}

pub fn icon_button(g: &str, tooltip: &str, classes: &[&str]) -> gtk::Button {
    let b = gtk::Button::with_label(g);
    b.add_css_class("icon");
    for c in classes {
        b.add_css_class(c);
    }
    if !tooltip.is_empty() {
        b.set_tooltip_text(Some(tooltip));
    }
    b.set_focusable(false);
    b
}

pub fn clear(container: &gtk::Box) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
}

pub fn initial(name: &str) -> String {
    name.chars().find(|c| c.is_alphanumeric()).map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "?".into())
}

pub fn avatar(name: &str, none: bool) -> gtk::Label {
    let a = gtk::Label::new(Some(&if none { glyph::USER.to_string() } else { initial(name) }));
    a.add_css_class("avatar");
    if none {
        a.add_css_class("none");
    }
    a.set_valign(gtk::Align::Center);
    a
}

/// A circular countdown that drains from full to empty over `total` seconds.
pub fn ring(theme: &Rc<Theme>, started: Instant, total: f64, size: i32) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::new();
    area.set_content_width(size);
    area.set_content_height(size);
    area.set_valign(gtk::Align::Center);
    let theme = theme.clone();
    area.set_draw_func(move |_, cr, w, h| {
        let t = theme.tokens.borrow();
        let remaining = (1.0 - started.elapsed().as_secs_f64() / total).clamp(0.0, 1.0);
        let (cx, cy) = (w as f64 / 2.0, h as f64 / 2.0);
        let lw = (w as f64 / 7.0).max(1.5);
        let r = cx.min(cy) - lw / 2.0;
        let set = |c: gtk::gdk::RGBA| cr.set_source_rgba(c.red() as f64, c.green() as f64, c.blue() as f64, c.alpha() as f64);
        cr.set_line_width(lw);
        set(t.hairline.gdk());
        cr.arc(cx, cy, r, 0.0, std::f64::consts::TAU);
        let _ = cr.stroke();
        set(t.accent.gdk());
        let start = -std::f64::consts::FRAC_PI_2;
        cr.arc(cx, cy, r, start, start + std::f64::consts::TAU * remaining);
        let _ = cr.stroke();
    });
    area
}

pub fn open_uri(window: &impl IsA<gtk::Window>, uri: &str) {
    if uri.is_empty() {
        return;
    }
    gtk::UriLauncher::new(uri).launch(Some(window), gtk::gio::Cancellable::NONE, |_| {});
}

/// Runs `f` once the widget tree has been laid out (next frame).
pub fn after_layout(widget: &impl IsA<gtk::Widget>, f: impl FnOnce() + 'static) {
    let f = std::cell::Cell::new(Some(f));
    widget.add_tick_callback(move |_, _| {
        if let Some(f) = f.take() {
            f();
        }
        glib::ControlFlow::Break
    });
}
