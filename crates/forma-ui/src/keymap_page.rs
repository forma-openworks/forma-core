//! Страница «Раскладка клавиш»: все действия по группам и их сочетания. Сочетание
//! записывается нажатием, занятые сочетания показываются сразу, изменённые действия
//! выделены акцентом и возвращаются к стандартным одной кнопкой.

use egui::{CornerRadius, CursorIcon, Event, FontId, Id, Key, Margin, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use forma_actions::{Action, Keymap, Registry, Shortcut, Title};
use forma_i18n::t;

use crate::settings_window::{self as sw, matches};
use crate::theme::Palette;

pub const PAGE: &str = "keymap";

/// Название действия на текущем языке.
pub fn title(a: &Action) -> String {
    match &a.title {
        Title::Key(k) => forma_i18n::tr(k),
        Title::Text(s) => s.clone(),
    }
}

/// Подписи для поиска по настройкам: названия всех действий.
pub fn keywords(registry: &Registry) -> Vec<String> {
    registry.iter().map(title).collect()
}

#[derive(Debug, Default)]
pub struct KeymapPage {
    filter: String,
    capture: Option<Capture>,
}

#[derive(Debug)]
struct Capture {
    action: String,
    key: Option<Shortcut>,
}

impl KeymapPage {
    /// `search` — строка поиска всего окна настроек: если она нашла страницу не по
    /// названию, а по действию, список действий отфильтрован по ней.
    pub fn ui(&mut self, ui: &mut Ui, pal: &Palette, registry: &Registry, keymap: &mut Keymap, search: &str) {
        // Пока записывается сочетание, клавиши принадлежат записи: фокус с полей ввода
        // снимается, а нажатия съедаются раньше, чем их увидит кто-то ещё на странице.
        if let Some(cap) = &mut self.capture {
            ui.memory_mut(|m| m.stop_text_input());
            if read_keys(ui, cap) {
                self.capture = None;
            }
        }
        let filter = &mut self.filter;
        crate::widgets::bar(
            ui,
            Id::new("forma-keymap-bar"),
            0.0,
            |ui| {
                let width = ui.available_width().min(300.0);
                crate::widgets::search_field(ui, Id::new("forma-keymap-filter"), filter, &t!("keymap-search-hint"), width, 13.0);
            },
            |ui| {
                if !keymap.overrides.is_empty() && ui.button(t!("keymap-reset-all")).clicked() {
                    keymap.overrides.clear();
                }
            },
        );
        ui.add_space(8.0);
        let page_title = t!("settings-page-keymap");
        let filter = if !self.filter.trim().is_empty() {
            self.filter.clone()
        } else if !matches(&page_title, search) {
            search.to_owned()
        } else {
            String::new()
        };

        let mut any = false;
        for group in registry.groups() {
            let actions: Vec<&Action> = registry
                .iter()
                .filter(|a| a.group == group)
                .filter(|a| matches(&title(a), &filter) || keymap.shortcuts(registry, &a.id).iter().any(|s| matches(&s.to_string(), &filter)))
                .collect();
            if actions.is_empty() {
                continue;
            }
            any = true;
            sw::group(ui, pal, &forma_i18n::tr(group));
            for a in actions {
                self.action_row(ui, pal, registry, keymap, a, &filter);
            }
            ui.add_space(6.0);
        }
        if !any {
            ui.label(RichText::new(t!("settings-nothing-found")).color(pal.text_faint));
        }
    }

    fn action_row(&mut self, ui: &mut Ui, pal: &Palette, registry: &Registry, keymap: &mut Keymap, a: &Action, filter: &str) {
        let modified = keymap.is_modified(&a.id);
        let keys: Vec<Shortcut> = keymap.shortcuts(registry, &a.id).to_vec();
        let name = title(a);
        let color = if modified { pal.accent } else { pal.text };
        // Сколько места справа займут кнопки и сочетания. Если вместе с названием они
        // не помещаются в строку, сочетания уходят на вторую строку под названием.
        let key_galleys: Vec<_> = keys.iter().map(|k| ui.painter().layout_no_wrap(k.to_string(), FontId::monospace(12.0), pal.text)).collect();
        let none = ui.painter().layout_no_wrap(t!("keymap-none"), FontId::proportional(12.0), pal.text_faint);
        let right_w = 8.0 + if modified { 52.0 } else { 26.0 } + if keys.is_empty() { none.size().x + 4.0 } else { key_galleys.iter().map(|g| g.size().x + 32.0).sum() };
        let name_w = ui.painter().layout_no_wrap(name.clone(), FontId::proportional(13.5), color).size().x;
        let width = ui.available_width();
        let two_lines = 10.0 + name_w + 16.0 + right_w > width;
        let (rect, resp) = ui.allocate_exact_size(vec2(width, if two_lines { 56.0 } else { 30.0 }), Sense::hover());
        if resp.hovered() {
            ui.painter().rect_filled(rect, CornerRadius::same(6), pal.list_hover);
        }
        let (name_y, keys_y) = if two_lines { (rect.top() + 15.0, rect.bottom() - 15.0) } else { (rect.center().y, rect.center().y) };
        let name_room = if two_lines { width - 20.0 } else { width - 26.0 - right_w };
        let galley = crate::widgets::elide(ui, name.clone(), FontId::proportional(13.5), color, name_room);
        let text_pos = pos2(rect.left() + 10.0, name_y - galley.size().y / 2.0);
        if !filter.trim().is_empty() && matches(&name, filter) {
            ui.painter().rect_filled(Rect::from_min_size(text_pos, galley.size()).expand2(vec2(3.0, 1.0)), CornerRadius::same(4), sw::highlight(pal));
        }
        ui.painter().galley(text_pos, galley, color);

        // Справа налево: сброс, добавление, сочетания.
        let mut x = rect.right() - 8.0;
        let button = |ui: &mut Ui, x: &mut f32, icon: &str, hint: String| {
            let r = Rect::from_center_size(pos2(*x - 11.0, keys_y), vec2(22.0, 22.0));
            *x -= 26.0;
            let b = ui.interact(r, ui.id().with((&a.id, icon)), Sense::click());
            if b.hovered() {
                ui.painter().rect_filled(r, CornerRadius::same(5), pal.bg_elevated);
            }
            crate::icons::paint(ui.painter(), r.shrink(3.0), icon, pal.text_muted);
            b.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(hint).clicked()
        };
        if modified && button(ui, &mut x, "reset", t!("keymap-reset")) {
            keymap.reset(&a.id);
        }
        if button(ui, &mut x, "add", t!("keymap-add")) {
            self.capture = Some(Capture { action: a.id.clone(), key: None });
        }
        if keys.is_empty() {
            ui.painter().galley(pos2(x - none.size().x - 4.0, keys_y - none.size().y / 2.0), none, pal.text_faint);
        }
        for (key, g) in keys.iter().zip(key_galleys).rev() {
            let text = key.to_string();
            let chip = Rect::from_min_max(pos2(x - g.size().x - 30.0, keys_y - 11.0), pos2(x - 4.0, keys_y + 11.0));
            x = chip.left() - 2.0;
            let c = ui.interact(chip, ui.id().with((&a.id, &text)), Sense::click());
            let hit = !filter.trim().is_empty() && matches(&text, filter);
            ui.painter().rect(chip, CornerRadius::same(6), if hit { sw::highlight(pal) } else { pal.chip_bg }, Stroke::new(1.0, pal.chip_border), egui::StrokeKind::Inside);
            ui.painter().galley(pos2(chip.left() + 8.0, chip.center().y - g.size().y / 2.0), g, pal.text);
            let cross = if c.hovered() { pal.danger } else { pal.text_faint };
            crate::icons::paint(ui.painter(), Rect::from_center_size(pos2(chip.right() - 10.0, chip.center().y), vec2(11.0, 11.0)), "close", cross);
            if c.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(t!("keymap-remove", shortcut = text)).clicked() {
                keymap.remove(registry, &a.id, *key);
            }
        }

        if self.capture.as_ref().is_some_and(|c| c.action == a.id) {
            self.capture_ui(ui, pal, registry, keymap, &a.id);
        }
    }

    /// Запись нового сочетания: следующее нажатие клавиш становится сочетанием.
    fn capture_ui(&mut self, ui: &mut Ui, pal: &Palette, registry: &Registry, keymap: &mut Keymap, id: &str) {
        let Some(cap) = &mut self.capture else { return };
        let mut cancel = false;
        let mut add: Option<bool> = None;
        egui::Frame::new().fill(pal.popup_bg).stroke(Stroke::new(1.0, pal.input_focus)).corner_radius(CornerRadius::same(8)).inner_margin(Margin::symmetric(12, 8)).show(ui, |ui| {
            let key = cap.key;
            crate::widgets::bar(
                ui,
                Id::new("forma-keymap-capture"),
                0.0,
                |ui| {
                    match key {
                        Some(k) => ui.label(RichText::new(k.to_string()).font(FontId::monospace(13.5)).color(pal.heading)),
                        None => ui.label(RichText::new(t!("keymap-press")).color(pal.text_muted)),
                    };
                },
                |ui| {
                    if ui.button(t!("settings-cancel")).clicked() {
                        cancel = true;
                    }
                    if let Some(k) = key {
                        let others = keymap.conflicts(registry, k, id);
                        if !others.is_empty() && ui.button(t!("keymap-conflict-remove")).clicked() {
                            add = Some(true);
                        }
                        if ui.button(t!("keymap-add-confirm")).clicked() {
                            add = Some(false);
                        }
                    }
                },
            );
            if let Some(k) = cap.key {
                let others = keymap.conflicts(registry, k, id);
                if !others.is_empty() {
                    let names: Vec<String> = others.iter().filter_map(|o| registry.get(o)).map(title).collect();
                    crate::widgets::icon_label(ui, "warning", t!("keymap-conflict", actions = names.join(", ")), 12.0, pal.warning);
                }
            }
        });
        ui.add_space(4.0);
        if let (Some(remove_others), Some(k)) = (add, cap.key) {
            if remove_others {
                for other in keymap.conflicts(registry, k, id) {
                    keymap.remove(registry, &other, k);
                }
            }
            keymap.add(registry, id, k);
            cancel = true;
        }
        if cancel {
            self.capture = None;
        }
    }
}

/// Забирает все нажатия кадра для записи сочетания. Escape без модификаторов
/// отменяет запись (а не закрывает окно настроек) — тогда возвращает `true`.
fn read_keys(ui: &mut Ui, cap: &mut Capture) -> bool {
    let mut cancel = false;
    ui.input_mut(|i| {
        for e in &i.events {
            match e {
                Event::Key { key: Key::Escape, pressed: true, modifiers, .. } if modifiers.is_none() => cancel = true,
                e => {
                    if let Some(s) = Shortcut::from_event(e) {
                        cap.key = Some(s);
                    }
                }
            }
        }
        i.events.retain(|e| !matches!(e, Event::Key { .. } | Event::Text(_)));
    });
    cancel
}
