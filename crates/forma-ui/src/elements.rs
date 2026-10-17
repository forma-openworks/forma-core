//! Дерево элементов плагина ([`forma_plugin_ui::Element`]) — виджетами Forma и цветами
//! темы, как свои панели программы. То, что программа умеет рисовать лучше (markdown —
//! renderer заметок Holotype, code — подсветка, картинки хранилища), она подставляет через
//! [`Hooks`].
//!
//! Что пользователь сделал с элементами, [`show`] возвращает списком [`Output`]: события
//! для плагина и команды от кнопок.

use std::collections::HashMap;

use egui::text::{CCursor, LayoutJob};
use egui::{Align, Color32, CornerRadius, CursorIcon, FontId, Frame, Id, Layout, Margin, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use forma_i18n::t;
use forma_plugin_ui::{Blank, Chip, Element, Event, EventKind, Item, Mark, Marker, Size, Span, Tab, Tone};
use serde_json::Value;

use crate::theme::Palette;

/// Что программа рисует сама.
pub trait Hooks {
    /// Markdown (у Holotype — как заметка).
    fn markdown(&mut self, ui: &mut Ui, pal: &Palette, text: &str) {
        ui.add(egui::Label::new(RichText::new(text).size(13.5).color(pal.text)).wrap());
    }

    /// Code с подсветкой.
    fn code(&mut self, ui: &mut Ui, pal: &Palette, text: &str, _lang: &str) {
        Frame::new().fill(pal.bg_deep).stroke(Stroke::new(1.0, pal.border)).corner_radius(6).inner_margin(Margin::same(8)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.add(egui::Label::new(RichText::new(text).font(FontId::monospace(12.5)).color(pal.text)).wrap());
        });
    }

    /// Картинка из хранилища или проекта (путь от корня).
    fn image(&mut self, ui: &mut Ui, pal: &Palette, _path: &str, _width: Option<f32>, alt: &str) {
        ui.label(RichText::new(alt).size(12.0).color(pal.text_faint));
    }

    /// Адрес картинки хранилища для загрузчиков egui (`file:///…`), если она есть: для
    /// картинок на холсте и сравнения «до и после».
    fn image_uri(&self, _path: &str) -> Option<String> {
        None
    }

    /// Подсвеченный code одной раскладкой, без рамки: для code с пропусками и местами, на
    /// которые нажимают, и строк, которые переставляют (API 1.3). Без подсветки — моноширинным
    /// цветом текста.
    fn code_job(&self, _ctx: &egui::Context, pal: &Palette, text: &str, _lang: &str, size: f32) -> LayoutJob {
        LayoutJob::simple(text.to_owned(), FontId::monospace(size), pal.text, f32::INFINITY)
    }
}

/// Без особых возможностей: всё рисуется простыми подписями.
pub struct Plain;

impl Hooks for Plain {}

/// Что пользователь сделал.
#[derive(Clone, Debug, PartialEq)]
pub enum Output {
    /// Событие элемента — для плагина.
    Event(Event),
    /// Кнопка с командой: запустить команду плагина.
    Command { command: String, args: Value },
}

/// Рисует дерево `root`. `id` — место, где элементы помнят своё состояние (текст поля,
/// открыта ли группа): у одного блока или панели он должен быть постоянным.
pub fn show(ui: &mut Ui, pal: &Palette, id: Id, root: &Element, hooks: &mut dyn Hooks) -> Vec<Output> {
    let mut d = Draw { pal, hooks, base: id, out: Vec::new(), groups: HashMap::new(), in_grid: false };
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
        d.element(ui, root, id);
    });
    d.out
}

/// Стандартный зазор между элементами столбца и строки.
const GAP: f32 = 6.0;

struct Draw<'a> {
    pal: &'a Palette,
    hooks: &'a mut dyn Hooks,
    base: Id,
    out: Vec<Output>,
    /// Сколько групп с таким заголовком уже нарисовано в этом кадре.
    groups: HashMap<String, usize>,
    /// Рисуется ячейка [`Element::Grid`]: поля ввода — узкие, а не во всю ширину.
    in_grid: bool,
}

/// Кегль code в элементах задач.
const CODE: f32 = 13.0;
/// Ширина поля ввода в ячейке таблицы.
const GRID_INPUT: f32 = 96.0;

/// Шапка блока: значок, название, плашка и текст справа — строкой во всю ширину. Программа
/// с блоками в рамке (Holotype) рисует ею шапку рамки, когда блок начинается с
/// [`Element::Header`].
pub fn header_row(ui: &mut Ui, pal: &Palette, title: &str, icon: Option<&str>, badge_text: &str, aside: &str) {
    ui.allocate_ui_with_layout(vec2(ui.available_width(), 22.0), Layout::left_to_right(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 7.0;
        if let Some(icon) = icon {
            let (r, _) = ui.allocate_exact_size(vec2(15.0, 15.0), Sense::hover());
            crate::icons::paint(ui.painter(), r, icon, pal.accent);
        }
        ui.label(RichText::new(title).size(13.5).family(crate::fonts::semibold()).color(pal.heading));
        if !badge_text.is_empty() {
            badge(ui, pal, badge_text, Tone::Normal);
        }
        if !aside.is_empty() {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new(aside).size(12.5).color(pal.text_muted));
            });
        }
    });
}

/// Цвет оттенка для рамок и заливок итога; обычный — ничего.
fn tone_accent(pal: &Palette, tone: Tone) -> Option<Color32> {
    match tone {
        Tone::Success => Some(pal.success),
        Tone::Danger => Some(pal.danger),
        Tone::Warning => Some(pal.warning),
        Tone::Accent => Some(pal.accent),
        Tone::Normal | Tone::Muted | Tone::Faint => None,
    }
}

/// Число для людей: с запятой по-русски.
fn format_number(v: f64, decimals: u8) -> String {
    let s = format!("{v:.*}", decimals as usize);
    if forma_i18n::language().starts_with("ru") { s.replace('.', ",") } else { s }
}

/// Code с пропусками `{{id}}`: текст, где пропуски заменены пробелами своей ширины, и
/// пропуски — номер в `blanks`, первый символ и длина (в символах).
fn blank_layout(text: &str, blanks: &[Blank]) -> (String, Vec<(usize, usize, usize)>) {
    let mut out = String::with_capacity(text.len());
    let mut holes = Vec::new();
    let mut chars = 0;
    let mut rest = text;
    while let Some(open) = rest.find("{{") {
        let before = &rest[..open];
        out.push_str(before);
        chars += before.chars().count();
        let after = &rest[open + 2..];
        let found = after.find("}}").and_then(|close| blanks.iter().position(|b| b.id == after[..close]).map(|i| (i, close)));
        match found {
            Some((i, close)) => {
                let w = blanks[i].width.clamp(1, 80) as usize;
                out.push_str(&" ".repeat(w));
                holes.push((i, chars, w));
                chars += w;
                rest = &after[close + 2..];
            }
            None => {
                out.push_str("{{");
                chars += 2;
                rest = after;
            }
        }
    }
    out.push_str(rest);
    (out, holes)
}

/// Где группа с заголовком `title` (`n`-я с таким заголовком) помнит, свёрнута ли она. Не
/// по месту в дереве, а по заголовку, как панели Blender по `bl_idname`: плагин добавил
/// строку выше — свёрнутая группа остаётся свёрнутой.
fn group_state(base: Id, title: &str, n: usize) -> Id {
    base.with(("group", title, n))
}

/// Ширина текста одной строкой.
fn text_width(ui: &Ui, text: &RichText) -> f32 {
    egui::WidgetText::from(text.clone()).into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, egui::TextStyle::Body).size().x
}

/// Место для элемента строки шириной `want`: не влез в остаток строки — целиком уходит на
/// следующую; текст переносится по словам, только если элемент шире всей строки (иначе
/// egui переносит его по оставшейся ширине — вплоть до буквы в строке, а элементы в рамке
/// ставит у курсора, и они вылезают за край). Остаток строки — `available_size_before_wrap`:
/// `available_width` в строке с переносом — ширина всей строки.
fn fit_row(ui: &mut Ui, want: f32) -> egui::TextWrapMode {
    if want > ui.available_size_before_wrap().x && ui.cursor().min.x > ui.max_rect().min.x + 0.5 {
        ui.end_row();
    }
    if want > ui.available_size_before_wrap().x { egui::TextWrapMode::Wrap } else { egui::TextWrapMode::Extend }
}

fn tone_color(pal: &Palette, tone: Tone) -> Color32 {
    match tone {
        Tone::Normal => pal.text,
        Tone::Muted => pal.text_muted,
        Tone::Faint => pal.text_faint,
        Tone::Accent => pal.accent,
        Tone::Success => pal.success,
        Tone::Warning => pal.warning,
        Tone::Danger => pal.danger,
    }
}

fn text_size(size: Size) -> f32 {
    match size {
        Size::Small => 12.0,
        Size::Normal => 13.5,
        Size::Large => 16.0,
    }
}

impl Draw<'_> {
    /// Место состояния элемента: по его `id`, а у безымянных — по пути в дереве.
    fn state_id(&self, path: Id, id: &str) -> Id {
        if id.is_empty() { path } else { self.base.with(("el", id)) }
    }

    fn event(&mut self, id: &str, kind: EventKind, value: Value) {
        if !id.is_empty() {
            self.out.push(Output::Event(Event { id: id.to_owned(), kind, value }));
        }
    }

    fn children(&mut self, ui: &mut Ui, children: &[Element], path: Id) {
        for (i, c) in children.iter().enumerate() {
            self.element(ui, c, path.with(i));
        }
    }

    fn element(&mut self, ui: &mut Ui, e: &Element, path: Id) {
        let pal = self.pal;
        match e {
            Element::Column { children, gap } => {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = gap.unwrap_or(GAP);
                    self.children(ui, children, path);
                });
            }
            Element::Row { children, gap } => {
                // Карточки разной высоты — по верхнему краю, как колонки; подписи и плашки —
                // по середине строки.
                let cards = children.iter().any(|c| matches!(c, Element::Group { width: Some(_), .. }));
                if cards {
                    ui.with_layout(egui::Layout::left_to_right(Align::Min).with_main_wrap(true), |ui| {
                        ui.spacing_mut().item_spacing.x = gap.unwrap_or(GAP);
                        self.children(ui, children, path);
                    });
                } else {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = gap.unwrap_or(GAP);
                        self.children(ui, children, path);
                    });
                }
            }
            Element::Group { title, icon, children, collapsible, open, width } => match width {
                // Карточка заданной ширины (в строке встают рядом).
                Some(w) => {
                    let w = w.min(ui.available_width());
                    ui.allocate_ui_with_layout(vec2(w, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.set_width(w);
                        self.group(ui, title, icon.as_deref(), children, *collapsible, *open, path);
                    });
                }
                None => self.group(ui, title, icon.as_deref(), children, *collapsible, *open, path),
            },
            Element::Separator => {
                let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
                ui.painter().hline(rect.x_range(), rect.center().y, Stroke::new(1.0, pal.border));
            }
            Element::Space { size } => ui.add_space(if *size > 0.0 { *size } else { 8.0 }),
            Element::Label { text, icon, tone, size, bold, mono } => {
                let color = tone_color(pal, *tone);
                let size = text_size(*size);
                match icon {
                    Some(icon) if !*mono => {
                        fit_row(ui, size + 6.0 + text_width(ui, &RichText::new(text).size(size)));
                        crate::widgets::icon_label(ui, icon, text.clone(), size, color);
                    }
                    Some(icon) => {
                        // Моноширинная подпись со значком: строка test, вывод программы.
                        let rt = RichText::new(text).font(FontId::monospace(size - 0.5)).color(color);
                        fit_row(ui, size + 6.0 + text_width(ui, &rt));
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 5.0;
                            let (r, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
                            crate::icons::paint(ui.painter(), r, icon, color);
                            ui.add(egui::Label::new(rt).wrap_mode(egui::TextWrapMode::Extend));
                        });
                    }
                    None => {
                        let mut rt = RichText::new(text).size(size).color(color);
                        if *bold {
                            rt = rt.family(crate::fonts::semibold());
                        }
                        if *mono {
                            rt = rt.font(FontId::monospace(size - 0.5));
                        }
                        let wrap = fit_row(ui, text_width(ui, &rt));
                        ui.add(egui::Label::new(rt).wrap_mode(wrap));
                    }
                }
            }
            Element::Heading { text, level } => {
                let (size, family, color) = match level {
                    0 | 1 => (19.0, crate::fonts::bold(), pal.heading),
                    2 => (15.5, crate::fonts::semibold(), pal.heading),
                    _ => (13.0, crate::fonts::semibold(), pal.text_muted),
                };
                ui.add(egui::Label::new(RichText::new(text).size(size).family(family).color(color)).wrap());
            }
            Element::Markdown { text } => {
                ui.vertical(|ui| self.hooks.markdown(ui, pal, text));
            }
            Element::Code { text, lang } => {
                ui.vertical(|ui| self.hooks.code(ui, pal, text, lang));
            }
            Element::Badge { text, tone } => badge(ui, pal, text, *tone),
            Element::Button { id, text, icon, primary, enabled, tooltip, command, args } => {
                let label = RichText::new(text).size(13.0);
                // Прямо в строку, без вложенного Ui, который занял бы её остаток.
                let want = text_width(ui, &label) + 2.0 * ui.spacing().button_padding.x + if icon.is_some() && !primary { 14.0 + ui.spacing().icon_spacing } else { 0.0 };
                let wrap = fit_row(ui, want);
                let button = match (primary, icon) {
                    (true, _) => egui::Button::new(label.color(pal.button_primary_text)).fill(pal.button_primary),
                    (false, Some(icon)) => egui::Button::image_and_text(crate::icons::image(ui.ctx(), icon, 14.0, pal.text_muted), label),
                    (false, None) => egui::Button::new(label),
                };
                let resp = ui.add_enabled(*enabled, button.min_size(vec2(0.0, 28.0)).wrap_mode(wrap));
                let resp = if tooltip.is_empty() { resp } else { resp.on_hover_text(tooltip) };
                if resp.clicked() {
                    match command {
                        Some(c) => self.out.push(Output::Command { command: c.clone(), args: args.clone() }),
                        None => self.event(id, EventKind::Click, Value::Null),
                    }
                }
            }
            Element::TextInput { id, value, placeholder, multiline, search, mono, prefix, suffix, tone } => {
                if *mono || !prefix.is_empty() || !suffix.is_empty() || *tone != Tone::Normal {
                    self.framed_input(ui, id, value, placeholder, *mono, prefix, suffix, *tone, path);
                } else {
                    self.text_input(ui, id, value, placeholder, *multiline, *search, path);
                }
            }
            Element::Checkbox { id, text, value } => {
                let mut v = *value;
                if crate::widgets::checkbox(ui, pal, &mut v, text.clone()).changed() {
                    self.event(id, EventKind::Change, Value::Bool(v));
                }
            }
            Element::Select { id, value, options } => {
                let opts: Vec<(String, String)> = options.iter().map(|o| (o.value.clone(), if o.text.is_empty() { o.value.clone() } else { o.text.clone() })).collect();
                let sid = self.state_id(path, id);
                if let Some(v) = crate::widgets::dropdown(ui, pal, sid, 220.0, value, &opts) {
                    self.event(id, EventKind::Change, Value::String(v));
                }
            }
            Element::Number { id, value, min, max, step, unit } => {
                let sid = self.state_id(path, id);
                let mut v = *value as f32;
                let resp = crate::widgets::number_field(ui, pal, sid, &mut v, *min as f32..=*max as f32, *step as f32, unit, 140.0);
                if resp.changed() {
                    self.event(id, EventKind::Change, serde_json::json!(v as f64));
                }
            }
            Element::List { id, items, selected } => self.list(ui, id, items, selected.as_deref(), path),
            Element::Table { columns, rows } => {
                egui::Grid::new(path.with("table")).striped(true).spacing(vec2(14.0, 6.0)).show(ui, |ui| {
                    if !columns.is_empty() {
                        for c in columns {
                            ui.label(RichText::new(c).size(12.5).family(crate::fonts::semibold()).color(pal.text_muted));
                        }
                        ui.end_row();
                    }
                    for r in rows {
                        for cell in r {
                            ui.label(RichText::new(cell).size(13.0).color(pal.text));
                        }
                        ui.end_row();
                    }
                });
            }
            Element::Progress { value, text } => {
                ui.horizontal(|ui| match value {
                    Some(v) => {
                        let bar = egui::ProgressBar::new(v.clamp(0.0, 1.0)).fill(pal.accent).desired_height(8.0).desired_width((ui.available_width() - 60.0).max(80.0));
                        ui.add(bar);
                        let pct = format!("{:.0} %", v.clamp(0.0, 1.0) * 100.0);
                        ui.label(RichText::new(if text.is_empty() { pct } else { text.clone() }).size(12.0).color(pal.text_muted));
                    }
                    None => {
                        ui.add(egui::Spinner::new().size(14.0).color(pal.accent));
                        if !text.is_empty() {
                            ui.label(RichText::new(text).size(12.5).color(pal.text_muted));
                        }
                    }
                });
            }
            Element::Image { path: file, width, alt } => {
                ui.vertical(|ui| self.hooks.image(ui, pal, file, *width, alt));
            }
            Element::Canvas { width, height, shapes, pan_zoom, background } => {
                crate::canvas::canvas(ui, pal, path, *width, *height, shapes, *pan_zoom, background.as_deref(), self.hooks);
            }
            Element::Compare { before, after, before_label, after_label } => {
                crate::canvas::compare(ui, pal, path, before, after, before_label, after_label, self.hooks);
            }
            Element::View3d { positions, indices, color, height, wireframe } => {
                let color = color.as_deref().map(|c| crate::canvas::color(pal, c));
                crate::view3d::view3d(ui, pal, path, positions, indices, color, *height, *wireframe);
            }
            Element::Header { title, icon, badge, aside } => header_row(ui, pal, title, icon.as_deref(), badge, aside),
            Element::Bar { left, right } => {
                // Стороны рисуются по очереди, но обеим нужен `self`: через RefCell.
                let me = std::cell::RefCell::new(&mut *self);
                egui::containers::Sides::new().shrink_left().show(
                    ui,
                    |ui| {
                        ui.spacing_mut().item_spacing.x = GAP;
                        me.borrow_mut().children(ui, left, path.with("left"));
                    },
                    |ui| {
                        ui.spacing_mut().item_spacing.x = GAP;
                        // Справа налево: первым ставится последний, чтобы порядок читался как в дереве.
                        for (i, c) in right.iter().enumerate().rev() {
                            me.borrow_mut().element(ui, c, path.with(("right", i)));
                        }
                    },
                );
            }
            Element::OptionRow { id, text, marker, mark, selected, tone, enabled } => self.option_row(ui, id, text, *marker, mark, *selected, *tone, *enabled, path),
            Element::Slider { id, value, min, max, step, unit, decimals, marks, enabled } => self.slider(ui, id, *value, *min, *max, *step, unit, *decimals, marks, *enabled, path),
            Element::Tabs { id, tabs, selected } => self.tabs(ui, id, tabs, selected),
            Element::Chips { id, items, enabled } => self.chips(ui, id, items, *enabled),
            Element::CodeBlanks { text, lang, blanks, enabled } => self.code_blanks(ui, text, lang, blanks, *enabled, path),
            Element::CodeSpans { id, text, lang, spans, selected, enabled } => self.code_spans(ui, id, text, lang, spans, selected.as_deref(), *enabled, path),
            Element::Grid { columns, rows } => {
                egui::Grid::new(path.with("grid")).spacing(vec2(16.0, 6.0)).min_col_width(28.0).show(ui, |ui| {
                    if !columns.is_empty() {
                        for c in columns {
                            ui.label(RichText::new(c).size(12.5).family(crate::fonts::semibold()).color(pal.text_muted));
                        }
                        ui.end_row();
                    }
                    let was = std::mem::replace(&mut self.in_grid, true);
                    for (r, row) in rows.iter().enumerate() {
                        for (c, cell) in row.iter().enumerate() {
                            self.element(ui, cell, path.with(("cell", r, c)));
                        }
                        ui.end_row();
                    }
                    self.in_grid = was;
                });
            }
            Element::Reorder { id, items, lang, enabled } => self.reorder(ui, id, items, lang, *enabled),
            Element::Unknown => {
                ui.label(RichText::new(t!("plugin-ui-unknown")).size(12.0).italics().color(pal.text_faint));
            }
        }
    }

    /// Метка варианта: кружок, квадрат с галочкой или квадрат с номером.
    #[allow(clippy::too_many_arguments)]
    fn paint_marker(&self, painter: &egui::Painter, center: egui::Pos2, marker: Marker, mark: &str, on: bool, color: Color32, hovered: bool) {
        let pal = self.pal;
        let off = Stroke::new(1.0, if hovered { pal.text_muted } else { pal.text_faint });
        match marker {
            Marker::Radio => {
                if on {
                    painter.circle_filled(center, 7.5, color);
                    painter.circle_filled(center, 2.9, pal.accent_text);
                } else {
                    painter.circle_stroke(center, 7.0, off);
                }
            }
            Marker::Check => {
                let area = Rect::from_center_size(center, vec2(15.0, 15.0));
                if on {
                    painter.rect_filled(area, CornerRadius::same(3), color);
                    let at = |x: f32, y: f32| pos2(area.left() + x * 15.0, area.top() + y * 15.0);
                    painter.add(egui::Shape::line(vec![at(0.24, 0.52), at(0.43, 0.70), at(0.77, 0.31)], Stroke::new(1.8, pal.accent_text)));
                } else {
                    painter.rect_stroke(area, CornerRadius::same(3), off, egui::StrokeKind::Inside);
                }
            }
            Marker::Number => {
                let area = Rect::from_center_size(center, vec2(19.0, 19.0));
                let (fill, text) = if on || !mark.is_empty() { (color.gamma_multiply(if on { 1.0 } else { 0.3 }), if on { pal.accent_text } else { pal.heading }) } else { (Color32::TRANSPARENT, pal.text_faint) };
                painter.rect(area, CornerRadius::same(4), fill, if mark.is_empty() && !on { off } else { Stroke::NONE }, egui::StrokeKind::Inside);
                if !mark.is_empty() {
                    painter.text(area.center(), egui::Align2::CENTER_CENTER, mark, FontId::new(11.5, crate::fonts::semibold()), text);
                }
            }
            Marker::None => {}
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn option_row(&mut self, ui: &mut Ui, id: &str, text: &str, marker: Marker, mark: &str, selected: bool, tone: Tone, enabled: bool, path: Id) {
        let pal = self.pal;
        let sid = self.state_id(path, id).with(("option", id));
        let lead = if marker == Marker::None { 12.0 } else { 40.0 };
        let frame = Frame::new().corner_radius(CornerRadius::same(8)).inner_margin(Margin { left: lead as i8, right: 12, top: 8, bottom: 8 });
        let mut prepared = frame.begin(ui);
        {
            let ui = &mut prepared.content_ui;
            ui.set_width(ui.available_width());
            if !enabled {
                ui.multiply_opacity(0.85);
            }
            self.hooks.markdown(ui, pal, text);
        }
        let rect = prepared.allocate_space(ui).rect;
        let resp = ui.interact(rect, sid, if enabled { Sense::click() } else { Sense::hover() });
        let hovered = enabled && resp.hovered();
        let accent = tone_accent(pal, tone);
        let (fill, stroke) = match accent {
            Some(c) => (c.gamma_multiply(0.13), c),
            None if selected => (pal.accent.gamma_multiply(0.12), pal.accent),
            None if hovered => (pal.bg_hover, pal.border_strong),
            None => (Color32::TRANSPARENT, pal.border),
        };
        prepared.frame.fill = fill;
        prepared.frame.stroke = Stroke::new(1.0, stroke);
        prepared.paint(ui);
        // Метка — посередине однострочного варианта (с inline code строка выше обычной), у
        // длинного — по первой строке.
        let y = if rect.height() <= 52.0 { rect.center().y } else { rect.top() + 8.0 + 10.5 };
        let center = pos2(rect.left() + 20.0, y);
        let color = accent.unwrap_or(pal.accent);
        let on = selected || (marker == Marker::Number && !mark.is_empty() && accent.is_some());
        self.paint_marker(ui.painter(), center, marker, mark, on, color, hovered);
        if enabled && resp.on_hover_cursor(CursorIcon::PointingHand).clicked() {
            self.event(id, EventKind::Click, Value::Null);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn slider(&mut self, ui: &mut Ui, id: &str, value: f64, min: f64, max: f64, step: f64, unit: &str, decimals: u8, marks: &[Mark], enabled: bool, path: Id) {
        let pal = self.pal;
        let sid = self.state_id(path, id).with("slider");
        let span = (max - min).max(f64::EPSILON);
        // Пока тянут — своё значение, чтобы ползунок не ждал ответа плагина.
        let dragging: Option<f64> = ui.data(|d| d.get_temp(sid));
        let shown = dragging.unwrap_or(value).clamp(min, max);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.label(RichText::new(format_number(shown, decimals)).font(FontId::monospace(26.0)).color(pal.heading));
            if !unit.is_empty() {
                ui.label(RichText::new(unit).size(13.0).color(pal.text_muted));
            }
        });
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), if enabled { Sense::click_and_drag() } else { Sense::hover() });
        let (x0, x1, y) = (rect.left() + 9.0, rect.right() - 9.0, rect.center().y);
        let x_of = |v: f64| x0 + ((v - min) / span).clamp(0.0, 1.0) as f32 * (x1 - x0);
        let painter = ui.painter();
        painter.line_segment([pos2(x0, y), pos2(x1, y)], Stroke::new(4.0, pal.border));
        painter.line_segment([pos2(x0, y), pos2(x_of(shown), y)], Stroke::new(4.0, if enabled { pal.accent } else { pal.text_faint }));
        for m in marks {
            let c = tone_accent(pal, m.tone).unwrap_or(pal.text_muted);
            let x = x_of(m.value);
            painter.line_segment([pos2(x, y - 9.0), pos2(x, y + 9.0)], Stroke::new(2.0, c));
            if !m.text.is_empty() {
                painter.text(pos2(x, y - 10.0), egui::Align2::CENTER_BOTTOM, &m.text, FontId::new(11.0, crate::fonts::semibold()), c);
            }
        }
        let hx = x_of(shown);
        let hot = enabled && (resp.hovered() || resp.dragged());
        painter.circle(pos2(hx, y), if hot { 8.5 } else { 7.5 }, pal.bg_elevated, Stroke::new(2.4, if enabled { pal.accent } else { pal.text_faint }));
        if enabled {
            let resp = resp.on_hover_cursor(CursorIcon::PointingHand);
            if (resp.dragged() || resp.clicked())
                && let Some(p) = resp.interact_pointer_pos()
            {
                let t = ((p.x - x0) / (x1 - x0)).clamp(0.0, 1.0) as f64;
                let raw = min + t * span;
                let v = if step > 0.0 { (min + ((raw - min) / step).round() * step).clamp(min, max) } else { raw };
                // Округление шага не должно тащить хвост вроде 25.000000000000004.
                let v = (v * 1e9).round() / 1e9;
                if Some(v) != dragging {
                    ui.data_mut(|d| d.insert_temp(sid, v));
                }
                if v != value {
                    self.event(id, EventKind::Change, serde_json::json!(v));
                }
            }
            if resp.drag_stopped() || (resp.clicked() && !resp.dragged()) {
                ui.data_mut(|d| d.remove::<f64>(sid));
            }
        }
        let edge = |v: f64| format!("{} {unit}", format_number(v, decimals.min(1)));
        ui.horizontal(|ui| {
            ui.label(RichText::new(edge(min)).font(FontId::monospace(11.5)).color(pal.text_faint));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new(edge(max)).font(FontId::monospace(11.5)).color(pal.text_faint));
            });
        });
    }

    fn tabs(&mut self, ui: &mut Ui, id: &str, tabs: &[Tab], selected: &str) {
        let pal = self.pal;
        let top = ui.cursor().top();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for tab in tabs {
                let rt = RichText::new(&tab.text).size(13.0);
                let w = text_width(ui, &rt) + 14.0 + 20.0;
                let (rect, resp) = ui.allocate_exact_size(vec2(w, 32.0), Sense::click());
                let on = tab.id == selected;
                let hovered = resp.hovered();
                let painter = ui.painter();
                if hovered && !on {
                    painter.rect_filled(rect.shrink2(vec2(0.0, 3.0)), CornerRadius::same(6), pal.bg_hover);
                }
                let dot = pos2(rect.left() + 12.0, rect.center().y);
                match tab.dot.and_then(|t| tone_accent(pal, t).or(Some(tone_color(pal, t)))) {
                    Some(c) => {
                        painter.circle_filled(dot, 3.5, c);
                    }
                    None => {
                        painter.circle_stroke(dot, 3.0, Stroke::new(1.0, pal.text_faint));
                    }
                }
                let color = if on { pal.heading } else if hovered { pal.text } else { pal.text_muted };
                let family = if on { crate::fonts::semibold() } else { egui::FontFamily::Proportional };
                painter.text(pos2(rect.left() + 22.0, rect.center().y), egui::Align2::LEFT_CENTER, &tab.text, FontId::new(13.0, family), color);
                if on {
                    painter.line_segment([pos2(rect.left() + 4.0, rect.bottom() - 1.0), pos2(rect.right() - 4.0, rect.bottom() - 1.0)], Stroke::new(2.0, pal.accent));
                }
                if resp.on_hover_cursor(CursorIcon::PointingHand).clicked() && !on {
                    self.event(id, EventKind::Select, Value::String(tab.id.clone()));
                }
            }
        });
        let r = ui.min_rect();
        let y = ui.cursor().top().max(top) - 6.0 + 0.5;
        ui.painter().hline(r.x_range(), y, Stroke::new(1.0, pal.border));
    }

    fn chips(&mut self, ui: &mut Ui, id: &str, items: &[Chip], enabled: bool) {
        let pal = self.pal;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
            for chip in items {
                let font = FontId::monospace(12.0);
                let g = ui.painter().layout_no_wrap(chip.text.clone(), font, pal.text);
                let dot_w = if chip.color.is_some() { 14.0 } else { 0.0 };
                let w = g.size().x + dot_w + 20.0;
                fit_row(ui, w);
                let (rect, resp) = ui.allocate_exact_size(vec2(w, 26.0), if enabled { Sense::click() } else { Sense::hover() });
                let dot_color = chip.color.as_deref().map(|c| crate::canvas::color(pal, c));
                let hovered = enabled && resp.hovered();
                let (fill, stroke) = match tone_accent(pal, chip.tone) {
                    Some(c) => (c.gamma_multiply(0.18), c),
                    None if chip.selected => {
                        let c = dot_color.unwrap_or(pal.accent);
                        (c.gamma_multiply(0.2), c)
                    }
                    None if hovered => (pal.bg_hover, pal.border_strong),
                    None => (pal.chip_bg, pal.chip_border),
                };
                let painter = ui.painter();
                painter.rect(rect, CornerRadius::same(13), fill, Stroke::new(1.0, stroke), egui::StrokeKind::Inside);
                let mut x = rect.left() + 10.0;
                if let Some(c) = dot_color {
                    painter.circle_filled(pos2(x + 3.5, rect.center().y), 3.5, c);
                    x += dot_w;
                }
                let color = if chip.selected || chip.tone != Tone::Normal { pal.heading } else { pal.text_muted };
                painter.galley(pos2(x, rect.center().y - g.size().y / 2.0), g, color);
                if enabled && resp.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                    self.event(id, EventKind::Select, Value::String(chip.id.clone()));
                }
            }
        });
    }

    /// Рамка code в элементах задач.
    fn code_frame<R>(&self, ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
        let pal = self.pal;
        Frame::new()
            .fill(pal.bg_deep)
            .stroke(Stroke::new(1.0, pal.border))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::symmetric(14, 10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                add(ui)
            })
            .inner
    }

    fn code_blanks(&mut self, ui: &mut Ui, text: &str, lang: &str, blanks: &[Blank], enabled: bool, path: Id) {
        let pal = self.pal;
        let (plain, holes) = blank_layout(text.trim_end_matches('\n'), blanks);
        let job = self.hooks.code_job(ui.ctx(), pal, &plain, lang, CODE);
        let mut events = Vec::new();
        self.code_frame(ui, |ui| {
            let galley = ui.painter().layout_job(job);
            let (rect, _) = ui.allocate_exact_size(galley.size(), Sense::hover());
            ui.painter().galley(rect.min, galley.clone(), pal.text);
            for (i, start, len) in holes {
                let b = &blanks[i];
                let a = galley.pos_from_cursor(CCursor::new(start));
                let z = galley.pos_from_cursor(CCursor::new(start + len));
                // Поле — по высоте букв, посередине строки (у строки code есть межстрочный зазор).
                let row_h = ui.fonts_mut(|f| f.row_height(&FontId::monospace(CODE)));
                let top = rect.min.y + a.min.y;
                let hole = Rect::from_min_max(pos2(rect.min.x + a.min.x, top), pos2(rect.min.x + z.min.x, top + row_h)).expand2(vec2(3.0, 2.0));
                let sid = self.base.with(("blank", path, &b.id));
                let (mut seen, mut value): (String, String) = ui.data(|d| d.get_temp(sid.with("text"))).unwrap_or_else(|| (b.value.clone(), b.value.clone()));
                if seen != b.value {
                    seen = b.value.clone();
                    value = b.value.clone();
                }
                let focused = ui.memory(|m| m.has_focus(sid));
                let stroke = tone_accent(pal, b.tone).unwrap_or(if focused { pal.accent } else { pal.border_strong });
                ui.painter().rect(hole, CornerRadius::same(4), pal.input_bg, Stroke::new(1.0, stroke), egui::StrokeKind::Inside);
                // Закрытое поле (ответ принят) не тускнеет: его текст — часть code.
                let edit = egui::TextEdit::singleline(&mut value).id(sid).font(FontId::monospace(CODE)).text_color(pal.heading).frame(Frame::NONE).margin(Margin::symmetric(3, 2)).desired_width(hole.width()).interactive(enabled);
                let resp = ui.put(hole, edit);
                if resp.changed() {
                    events.push((b.id.clone(), value.clone()));
                }
                ui.data_mut(|d| d.insert_temp(sid.with("text"), (seen, value)));
            }
        });
        for (id, v) in events {
            self.event(&id, EventKind::Change, Value::String(v));
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn code_spans(&mut self, ui: &mut Ui, id: &str, text: &str, lang: &str, spans: &[Span], selected: Option<&str>, enabled: bool, path: Id) {
        let pal = self.pal;
        let text = text.trim_end_matches('\n');
        let job = self.hooks.code_job(ui.ctx(), pal, text, lang, CODE);
        let sid = self.state_id(path, id).with("spans");
        let mut clicked = None;
        self.code_frame(ui, |ui| {
            let galley = ui.painter().layout_job(job);
            let (rect, _) = ui.allocate_exact_size(galley.size(), Sense::hover());
            let bg = ui.painter().add(egui::Shape::Noop);
            let mut shapes = Vec::new();
            let n = text.chars().count() as u32;
            for s in spans.iter().filter(|s| s.start < s.end && s.end <= n) {
                let a = galley.pos_from_cursor(CCursor::new(s.start as usize));
                let z = galley.pos_from_cursor(CCursor::new(s.end as usize));
                // Место на одной строке (token); перенос строки — до конца первой.
                let right = if (z.min.y - a.min.y).abs() < 1.0 { z.min.x } else { galley.rect.right() };
                let r = Rect::from_min_max(rect.min + a.min.to_vec2(), pos2(rect.min.x + right, rect.min.y + a.max.y)).expand2(vec2(2.0, 0.5));
                let resp = ui.interact(r, sid.with(&s.id), if enabled { Sense::click() } else { Sense::hover() });
                let on = selected == Some(s.id.as_str());
                let tone = tone_accent(pal, s.tone);
                let (fill, stroke) = match tone {
                    Some(c) => (c.gamma_multiply(0.25), Some(c)),
                    None if on => (pal.accent.gamma_multiply(0.25), Some(pal.accent)),
                    None if enabled && resp.hovered() => (pal.bg_hover, Some(pal.border_strong)),
                    None => (Color32::TRANSPARENT, None),
                };
                if fill != Color32::TRANSPARENT || stroke.is_some() {
                    shapes.push(egui::Shape::rect_filled(r, CornerRadius::same(3), fill));
                    if let Some(c) = stroke {
                        shapes.push(egui::Shape::rect_stroke(r, CornerRadius::same(3), Stroke::new(1.0, c), egui::StrokeKind::Inside));
                    }
                }
                if enabled && resp.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                    clicked = Some(s.id.clone());
                }
            }
            ui.painter().set(bg, egui::Shape::Vec(shapes));
            ui.painter().galley(rect.min, galley, pal.text);
        });
        if let Some(s) = clicked {
            self.event(id, EventKind::Click, Value::String(s));
        }
    }

    fn reorder(&mut self, ui: &mut Ui, id: &str, items: &[Item], lang: &str, enabled: bool) {
        let pal = self.pal;
        let mut moved: Option<(usize, usize)> = None;
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            for (i, it) in items.iter().enumerate() {
                let stroke = tone_accent(pal, it.tone).unwrap_or(pal.border);
                Frame::new().fill(pal.bg_deep).stroke(Stroke::new(1.0, stroke)).corner_radius(CornerRadius::same(8)).inner_margin(Margin { left: 12, right: 6, top: 4, bottom: 4 }).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.allocate_ui_with_layout(vec2(ui.available_width(), 28.0), Layout::left_to_right(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 10.0;
                        ui.label(RichText::new(format!("{}", i + 1)).font(FontId::monospace(12.0)).color(pal.text_faint));
                        let line = if lang.is_empty() {
                            LayoutJob::simple_singleline(it.text.clone(), FontId::monospace(CODE), pal.heading)
                        } else {
                            self.hooks.code_job(ui.ctx(), pal, &it.text, lang, CODE)
                        };
                        ui.add(egui::Label::new(line).wrap_mode(egui::TextWrapMode::Truncate));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            for (icon, to, can) in [("chevron-down", i + 1, i + 1 < items.len()), ("chevron-up", i.wrapping_sub(1), i > 0)] {
                                let (r, resp) = ui.allocate_exact_size(vec2(26.0, 26.0), if enabled && can { Sense::click() } else { Sense::hover() });
                                let hot = enabled && can && resp.hovered();
                                if hot {
                                    ui.painter().rect_filled(r, CornerRadius::same(6), pal.bg_hover);
                                }
                                let color = if !enabled || !can { pal.text_faint.gamma_multiply(0.6) } else if hot { pal.heading } else { pal.text_muted };
                                crate::icons::paint(ui.painter(), Rect::from_center_size(r.center(), vec2(14.0, 14.0)), icon, color);
                                if enabled && can && resp.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                                    moved = Some((i, to));
                                }
                            }
                        });
                    });
                });
            }
        });
        if let Some((from, to)) = moved {
            let mut order: Vec<String> = items.iter().map(|it| it.id.clone()).collect();
            order.swap(from, to);
            self.event(id, EventKind::Change, serde_json::json!(order));
        }
    }

    /// Поле ввода в рамке: моноширинное, с текстом до и после, рамкой цветом итога.
    #[allow(clippy::too_many_arguments)]
    fn framed_input(&mut self, ui: &mut Ui, id: &str, value: &str, placeholder: &str, mono: bool, prefix: &str, suffix: &str, tone: Tone, path: Id) {
        let pal = self.pal;
        let sid = self.state_id(path, id);
        let (mut seen, mut text): (String, String) = ui.data(|d| d.get_temp(sid.with("text"))).unwrap_or_else(|| (value.to_owned(), value.to_owned()));
        if seen != value {
            seen = value.to_owned();
            text = value.to_owned();
        }
        let font = if mono { FontId::monospace(CODE) } else { FontId::proportional(13.5) };
        let focused = ui.memory(|m| m.has_focus(sid));
        let stroke = tone_accent(pal, tone).unwrap_or(if focused { pal.accent } else { pal.border });
        let width = if self.in_grid { GRID_INPUT } else { ui.available_width() };
        let mut changed = false;
        ui.allocate_ui(vec2(width, 0.0), |ui| {
            Frame::new().fill(pal.input_bg).stroke(Stroke::new(1.0, stroke)).corner_radius(CornerRadius::same(6)).inner_margin(Margin::symmetric(8, 5)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                egui::containers::Sides::new().shrink_left().show(
                    ui,
                    |ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        if !prefix.is_empty() {
                            ui.label(RichText::new(prefix).font(font.clone()).color(pal.text_faint));
                        }
                        let w = ui.available_width() - if suffix.is_empty() { 0.0 } else { 14.0 };
                        let edit = egui::TextEdit::singleline(&mut text).id(sid).font(font.clone()).text_color(pal.heading).hint_text(placeholder).frame(Frame::NONE).margin(Margin::ZERO).desired_width(w.max(30.0));
                        changed = ui.add(edit).changed();
                    },
                    |ui| {
                        if !suffix.is_empty() {
                            ui.label(RichText::new(suffix).font(font.clone()).color(pal.text_faint));
                        }
                    },
                );
            });
        });
        if changed {
            self.event(id, EventKind::Change, Value::String(text.clone()));
        }
        ui.data_mut(|d| d.insert_temp(sid.with("text"), (seen, text)));
    }

    #[allow(clippy::too_many_arguments)]
    fn group(&mut self, ui: &mut Ui, title: &str, icon: Option<&str>, children: &[Element], collapsible: bool, open: bool, path: Id) {
        let pal = self.pal;
        let state_id = if title.is_empty() {
            path.with("group")
        } else {
            let n = self.groups.entry(title.to_owned()).or_default();
            *n += 1;
            group_state(self.base, title, *n)
        };
        let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), state_id, open);
        Frame::new().fill(pal.settings_card).stroke(Stroke::new(1.0, pal.border)).corner_radius(CornerRadius::same(8)).inner_margin(Margin::same(10)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            // Внутри группы — всегда сверху вниз, даже если сама группа стоит в строке.
            ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| self.group_body(ui, title, icon, children, collapsible, open, path, state_id, &mut state));
        });
    }

    /// Шапка и содержимое группы (уже внутри рамки).
    #[allow(clippy::too_many_arguments)]
    fn group_body(&mut self, ui: &mut Ui, title: &str, icon: Option<&str>, children: &[Element], collapsible: bool, open: bool, path: Id, state_id: Id, state: &mut egui::collapsing_header::CollapsingState) {
        let pal = self.pal;
        if !title.is_empty() || collapsible {
            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 20.0), if collapsible { Sense::click() } else { Sense::hover() });
            let mut x = rect.left();
            if collapsible {
                let openness = state.openness(ui.ctx());
                let c = egui::pos2(x + 6.0, rect.center().y);
                let color = if resp.hovered() { pal.heading } else { pal.text_muted };
                // Шеврон: вправо — свёрнуто, вниз — открыто.
                let a = std::f32::consts::FRAC_PI_2 * openness;
                let rot = |dx: f32, dy: f32| egui::pos2(c.x + dx * a.cos() - dy * a.sin(), c.y + dx * a.sin() + dy * a.cos());
                ui.painter().line(vec![rot(-2.0, -4.0), rot(2.0, 0.0), rot(-2.0, 4.0)], Stroke::new(1.4, color));
                x += 16.0;
            }
            if let Some(icon) = icon {
                let r = egui::Rect::from_center_size(egui::pos2(x + 7.0, rect.center().y), vec2(14.0, 14.0));
                crate::icons::paint(ui.painter(), r, icon, pal.text_muted);
                x += 20.0;
            }
            ui.painter().text(egui::pos2(x, rect.center().y), egui::Align2::LEFT_CENTER, title, FontId::new(13.0, crate::fonts::semibold()), pal.heading);
            if collapsible {
                if resp.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                    state.toggle(ui);
                }
                state.store(ui.ctx());
            }
        }
        let body = |ui: &mut Ui, d: &mut Self| {
            ui.add_space(4.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = GAP;
                d.children(ui, children, path);
            });
        };
        if collapsible {
            let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), state_id, open);
            state.show_body_unindented(ui, |ui| body(ui, self));
        } else {
            body(ui, self);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn text_input(&mut self, ui: &mut Ui, id: &str, value: &str, placeholder: &str, multiline: bool, search: bool, path: Id) {
        let sid = self.state_id(path, id);
        // Поле помнит то, что набрано; плагин может прислать новое значение — тогда берётся оно.
        let (mut seen, mut text): (String, String) = ui.data(|d| d.get_temp(sid.with("text"))).unwrap_or_else(|| (value.to_owned(), value.to_owned()));
        if seen != value {
            seen = value.to_owned();
            text = value.to_owned();
        }
        let width = if self.in_grid { GRID_INPUT } else { ui.available_width() };
        let resp = if search {
            crate::widgets::search_field(ui, sid, &mut text, placeholder, width, 13.5)
        } else if multiline {
            ui.add(egui::TextEdit::multiline(&mut text).id(sid).hint_text(placeholder).desired_width(width).desired_rows(3))
        } else {
            ui.add(egui::TextEdit::singleline(&mut text).id(sid).hint_text(placeholder).desired_width(width))
        };
        if resp.changed() {
            self.event(id, EventKind::Change, Value::String(text.clone()));
        }
        if !multiline && resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            self.event(id, EventKind::Submit, Value::String(text.clone()));
        }
        ui.data_mut(|d| d.insert_temp(sid.with("text"), (seen, text)));
    }

    fn list(&mut self, ui: &mut Ui, id: &str, items: &[Item], selected: Option<&str>, path: Id) {
        let pal = self.pal;
        let sid = self.state_id(path, id);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            for it in items {
                let h = if it.detail.is_empty() { 28.0 } else { 44.0 };
                let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::hover());
                let resp = ui.interact(rect, sid.with(("row", &it.id)), Sense::click());
                let is_sel = selected == Some(it.id.as_str());
                let fill = if is_sel {
                    pal.list_selected
                } else if resp.hovered() {
                    pal.list_hover
                } else {
                    Color32::TRANSPARENT
                };
                let painter = ui.painter();
                painter.rect_filled(rect, CornerRadius::same(6), fill);
                let mut x = rect.left() + 8.0;
                let line_y = if it.detail.is_empty() { rect.center().y } else { rect.top() + 14.0 };
                if let Some(icon) = &it.icon {
                    let r = egui::Rect::from_center_size(egui::pos2(x + 7.0, line_y), vec2(14.0, 14.0));
                    crate::icons::paint(painter, r, icon, if is_sel { pal.accent } else { pal.text_muted });
                    x += 22.0;
                }
                let right = rect.right() - 8.0;
                let mut text_right = right;
                if !it.badge.is_empty() {
                    let g = painter.layout_no_wrap(it.badge.clone(), FontId::proportional(11.5), pal.text_faint);
                    text_right -= g.size().x + 10.0;
                    painter.galley(egui::pos2(right - g.size().x, line_y - g.size().y / 2.0), g, pal.text_faint);
                }
                let color = if it.tone == Tone::Normal { pal.text } else { tone_color(pal, it.tone) };
                let g = crate::widgets::elide(ui, it.text.clone(), FontId::proportional(13.5), color, (text_right - x).max(10.0));
                painter.galley(egui::pos2(x, line_y - g.size().y / 2.0), g, color);
                if !it.detail.is_empty() {
                    let g = crate::widgets::elide(ui, it.detail.clone(), FontId::proportional(12.0), pal.text_faint, (right - x).max(10.0));
                    painter.galley(egui::pos2(x, rect.top() + 26.0), g, pal.text_faint);
                }
                if resp.double_clicked() {
                    self.event(id, EventKind::Activate, Value::String(it.id.clone()));
                } else if resp.clicked() && !is_sel {
                    self.event(id, EventKind::Select, Value::String(it.id.clone()));
                }
            }
        });
    }
}

/// Небольшая плашка с текстом.
fn badge(ui: &mut Ui, pal: &Palette, text: &str, tone: Tone) {
    let color = if tone == Tone::Normal { pal.text_muted } else { tone_color(pal, tone) };
    let rt = RichText::new(text).size(11.5).color(color);
    // Поля 6 + 6 и рамка 1 + 1.
    let wrap = fit_row(ui, text_width(ui, &rt) + 14.0);
    Frame::new()
        .fill(pal.chip_bg)
        .stroke(Stroke::new(1.0, pal.chip_border))
        .corner_radius(CornerRadius::same(5))
        .inner_margin(Margin::symmetric(6, 1))
        .show(ui, |ui| {
            ui.add(egui::Label::new(rt).wrap_mode(wrap));
        });
}

#[cfg(test)]
mod tests {
    use forma_plugin_ui::{self as ui, Choice};
    use forma_plugin_ui::{Blank, Chip, Mark, Marker, Span, Tab};

    use super::*;

    /// Все виды элементов рисуются (и незнакомый тоже) и сами ничего не сообщают.
    #[test]
    fn every_element_draws() {
        forma_i18n::init_with(&[], None);
        let tree = ui::column([
            ui::heading("Глоссарий"),
            ui::search("query", "uv", "Найти"),
            ui::text_input("note", "", "Заметка"),
            ui::checkbox("all", "Все", true),
            Element::Select { id: "mode".into(), value: "a".into(), options: vec![Choice { value: "a".into(), text: "А".into() }] },
            Element::Number { id: "n".into(), value: 3.0, min: 0.0, max: 10.0, step: 1.0, unit: "px".into() },
            ui::list("terms", [ui::item("uv", "UV")], Some("uv".into())),
            Element::Table { columns: vec!["Объект".into(), "Tris".into()], rows: vec![vec!["Фюзеляж".into(), "7 800".into()]] },
            Element::Progress { value: Some(0.4), text: String::new() },
            Element::Progress { value: None, text: "Считаю".into() },
            Element::Group { title: "Свёрнуто".into(), icon: Some("book".into()), children: vec![ui::markdown("**да**"), ui::code("fn main() {}", "rust")], collapsible: true, open: false, width: None },
            ui::row([ui::card("Кадр 1", 200.0, [ui::label("Взлёт")]), ui::card("Кадр 2", 200.0, [ui::label("Набор высоты")])]),
            Element::Canvas { width: 400.0, height: 120.0, shapes: vec![ui::rect(0.0, 0.0, 80.0, 40.0, "accent", 4.0), ui::text_at(4.0, 4.0, "Ноды", 13.0, "text")], pan_zoom: true, background: Some("panel".into()) },
            Element::Compare { before: "a.png".into(), after: "b.png".into(), before_label: "До".into(), after_label: "После".into() },
            Element::View3d { positions: vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0], indices: vec![0, 1, 2], color: None, height: 120.0, wireframe: true },
            ui::row([ui::button("more", "Ещё"), ui::command_button("Найти", "terms.find", Value::Null)]),
            Element::Badge { text: "preview".into(), tone: Tone::Muted },
            Element::Image { path: "a.png".into(), width: None, alt: "картинка".into() },
            Element::Separator,
            Element::Space { size: 0.0 },
            ui::header("quiz", "Один вариант", "1 балл"),
            ui::bar([ui::hint("Выбери один вариант")], [ui::primary_button("check", "Проверить", false)]),
            ui::option("a", "`i32`", Marker::Radio, true, Tone::Success),
            ui::option("b", "Описание", Marker::Check, false, Tone::Normal),
            Element::OptionRow { id: "p".into(), text: "useState".into(), marker: Marker::Number, mark: "1".into(), selected: false, tone: Tone::Accent, enabled: false },
            Element::Slider { id: "fps".into(), value: 25.0, min: 0.0, max: 50.0, step: 0.1, unit: "мс".into(), decimals: 1, marks: vec![Mark { value: 16.7, tone: Tone::Success, text: "16,7".into() }], enabled: true },
            Element::Tabs { id: "q".into(), tabs: vec![Tab { id: "1".into(), text: "Вопрос 1".into(), dot: Some(Tone::Danger) }, Tab { id: "2".into(), text: "Вопрос 2".into(), dot: None }], selected: "1".into() },
            Element::Chips { id: "g".into(), items: vec![Chip { id: "c".into(), text: "client · render".into(), color: Some("accent".into()), selected: true, tone: Tone::Normal }], enabled: true },
            Element::CodeBlanks { text: "let x: {{t}} = 5;
let y = {{u}};".into(), lang: "rust".into(), blanks: vec![Blank { id: "t".into(), value: "i32".into(), ..Default::default() }, Blank { id: "u".into(), tone: Tone::Danger, ..Default::default() }], enabled: true },
            Element::CodeSpans { id: "spot".into(), text: "let a = \"110\";".into(), lang: "rust".into(), spans: vec![Span { id: "s".into(), start: 8, end: 13, tone: Tone::Success }, Span { id: "bad".into(), start: 40, end: 50, tone: Tone::Normal }], selected: Some("s".into()), enabled: true },
            Element::Grid { columns: vec!["i".into(), "sum".into()], rows: vec![vec![ui::label("1"), ui::text_input("r1", "2", "")]] },
            Element::Reorder { id: "lines".into(), items: vec![ui::item("a", "let sum = 0;"), ui::item("b", "return sum;")], lang: "rust".into(), enabled: true },
            Element::TextInput { id: "re".into(), value: "\\d+".into(), placeholder: String::new(), multiline: false, search: false, mono: true, prefix: "/".into(), suffix: "/".into(), tone: Tone::Danger },
            Element::Label { text: "formatPrice(990)".into(), icon: Some("check".into()), tone: Tone::Success, size: Size::Normal, bold: false, mono: true },
            Element::Unknown,
        ]);
        let (ctx, pal) = setup();
        for _ in 0..2 {
            let mut out = Vec::new();
            let mut full = ctx.run_ui(egui::RawInput::default(), |ui| out = show(ui, &pal, Id::new("t"), &tree, &mut Plain));
            full.textures_delta.clear();
            assert!(out.is_empty(), "без действий пользователя — ни событий, ни команд");
        }
    }

    fn setup() -> (egui::Context, Palette) {
        let ctx = egui::Context::default();
        // Начертания Forma — обычным шрифтом egui: здесь важна раскладка, а не вид букв.
        let mut fonts = egui::FontDefinitions::default();
        let regular = fonts.families[&egui::FontFamily::Proportional].clone();
        for family in [crate::fonts::semibold(), crate::fonts::bold()] {
            fonts.families.insert(family, regular.clone());
        }
        ctx.set_fonts(fonts);
        let pal = crate::theme::Theme::resolve(crate::theme::ThemeFile::parse("[theme]\nid = \"t\"\ndark = true").unwrap()).pal;
        (ctx, pal)
    }

    /// Прямоугольник, который дерево заняло в столбце шириной `width` (второй кадр — когда
    /// шрифты уже готовы).
    fn drawn(ctx: &egui::Context, pal: &Palette, id: &str, tree: &Element, width: f32) -> egui::Rect {
        let mut rect = egui::Rect::NOTHING;
        for _ in 0..2 {
            let mut full = ctx.run_ui(egui::RawInput::default(), |ui| {
                rect = ui.allocate_ui(vec2(width, 4000.0), |ui| {
                    ui.set_max_width(width);
                    show(ui, pal, Id::new(id), tree, &mut Plain);
                }).response.rect;
            });
            full.textures_delta.clear();
        }
        rect
    }

    /// Щелчок в точке `at` по дереву: события, которые оно вернуло.
    fn click(tree: &Element, at: egui::Pos2) -> Vec<Output> {
        forma_i18n::init_with(&[], None);
        let (ctx, pal) = setup();
        let frame = |events: Vec<egui::Event>| {
            let input = egui::RawInput { screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(400.0, 600.0))), events, ..Default::default() };
            let mut out = Vec::new();
            let mut full = ctx.run_ui(input, |ui| out = show(ui, &pal, Id::new("click"), tree, &mut Plain));
            full.textures_delta.clear();
            out
        };
        let press = |pressed| egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
        let mut out = frame(vec![]);
        out.extend(frame(vec![egui::Event::PointerMoved(at)]));
        out.extend(frame(vec![press(true)]));
        out.extend(frame(vec![press(false)]));
        out.extend(frame(vec![]));
        out
    }

    /// Вариант ответа сообщает `click`, стрелка «вниз» — новый порядок строк, место в code — свой id.
    #[test]
    fn task_elements_report_what_was_done() {
        let opt = ui::option("b", "Второй", Marker::Radio, false, Tone::Normal);
        assert_eq!(click(&opt, pos2(150.0, 14.0)), vec![Output::Event(Event { id: "b".into(), kind: EventKind::Click, value: Value::Null })]);
        let off = Element::OptionRow { id: "b".into(), text: "Второй".into(), marker: Marker::Radio, mark: String::new(), selected: false, tone: Tone::Normal, enabled: false };
        assert!(click(&off, pos2(150.0, 14.0)).is_empty(), "выключенный вариант молчит");
        let lines = Element::Reorder { id: "lines".into(), items: vec![ui::item("a", "first"), ui::item("b", "second")], lang: String::new(), enabled: true };
        // Первая строка: стрелка «вниз» — крайняя справа (ширина 400, поле 6, кнопка 26).
        let out = click(&lines, pos2(400.0 - 6.0 - 13.0 - 1.0, 18.0));
        assert_eq!(out, vec![Output::Event(Event { id: "lines".into(), kind: EventKind::Change, value: serde_json::json!(["b", "a"]) })]);
        let spot = Element::CodeSpans { id: "spot".into(), text: "ab cd".into(), lang: String::new(), spans: vec![Span { id: "cd".into(), start: 3, end: 5, tone: Tone::Normal }], selected: None, enabled: true };
        let x = 1.0 + 14.0 + 3.5 * 7.8;
        let out = click(&spot, pos2(x, 18.0));
        assert_eq!(out, vec![Output::Event(Event { id: "spot".into(), kind: EventKind::Click, value: Value::String("cd".into()) })], "место «cd» в {x}");
    }

    /// Пропуски `{{id}}` становятся пробелами своей ширины; незнакомые и `{{` без пары — как были.
    #[test]
    fn blanks_turn_into_spaces_of_their_width() {
        let blanks = [Blank { id: "t".into(), width: 3, ..Default::default() }, Blank { id: "v".into(), width: 2, ..Default::default() }];
        let (plain, holes) = blank_layout("let ы: {{t}} = {{v}}; {{x}} {{", &blanks);
        assert_eq!(plain, "let ы:     =   ; {{x}} {{");
        assert_eq!(holes, vec![(0, 7, 3), (1, 13, 2)], "места в символах, не в байтах");
        assert_eq!(format_number(16.0 / 3.0, 1).len(), 3);
    }

    /// Плашки и надписи строки в узкой панели уходят на следующую строку целиком, а не
    /// сжимаются в столбик по букве; шире всей строки — переносятся по словам.
    #[test]
    fn row_items_wrap_whole() {
        forma_i18n::init_with(&[], None);
        let (ctx, pal) = setup();
        let badge = |text: &str| Element::Badge { text: text.into(), tone: Tone::Success };
        let tree = ui::row([badge("проверка: готово"), ui::label("попытка 3"), badge("предсказание: верно"), badge("разбор: Зачтено")]);
        let r = drawn(&ctx, &pal, "row", &tree, 200.0);
        assert!(r.width() <= 200.5, "строка не вылезает за край: {r:?}");
        assert!(r.height() < 100.0, "по плашке в строке, а не по букве: {r:?}");
        let long = ui::row([ui::label("x"), badge("очень длинная плашка, которая не влезет даже в целую строку панели")]);
        let r = drawn(&ctx, &pal, "long", &long, 160.0);
        assert!(r.width() <= 160.5 && r.height() < 100.0, "перенос по словам: {r:?}");
    }

    /// Свёрнутая группа остаётся свёрнутой, когда плагин добавил элемент выше.
    #[test]
    fn collapsed_group_survives_new_rows_above() {
        forma_i18n::init_with(&[], None);
        let (ctx, pal) = setup();
        let group = Element::Group { title: "Итог".into(), icon: None, children: vec![Element::Space { size: 300.0 }], collapsible: true, open: true, width: None };
        let before = ui::column([group.clone()]);
        let after = ui::column([ui::label("Проверка закончилась"), group]);
        assert!(drawn(&ctx, &pal, "open", &after, 300.0).height() > 300.0, "по умолчанию открыта");
        let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(&ctx, group_state(Id::new("t"), "Итог", 1), true);
        state.set_open(false);
        state.store(&ctx);
        assert!(drawn(&ctx, &pal, "t", &before, 300.0).height() < 100.0);
        assert!(drawn(&ctx, &pal, "t", &after, 300.0).height() < 100.0, "строка выше не раскрыла группу");
    }
}
