//! A keyboard-first multi/single select: chips + a search field + a
//! suggestion popover. ↑/↓ move, Enter picks, Backspace on an empty field
//! removes the last chip, Esc closes the suggestions. An optional "Add …"
//! suggestion creates the record on the fly (people, projects).

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gtk::{gdk, glib, prelude::*};

use super::widgets::{self, ellipsized, glyph, hbox, vbox};

#[derive(Clone)]
pub struct Suggestion {
    pub id: i64,
    pub label: String,
    pub detail: String,
}

#[derive(Clone)]
pub struct Item {
    pub id: i64,
    pub label: String,
    pub done: bool,
}

type Source = Box<dyn Fn(&str, &[i64]) -> Vec<Suggestion>>;
type Create = Box<dyn Fn(&str) -> Option<Item>>;

pub struct Picker {
    pub root: gtk::Box,
    pub entry: gtk::Entry,
    chips: gtk::FlowBox,
    popover: gtk::Popover,
    list: gtk::ListBox,
    items: RefCell<Vec<Item>>,
    shown: RefCell<Vec<Suggestion>>,
    /// Whether the last row in `shown` is the "Add …" row.
    has_create: Cell<bool>,
    single: bool,
    min_chars: usize,
    source: Source,
    create: Option<Create>,
    on_chip: Option<Rc<dyn Fn(i64)>>,
    on_change: RefCell<Option<Box<dyn Fn()>>>,
}

impl Picker {
    pub fn new(placeholder: &str, single: bool, min_chars: usize, source: Source, create: Option<Create>, on_chip: Option<Rc<dyn Fn(i64)>>) -> Rc<Picker> {
        let root = vbox(4);
        root.add_css_class("field-box");
        let chips = gtk::FlowBox::new();
        chips.set_selection_mode(gtk::SelectionMode::None);
        chips.set_column_spacing(4);
        chips.set_row_spacing(4);
        chips.set_max_children_per_line(20);
        chips.set_margin_start(4);
        chips.set_margin_top(3);
        chips.set_visible(false);
        let entry = gtk::Entry::new();
        entry.add_css_class("bare");
        entry.set_placeholder_text(Some(placeholder));
        root.append(&chips);
        root.append(&entry);

        let list = gtk::ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::Single);
        list.set_activate_on_single_click(true);
        let scroll = gtk::ScrolledWindow::new();
        scroll.set_hscrollbar_policy(gtk::PolicyType::Never);
        scroll.set_propagate_natural_height(true);
        scroll.set_max_content_height(280);
        scroll.set_child(Some(&list));
        let popover = gtk::Popover::new();
        popover.set_child(Some(&scroll));
        popover.set_autohide(false);
        popover.set_has_arrow(false);
        popover.set_position(gtk::PositionType::Bottom);
        popover.set_can_focus(false);
        popover.set_parent(&root);

        let p = Rc::new(Picker {
            root,
            entry,
            chips,
            popover,
            list,
            items: RefCell::new(vec![]),
            shown: RefCell::new(vec![]),
            has_create: Cell::new(false),
            single,
            min_chars,
            source,
            create,
            on_chip,
            on_change: RefCell::new(None),
        });
        p.wire();
        p
    }

    fn wire(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        self.entry.connect_changed(move |_| {
            if let Some(p) = weak.upgrade() {
                p.suggest();
            }
        });

        let weak = Rc::downgrade(self);
        self.list.connect_row_activated(move |_, row| {
            if let Some(p) = weak.upgrade() {
                p.pick(row.index() as usize);
                p.entry.grab_focus();
            }
        });

        let focus = gtk::EventControllerFocus::new();
        let weak = Rc::downgrade(self);
        focus.connect_enter(move |_| {
            if let Some(p) = weak.upgrade()
                && p.min_chars == 0 {
                    p.suggest();
                }
        });
        let weak = Rc::downgrade(self);
        focus.connect_leave(move |_| {
            let weak = weak.clone();
            // Let a click on a suggestion land before closing.
            glib::timeout_add_local_once(Duration::from_millis(160), move || {
                if let Some(p) = weak.upgrade()
                    && !p.focused() {
                        p.popover.popdown();
                    }
            });
        });
        self.entry.add_controller(focus);

        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(self);
        keys.connect_key_pressed(move |_, key, _, state| {
            let Some(p) = weak.upgrade() else { return glib::Propagation::Proceed };
            p.on_key(key, state)
        });
        self.entry.add_controller(keys);

        // Clicking anywhere in the field focuses the search entry.
        let click = gtk::GestureClick::new();
        let entry = self.entry.clone();
        click.connect_pressed(move |_, _, _, _| {
            entry.grab_focus();
        });
        self.root.add_controller(click);
    }

    fn on_key(self: &Rc<Self>, key: gdk::Key, state: gdk::ModifierType) -> glib::Propagation {
        use gdk::Key;
        let open = self.popover.is_visible();
        let shift = state.contains(gdk::ModifierType::SHIFT_MASK);
        match key {
            Key::Down | Key::Up => {
                if !open {
                    self.suggest_force();
                } else {
                    self.move_cursor(if key == Key::Down { 1 } else { -1 });
                }
                glib::Propagation::Stop
            }
            Key::Return | Key::KP_Enter if open && !shift => {
                let i = self.list.selected_row().map(|r| r.index() as usize).unwrap_or(0);
                self.pick(i);
                glib::Propagation::Stop
            }
            Key::Escape if open => {
                self.popover.popdown();
                glib::Propagation::Stop
            }
            Key::BackSpace if self.entry.text().is_empty() && !self.items.borrow().is_empty() => {
                self.items.borrow_mut().pop();
                self.render_chips();
                glib::Propagation::Stop
            }
            Key::Tab | Key::ISO_Left_Tab => {
                self.popover.popdown();
                glib::Propagation::Proceed
            }
            _ => glib::Propagation::Proceed,
        }
    }

    /// Whether keyboard focus is in this picker (independent of whether the
    /// window itself is active).
    fn focused(&self) -> bool {
        self.entry.root().and_then(|r| r.focus()).is_some_and(|f| f.is_ancestor(&self.root))
    }

    fn move_cursor(&self, delta: i32) {
        let n = self.shown.borrow().len() as i32;
        if n == 0 {
            return;
        }
        let i = self.list.selected_row().map(|r| r.index()).unwrap_or(-1);
        let next = (i + delta).rem_euclid(n);
        if let Some(row) = self.list.row_at_index(next) {
            self.list.select_row(Some(&row));
            row.grab_focus();
            self.entry.grab_focus_without_selecting();
        }
    }

    fn suggest_force(self: &Rc<Self>) {
        self.suggest_with(true);
    }

    fn suggest(self: &Rc<Self>) {
        self.suggest_with(false);
    }

    fn suggest_with(self: &Rc<Self>, force: bool) {
        let text = self.entry.text().to_string();
        let query = text.trim();
        if query.chars().count() < self.min_chars && !(force && self.min_chars == 0) {
            self.popover.popdown();
            return;
        }
        let exclude: Vec<i64> = self.items.borrow().iter().map(|i| i.id).collect();
        let mut shown = (self.source)(query, &exclude);
        let exact = shown.iter().any(|s| s.label.eq_ignore_ascii_case(query));
        let has_create = self.create.is_some() && !query.is_empty() && !exact;
        if has_create {
            shown.push(Suggestion { id: -1, label: format!("Add “{query}”"), detail: String::new() });
        }
        self.has_create.set(has_create);

        while let Some(row) = self.list.row_at_index(0) {
            self.list.remove(&row);
        }
        for (i, s) in shown.iter().enumerate() {
            let line = hbox(10);
            if has_create && i == shown.len() - 1 {
                line.append(&glyph(widgets::glyph::PLUS, &[]));
            }
            let l = ellipsized(&s.label, &["title"]);
            l.set_hexpand(true);
            line.append(&l);
            if !s.detail.is_empty() {
                line.append(&ellipsized(&s.detail, &["detail"]));
            }
            let row = gtk::ListBoxRow::new();
            row.set_child(Some(&line));
            row.add_css_class("suggestion");
            if has_create && i == shown.len() - 1 {
                row.add_css_class("create");
            }
            row.set_focusable(false);
            self.list.append(&row);
        }
        let empty = shown.is_empty();
        *self.shown.borrow_mut() = shown;
        if empty {
            self.popover.popdown();
            return;
        }
        if let Some(first) = self.list.row_at_index(0) {
            self.list.select_row(Some(&first));
        }
        let width = self.root.width().max(220);
        if let Some(child) = self.popover.child() {
            child.set_size_request(width - 12, -1);
        }
        if force || self.focused() {
            self.popover.popup();
        }
    }

    fn pick(self: &Rc<Self>, index: usize) {
        let Some(s) = self.shown.borrow().get(index).cloned() else { return };
        let is_create = self.has_create.get() && index == self.shown.borrow().len() - 1;
        let item = if is_create {
            let query = self.entry.text().trim().to_string();
            match self.create.as_ref().and_then(|c| c(&query)) {
                Some(item) => item,
                None => return,
            }
        } else {
            Item { id: s.id, label: s.label.clone(), done: false }
        };
        {
            let mut items = self.items.borrow_mut();
            if self.single {
                items.clear();
            }
            if !items.iter().any(|i| i.id == item.id) {
                items.push(item);
            }
        }
        self.entry.set_text("");
        self.popover.popdown();
        self.render_chips();
    }

    pub fn set(self: &Rc<Self>, items: Vec<Item>) {
        *self.items.borrow_mut() = items;
        self.render_chips();
    }

    /// Called whenever the chosen items change (pick, remove, set).
    pub fn connect_change(&self, f: impl Fn() + 'static) {
        *self.on_change.borrow_mut() = Some(Box::new(f));
    }

    pub fn ids(&self) -> Vec<i64> {
        self.items.borrow().iter().map(|i| i.id).collect()
    }

    fn render_chips(self: &Rc<Self>) {
        while let Some(c) = self.chips.first_child() {
            self.chips.remove(&c);
        }
        let items = self.items.borrow().clone();
        for item in &items {
            let chip = hbox(0);
            chip.add_css_class("chip");
            if item.done {
                chip.add_css_class("done");
            }
            if let Some(on_chip) = &self.on_chip {
                let b = gtk::Button::new();
                b.set_child(Some(&ellipsized(&item.label, &[])));
                b.add_css_class("flat");
                b.set_focusable(false);
                b.set_tooltip_text(Some("Open task"));
                b.set_has_frame(false);
                let id = item.id;
                let on = on_chip.clone();
                b.connect_clicked(move |_| on(id));
                chip.append(&b);
            } else {
                chip.append(&ellipsized(&item.label, &[]));
            }
            let x = gtk::Button::with_label(widgets::glyph::CLOSE);
            x.add_css_class("x");
            x.set_focusable(false);
            x.set_tooltip_text(Some("Remove"));
            let weak = Rc::downgrade(self);
            let id = item.id;
            x.connect_clicked(move |_| {
                if let Some(p) = weak.upgrade() {
                    p.items.borrow_mut().retain(|i| i.id != id);
                    p.render_chips();
                    p.entry.grab_focus();
                }
            });
            chip.append(&x);
            self.chips.insert(&chip, -1);
            if let Some(child) = self.chips.last_child() {
                child.set_focusable(false);
            }
        }
        self.chips.set_visible(!items.is_empty());
        if let Some(f) = self.on_change.borrow().as_ref() {
            f();
        }
        if self.single {
            self.entry.set_placeholder_text(if items.is_empty() { Some("Select…") } else { Some("Change…") });
        }
    }
}
