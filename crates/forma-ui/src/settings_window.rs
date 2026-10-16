//! Окно настроек как в IDE от JetBrains: слева поиск и дерево страниц со значками (страница
//! может лежать внутри другой, такие ветки сворачиваются), справа путь к странице, её
//! заголовок и сама страница, внизу «Импорт…», «Экспорт…», «Отмена», «Применить», «ОК».
//!
//! На странице настройки собраны в карточки ([`section`]): подпись слева, элемент справа,
//! строки разделены тонкими линиями.
//!
//! Окно ничего не знает о самих настройках: программа передаёт список страниц и
//! рисует их содержимое. Изменения копятся в черновике программы и применяются
//! кнопками «Применить» и «ОК».

use std::collections::HashSet;

use egui::{Align, Color32, CornerRadius, CursorIcon, FontId, Id, Layout, Margin, Rect, RichText, Sense, Stroke, Ui, UiBuilder, pos2, vec2};
use forma_i18n::t;

use crate::theme::Palette;

/// Страница настроек.
#[derive(Clone, Debug)]
pub struct PageInfo {
    pub id: String,
    /// Страница, внутри которой эта лежит в дереве; `None` — верхний уровень.
    pub parent: Option<String>,
    pub title: String,
    /// Значок в дереве (имя из [`crate::icons`]); у вложенных страниц его не видно.
    pub icon: String,
    /// Подписи настроек на странице: по ним страница находится поиском.
    pub keywords: Vec<String>,
    /// Место среди соседей в дереве: меньше — выше (см. [`order`]).
    pub order: i32,
    /// Пояснение под заголовком страницы.
    pub description: Option<String>,
}

impl PageInfo {
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self { id: id.into(), parent: None, title: title.into(), icon: "settings".into(), keywords: Vec::new(), order: order::APP, description: None }
    }

    pub fn icon(mut self, icon: &str) -> Self {
        icon.clone_into(&mut self.icon);
        self
    }

    /// Вложенная страница внутри `parent`.
    pub fn under(mut self, parent: impl Into<String>) -> Self {
        self.parent = Some(parent.into());
        self
    }

    pub fn order(mut self, order: i32) -> Self {
        self.order = order;
        self
    }

    pub fn keywords(mut self, keywords: Vec<String>) -> Self {
        self.keywords = keywords;
        self
    }

    pub fn description(mut self, text: impl Into<String>) -> Self {
        self.description = Some(text.into());
        self
    }

    /// Совпадает ли страница с поиском: по названию или по подписям настроек.
    fn hit(&self, search: &str) -> bool {
        matches(&self.title, search) || self.keywords.iter().any(|k| matches(k, search))
    }
}

/// Места страниц верхнего уровня в дереве: страницы программ встают между страницами
/// платформы, как в IDE от JetBrains.
pub mod order {
    pub const APPEARANCE: i32 = 100;
    pub const KEYMAP: i32 = 200;
    pub const EDITOR: i32 = 300;
    /// Страницы программы, которым не нашлось места точнее.
    pub const APP: i32 = 400;
    pub const TOOLS: i32 = 500;
    pub const PLUGINS: i32 = 900;
}

/// Что нажали внизу окна.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Footer {
    Ok,
    Cancel,
    Apply,
    Export,
    Import,
    /// Значок «Открыть settings.json» над страницей (как у Settings в VS Code).
    OpenJson,
}

#[derive(Debug, Default)]
pub struct SettingsWindow {
    /// Открытая страница.
    pub page: String,
    pub search: String,
    focus_search: bool,
    /// Свёрнутые ветки дерева (id страниц с вложенными).
    collapsed: HashSet<String>,
}

/// Совпадает ли текст с поиском (без учёта регистра). Пустой поиск совпадает со всем.
pub fn matches(text: &str, search: &str) -> bool {
    let search = search.trim();
    search.is_empty() || text.to_lowercase().contains(&search.to_lowercase())
}

/// Ширина дерева страниц в широком окне (в узком она сжимается).
const LEFT_W: f32 = 252.0;
const FOOTER_H: f32 = 54.0;
/// Уже этого окно показывает страницы выпадающим списком над страницей, без дерева слева.
const NARROW_W: f32 = 620.0;
/// Высота строки дерева.
const TREE_ROW_H: f32 = 30.0;
/// Сдвиг вложенной страницы: под название страницы-родителя (стрелка и значок).
const TREE_INDENT: f32 = 24.0;

/// Ветка дерева: страница верхнего уровня и вложенные в неё (в порядке дерева).
struct Branch<'a> {
    page: &'a PageInfo,
    children: Vec<&'a PageInfo>,
}

/// Дерево страниц: два уровня. Страница, чей родитель не найден или сам вложен, — наверху.
fn branches(pages: &[PageInfo]) -> Vec<Branch<'_>> {
    let is_top = |p: &PageInfo| match &p.parent {
        None => true,
        Some(parent) => !pages.iter().any(|q| &q.id == parent && q.parent.is_none()),
    };
    let mut top: Vec<&PageInfo> = pages.iter().filter(|p| is_top(p)).collect();
    top.sort_by_key(|p| p.order);
    top.into_iter()
        .map(|page| {
            let mut children: Vec<&PageInfo> = pages.iter().filter(|p| !is_top(p) && p.parent.as_deref() == Some(page.id.as_str())).collect();
            children.sort_by_key(|p| p.order);
            Branch { page, children }
        })
        .collect()
}

impl SettingsWindow {
    pub fn new(page: impl Into<String>) -> Self {
        Self { page: page.into(), search: String::new(), focus_search: true, collapsed: HashSet::new() }
    }

    /// Рисует окно размером не больше `max` и возвращает нажатую внизу кнопку.
    /// `modified(id)` — есть ли на странице непримённые изменения, `dirty` — хоть где-то;
    /// `json(id)` — показать ли над страницей значок «Открыть settings.json».
    #[allow(clippy::too_many_arguments)]
    pub fn show(&mut self, ui: &mut Ui, pal: &Palette, max: egui::Vec2, pages: &[PageInfo], modified: &dyn Fn(&str) -> bool, dirty: bool, json: &dyn Fn(&str) -> bool, body: &mut dyn FnMut(&mut Ui, &str, &str)) -> Option<Footer> {
        let mut header_action = None;
        let size = vec2(max.x.min(1040.0), max.y.min(720.0)).max(vec2(200.0, 200.0));
        let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
        let narrow = size.x < NARROW_W;
        let left_w = if narrow { 0.0 } else { (size.x * 0.27).clamp(186.0, LEFT_W) };
        let left = Rect::from_min_size(rect.min, vec2(left_w, size.y));
        let right = Rect::from_min_max(pos2(left.right(), rect.top()), rect.max);
        if !narrow {
            let painter = ui.painter();
            painter.rect_filled(left, CornerRadius { nw: 14, sw: 14, ne: 0, se: 0 }, pal.sidebar_bg);
            painter.vline(left.right(), left.y_range(), Stroke::new(1.0, pal.border));
        }

        // Видимые ветки: при поиске — страницы, которые совпали, и их родители.
        let searching = !self.search.trim().is_empty();
        let visible: Vec<Branch> = branches(pages)
            .into_iter()
            .filter_map(|b| {
                if !searching {
                    return Some(b);
                }
                let children: Vec<&PageInfo> = b.children.iter().copied().filter(|c| c.hit(&self.search)).collect();
                (b.page.hit(&self.search) || !children.is_empty()).then_some(Branch { page: b.page, children })
            })
            .collect();
        // Страницы по порядку дерева; при поиске — только совпавшие.
        let flat: Vec<&PageInfo> = visible
            .iter()
            .flat_map(|b| std::iter::once(b.page).filter(|p| !searching || p.hit(&self.search)).chain(b.children.iter().copied()))
            .collect();
        // Открытая страница пропала из поиска — показываем первую найденную.
        if !flat.iter().any(|p| p.id == self.page)
            && let Some(first) = flat.first()
        {
            self.page = first.id.clone();
        }

        let pad = if narrow { vec2(16.0, 14.0) } else { vec2(26.0, 20.0) };
        let mut body_top = right.top() + pad.y;
        if narrow {
            // Сверху: поиск и выбор страницы.
            let top = Rect::from_min_max(rect.min + pad, pos2(rect.right() - pad.x, rect.bottom() - FOOTER_H));
            let mut top_ui = ui.new_child(UiBuilder::new().max_rect(top).layout(Layout::top_down(Align::Min)));
            self.search_field(&mut top_ui);
            top_ui.add_space(8.0);
            let options: Vec<(String, String)> = flat
                .iter()
                .map(|p| {
                    let mark = if modified(&p.id) { "  ●" } else { "" };
                    let parent = p.parent.as_ref().and_then(|id| pages.iter().find(|q| &q.id == id));
                    let path = parent.map_or_else(|| p.title.clone(), |parent| format!("{} › {}", parent.title, p.title));
                    (p.id.clone(), format!("{path}{mark}"))
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
                ui.spacing_mut().item_spacing.y = 2.0;
                if visible.is_empty() {
                    ui.label(RichText::new(t!("settings-nothing-found")).size(12.5).color(pal.text_faint));
                }
                for b in &visible {
                    let has_children = !b.children.is_empty();
                    // При поиске ветки не сворачиваются: иначе найденное было бы не видно.
                    let open = searching || !self.collapsed.contains(&b.page.id);
                    let hidden_modified = !open && b.children.iter().any(|c| modified(&c.id));
                    let row = TreeRow { page: b.page, depth: 0, expander: has_children.then_some(open), selected: b.page.id == self.page, modified: modified(&b.page.id) || hidden_modified };
                    let (resp, toggle) = row.show(ui, pal);
                    if toggle {
                        if open {
                            self.collapsed.insert(b.page.id.clone());
                        } else {
                            self.collapsed.remove(&b.page.id);
                        }
                    } else if resp.clicked() {
                        self.page = b.page.id.clone();
                        self.collapsed.remove(&b.page.id);
                    }
                    if open {
                        for c in &b.children {
                            let row = TreeRow { page: c, depth: 1, expander: None, selected: c.id == self.page, modified: modified(&c.id) };
                            if row.show(ui, pal).0.clicked() {
                                self.page = c.id.clone();
                            }
                        }
                    }
                }
            });
        }

        // Справа (в узком окне — под выбором страницы): путь к странице, страница, кнопки.
        let body_rect = Rect::from_min_max(pos2(right.left() + pad.x, body_top), pos2(right.right() - pad.x + 6.0, right.bottom() - FOOTER_H));
        let mut right_ui = ui.new_child(UiBuilder::new().max_rect(body_rect).layout(Layout::top_down(Align::Min)));
        right_ui.set_clip_rect(body_rect.intersect(ui.clip_rect()));
        if let Some(page) = pages.iter().find(|p| p.id == self.page) {
            let parent = page.parent.as_ref().and_then(|id| pages.iter().find(|q| &q.id == id));
            if !narrow {
                right_ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    // Путь: родитель (щелчок — к нему) и сама страница.
                    if let Some(parent) = parent {
                        let crumb = ui.add(egui::Label::new(RichText::new(&parent.title).size(14.5).color(pal.text_faint)).sense(Sense::click()));
                        if crumb.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                            self.page = parent.id.clone();
                        }
                        ui.label(RichText::new("›").size(14.5).color(pal.text_faint));
                    }
                    ui.label(RichText::new(&page.title).size(17.0).family(crate::fonts::semibold()).color(pal.heading));
                    if json(&page.id) {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if crate::icons::button(ui, "file-json", false).on_hover_text(t!("settings-open-json")).clicked() {
                                header_action = Some(Footer::OpenJson);
                            }
                        });
                    }
                });
            }
            if let Some(text) = &page.description {
                right_ui.add_space(2.0);
                right_ui.add(egui::Label::new(RichText::new(text).size(12.5).color(pal.text_muted)).wrap());
            }
            right_ui.add_space(14.0);
            let search = self.search.clone();
            let id = page.id.clone();
            egui::ScrollArea::vertical().id_salt(("settings-page", &id)).auto_shrink([false, false]).show(&mut right_ui, |ui| {
                ui.set_width(ui.available_width() - 10.0);
                body(ui, &id, &search);
                ui.add_space(8.0);
            });
        }

        let footer = Rect::from_min_max(pos2(right.left(), right.bottom() - FOOTER_H), right.max);
        ui.painter().hline(footer.x_range(), footer.top(), Stroke::new(1.0, pal.border));
        footer_buttons(ui, pal, footer.shrink2(vec2(if narrow { 14.0 } else { 20.0 }, 11.0)), dirty).or(header_action)
    }

    fn search_field(&mut self, ui: &mut Ui) {
        let search = crate::widgets::search_field(ui, Id::new("forma-settings-search"), &mut self.search, &t!("settings-search-hint"), f32::INFINITY, 13.5);
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
    let more_w = ui.spacing().interact_size.y + gap + 8.0;
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
        let more = crate::icons::button(&mut footer_ui, "more", false).on_hover_text(format!("{import} · {export}"));
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

/// Строка дерева страниц.
struct TreeRow<'a> {
    page: &'a PageInfo,
    depth: usize,
    /// Есть вложенные: `Some(раскрыта ли)`.
    expander: Option<bool>,
    selected: bool,
    /// Непримённые изменения (у свёрнутой ветки — и во вложенных): точка справа.
    modified: bool,
}

impl TreeRow<'_> {
    /// Ответ строки и нажали ли стрелку ветки.
    fn show(&self, ui: &mut Ui, pal: &Palette) -> (egui::Response, bool) {
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), TREE_ROW_H), Sense::click());
        let painter = ui.painter();
        if self.selected {
            painter.rect_filled(rect, CornerRadius::same(7), pal.list_selected);
        } else if resp.hovered() {
            painter.rect_filled(rect, CornerRadius::same(7), pal.list_hover);
        }
        let cy = rect.center().y;
        let mut x = rect.left() + 4.0 + self.depth as f32 * TREE_INDENT;
        let arrow = Rect::from_center_size(pos2(x + 8.0, cy), vec2(14.0, 14.0));
        let on_arrow = resp.hover_pos().is_some_and(|p| arrow.expand2(vec2(4.0, 8.0)).contains(p));
        if let Some(open) = self.expander {
            let color = if on_arrow { pal.text } else { pal.text_faint };
            crate::icons::paint(painter, arrow, if open { "chevron-down" } else { "chevron-right" }, color);
        }
        x += 18.0;
        if self.depth == 0 {
            let color = if self.selected { pal.accent } else { pal.text_muted };
            crate::icons::paint(painter, Rect::from_center_size(pos2(x + 8.0, cy), vec2(16.0, 16.0)), &self.page.icon, color);
            x += 24.0;
        }
        let color = if self.selected { pal.heading } else { pal.text };
        let room = rect.right() - x - if self.modified { 22.0 } else { 6.0 };
        let galley = crate::widgets::elide(ui, self.page.title.as_str(), FontId::proportional(13.5), color, room);
        painter.galley(pos2(x, cy - galley.size().y / 2.0), galley, color);
        if self.modified {
            painter.circle_filled(pos2(rect.right() - 12.0, cy), 3.0, pal.accent);
        }
        let toggle = self.expander.is_some() && on_arrow && resp.clicked();
        (resp.on_hover_cursor(CursorIcon::PointingHand), toggle)
    }
}

/// Главная кнопка окна: залита акцентом.
pub fn primary_button(ui: &mut Ui, pal: &Palette, text: &str) -> egui::Response {
    ui.add(egui::Button::new(RichText::new(text).color(pal.button_primary_text)).fill(pal.button_primary).min_size(vec2(96.0, 30.0)))
}

/// Заголовок группы на странице без карточек (длинные списки: сочетания клавиш, ключи темы).
pub fn group(ui: &mut Ui, pal: &Palette, title: &str) {
    ui.add_space(6.0);
    ui.label(RichText::new(title).size(13.0).family(crate::fonts::semibold()).color(pal.heading));
    ui.add_space(4.0);
}

/// Где карточка хранит, сколько в ней уже строк: строки отделяются линиями.
fn rows_key(ui: &Ui) -> Id {
    ui.id().with("forma-settings-card-rows")
}

/// Внутри ли карточки строка: номер строки в карточке (и следующей строке — следующий).
fn card_row(ui: &Ui) -> Option<usize> {
    let key = rows_key(ui);
    let n = ui.data(|d| d.get_temp::<usize>(key))?;
    ui.data_mut(|d| d.insert_temp(key, n + 1));
    Some(n)
}

/// Группа настроек — карточка с заголовком над ней (`title` может быть пустым). Строки
/// ([`row`], [`check`]) внутри неё разделены тонкими линиями.
pub fn section<R>(ui: &mut Ui, pal: &Palette, title: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    if !title.is_empty() {
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            ui.label(RichText::new(title).size(13.0).family(crate::fonts::semibold()).color(pal.text_muted));
        });
        ui.add_space(5.0);
    }
    let frame = egui::Frame::new().fill(pal.settings_card).stroke(Stroke::new(1.0, pal.border)).corner_radius(10).inner_margin(Margin::symmetric(14, 4));
    let out = frame
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let key = rows_key(ui);
            ui.data_mut(|d| d.insert_temp(key, 0usize));
            let out = add(ui);
            ui.data_mut(|d| d.remove::<usize>(key));
            out
        })
        .inner;
    ui.add_space(18.0);
    out
}

/// Линия между строками карточки.
fn divider(ui: &mut Ui, pal: &Palette) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().hline(rect.x_range(), rect.center().y, Stroke::new(1.0, pal.settings_divider));
}

/// Отступы строки: в карточке — с линией сверху (кроме первой) и воздухом вокруг.
fn row_start(ui: &mut Ui, pal: &Palette) -> bool {
    match card_row(ui) {
        Some(n) => {
            if n > 0 {
                divider(ui, pal);
            }
            ui.add_space(6.0);
            true
        }
        None => false,
    }
}

fn row_end(ui: &mut Ui, card: bool) {
    ui.add_space(if card { 6.0 } else { 2.0 });
}

/// Наибольшая ширина подписи в строке настройки.
pub const LABEL_W: f32 = 230.0;
/// Уже этого подпись настройки стоит над элементом, а не слева от него.
const STACK_BELOW: f32 = 440.0;

/// Ширина колонки подписей на странице шириной `width`; `None` — подписи над элементами.
fn label_width(width: f32) -> Option<f32> {
    (width >= STACK_BELOW).then(|| (width * 0.4).clamp(150.0, LABEL_W))
}

/// Строка настройки: подпись слева, элемент справа (на узкой странице — подпись сверху).
/// Подпись, совпавшая с поиском, подсвечивается. Если значение отличается от стандартного
/// (`modified`), рядом появляется «↺»; возвращает `true`, когда по ней щёлкнули.
pub fn row(ui: &mut Ui, pal: &Palette, search: &str, label: &str, modified: bool, add: impl FnOnce(&mut Ui)) -> bool {
    let card = row_start(ui, pal);
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
                reset = crate::icons::flat_button(ui, "reset", pal.text_muted).on_hover_text(t!("settings-reset")).clicked();
            }
        });
        reset
    };
    match label_width(ui.available_width()) {
        Some(w) => {
            ui.horizontal(|ui| {
                paint_label(ui, w, 28.0);
                reset = controls(ui);
            });
        }
        None => {
            paint_label(ui, ui.available_width(), 18.0);
            reset = controls(ui);
        }
    }
    row_end(ui, card);
    reset
}

/// Настройка-галочка: подпись справа от галочки, совпадение с поиском подсвечено.
/// Возвращает `true`, когда нажали «↺» (значение отличается от стандартного — `modified`).
pub fn check(ui: &mut Ui, pal: &Palette, search: &str, label: &str, value: &mut bool, modified: bool) -> bool {
    let card = row_start(ui, pal);
    let mut reset = false;
    ui.horizontal(|ui| {
        // Длинная подпись переносится, оставляя место для «↺».
        let room = ui.available_width() - if modified { 28.0 } else { 0.0 };
        let resp = ui
            .scope(|ui| {
                ui.set_max_width(room);
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                crate::widgets::checkbox(ui, pal, value, RichText::new(label).color(pal.text))
            })
            .inner;
        if !search.trim().is_empty() && matches(label, search) {
            let r = resp.rect;
            ui.painter().rect_filled(r.expand2(vec2(3.0, 1.0)), CornerRadius::same(4), highlight(pal));
        }
        if modified {
            reset = crate::icons::flat_button(ui, "reset", pal.text_muted).on_hover_text(t!("settings-reset")).clicked();
        }
    });
    row_end(ui, card);
    reset
}

/// Подсветка совпадения с поиском.
pub fn highlight(pal: &Palette) -> Color32 {
    pal.highlight
}

/// Отступ слева, с которого на странице шириной `width` начинаются элементы настроек.
pub fn label_indent(width: f32) -> f32 {
    label_width(width).map_or(0.0, |w| w + 8.0)
}

/// Предупреждение под настройкой: значок и мелкий текст.
pub fn warning(ui: &mut Ui, pal: &Palette, text: &str) {
    let indent = label_width(ui.available_width()).map_or(0.0, |w| w + ui.spacing().item_spacing.x);
    ui.horizontal(|ui| {
        ui.add_space(indent);
        crate::widgets::icon_label(ui, "warning", text, 11.5, pal.warning);
    });
    ui.add_space(4.0);
}

/// Пояснение под настройкой мелким текстом.
pub fn hint(ui: &mut Ui, pal: &Palette, text: &str) {
    let indent = label_width(ui.available_width()).map_or(0.0, |w| w + ui.spacing().item_spacing.x);
    ui.horizontal(|ui| {
        ui.add_space(indent);
        ui.add(egui::Label::new(RichText::new(text).size(11.5).color(pal.text_faint)).wrap());
    });
    ui.add_space(4.0);
}

/// Уникальный id элемента на странице настроек.
pub fn id(page: &str, name: &str) -> Id {
    Id::new(("forma-settings", page, name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_orders_pages_and_nests_two_levels() {
        let pages = vec![
            PageInfo::new("plugins", "Плагины").order(order::PLUGINS),
            PageInfo::new("colors", "Цвета консоли").under("run"),
            PageInfo::new("appearance", "Внешний вид").order(order::APPEARANCE),
            PageInfo::new("run", "Запуск и терминал").order(order::TOOLS),
            PageInfo::new("fonts", "Шрифты").under("appearance"),
            // Родителя нет — страница встаёт наверх, а не теряется.
            PageInfo::new("lost", "Потерянная").under("nowhere").order(order::APP),
            // Внутри вложенной — тоже наверх: уровней только два.
            PageInfo::new("deep", "Глубокая").under("fonts").order(order::APP),
        ];
        let tree: Vec<(&str, Vec<&str>)> = branches(&pages).iter().map(|b| (b.page.id.as_str(), b.children.iter().map(|c| c.id.as_str()).collect())).collect();
        assert_eq!(tree, vec![("appearance", vec!["fonts"]), ("lost", vec![]), ("deep", vec![]), ("run", vec!["colors"]), ("plugins", vec![])]);
    }

    #[test]
    fn pages_are_found_by_title_and_keywords() {
        let page = PageInfo::new("run", "Запуск и терминал").keywords(vec!["Высота строки".into()]);
        assert!(page.hit("ТЕРМИНАЛ"));
        assert!(page.hit("высота"));
        assert!(!page.hit("плагин"));
    }
}
