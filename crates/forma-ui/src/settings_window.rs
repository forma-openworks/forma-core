//! Окно настроек как в Rider: слева поиск и дерево разделов, справа страница,
//! внизу «Импорт…», «Экспорт…», «Отмена», «Применить», «ОК».
//!
//! Окно ничего не знает о самих настройках: программа передаёт список страниц и
//! рисует их содержимое. Изменения копятся в черновике программы и применяются
//! кнопками «Применить» и «ОК».

use egui::{Align, Align2, Color32, CornerRadius, CursorIcon, FontId, Id, Layout, Margin, Rect, RichText, Sense, Stroke, TextEdit, Ui, UiBuilder, pos2, vec2};
use forma_i18n::t;

use crate::theme::Palette;

/// Страница настроек.
#[derive(Clone, Debug)]
pub struct PageInfo {
    pub id: String,
    /// Раздел дерева: «Платформа Forma» или название программы.
    pub section: String,
    pub title: String,
    /// Подписи настроек на странице: по ним страница находится поиском.
    pub keywords: Vec<String>,
}

/// Что нажали внизу окна.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Footer {
    Ok,
    Cancel,
    Apply,
    Export,
    Import,
}

#[derive(Debug, Default)]
pub struct SettingsWindow {
    /// Открытая страница.
    pub page: String,
    pub search: String,
    focus_search: bool,
}

/// Совпадает ли текст с поиском (без учёта регистра). Пустой поиск совпадает со всем.
pub fn matches(text: &str, search: &str) -> bool {
    let search = search.trim();
    search.is_empty() || text.to_lowercase().contains(&search.to_lowercase())
}

const LEFT_W: f32 = 236.0;
const FOOTER_H: f32 = 54.0;

impl SettingsWindow {
    pub fn new(page: impl Into<String>) -> Self {
        Self { page: page.into(), search: String::new(), focus_search: true }
    }

    /// Рисует окно размером не больше `max` и возвращает нажатую внизу кнопку.
    /// `modified(id)` — есть ли на странице непримённые изменения, `dirty` — хоть где-то.
    #[allow(clippy::too_many_arguments)]
    pub fn show(&mut self, ui: &mut Ui, pal: &Palette, max: egui::Vec2, pages: &[PageInfo], modified: &dyn Fn(&str) -> bool, dirty: bool, body: &mut dyn FnMut(&mut Ui, &str, &str)) -> Option<Footer> {
        let size = vec2(max.x.min(940.0), max.y.min(640.0));
        let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
        let left = Rect::from_min_size(rect.min, vec2(LEFT_W, size.y));
        let right = Rect::from_min_max(pos2(left.right(), rect.top()), rect.max);
        let painter = ui.painter();
        painter.rect_filled(left, CornerRadius { nw: 14, sw: 14, ne: 0, se: 0 }, pal.bg_panel);
        painter.vline(left.right(), left.y_range(), Stroke::new(1.0, pal.border));

        let visible: Vec<&PageInfo> = pages.iter().filter(|p| matches(&p.title, &self.search) || p.keywords.iter().any(|k| matches(k, &self.search))).collect();
        // Открытая страница пропала из поиска — показываем первую найденную.
        if !visible.iter().any(|p| p.id == self.page)
            && let Some(first) = visible.first()
        {
            self.page = first.id.clone();
        }

        // Слева: поиск и дерево.
        let mut left_ui = ui.new_child(UiBuilder::new().max_rect(left.shrink2(vec2(12.0, 14.0))).layout(Layout::top_down(Align::Min)));
        let search = left_ui.add(TextEdit::singleline(&mut self.search).hint_text(format!("🔍 {}", t!("settings-search-hint"))).desired_width(f32::INFINITY).margin(Margin::symmetric(8, 6)));
        if std::mem::take(&mut self.focus_search) {
            search.request_focus();
        }
        left_ui.add_space(10.0);
        egui::ScrollArea::vertical().id_salt("settings-tree").auto_shrink([false, false]).show(&mut left_ui, |ui| {
            if visible.is_empty() {
                ui.label(RichText::new(t!("settings-nothing-found")).size(12.5).color(pal.text_faint));
            }
            let mut section: Option<&str> = None;
            for page in &visible {
                if section != Some(page.section.as_str()) {
                    if section.is_some() {
                        ui.add_space(8.0);
                    }
                    ui.label(RichText::new(&page.section).size(11.5).color(pal.text_faint));
                    ui.add_space(2.0);
                    section = Some(&page.section);
                }
                if tree_item(ui, pal, &page.title, page.id == self.page, modified(&page.id)).clicked() {
                    self.page = page.id.clone();
                }
            }
        });

        // Справа: путь к странице, страница, кнопки.
        let body_rect = Rect::from_min_max(right.min + vec2(24.0, 18.0), pos2(right.right() - 20.0, right.bottom() - FOOTER_H));
        let mut right_ui = ui.new_child(UiBuilder::new().max_rect(body_rect).layout(Layout::top_down(Align::Min)));
        if let Some(page) = pages.iter().find(|p| p.id == self.page) {
            right_ui.horizontal(|ui| {
                ui.label(RichText::new(&page.section).size(13.0).color(pal.text_faint));
                ui.label(RichText::new("›").size(13.0).color(pal.text_faint));
                ui.label(RichText::new(&page.title).size(15.0).family(crate::fonts::semibold()).color(pal.heading));
            });
            right_ui.add_space(12.0);
            let search = self.search.clone();
            egui::ScrollArea::vertical().id_salt(("settings-page", &page.id)).auto_shrink([false, false]).show(&mut right_ui, |ui| {
                ui.set_width(ui.available_width() - 8.0);
                body(ui, &page.id, &search);
            });
        }

        let footer = Rect::from_min_max(pos2(right.left(), right.bottom() - FOOTER_H), right.max);
        ui.painter().hline(footer.x_range(), footer.top(), Stroke::new(1.0, pal.border));
        let mut out = None;
        let mut footer_ui = ui.new_child(UiBuilder::new().max_rect(footer.shrink2(vec2(20.0, 11.0))).layout(Layout::left_to_right(Align::Center)));
        if footer_ui.button(t!("settings-import")).clicked() {
            out = Some(Footer::Import);
        }
        if footer_ui.button(t!("settings-export")).clicked() {
            out = Some(Footer::Export);
        }
        footer_ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if primary_button(ui, pal, &t!("settings-ok")).clicked() {
                out = Some(Footer::Ok);
            }
            if ui.add_enabled(dirty, egui::Button::new(t!("settings-apply")).min_size(vec2(84.0, 30.0))).clicked() {
                out = Some(Footer::Apply);
            }
            if ui.add(egui::Button::new(t!("settings-cancel")).min_size(vec2(84.0, 30.0))).clicked() {
                out = Some(Footer::Cancel);
            }
        });
        out
    }
}

/// Пункт дерева страниц. Страница с непримёнными изменениями помечена точкой.
fn tree_item(ui: &mut Ui, pal: &Palette, title: &str, selected: bool, modified: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::click());
    let painter = ui.painter();
    if selected {
        painter.rect_filled(rect, CornerRadius::same(7), pal.accent_soft);
    } else if resp.hovered() {
        painter.rect_filled(rect, CornerRadius::same(7), pal.bg_hover);
    }
    let color = if selected { pal.heading } else { pal.text };
    painter.text(pos2(rect.left() + 12.0, rect.center().y), Align2::LEFT_CENTER, title, FontId::proportional(13.5), color);
    if modified {
        painter.circle_filled(pos2(rect.right() - 12.0, rect.center().y), 3.0, pal.accent);
    }
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// Главная кнопка окна: залита акцентом.
pub fn primary_button(ui: &mut Ui, pal: &Palette, text: &str) -> egui::Response {
    let text_color = if pal.dark { pal.bg } else { Color32::WHITE };
    ui.add(egui::Button::new(RichText::new(text).color(text_color)).fill(pal.accent).min_size(vec2(96.0, 30.0)))
}

/// Заголовок группы настроек на странице.
pub fn group(ui: &mut Ui, pal: &Palette, title: &str) {
    ui.add_space(6.0);
    ui.label(RichText::new(title).size(13.0).family(crate::fonts::semibold()).color(pal.heading));
    ui.add_space(4.0);
}

/// Ширина подписи в строке настройки.
pub const LABEL_W: f32 = 210.0;

/// Строка настройки: подпись слева, элемент справа. Подпись, совпавшая с поиском,
/// подсвечивается. Если значение отличается от стандартного (`modified`), рядом
/// появляется «↺»; возвращает `true`, когда по ней щёлкнули.
pub fn row(ui: &mut Ui, pal: &Palette, search: &str, label: &str, modified: bool, add: impl FnOnce(&mut Ui)) -> bool {
    let mut reset = false;
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(LABEL_W, 26.0), Sense::hover());
        let hit = !search.trim().is_empty() && matches(label, search);
        let galley = ui.painter().layout(label.to_owned(), FontId::proportional(13.5), pal.text, LABEL_W - 8.0);
        let pos = pos2(rect.left(), rect.center().y - galley.size().y / 2.0);
        if hit {
            ui.painter().rect_filled(Rect::from_min_size(pos, galley.size()).expand2(vec2(4.0, 1.0)), CornerRadius::same(4), highlight(pal));
        }
        ui.painter().galley(pos, galley, pal.text);
        add(ui);
        if modified {
            reset = ui.add(egui::Button::new(RichText::new("↺").color(pal.text_muted)).frame(false)).on_hover_text(t!("settings-reset")).clicked();
        }
    });
    ui.add_space(2.0);
    reset
}

/// Настройка-галочка: подпись справа от галочки, совпадение с поиском подсвечено.
/// Возвращает `true`, когда нажали «↺» (значение отличается от стандартного — `modified`).
pub fn check(ui: &mut Ui, pal: &Palette, search: &str, label: &str, value: &mut bool, modified: bool) -> bool {
    let mut reset = false;
    ui.horizontal(|ui| {
        let resp = ui.checkbox(value, RichText::new(label).color(pal.text));
        if !search.trim().is_empty() && matches(label, search) {
            let r = resp.rect;
            ui.painter().rect_filled(r.expand2(vec2(3.0, 1.0)), CornerRadius::same(4), highlight(pal));
        }
        if modified {
            reset = ui.add(egui::Button::new(RichText::new("↺").color(pal.text_muted)).frame(false)).on_hover_text(t!("settings-reset")).clicked();
        }
    });
    ui.add_space(2.0);
    reset
}

/// Подсветка совпадения с поиском.
pub fn highlight(pal: &Palette) -> Color32 {
    if pal.dark { Color32::from_rgba_unmultiplied(250, 204, 21, 60) } else { Color32::from_rgba_unmultiplied(250, 204, 21, 110) }
}

/// Пояснение под настройкой мелким текстом.
pub fn hint(ui: &mut Ui, pal: &Palette, text: &str) {
    ui.horizontal(|ui| {
        ui.add_space(LABEL_W + 8.0);
        ui.add(egui::Label::new(RichText::new(text).size(11.5).color(pal.text_faint)).wrap());
    });
}

/// Уникальный id элемента на странице настроек.
pub fn id(page: &str, name: &str) -> Id {
    Id::new(("forma-settings", page, name))
}
