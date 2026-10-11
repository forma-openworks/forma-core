//! «Найти действие» (`Ctrl+Shift+A`), как в JetBrains: поиск по всем командам
//! программы и страницам настроек, с их сочетаниями клавиш.

use egui::{Align2, CornerRadius, FontId, Key, Margin, Modifiers, RichText, Sense, TextEdit, Ui, pos2, vec2};
use forma_i18n::t;

use crate::theme::Palette;

/// Пункт поиска: действие или страница настроек.
#[derive(Clone, Debug)]
pub struct Item {
    pub id: String,
    pub title: String,
    /// Группа справа от названия: «Запуск», «Настройки».
    pub group: String,
    pub shortcut: String,
}

#[derive(Debug)]
pub struct FindAction {
    pub query: String,
    selected: usize,
    focus: bool,
}

impl Default for FindAction {
    fn default() -> Self {
        Self { query: String::new(), selected: 0, focus: true }
    }
}

/// Подходит ли пункт: каждое слово запроса есть в названии или группе.
fn fits(item: &Item, query: &str) -> bool {
    let hay = format!("{} {}", item.title, item.group).to_lowercase();
    query.to_lowercase().split_whitespace().all(|w| hay.contains(w))
}

const ROW_H: f32 = 30.0;
const ROWS: usize = 12;

impl FindAction {
    /// Рисует поиск и список. Возвращает id выбранного пункта.
    pub fn ui(&mut self, ui: &mut Ui, pal: &Palette, items: &[Item]) -> Option<String> {
        let mut found: Vec<&Item> = items.iter().filter(|i| fits(i, &self.query)).collect();
        // Сначала те, чьё название начинается с запроса.
        let q = self.query.trim().to_lowercase();
        found.sort_by_key(|i| !i.title.to_lowercase().starts_with(&q));
        self.selected = self.selected.min(found.len().saturating_sub(1));
        let (down, up, enter) = ui.input_mut(|i| {
            (i.consume_key(Modifiers::NONE, Key::ArrowDown), i.consume_key(Modifiers::NONE, Key::ArrowUp), i.consume_key(Modifiers::NONE, Key::Enter))
        });
        if !found.is_empty() {
            if down {
                self.selected = (self.selected + 1) % found.len();
            }
            if up {
                self.selected = (self.selected + found.len() - 1) % found.len();
            }
        }

        let edit = ui.add(TextEdit::singleline(&mut self.query).hint_text(format!("🔍 {}", t!("find-action-hint"))).desired_width(f32::INFINITY).margin(Margin::symmetric(10, 8)).font(FontId::proportional(15.0)));
        if std::mem::take(&mut self.focus) {
            edit.request_focus();
        }
        if edit.changed() {
            self.selected = 0;
        }
        ui.add_space(8.0);

        let mut picked = None;
        // Высота постоянная: окно не прыгает, пока набираешь запрос.
        let height = ROW_H * ROWS as f32;
        egui::ScrollArea::vertical().max_height(height).min_scrolled_height(height).auto_shrink([false, false]).show(ui, |ui| {
            if found.is_empty() {
                ui.label(RichText::new(t!("settings-nothing-found")).color(pal.text_faint));
            }
            for (n, item) in found.iter().enumerate() {
                let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
                if n == self.selected {
                    ui.painter().rect_filled(rect, CornerRadius::same(6), pal.accent_soft);
                    if down || up {
                        resp.scroll_to_me(None);
                    }
                } else if resp.hovered() {
                    ui.painter().rect_filled(rect, CornerRadius::same(6), pal.bg_hover);
                }
                let painter = ui.painter().with_clip_rect(rect);
                let title = painter.text(pos2(rect.left() + 10.0, rect.center().y), Align2::LEFT_CENTER, &item.title, FontId::proportional(13.5), pal.heading);
                painter.text(pos2(title.right() + 10.0, rect.center().y), Align2::LEFT_CENTER, &item.group, FontId::proportional(12.0), pal.text_faint);
                if !item.shortcut.is_empty() {
                    painter.text(pos2(rect.right() - 10.0, rect.center().y), Align2::RIGHT_CENTER, &item.shortcut, FontId::monospace(12.0), pal.text_muted);
                }
                if resp.clicked() {
                    picked = Some(item.id.clone());
                }
            }
        });
        if enter {
            picked = found.get(self.selected).map(|i| i.id.clone());
        }
        picked
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_word_must_fit() {
        let item = Item { id: "file.saveAll".into(), title: "Сохранить всё".into(), group: "Файл".into(), shortcut: "Ctrl+S".into() };
        assert!(fits(&item, "сохр"));
        assert!(fits(&item, "всё файл"));
        assert!(!fits(&item, "сохр запуск"));
        assert!(fits(&item, ""));
    }
}
