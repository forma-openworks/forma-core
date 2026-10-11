//! Страница «Раскладка клавиш»: все действия по группам и их сочетания. Сочетание
//! записывается нажатием, занятые сочетания показываются сразу, изменённые действия
//! выделены акцентом и возвращаются к стандартным одной кнопкой.

use egui::{Align, Align2, CornerRadius, CursorIcon, Event, FontId, Key, Layout, Margin, Rect, RichText, Sense, Stroke, TextEdit, Ui, pos2, vec2};
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
        ui.horizontal(|ui| {
            ui.add(TextEdit::singleline(&mut self.filter).hint_text(format!("🔍 {}", t!("keymap-search-hint"))).desired_width(300.0).margin(Margin::symmetric(8, 5)));
            if !keymap.overrides.is_empty() {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button(t!("keymap-reset-all")).clicked() {
                        keymap.overrides.clear();
                    }
                });
            }
        });
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
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::hover());
        if resp.hovered() {
            ui.painter().rect_filled(rect, CornerRadius::same(6), pal.bg_hover);
        }
        let name = title(a);
        let color = if modified { pal.accent } else { pal.text };
        let galley = ui.painter().layout_no_wrap(name.clone(), FontId::proportional(13.5), color);
        let text_pos = pos2(rect.left() + 10.0, rect.center().y - galley.size().y / 2.0);
        if !filter.trim().is_empty() && matches(&name, filter) {
            ui.painter().rect_filled(Rect::from_min_size(text_pos, galley.size()).expand2(vec2(3.0, 1.0)), CornerRadius::same(4), sw::highlight(pal));
        }
        ui.painter().galley(text_pos, galley, color);

        // Справа налево: «↺», «+», сочетания.
        let mut x = rect.right() - 8.0;
        let button = |ui: &mut Ui, x: &mut f32, glyph: &str, hint: String| {
            let r = Rect::from_center_size(pos2(*x - 11.0, rect.center().y), vec2(22.0, 22.0));
            *x -= 26.0;
            let b = ui.interact(r, ui.id().with((&a.id, glyph)), Sense::click());
            if b.hovered() {
                ui.painter().rect_filled(r, CornerRadius::same(5), pal.bg_elevated);
            }
            ui.painter().text(r.center(), Align2::CENTER_CENTER, glyph, FontId::proportional(14.0), pal.text_muted);
            b.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(hint).clicked()
        };
        if modified && button(ui, &mut x, "↺", t!("keymap-reset")) {
            keymap.reset(&a.id);
        }
        if button(ui, &mut x, "+", t!("keymap-add")) {
            self.capture = Some(Capture { action: a.id.clone(), key: None });
        }
        if keys.is_empty() {
            let g = ui.painter().layout_no_wrap(t!("keymap-none"), FontId::proportional(12.0), pal.text_faint);
            ui.painter().galley(pos2(x - g.size().x - 4.0, rect.center().y - g.size().y / 2.0), g, pal.text_faint);
        }
        for key in keys.iter().rev() {
            let text = key.to_string();
            let g = ui.painter().layout_no_wrap(text.clone(), FontId::monospace(12.0), pal.text);
            let chip = Rect::from_min_max(pos2(x - g.size().x - 30.0, rect.center().y - 11.0), pos2(x - 4.0, rect.center().y + 11.0));
            x = chip.left() - 2.0;
            let c = ui.interact(chip, ui.id().with((&a.id, &text)), Sense::click());
            let hit = !filter.trim().is_empty() && matches(&text, filter);
            ui.painter().rect(chip, CornerRadius::same(6), if hit { sw::highlight(pal) } else { pal.bg_elevated }, Stroke::new(1.0, pal.border), egui::StrokeKind::Inside);
            ui.painter().galley(pos2(chip.left() + 8.0, chip.center().y - g.size().y / 2.0), g, pal.text);
            let cross = if c.hovered() { pal.danger } else { pal.text_faint };
            ui.painter().text(pos2(chip.right() - 10.0, chip.center().y), Align2::CENTER_CENTER, "×", FontId::proportional(13.0), cross);
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
        egui::Frame::new().fill(pal.bg_elevated).stroke(Stroke::new(1.0, pal.accent)).corner_radius(CornerRadius::same(8)).inner_margin(Margin::symmetric(12, 8)).show(ui, |ui| {
            ui.horizontal(|ui| {
                match cap.key {
                    Some(k) => ui.label(RichText::new(k.to_string()).font(FontId::monospace(13.5)).color(pal.heading)),
                    None => ui.label(RichText::new(t!("keymap-press")).color(pal.text_muted)),
                };
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button(t!("settings-cancel")).clicked() {
                        cancel = true;
                    }
                    if let Some(k) = cap.key {
                        let others = keymap.conflicts(registry, k, id);
                        if !others.is_empty() && ui.button(t!("keymap-conflict-remove")).clicked() {
                            add = Some(true);
                        }
                        if ui.button(t!("keymap-add-confirm")).clicked() {
                            add = Some(false);
                        }
                    }
                });
            });
            if let Some(k) = cap.key {
                let others = keymap.conflicts(registry, k, id);
                if !others.is_empty() {
                    let names: Vec<String> = others.iter().filter_map(|o| registry.get(o)).map(title).collect();
                    ui.label(RichText::new(format!("⚠ {}", t!("keymap-conflict", actions = names.join(", ")))).size(12.0).color(pal.warning));
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
