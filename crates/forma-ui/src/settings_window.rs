//! Окно настроек как в Rider: слева поиск и дерево разделов, справа страница,
//! внизу «Импорт…», «Экспорт…», «Отмена», «Применить», «ОК».
//!
//! Окно ничего не знает о самих настройках: программа передаёт список страниц и
//! рисует их содержимое. Изменения копятся в черновике программы и применяются
//! кнопками «Применить» и «ОК».

use egui::{Align, Color32, CornerRadius, CursorIcon, FontId, Id, Layout, Margin, Rect, RichText, Sense, Stroke, TextEdit, Ui, UiBuilder, pos2, vec2};
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

/// Ширина дерева страниц в широком окне (в узком она сжимается).
const LEFT_W: f32 = 236.0;
const FOOTER_H: f32 = 54.0;
/// Уже этого окно показывает страницы выпадающим списком над страницей, без дерева слева.
const NARROW_W: f32 = 620.0;

impl SettingsWindow {
    pub fn new(page: impl Into<String>) -> Self {
        Self { page: page.into(), search: String::new(), focus_search: true }
    }

    /// Рисует окно размером не больше `max` и возвращает нажатую внизу кнопку.
    /// `modified(id)` — есть ли на странице непримённые изменения, `dirty` — хоть где-то.
    #[allow(clippy::too_many_arguments)]
    pub fn show(&mut self, ui: &mut Ui, pal: &Palette, max: egui::Vec2, pages: &[PageInfo], modified: &dyn Fn(&str) -> bool, dirty: bool, body: &mut dyn FnMut(&mut Ui, &str, &str)) -> Option<Footer> {
        let size = vec2(max.x.min(940.0), max.y.min(640.0)).max(vec2(200.0, 200.0));
        let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
        let narrow = size.x < NARROW_W;
        let left_w = if narrow { 0.0 } else { (size.x * 0.27).clamp(176.0, LEFT_W) };
        let left = Rect::from_min_size(rect.min, vec2(left_w, size.y));
        let right = Rect::from_min_max(pos2(left.right(), rect.top()), rect.max);
        if !narrow {
            let painter = ui.painter();
            painter.rect_filled(left, CornerRadius { nw: 14, sw: 14, ne: 0, se: 0 }, pal.sidebar_bg);
            painter.vline(left.right(), left.y_range(), Stroke::new(1.0, pal.border));
        }

        let visible: Vec<&PageInfo> = pages.iter().filter(|p| matches(&p.title, &self.search) || p.keywords.iter().any(|k| matches(k, &self.search))).collect();
        // Открытая страница пропала из поиска — показываем первую найденную.
        if !visible.iter().any(|p| p.id == self.page)
            && let Some(first) = visible.first()
        {
            self.page = first.id.clone();
        }

        let pad = if narrow { vec2(16.0, 14.0) } else { vec2(24.0, 18.0) };
        let mut body_top = right.top() + pad.y;
        if narrow {
            // Сверху: поиск и выбор страницы.
            let top = Rect::from_min_max(rect.min + pad, pos2(rect.right() - pad.x, rect.bottom() - FOOTER_H));
            let mut top_ui = ui.new_child(UiBuilder::new().max_rect(top).layout(Layout::top_down(Align::Min)));
            self.search_field(&mut top_ui);
            top_ui.add_space(8.0);
            let options: Vec<(String, String)> = visible
                .iter()
                .map(|p| {
                    let mark = if modified(&p.id) { "  ●" } else { "" };
                    (p.id.clone(), format!("{} › {}{mark}", p.section, p.title))
                })
                .collect();
            if options.is_empty() {
                top_ui.label(RichText::new(t!("settings-nothing-found")).size(12.5).color(pal.text_faint));
            } else if let Some(page) = crate::widgets::dropdown(&mut top_ui, pal, Id::new("forma-settings-page-pick"), f32::INFINITY, &self.page, &options) {
                self.page = page;
            }
            body_top = top_ui.min_rect().bottom() + 14.0;
        } else {
            // Слева: поиск и дерево.
            let mut left_ui = ui.new_child(UiBuilder::new().max_rect(left.shrink2(vec2(12.0, 14.0))).layout(Layout::top_down(Align::Min)));
            self.search_field(&mut left_ui);
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
                        ui.add(egui::Label::new(RichText::new(&page.section).size(11.5).color(pal.text_faint)).truncate());
                        ui.add_space(2.0);
                        section = Some(&page.section);
                    }
                    if tree_item(ui, pal, &page.title, page.id == self.page, modified(&page.id)).clicked() {
                        self.page = page.id.clone();
                    }
                }
            });
        }

        // Справа (в узком окне — под выбором страницы): путь к странице, страница, кнопки.
        let body_rect = Rect::from_min_max(pos2(right.left() + pad.x, body_top), pos2(right.right() - pad.x + 4.0, right.bottom() - FOOTER_H));
        let mut right_ui = ui.new_child(UiBuilder::new().max_rect(body_rect).layout(Layout::top_down(Align::Min)));
        right_ui.set_clip_rect(body_rect.intersect(ui.clip_rect()));
        if let Some(page) = pages.iter().find(|p| p.id == self.page) {
            if !narrow {
                right_ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(&page.section).size(13.0).color(pal.text_faint));
                    ui.label(RichText::new("›").size(13.0).color(pal.text_faint));
                    ui.label(RichText::new(&page.title).size(15.0).family(crate::fonts::semibold()).color(pal.heading));
                });
                right_ui.add_space(12.0);
            }
            let search = self.search.clone();
            egui::ScrollArea::vertical().id_salt(("settings-page", &page.id)).auto_shrink([false, false]).show(&mut right_ui, |ui| {
                ui.set_width(ui.available_width() - 8.0);
                body(ui, &page.id, &search);
            });
        }

        let footer = Rect::from_min_max(pos2(right.left(), right.bottom() - FOOTER_H), right.max);
        ui.painter().hline(footer.x_range(), footer.top(), Stroke::new(1.0, pal.border));
        footer_buttons(ui, pal, footer.shrink2(vec2(if narrow { 14.0 } else { 20.0 }, 11.0)), dirty)
    }

    fn search_field(&mut self, ui: &mut Ui) {
        let search = ui.add(TextEdit::singleline(&mut self.search).hint_text(format!("🔍 {}", t!("settings-search-hint"))).desired_width(f32::INFINITY).margin(Margin::symmetric(8, 6)));
        if std::mem::take(&mut self.focus_search) {
            search.request_focus();
        }
    }
}

/// Кнопки внизу окна. Когда места мало, «Импорт» и «Экспорт» прячутся в меню «⋯»,
/// а если не хватает и так — остальные кнопки теряют наименьшую ширину.
fn footer_buttons(ui: &mut Ui, pal: &Palette, rect: Rect, dirty: bool) -> Option<Footer> {
    let (import, export) = (t!("settings-import"), t!("settings-export"));
    let (ok, apply, cancel) = (t!("settings-ok"), t!("settings-apply"), t!("settings-cancel"));
    let gap = ui.spacing().item_spacing.x;
    let bw = crate::widgets::button_width;
    let main_w = bw(ui, &ok, 96.0) + bw(ui, &apply, 84.0) + bw(ui, &cancel, 84.0) + gap * 2.0;
    let extra_w = bw(ui, &import, 0.0) + bw(ui, &export, 0.0) + gap * 2.0 + 16.0;
    let more_w = bw(ui, "⋯", 0.0) + gap + 8.0;
    let roomy = main_w + extra_w <= rect.width();
    let tight = main_w + more_w > rect.width();
    let mut out = None;
    let mut footer_ui = ui.new_child(UiBuilder::new().max_rect(rect).layout(Layout::left_to_right(Align::Center)));
    if roomy {
        if footer_ui.button(&import).clicked() {
            out = Some(Footer::Import);
        }
        if footer_ui.button(&export).clicked() {
            out = Some(Footer::Export);
        }
    } else {
        let more = footer_ui.button("⋯").on_hover_text(format!("{import} · {export}"));
        egui::Popup::menu(&more).style(crate::theme::menu_style).show(|ui| {
            if ui.button(&import).clicked() {
                out = Some(Footer::Import);
            }
            if ui.button(&export).clicked() {
                out = Some(Footer::Export);
            }
        });
    }
    footer_ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        let min = |w: f32| vec2(if tight { 0.0 } else { w }, 30.0);
        let ok = ui.add(egui::Button::new(RichText::new(ok).color(pal.button_primary_text)).fill(pal.button_primary).min_size(min(96.0)));
        if ok.clicked() {
            out = Some(Footer::Ok);
        }
        if ui.add_enabled(dirty, egui::Button::new(apply).min_size(min(84.0))).clicked() {
            out = Some(Footer::Apply);
        }
        if ui.add(egui::Button::new(cancel).min_size(min(84.0))).clicked() {
            out = Some(Footer::Cancel);
        }
    });
    out
}

/// Пункт дерева страниц. Страница с непримёнными изменениями помечена точкой.
fn tree_item(ui: &mut Ui, pal: &Palette, title: &str, selected: bool, modified: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::click());
    let painter = ui.painter();
    if selected {
        painter.rect_filled(rect, CornerRadius::same(7), pal.list_selected);
    } else if resp.hovered() {
        painter.rect_filled(rect, CornerRadius::same(7), pal.list_hover);
    }
    let color = if selected { pal.heading } else { pal.text };
    let galley = crate::widgets::elide(ui, title, FontId::proportional(13.5), color, rect.width() - 12.0 - if modified { 22.0 } else { 6.0 });
    painter.galley(pos2(rect.left() + 12.0, rect.center().y - galley.size().y / 2.0), galley, color);
    if modified {
        painter.circle_filled(pos2(rect.right() - 12.0, rect.center().y), 3.0, pal.accent);
    }
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// Главная кнопка окна: залита акцентом.
pub fn primary_button(ui: &mut Ui, pal: &Palette, text: &str) -> egui::Response {
    ui.add(egui::Button::new(RichText::new(text).color(pal.button_primary_text)).fill(pal.button_primary).min_size(vec2(96.0, 30.0)))
}

/// Заголовок группы настроек на странице.
pub fn group(ui: &mut Ui, pal: &Palette, title: &str) {
    ui.add_space(6.0);
    ui.label(RichText::new(title).size(13.0).family(crate::fonts::semibold()).color(pal.heading));
    ui.add_space(4.0);
}

/// Наибольшая ширина подписи в строке настройки.
pub const LABEL_W: f32 = 210.0;
/// Уже этого подпись настройки стоит над элементом, а не слева от него.
const STACK_BELOW: f32 = 440.0;

/// Ширина колонки подписей на странице шириной `width`; `None` — подписи над элементами.
fn label_width(width: f32) -> Option<f32> {
    (width >= STACK_BELOW).then(|| (width * 0.38).clamp(150.0, LABEL_W))
}

/// Строка настройки: подпись слева, элемент справа (на узкой странице — подпись сверху).
/// Подпись, совпавшая с поиском, подсвечивается. Если значение отличается от стандартного
/// (`modified`), рядом появляется «↺»; возвращает `true`, когда по ней щёлкнули.
pub fn row(ui: &mut Ui, pal: &Palette, search: &str, label: &str, modified: bool, add: impl FnOnce(&mut Ui)) -> bool {
    let mut reset = false;
    let hit = !search.trim().is_empty() && matches(label, search);
    let paint_label = |ui: &mut Ui, width: f32, min_h: f32| {
        let galley = ui.painter().layout(label.to_owned(), FontId::proportional(13.5), pal.text, (width - 8.0).max(40.0));
        let (rect, _) = ui.allocate_exact_size(vec2(width, galley.size().y.max(min_h)), Sense::hover());
        let pos = pos2(rect.left(), rect.center().y - galley.size().y / 2.0);
        if hit {
            ui.painter().rect_filled(Rect::from_min_size(pos, galley.size()).expand2(vec2(4.0, 1.0)), CornerRadius::same(4), highlight(pal));
        }
        ui.painter().galley(pos, galley, pal.text);
    };
    // Элементы переносятся, если им мало места справа от подписи.
    let controls = |ui: &mut Ui| {
        let mut reset = false;
        ui.horizontal_wrapped(|ui| {
            // Справа остаётся место для «↺», чтобы он не падал на отдельную строку.
            let room = ui.available_width() - if modified { 28.0 } else { 0.0 };
            ui.scope(|ui| {
                ui.set_max_width(room);
                add(ui);
            });
            if modified {
                reset = ui.add(egui::Button::new(RichText::new("↺").color(pal.text_muted)).frame(false)).on_hover_text(t!("settings-reset")).clicked();
            }
        });
        reset
    };
    match label_width(ui.available_width()) {
        Some(w) => {
            ui.horizontal(|ui| {
                paint_label(ui, w, 26.0);
                reset = controls(ui);
            });
        }
        None => {
            paint_label(ui, ui.available_width(), 18.0);
            reset = controls(ui);
        }
    }
    ui.add_space(2.0);
    reset
}

/// Настройка-галочка: подпись справа от галочки, совпадение с поиском подсвечено.
/// Возвращает `true`, когда нажали «↺» (значение отличается от стандартного — `modified`).
pub fn check(ui: &mut Ui, pal: &Palette, search: &str, label: &str, value: &mut bool, modified: bool) -> bool {
    let mut reset = false;
    ui.horizontal(|ui| {
        // Длинная подпись переносится, оставляя место для «↺».
        let room = ui.available_width() - if modified { 28.0 } else { 0.0 };
        let resp = ui
            .scope(|ui| {
                ui.set_max_width(room);
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                ui.checkbox(value, RichText::new(label).color(pal.text))
            })
            .inner;
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
    pal.highlight
}

/// Пояснение под настройкой мелким текстом.
pub fn hint(ui: &mut Ui, pal: &Palette, text: &str) {
    let indent = label_width(ui.available_width()).map_or(0.0, |w| w + ui.spacing().item_spacing.x);
    ui.horizontal(|ui| {
        ui.add_space(indent);
        ui.add(egui::Label::new(RichText::new(text).size(11.5).color(pal.text_faint)).wrap());
    });
}

/// Уникальный id элемента на странице настроек.
pub fn id(page: &str, name: &str) -> Id {
    Id::new(("forma-settings", page, name))
}
