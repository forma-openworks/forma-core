//! Дерево элементов плагина ([`forma_plugin_ui::Element`]) — виджетами Forma и цветами
//! темы, как свои панели программы. То, что программа умеет рисовать лучше (markdown —
//! renderer заметок Holotype, code — подсветка, картинки хранилища), она подставляет через
//! [`Hooks`].
//!
//! Что пользователь сделал с элементами, [`show`] возвращает списком [`Output`]: события
//! для плагина и команды от кнопок.

use std::collections::HashMap;

use egui::{Color32, CornerRadius, CursorIcon, FontId, Frame, Id, Margin, RichText, Sense, Stroke, Ui, vec2};
use forma_i18n::t;
use forma_plugin_ui::{Element, Event, EventKind, Item, Size, Tone};
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
    let mut d = Draw { pal, hooks, base: id, out: Vec::new(), groups: HashMap::new() };
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
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = gap.unwrap_or(GAP);
                    self.children(ui, children, path);
                });
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
            Element::Label { text, icon, tone, size, bold } => {
                let color = tone_color(pal, *tone);
                let size = text_size(*size);
                match icon {
                    Some(icon) => {
                        fit_row(ui, size + 6.0 + text_width(ui, &RichText::new(text).size(size)));
                        crate::widgets::icon_label(ui, icon, text.clone(), size, color);
                    }
                    None => {
                        let mut rt = RichText::new(text).size(size).color(color);
                        if *bold {
                            rt = rt.family(crate::fonts::semibold());
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
            Element::TextInput { id, value, placeholder, multiline, search } => self.text_input(ui, id, value, placeholder, *multiline, *search, path),
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
            Element::Unknown => {
                ui.label(RichText::new(t!("plugin-ui-unknown")).size(12.0).italics().color(pal.text_faint));
            }
        }
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
        let width = ui.available_width();
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
