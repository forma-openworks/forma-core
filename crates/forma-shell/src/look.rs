//! Оформление окна: фоновая картинка и быстрая смена темы.
//!
//! Картинка рисуется полупрозрачным слоем поверх панелей и под меню и диалогами, как
//! фон в Rider. Быстрая смена темы (`Ctrl+\``) применяет тему сразу при наведении или
//! стрелках: Enter её оставляет, Escape возвращает прежнюю.

use std::path::PathBuf;

use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Id, Key, LayerId, Modifiers, Order, Pos2, Rect, RichText, Sense, Ui, vec2};
use forma_i18n::t;
use forma_ui::theme::{Background, Fit, Origin, Palette, Themes};

/// Фоновая картинка в видеопамяти.
#[derive(Default)]
pub struct Backdrop {
    /// Какой файл загружен (или не загрузился — тогда без текстуры).
    path: Option<PathBuf>,
    texture: Option<egui::TextureHandle>,
}

impl Backdrop {
    /// Рисует картинку `bg` поверх панелей окна.
    pub fn paint(&mut self, ctx: &egui::Context, bg: Option<Background>) {
        let Some(bg) = bg.filter(|b| b.opacity > 0.0) else { return };
        if self.path.as_ref() != Some(&bg.image) {
            self.texture = load_texture(ctx, &bg.image);
            self.path = Some(bg.image.clone());
        }
        let Some(tex) = &self.texture else { return };
        let screen = ctx.content_rect();
        let (rect, uv) = placement(bg.fit, screen, tex.size_vec2());
        let tint = Color32::from_white_alpha((bg.opacity * 255.0).round() as u8);
        let painter = ctx.layer_painter(LayerId::new(Order::Middle, Id::new("forma-backdrop"))).with_clip_rect(screen);
        painter.image(tex.id(), rect, uv, tint);
    }
}

fn load_texture(ctx: &egui::Context, path: &PathBuf) -> Option<egui::TextureHandle> {
    let img = match image::open(path) {
        Ok(img) => img.to_rgba8(),
        Err(e) => {
            log::warn!("Failed to load background image {}: {e}", path.display());
            return None;
        }
    };
    let size = [img.width() as usize, img.height() as usize];
    let color = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
    // Повтор нужен для заполнения плиткой; остальным режимам он не мешает.
    let options = egui::TextureOptions { wrap_mode: egui::TextureWrapMode::Repeat, ..egui::TextureOptions::LINEAR };
    Some(ctx.load_texture("forma-backdrop", color, options))
}

/// Где рисовать картинку размера `image` в окне `screen` и какие координаты текстуры брать.
fn placement(fit: Fit, screen: Rect, image: egui::Vec2) -> (Rect, Rect) {
    let full = Rect::from_min_max(Pos2::ZERO, egui::pos2(1.0, 1.0));
    let centered = |size: egui::Vec2| Rect::from_center_size(screen.center(), size);
    match fit {
        Fit::Cover => (centered(image * (screen.width() / image.x).max(screen.height() / image.y)), full),
        Fit::Contain => (centered(image * (screen.width() / image.x).min(screen.height() / image.y)), full),
        Fit::Stretch => (screen, full),
        Fit::Center => (centered(image), full),
        Fit::Tile => (screen, Rect::from_min_max(Pos2::ZERO, egui::pos2(screen.width() / image.x, screen.height() / image.y))),
    }
}

/// Быстрая смена темы: список и тема, которую вернуть при отмене.
pub struct ThemeSwitcher {
    /// id, название, тёмная ли, встроенная ли.
    themes: Vec<(String, String, bool, bool)>,
    selected: usize,
    original: String,
    /// Тема выбрана (Enter или щелчок) — при закрытии её не откатываем.
    pub committed: bool,
}

impl ThemeSwitcher {
    /// `current` — тема из настроек (пусто — тема по умолчанию `default`).
    pub fn new(current: &str, default: &str, themes: &Themes) -> Self {
        let list: Vec<(String, String, bool, bool)> = themes.list().iter().map(|t| (t.id.clone(), t.name.clone(), t.dark, t.origin == Origin::Builtin)).collect();
        let shown = if current.is_empty() { default } else { current };
        let selected = list.iter().position(|(id, ..)| id == shown).unwrap_or(0);
        Self { themes: list, selected, original: current.to_owned(), committed: false }
    }

    /// Тема, которая была до открытия окна.
    pub fn original(&self) -> &str {
        &self.original
    }

    /// Список тем. Меняет тему в `theme` сразу при выборе. Возвращает `true`, когда окно
    /// пора закрыть (тему выбрали).
    pub fn ui(&mut self, ui: &mut Ui, pal: &Palette, theme: &mut String) -> bool {
        ui.set_width(crate::chrome::modal_width(ui.ctx(), 380.0));
        ui.label(RichText::new(t!("theme-switcher-title")).size(16.0).family(forma_ui::fonts::semibold()).color(pal.heading));
        ui.label(RichText::new(t!("theme-switcher-hint")).size(11.5).color(pal.text_faint));
        ui.add_space(8.0);
        let (down, up, enter) = ui.input_mut(|i| {
            (i.consume_key(Modifiers::NONE, Key::ArrowDown), i.consume_key(Modifiers::NONE, Key::ArrowUp), i.consume_key(Modifiers::NONE, Key::Enter))
        });
        let n = self.themes.len();
        if n > 0 && down {
            self.selected = (self.selected + 1) % n;
        }
        if n > 0 && up {
            self.selected = (self.selected + n - 1) % n;
        }
        let mut close = enter;
        // Длинный список тем в низком окне прокручивается.
        let max_h = (ui.ctx().content_rect().height() - 200.0).max(90.0);
        egui::ScrollArea::vertical().max_height(max_h).show(ui, |ui| {
            for (i, (_, name, dark, builtin)) in self.themes.iter().enumerate() {
                let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click());
                // Наведение сразу показывает тему; движение мыши без наведения ничего не меняет.
                if resp.hovered() && ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO) {
                    self.selected = i;
                }
                if i == self.selected {
                    ui.painter().rect_filled(rect, CornerRadius::same(6), pal.list_selected);
                    if down || up {
                        resp.scroll_to_me(None);
                    }
                }
                let color = if i == self.selected { pal.heading } else { pal.text };
                // Справа: «По умолчанию» у встроенных тем и тёмная ли тема.
                let kind = if *dark { t!("theme-dark") } else { t!("theme-light") };
                let tag = if *builtin { format!("{} · {kind}", t!("theme-default")) } else { kind };
                let tag = ui.painter().text(rect.right_center() - vec2(12.0, 0.0), Align2::RIGHT_CENTER, tag, FontId::proportional(11.5), pal.text_faint);
                let name = forma_ui::widgets::elide(ui, name.clone(), FontId::proportional(13.5), color, tag.left() - 12.0 - (rect.left() + 12.0));
                ui.painter().galley(egui::pos2(rect.left() + 12.0, rect.center().y - name.size().y / 2.0), name, color);
                if resp.clicked() {
                    self.selected = i;
                    close = true;
                }
            }
        });
        if let Some((id, ..)) = self.themes.get(self.selected)
            && theme != id
        {
            theme.clone_from(id);
        }
        if close {
            self.committed = true;
        }
        close
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placement_modes() {
        let screen = Rect::from_min_size(Pos2::ZERO, vec2(200.0, 100.0));
        let image = vec2(100.0, 100.0);
        let (r, _) = placement(Fit::Cover, screen, image);
        assert_eq!(r.size(), vec2(200.0, 200.0), "закрывает окно целиком");
        let (r, _) = placement(Fit::Contain, screen, image);
        assert_eq!(r.size(), vec2(100.0, 100.0), "помещается целиком");
        let (r, uv) = placement(Fit::Tile, screen, image);
        assert_eq!((r, uv.max), (screen, egui::pos2(2.0, 1.0)), "плитка: две по ширине");
        let (r, _) = placement(Fit::Center, screen, image);
        assert_eq!(r.center(), screen.center());
    }
}
