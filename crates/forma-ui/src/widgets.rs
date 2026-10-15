//! Виджеты в фирменном стиле FORMA (перенесены из Forma: Holotype) и помощники,
//! с которыми интерфейс не ломается в узких областях: обрезка текста многоточием
//! и панель, которая переносит кнопки на вторую строку.

use std::sync::Arc;

use egui::text::{LayoutJob, TextWrapping};
use egui::{Align, Color32, CornerRadius, CursorIcon, FontId, Galley, Id, Layout, Rect, Sense, Stroke, Ui, UiBuilder, pos2, vec2};

use crate::theme::{self, Palette};

/// Текст в одну строку не шире `max_width`: не поместившийся хвост заменяется на «…».
pub fn elide(ui: &Ui, text: impl Into<String>, font: FontId, color: Color32, max_width: f32) -> Arc<Galley> {
    let mut job = LayoutJob::simple_singleline(text.into(), font, color);
    job.wrap = TextWrapping::truncate_at_width(max_width.max(0.0));
    ui.painter().layout_job(job)
}

/// Как [`elide`], но обрезается начало: у пути важнее конец («… › item › Item.java»).
pub fn elide_start(ui: &Ui, text: &str, font: FontId, color: Color32, max_width: f32) -> Arc<Galley> {
    let full = ui.painter().layout_no_wrap(text.to_owned(), font.clone(), color);
    if full.size().x <= max_width {
        return full;
    }
    let chars: Vec<char> = text.chars().collect();
    // Сколько символов убрать спереди: ищем наименьшее подходящее число делением пополам.
    let fits = |skip: usize| {
        let tail: String = std::iter::once('…').chain(chars[skip..].iter().copied()).collect();
        let g = ui.painter().layout_no_wrap(tail, font.clone(), color);
        (g.size().x <= max_width).then_some(g)
    };
    let (mut lo, mut hi) = (1, chars.len());
    let mut best = None;
    while lo <= hi {
        let mid = (lo + hi) / 2;
        match fits(mid) {
            Some(g) => {
                best = Some(g);
                hi = mid - 1;
            }
            None => lo = mid + 1,
        }
    }
    best.unwrap_or_else(|| elide(ui, text, font, color, max_width))
}

/// Поле поиска: значок лупы слева и подсказка. `font` — размер текста.
pub fn search_field(ui: &mut Ui, id: Id, text: &mut String, hint: &str, width: f32, font: f32) -> egui::Response {
    let pad = (font * 0.55).round();
    let left = (pad + font + 6.0) as i8;
    let edit = egui::TextEdit::singleline(text)
        .id(id)
        .hint_text(hint)
        .font(FontId::proportional(font))
        .desired_width(width)
        .margin(egui::Margin { left, right: pad as i8, top: (pad * 0.75) as i8, bottom: (pad * 0.75) as i8 });
    let resp = ui.add(edit);
    let icon = Rect::from_center_size(pos2(resp.rect.left() + pad + font / 2.0, resp.rect.center().y), vec2(font, font));
    let color = ui.visuals().widgets.inactive.fg_stroke.color.gamma_multiply(0.7);
    crate::icons::paint(ui.painter(), icon, "search", color);
    resp
}

/// Подпись со значком слева, текст переносится по ширине.
pub fn icon_label(ui: &mut Ui, icon: &str, text: impl Into<String>, size: f32, color: Color32) -> egui::Response {
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 5.0;
        let (rect, _) = ui.allocate_exact_size(vec2(size + 1.0, size * 1.3), Sense::hover());
        crate::icons::paint(ui.painter(), Rect::from_center_size(rect.center(), vec2(size, size)), icon, color);
        ui.add(egui::Label::new(egui::RichText::new(text.into()).size(size).color(color)).wrap())
    })
    .inner
}

/// Ширина обычной кнопки с текстом `text` (с учётом наименьшей ширины `min`).
pub fn button_width(ui: &Ui, text: &str, min: f32) -> f32 {
    let font = egui::TextStyle::Button.resolve(ui.style());
    let g = ui.painter().layout_no_wrap(text.to_owned(), font, Color32::WHITE);
    (g.size().x + ui.spacing().button_padding.x * 2.0).max(min)
}

/// Ширины групп панели с прошлого кадра и высота строки.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct BarMemory {
    left: f32,
    right: f32,
    height: f32,
}

/// Строка панели инструментов: группа `left` прижата влево, группа `right` — вправо
/// (её элементы добавляются справа налево, как в `Layout::right_to_left`).
///
/// Если группы не помещаются рядом, правая уходит на вторую строку, а группа, которой
/// мало и целой строки, переносится по элементам. Ширины групп берутся с прошлого кадра;
/// когда раскладка меняется, кадр пересчитывается заново, поэтому наложения не видно.
/// `pad` — отступ слева и справа у каждой строки.
pub fn bar(ui: &mut Ui, id: Id, pad: f32, left: impl FnOnce(&mut Ui), right: impl FnOnce(&mut Ui)) {
    const GAP: f32 = 12.0;
    let avail = ui.available_width() - pad * 2.0;
    let known: Option<BarMemory> = ui.data(|d| d.get_temp(id));
    let mem = known.unwrap_or_default();
    let row_h = if mem.height > 0.0 { mem.height } else { ui.spacing().interact_size.y };
    let one_row = mem.left + mem.right + GAP <= avail;
    let mut now = mem;
    let top = ui.cursor().min + vec2(pad, 0.0);
    // У групп постоянные id: при смене раскладки поля ввода не теряют фокус.
    let group = |ui: &mut Ui, salt: &str, min: egui::Pos2, layout: Layout, add: Box<dyn FnOnce(&mut Ui) + '_>| {
        let rect = Rect::from_min_size(min, vec2(avail, row_h));
        let mut child = ui.new_child(UiBuilder::new().id(id.with(salt)).max_rect(rect).layout(layout));
        add(&mut child);
        child.min_rect()
    };
    if one_row {
        let l = group(ui, "left", top, Layout::left_to_right(Align::Center), Box::new(left));
        let r = group(ui, "right", top, Layout::right_to_left(Align::Center), Box::new(right));
        now.left = l.width();
        now.right = r.width();
        now.height = l.height().max(r.height());
        ui.advance_cursor_after_rect(l.union(r));
    } else {
        let wrap_left = mem.left > avail;
        let l = group(ui, "left", top, Layout::left_to_right(Align::Center).with_main_wrap(wrap_left), Box::new(left));
        ui.advance_cursor_after_rect(l);
        let wrap_right = mem.right > avail;
        let r = group(ui, "right", ui.cursor().min + vec2(pad, 0.0), Layout::right_to_left(Align::Center).with_main_wrap(wrap_right), Box::new(right));
        ui.advance_cursor_after_rect(r);
        // Перенесённая группа своей настоящей ширины не показывает — помним прежнюю.
        if !wrap_left {
            now.left = l.width();
        }
        if !wrap_right {
            now.right = r.width();
        }
        now.height = if wrap_left || wrap_right { mem.height } else { l.height().max(r.height()) };
    }
    if known != Some(now) {
        ui.data_mut(|d| d.insert_temp(id, now));
    }
    if (now.left + now.right + GAP <= avail) != one_row {
        ui.ctx().request_discard("forma toolbar layout");
    }
}

/// Текст по центру с переносом по ширине не больше `max_width`.
/// Ширину ограничивает сама область: надпись egui иначе растягивает перенос на всю ширину.
pub fn centered_text(ui: &mut Ui, text: &str, size: f32, color: Color32, max_width: f32) {
    let width = ui.available_width().min(max_width);
    ui.allocate_ui_with_layout(vec2(width, 0.0), egui::Layout::top_down(egui::Align::Center), |ui| {
        let mut job = egui::text::LayoutJob::simple(text.to_owned(), FontId::proportional(size), color, width);
        job.halign = egui::Align::Center;
        ui.label(job);
    });
}

/// Вкладка документа в заголовке области, как в IDE от JetBrains: значок, название и
/// крестик, а у несохранённого документа, пока курсор не над вкладкой, — точка.
/// Открытая вкладка отмечена полоской по нижнему краю заголовка — отдельно от названия,
/// а не подчёркиванием под ним. Возвращает ответ и наведён ли курсор на крестик.
pub fn doc_tab(ui: &mut Ui, pal: &Palette, icon: &str, title: &str, active: bool, dirty: bool) -> (egui::Response, bool) {
    doc_tab_hinted(ui, pal, icon, title, None, active, dirty)
}

/// Как [`doc_tab`], но после названия — подпись мельче и своим цветом: папка у файлов с
/// одинаковыми именами (`main.rs  src/bin`), пометка «удалён».
#[allow(clippy::too_many_arguments)]
pub fn doc_tab_hinted(ui: &mut Ui, pal: &Palette, icon: &str, title: &str, hint: Option<(&str, Color32)>, active: bool, dirty: bool) -> (egui::Response, bool) {
    let tab = DocTab { icon, title, hint, active, dirty };
    let (rect, resp) = ui.allocate_exact_size(vec2(tab.width(ui), doc_tab_height(ui)), Sense::click_and_drag());
    let on_close = tab.paint(ui, pal, rect, &resp, false);
    (resp.on_hover_cursor(CursorIcon::PointingHand), on_close)
}

/// Высота вкладки документа в строке вкладок.
pub fn doc_tab_height(ui: &Ui) -> f32 {
    ui.available_height().clamp(22.0, 30.0)
}

/// Вкладка документа для строк вкладок, которые программа раскладывает сама (вкладку тащат,
/// соседние расступаются): ширина отдельно, отрисовка — в заданном прямоугольнике.
#[derive(Clone, Copy)]
pub struct DocTab<'a> {
    pub icon: &'a str,
    pub title: &'a str,
    /// Подпись после названия и её цвет (см. [`doc_tab_hinted`]).
    pub hint: Option<(&'a str, Color32)>,
    pub active: bool,
    pub dirty: bool,
}

impl DocTab<'_> {
    pub fn width(&self, ui: &Ui) -> f32 {
        let text = |s: &str, size: f32| ui.painter().layout_no_wrap(s.to_owned(), FontId::proportional(size), Color32::WHITE).size().x;
        let hint = self.hint.map_or(0.0, |(h, _)| text(h, 11.5) + 6.0);
        8.0 + 14.0 + 6.0 + text(self.title, 13.0) + hint + 6.0 + 16.0 + 6.0
    }

    /// Рисует вкладку в `rect`; `resp` — её ответ (наведение, крестик). `lifted` — вкладку
    /// тащат по строке: она поверх соседних, со своим фоном. Возвращает, наведён ли курсор
    /// на крестик.
    pub fn paint(&self, ui: &Ui, pal: &Palette, rect: Rect, resp: &egui::Response, lifted: bool) -> bool {
        let hovered = resp.hovered() && !lifted;
        let text_color = if self.active || hovered || lifted { pal.tab_active_text } else { pal.tab_text };
        let galley = ui.painter().layout_no_wrap(self.title.to_owned(), FontId::proportional(13.0), text_color);
        let hint = self.hint.map(|(text, color)| ui.painter().layout_no_wrap(text.to_owned(), FontId::proportional(11.5), color));
        let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
        let t = ui.ctx().animate_bool_with_time(resp.id.with("active"), self.active, 0.15);
        let hover_t = ui.ctx().animate_bool_with_time(resp.id.with("hover"), hovered && !self.active, 0.1);
        if lifted {
            painter.rect_filled(rect, CornerRadius::ZERO, pal.area_header);
            painter.rect_filled(rect, CornerRadius::ZERO, pal.tab_active_text.gamma_multiply(0.06));
        } else if hover_t > 0.0 {
            // Под указателем вся вкладка чуть светлее, а внизу проступает бледная полоска —
            // тот же знак, что у открытой вкладки, а не «кнопка» посреди строки.
            painter.rect_filled(rect, CornerRadius::ZERO, pal.tab_active_text.gamma_multiply(0.05 * hover_t));
            let bar = Rect::from_min_max(pos2(rect.left() + 3.0, rect.bottom() - 2.0), pos2(rect.right() - 3.0, rect.bottom()));
            painter.rect_filled(bar, CornerRadius::same(1), pal.tab_text.gamma_multiply(0.4 * hover_t));
        }
        if t > 0.0 {
            // Полоска «вырастает» из середины вкладки.
            let half = (rect.width() / 2.0 - 3.0).max(0.0) * t;
            let bar = Rect::from_min_max(pos2(rect.center().x - half, rect.bottom() - 2.0), pos2(rect.center().x + half, rect.bottom()));
            painter.rect_filled(bar, CornerRadius::same(1), pal.tab_indicator);
        }
        let cy = rect.center().y;
        let icon_rect = Rect::from_center_size(pos2(rect.left() + 15.0, cy), vec2(14.0, 14.0));
        crate::icons::paint(&painter, icon_rect, self.icon, pal.text_muted);
        let title_right = icon_rect.right() + 6.0 + galley.size().x;
        painter.galley(pos2(icon_rect.right() + 6.0, cy - galley.size().y / 2.0), galley, text_color);
        if let Some(g) = hint {
            let color = g.job.sections.first().map_or(pal.text_faint, |s| s.format.color);
            painter.galley(pos2(title_right + 6.0, cy - g.size().y / 2.0 + 0.5), g, color);
        }

        let close = Rect::from_center_size(pos2(rect.right() - 14.0, cy), vec2(16.0, 16.0));
        let on_close = hovered && resp.hover_pos().is_some_and(|p| close.contains(p));
        if hovered || (self.active && !self.dirty) {
            if on_close {
                painter.rect_filled(close, CornerRadius::same(4), pal.tab_hover);
            }
            crate::icons::paint(&painter, close.shrink(3.0), "close", if on_close { pal.tab_active_text } else { pal.tab_text });
        } else if self.dirty {
            painter.circle_filled(close.center(), 3.5, pal.accent);
        }
        on_close
    }
}

/// Высота ряда кнопок диалога.
pub const BUTTON_ROW_H: f32 = 30.0;

/// Ряд кнопок диалога, справа налево (главная — первой). Высота ряда — по кнопкам:
/// `ui.with_layout` отдал бы ряду всё свободное место окна и поставил кнопки посередине,
/// и в диалоге появилась бы пустота, тем большая, чем выше окно программы.
pub fn button_row<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.allocate_ui_with_layout(vec2(ui.available_width(), BUTTON_ROW_H), Layout::right_to_left(Align::Center), add).inner
}

/// Небольшая кнопка-значок в заголовке области.
pub fn header_button(ui: &mut Ui, pal: &Palette, icon: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(5), pal.tab_hover);
    }
    let color = if resp.hovered() { pal.tab_active_text } else { pal.tab_text };
    crate::icons::paint(ui.painter(), Rect::from_center_size(rect.center(), vec2(14.0, 14.0)), icon, color);
    resp
}

/// Сторона квадрата флажка и диаметр кружка переключателя.
const MARK: f32 = 15.0;
/// Отступ подписи от отметки.
const MARK_GAP: f32 = 8.0;
/// Где начинается подпись флажка или переключателя — чтобы выровнять под ней пояснение.
pub const CHECK_INDENT: f32 = MARK + MARK_GAP;

/// Вид отметки: квадрат — независимый выбор, круг — один вариант из нескольких.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mark {
    Square,
    Round,
}

/// Флажок — квадрат: независимый выбор («да или нет»). Щелчок меняет `checked`.
pub fn checkbox(ui: &mut Ui, pal: &Palette, checked: &mut bool, text: impl Into<egui::WidgetText>) -> egui::Response {
    let mut resp = marked(ui, pal, Mark::Square, *checked, text.into());
    if resp.clicked() {
        *checked = !*checked;
        resp.mark_changed();
    }
    resp
}

/// Переключатель — круг: один вариант из нескольких. `selected` — выбран ли этот вариант;
/// что делать по щелчку, решает вызывающий (или [`radio_value`]).
pub fn radio(ui: &mut Ui, pal: &Palette, selected: bool, text: impl Into<egui::WidgetText>) -> egui::Response {
    marked(ui, pal, Mark::Round, selected, text.into())
}

/// Переключатель варианта `value`: щелчок делает его текущим (`current`).
pub fn radio_value<T: PartialEq>(ui: &mut Ui, pal: &Palette, current: &mut T, value: T, text: impl Into<egui::WidgetText>) -> egui::Response {
    let mut resp = radio(ui, pal, *current == value, text);
    if resp.clicked() && *current != value {
        *current = value;
        resp.mark_changed();
    }
    resp
}

/// Отметка с подписью — общее у флажка и переключателя. Длинная подпись переносится (если
/// так настроен `ui`), отметка стоит по её первой строке.
fn marked(ui: &mut Ui, pal: &Palette, mark: Mark, on: bool, text: egui::WidgetText) -> egui::Response {
    let galley = text.into_galley(ui, None, (ui.available_width() - CHECK_INDENT).max(20.0), egui::TextStyle::Button);
    let height = galley.size().y.max(ui.spacing().interact_size.y).max(MARK);
    let (rect, resp) = ui.allocate_exact_size(vec2(CHECK_INDENT + galley.size().x, height), Sense::click());
    let kind = if mark == Mark::Square { egui::WidgetType::Checkbox } else { egui::WidgetType::RadioButton };
    resp.widget_info(|| egui::WidgetInfo::selected(kind, ui.is_enabled(), on, galley.text()));
    if !ui.is_rect_visible(rect) {
        return resp;
    }
    let enabled = ui.is_enabled();
    let hovered = enabled && resp.hovered();
    let dim = |c: Color32| if enabled { c } else { c.gamma_multiply(0.45) };
    let text_pos = pos2(rect.left() + CHECK_INDENT, rect.center().y - galley.size().y / 2.0);
    let first_row = galley.rows.first().map_or(galley.size().y, |r| r.rect().height());
    let center = pos2(rect.left() + MARK / 2.0, (text_pos.y + first_row / 2.0).round());
    let area = Rect::from_center_size(center, vec2(MARK, MARK));
    let (fill, stroke) = if on {
        (dim(if hovered { pal.accent.lerp_to_gamma(Color32::WHITE, 0.12) } else { pal.accent }), Stroke::NONE)
    } else {
        (if hovered { pal.bg_hover } else { Color32::TRANSPARENT }, Stroke::new(1.0, dim(if hovered { pal.text_muted } else { pal.text_faint })))
    };
    let painter = ui.painter();
    match mark {
        Mark::Square => {
            painter.rect(area, CornerRadius::same(3), fill, stroke, egui::StrokeKind::Inside);
            if on {
                let at = |x: f32, y: f32| pos2(area.left() + x * MARK, area.top() + y * MARK);
                painter.add(egui::Shape::line(vec![at(0.24, 0.52), at(0.43, 0.70), at(0.77, 0.31)], Stroke::new(1.8, dim(pal.accent_text))));
            }
        }
        Mark::Round => {
            painter.circle(center, MARK / 2.0 - stroke.width / 2.0, fill, stroke);
            if on {
                painter.circle_filled(center, MARK * 0.19, dim(pal.accent_text));
            }
        }
    }
    // Фокус с клавиатуры (Tab) — кольцо вокруг отметки.
    if resp.has_focus() {
        let ring = Stroke::new(1.5, pal.accent.gamma_multiply(0.7));
        match mark {
            Mark::Square => painter.rect_stroke(area.expand(2.5), CornerRadius::same(5), ring, egui::StrokeKind::Middle),
            Mark::Round => painter.circle_stroke(center, MARK / 2.0 + 2.5, ring),
        };
    }
    painter.galley(text_pos, galley, dim(pal.text));
    resp
}

/// Ширина переключателя-сегментов (без растягивания на всю ширину).
pub fn segmented_width(ui: &Ui, labels: &[&str]) -> f32 {
    let font = FontId::proportional(13.0);
    labels.iter().map(|l| ui.painter().layout_no_wrap((*l).to_owned(), font.clone(), Color32::WHITE).size().x + 24.0).sum::<f32>() + 6.0
}

/// Переключатель-сегменты: подсветка выбранного варианта плавно «переезжает».
pub fn segmented(ui: &mut Ui, pal: &Palette, id: Id, labels: &[&str], selected: usize, full_width: bool) -> Option<usize> {
    let font = FontId::proportional(13.0);
    let galleys: Vec<_> = labels.iter().map(|l| ui.painter().layout_no_wrap((*l).to_owned(), font.clone(), pal.text)).collect();
    let h = 28.0;
    let inner = 3.0;
    let widths: Vec<f32> = if full_width {
        let w = (ui.available_width() - inner * 2.0) / labels.len() as f32;
        vec![w; labels.len()]
    } else {
        galleys.iter().map(|g| g.size().x + 24.0).collect()
    };
    let total: f32 = widths.iter().sum::<f32>() + inner * 2.0;
    let (rect, _) = ui.allocate_exact_size(vec2(total, h), Sense::hover());
    let painter = ui.painter().clone();
    painter.rect_filled(rect, CornerRadius::same(9), pal.segmented_bg);
    let mut xs = Vec::with_capacity(widths.len());
    let mut x = inner;
    for w in &widths {
        xs.push(x);
        x += w;
    }
    let sel = selected.min(labels.len().saturating_sub(1));
    let ax = ui.ctx().animate_value_with_time(id.with("x"), xs[sel], 0.2);
    let aw = ui.ctx().animate_value_with_time(id.with("w"), widths[sel], 0.2);
    let pill = Rect::from_min_size(pos2(rect.left() + ax, rect.top() + inner), vec2(aw, h - inner * 2.0));
    painter.rect(
        pill,
        CornerRadius::same(7),
        pal.segmented_selected,
        Stroke::new(1.0, pal.border),
        egui::StrokeKind::Inside,
    );
    let mut clicked = None;
    for (i, galley) in galleys.into_iter().enumerate() {
        let item = Rect::from_min_size(pos2(rect.left() + xs[i], rect.top()), vec2(widths[i], h));
        let resp = ui.interact(item, id.with(i), Sense::click());
        let hover = ui.ctx().animate_bool_with_time(id.with(("h", i)), resp.hovered(), 0.12);
        let color = if i == sel { pal.heading } else { pal.text_muted.lerp_to_gamma(pal.text, hover) };
        painter.galley(item.center() - galley.size() / 2.0, galley, color);
        if resp.on_hover_cursor(CursorIcon::PointingHand).clicked() && i != sel {
            clicked = Some(i);
        }
    }
    clicked
}

/// Число для подписи: без лишних нулей в конце (`14`, `14.5`, `1.45`).
pub fn trim_number(value: f32, decimals: usize) -> String {
    let s = format!("{value:.decimals$}");
    if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_owned() } else { s }
}

/// Сколько знаков после точки нужно, чтобы показать шаг `step` (`0.5` → 1, `0.05` → 2).
fn step_decimals(step: f32) -> usize {
    (0..4).find(|&d| ((step * 10f32.powi(d as i32)).round() - step * 10f32.powi(d as i32)).abs() < 1e-3).unwrap_or(3)
}

/// Значение в пределах и на сетке шага (от начала пределов).
fn snap(value: f32, range: &std::ops::RangeInclusive<f32>, step: f32) -> f32 {
    let (lo, hi) = (*range.start(), *range.end());
    let v = if step > 0.0 { lo + ((value - lo) / step).round() * step } else { value };
    // Округление убирает хвосты вроде 1.4500001 после сложения шагов.
    let d = step_decimals(step) as i32 + 1;
    let p = 10f32.powi(d);
    ((v * p).round() / p).clamp(lo, hi)
}

/// Что поле числа помнит между кадрами.
#[derive(Clone, Debug, Default)]
struct NumberMemory {
    /// Протяжка: значение в начале и пройденное мышью расстояние.
    drag: Option<(f32, f32)>,
    /// Ввод с клавиатуры: набранный текст и нужно ли отдать полю фокус.
    edit: Option<(String, bool)>,
}

/// Ширина стрелки шага у края поля числа.
const NUMBER_ARROW_W: f32 = 22.0;

/// Поле числа вместо ползунка, как поля чисел в Blender: значение с единицей посередине,
/// по краям стрелки шага, а мягкая заливка показывает, где значение в пределах `range`.
/// Протяжка влево-вправо меняет значение (с Shift — медленнее), щелчок — ввод числа с
/// клавиатуры (Enter — принять, Escape — отменить), стрелки клавиатуры — шаг.
#[allow(clippy::too_many_arguments)]
pub fn number_field(ui: &mut Ui, pal: &Palette, id: Id, value: &mut f32, range: std::ops::RangeInclusive<f32>, step: f32, suffix: &str, width: f32) -> egui::Response {
    let (lo, hi) = (*range.start(), *range.end());
    let decimals = step_decimals(step);
    let width = width.min(ui.available_width()).max(90.0);
    let (rect, _) = ui.allocate_exact_size(vec2(width, 28.0), Sense::hover());
    let mut resp = ui.interact(rect, id, Sense::click_and_drag());
    let enabled = ui.is_enabled();
    let mut mem: NumberMemory = ui.data(|d| d.get_temp(id)).unwrap_or_default();
    let before = *value;
    let left = Rect::from_min_size(rect.min, vec2(NUMBER_ARROW_W, rect.height()));
    let right = Rect::from_min_max(pos2(rect.right() - NUMBER_ARROW_W, rect.top()), rect.max);
    let pointer = resp.interact_pointer_pos().or(resp.hover_pos());
    let on_left = pointer.is_some_and(|p| left.contains(p));
    let on_right = pointer.is_some_and(|p| right.contains(p));

    if enabled && mem.edit.is_none() {
        // Протяжка — только из середины: со стрелок тянуть нечего.
        if resp.drag_started() && !on_left && !on_right {
            mem.drag = Some((*value, 0.0));
        }
        if let Some((start, dx)) = &mut mem.drag {
            if resp.dragged() {
                let slow = if ui.input(|i| i.modifiers.shift) { 0.2 } else { 1.0 };
                *dx += resp.drag_delta().x * slow;
                // Весь диапазон — примерно за полторы ширины поля.
                let per_px = (hi - lo) / (rect.width() * 1.5);
                *value = snap(*start + *dx * per_px, &range, step);
            }
            if resp.drag_stopped() || !resp.dragged() {
                mem.drag = None;
            }
        } else if resp.clicked() {
            if on_left {
                *value = snap(*value - step, &range, step);
            } else if on_right {
                *value = snap(*value + step, &range, step);
            } else {
                mem.edit = Some((trim_number(*value, decimals), true));
            }
        }
        if resp.has_focus() {
            let (down, up) = ui.input_mut(|i| {
                let down = i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft) || i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown);
                let up = i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight) || i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp);
                (down, up)
            });
            if down {
                *value = snap(*value - step, &range, step);
            }
            if up {
                *value = snap(*value + step, &range, step);
            }
        }
    }

    let painter = ui.painter_at(rect.expand(3.0));
    let editing = mem.edit.is_some();
    let active = editing || mem.drag.is_some();
    let hovered = enabled && resp.hovered();
    let dim = |c: Color32| if enabled { c } else { c.gamma_multiply(0.45) };
    let fill = if hovered && !editing { pal.control_hover } else { pal.input_bg };
    let stroke = if active || resp.has_focus() { pal.input_focus } else { pal.border };
    painter.rect(rect, CornerRadius::same(7), fill, Stroke::new(1.0, dim(stroke)), egui::StrokeKind::Inside);
    if !editing {
        // Заливка до места значения в пределах: та же скруглённая плашка, обрезанная справа.
        let t = if hi > lo { ((*value - lo) / (hi - lo)).clamp(0.0, 1.0) } else { 0.0 };
        let filled = Rect::from_min_max(rect.min, pos2(rect.left() + rect.width() * t, rect.bottom()));
        ui.painter().with_clip_rect(filled.intersect(ui.clip_rect())).rect_filled(rect.shrink(1.0), CornerRadius::same(6), dim(pal.accent_soft.gamma_multiply(0.8)));
        // Стрелки шага — только под указателем (как в Blender), у предела — бледнее.
        let arrows = enabled && (hovered || resp.has_focus()) && mem.drag.is_none();
        for (zone, sign, hot) in [(left, -1.0f32, on_left), (right, 1.0, on_right)] {
            if !arrows {
                break;
            }
            let can = if sign < 0.0 { *value > lo } else { *value < hi };
            if hot && hovered && can {
                painter.rect_filled(zone.shrink(3.0), CornerRadius::same(5), pal.bg_hover);
            }
            let color = if !can { pal.text_faint.gamma_multiply(0.4) } else if hot && hovered { pal.text } else { pal.text_faint };
            let c = zone.center();
            let (w, h) = (2.6, 4.2);
            let tip = pos2(c.x + sign * w, c.y);
            let pts = vec![pos2(c.x - sign * w, c.y - h), tip, pos2(c.x - sign * w, c.y + h)];
            painter.add(egui::Shape::line(pts, Stroke::new(1.5, dim(color))));
        }
        let text = format!("{}{}", trim_number(*value, decimals), suffix);
        let color = if active { pal.heading } else { pal.text };
        let galley = painter.layout_no_wrap(text, FontId::proportional(13.5), dim(color));
        painter.galley(rect.center() - galley.size() / 2.0, galley, dim(color));
        if enabled {
            if on_left || on_right {
                resp = resp.on_hover_cursor(CursorIcon::PointingHand);
            } else {
                resp = resp.on_hover_cursor(CursorIcon::ResizeHorizontal);
            }
        }
    }

    // Ввод числа с клавиатуры прямо в поле.
    if let Some((text, focus)) = &mut mem.edit {
        let edit_id = id.with("edit");
        let inner = rect.shrink2(vec2(10.0, 0.0));
        let mut child = ui.new_child(UiBuilder::new().max_rect(inner).layout(Layout::left_to_right(Align::Center)));
        let edit = egui::TextEdit::singleline(text).id(edit_id).font(FontId::proportional(13.5)).frame(egui::Frame::NONE).horizontal_align(Align::Center).desired_width(inner.width());
        let out = edit.show(&mut child);
        if std::mem::take(focus) {
            out.response.request_focus();
            let mut state = out.state.clone();
            state.cursor.set_char_range(Some(egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(text.chars().count()))));
            state.store(ui.ctx(), edit_id);
        } else if out.response.lost_focus() {
            let cancel = ui.input(|i| i.key_pressed(egui::Key::Escape));
            if !cancel && let Ok(v) = text.trim().trim_end_matches(suffix.trim()).trim().replace(',', ".").parse::<f32>() {
                *value = snap(v, &range, step);
            }
            mem.edit = None;
        }
    }
    if *value != before {
        resp.mark_changed();
    }
    ui.data_mut(|d| d.insert_temp(id, mem));
    resp
}

/// Цвет текстом: `#rrggbb` (или `#rrggbbaa`, если он прозрачный).
pub fn color_hex(c: Color32) -> String {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    if a == 255 { format!("#{r:02x}{g:02x}{b:02x}") } else { format!("#{r:02x}{g:02x}{b:02x}{a:02x}") }
}

/// Поле цвета: образец и код `#rrggbb`; щелчок открывает палитру и поле для кода цвета.
pub fn color_field(ui: &mut Ui, pal: &Palette, id: Id, color: &mut Color32, width: f32) -> egui::Response {
    let width = width.min(ui.available_width()).max(90.0);
    let (rect, _) = ui.allocate_exact_size(vec2(width, 28.0), Sense::hover());
    let mut resp = ui.interact(rect, id, Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
    let popup_id = id.with("popup");
    let open = egui::Popup::is_id_open(ui.ctx(), popup_id);
    let enabled = ui.is_enabled();
    let dim = |c: Color32| if enabled { c } else { c.gamma_multiply(0.45) };
    let painter = ui.painter();
    let fill = if (resp.hovered() && enabled) || open { pal.control_hover } else { pal.input_bg };
    painter.rect(rect, CornerRadius::same(7), fill, Stroke::new(1.0, dim(if open { pal.input_focus } else { pal.border })), egui::StrokeKind::Inside);
    let swatch = Rect::from_center_size(pos2(rect.left() + 16.0, rect.center().y), vec2(18.0, 18.0));
    painter.rect(swatch, CornerRadius::same(5), dim(*color), Stroke::new(1.0, pal.border_strong), egui::StrokeKind::Inside);
    let hex = painter.layout_no_wrap(color_hex(*color), FontId::monospace(12.5), dim(pal.text));
    painter.galley(pos2(swatch.right() + 9.0, rect.center().y - hex.size().y / 2.0), hex, dim(pal.text));
    let before = *color;
    egui::Popup::menu(&resp).id(popup_id).style(theme::menu_style).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
        ui.spacing_mut().slider_width = 220.0;
        egui::color_picker::color_picker_color32(ui, color, egui::color_picker::Alpha::Opaque);
        ui.add_space(6.0);
        // Код цвета можно вставить или набрать: `#ff7b70`.
        let key = popup_id.with("hex");
        let mut text: String = ui.data(|d| d.get_temp(key)).unwrap_or_else(|| color_hex(*color));
        let r = ui.add(egui::TextEdit::singleline(&mut text).font(FontId::monospace(13.0)).desired_width(f32::INFINITY));
        if r.changed()
            && let Ok(c) = Color32::from_hex(text.trim())
        {
            *color = c;
        }
        if r.has_focus() {
            ui.data_mut(|d| d.insert_temp(key, text));
        } else {
            ui.data_mut(|d| d.remove::<String>(key));
        }
    });
    if *color != before {
        resp.mark_changed();
    }
    resp
}

/// Выпадающий список: поле с текущим значением, по щелчку — список вариантов с отметкой
/// у выбранного. Для выборов, которые со временем растут: язык, тема, шрифт.
/// Возвращает выбранный вариант, если он сменился.
pub fn dropdown<T: PartialEq + Clone>(ui: &mut Ui, pal: &Palette, id: Id, width: f32, current: &T, options: &[(T, String)]) -> Option<T> {
    let options: Vec<(T, String, Option<&str>, Option<&str>)> = options.iter().map(|(v, l)| (v.clone(), l.clone(), None, None)).collect();
    dropdown_full(ui, pal, id, width, current, &options)
}

/// Как [`dropdown`], но у вариантов могут быть значки (имена из [`crate::icons`]).
pub fn dropdown_with_icons<T: PartialEq + Clone>(ui: &mut Ui, pal: &Palette, id: Id, width: f32, current: &T, options: &[(T, String, Option<&str>)]) -> Option<T> {
    let options: Vec<(T, String, Option<&str>, Option<&str>)> = options.iter().map(|(v, l, i)| (v.clone(), l.clone(), *i, None)).collect();
    dropdown_full(ui, pal, id, width, current, &options)
}

/// Как [`dropdown`], но у вариантов может быть приглушённая пометка справа
/// (например, «По умолчанию» у встроенных тем).
pub fn dropdown_tagged<T: PartialEq + Clone>(ui: &mut Ui, pal: &Palette, id: Id, width: f32, current: &T, options: &[(T, String, Option<String>)]) -> Option<T> {
    let options: Vec<(T, String, Option<&str>, Option<&str>)> = options.iter().map(|(v, l, tag)| (v.clone(), l.clone(), None, tag.as_deref())).collect();
    dropdown_full(ui, pal, id, width, current, &options)
}

/// Выпадающий список: вариант, его значок и пометка справа.
fn dropdown_full<T: PartialEq + Clone>(ui: &mut Ui, pal: &Palette, id: Id, width: f32, current: &T, options: &[(T, String, Option<&str>, Option<&str>)]) -> Option<T> {
    let chosen = options.iter().find(|(v, ..)| v == current);
    let label = chosen.map(|(_, l, ..)| l.as_str()).unwrap_or("—");
    let tag_font = FontId::proportional(11.5);
    // В узкой области список сжимается до доступной ширины.
    let width = width.min(ui.available_width()).max(60.0);
    let (rect, _) = ui.allocate_exact_size(vec2(width, 28.0), Sense::hover());
    let resp = ui.interact(rect, id, Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
    let open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&resp));
    let painter = ui.painter();
    let fill = if resp.hovered() || open { pal.control_hover } else { pal.input_bg };
    let stroke = if open { pal.input_focus } else { pal.border };
    painter.rect(rect, CornerRadius::same(7), fill, Stroke::new(1.0, stroke), egui::StrokeKind::Inside);
    let mut x = rect.left() + 10.0;
    if let Some(icon) = chosen.and_then(|(_, _, i, _)| *i) {
        crate::icons::paint(painter, Rect::from_center_size(pos2(x + 7.0, rect.center().y), vec2(15.0, 15.0)), icon, pal.text);
        x += 22.0;
    }
    // Пометка выбранного варианта — перед стрелкой, если рядом с названием есть место.
    let mut text_right = rect.right() - 24.0;
    if let Some(tag) = chosen.and_then(|(.., t)| *t) {
        let g = painter.layout_no_wrap(tag.to_owned(), tag_font.clone(), pal.text_faint);
        if text_right - g.size().x - 8.0 - x >= 80.0 {
            text_right -= g.size().x + 8.0;
            painter.galley(pos2(text_right + 8.0, rect.center().y - g.size().y / 2.0), g, pal.text_faint);
        }
    }
    let galley = elide(ui, label, FontId::proportional(13.5), pal.text, text_right - x);
    painter.galley(pos2(x, rect.center().y - galley.size().y / 2.0), galley, pal.text);
    let arrow = Rect::from_center_size(pos2(rect.right() - 13.0, rect.center().y), vec2(14.0, 14.0));
    crate::icons::paint(painter, arrow, if open { "chevron-up" } else { "chevron-down" }, pal.text_muted);

    let mut picked = None;
    egui::Popup::menu(&resp).style(theme::menu_style).width(width).show(|ui| {
        ui.set_min_width(width - 12.0);
        egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
            for (value, text, icon, tag) in options {
                let selected = value == current;
                let button = match icon {
                    Some(icon) => {
                        let color = if selected { ui.visuals().selection.stroke.color } else { ui.visuals().widgets.inactive.fg_stroke.color };
                        egui::Button::new((crate::icons::image(ui.ctx(), icon, crate::icons::SIZE - 2.0, color), text.as_str()))
                    }
                    None => egui::Button::new(text.as_str()),
                };
                let resp = ui.add(button.selected(selected).min_size(vec2(ui.available_width(), 0.0)));
                if let Some(tag) = tag {
                    let color = if selected { ui.visuals().selection.stroke.color.gamma_multiply(0.75) } else { pal.text_faint };
                    ui.painter().text(pos2(resp.rect.right() - 30.0, resp.rect.center().y), egui::Align2::RIGHT_CENTER, *tag, tag_font.clone(), color);
                }
                if selected {
                    let mark = Rect::from_center_size(pos2(resp.rect.right() - 14.0, resp.rect.center().y), vec2(14.0, 14.0));
                    crate::icons::paint(ui.painter(), mark, "check", ui.visuals().selection.stroke.color);
                }
                if resp.clicked() && !selected {
                    picked = Some(value.clone());
                }
            }
        });
    });
    picked
}

/// Пункт верхнего меню с плавным появлением содержимого.
///
/// Меню открывается и щелчком, и простым наведением. Открытое наведением меню само
/// закрывается, когда курсор надолго уходит из него, а щелчок по заголовку «закрепляет» его.
/// `keep_open` — меню не закрывается по щелчку внутри (для переключателей);
/// обычные пункты в таком меню закрывают его сами через `ui.close()`.
pub fn top_menu(ui: &mut Ui, title: &str, keep_open: bool, add: impl FnOnce(&mut Ui)) {
    top_menu_with(ui, egui::Button::new(title), title, keep_open, add);
}

/// Пункт верхнего меню со значком вместо названия (например, «☰» в узком окне).
pub fn top_menu_icon(ui: &mut Ui, icon: &str, hint: &str, keep_open: bool, add: impl FnOnce(&mut Ui)) {
    let color = ui.visuals().widgets.inactive.fg_stroke.color;
    let img = crate::icons::image(ui.ctx(), icon, crate::icons::SIZE, color);
    top_menu_with(ui, egui::Button::new(img), icon, keep_open, add).on_hover_text(hint);
}

fn top_menu_with(ui: &mut Ui, button: egui::Button<'_>, id_salt: &str, keep_open: bool, add: impl FnOnce(&mut Ui)) -> egui::Response {
    use egui::containers::menu::MenuConfig;
    use egui::containers::SetOpenCommand;
    use egui::Popup;
    let ctx = ui.ctx().clone();
    let fade_id = ui.id().with(("menu-fade", id_salt));
    // Заголовок открытого меню подсвечен.
    let resp = ui
        .scope(|ui| {
            if Popup::is_id_open(&ctx, ui.next_auto_id().with("popup")) {
                let w = &mut ui.style_mut().visuals.widgets;
                w.inactive.weak_bg_fill = w.open.weak_bg_fill;
            }
            ui.add(button)
        })
        .inner;
    let popup_id = Popup::default_response_id(&resp);
    let by_hover_id = popup_id.with("by-hover");
    let away_id = popup_id.with("away-since");
    let is_open = Popup::is_id_open(&ctx, popup_id);
    let mut by_hover: bool = ctx.data(|d| d.get_temp(by_hover_id)).unwrap_or(false);
    let pointer_down = ctx.input(|i| i.pointer.any_down());
    let mut cmd = None;
    if resp.clicked() {
        // Щелчок по меню, открытому наведением, оставляет его открытым.
        if !(is_open && by_hover) {
            cmd = Some(SetOpenCommand::Toggle);
        }
        by_hover = false;
    } else if resp.hovered() && !is_open && !pointer_down {
        cmd = Some(SetOpenCommand::Bool(true));
        by_hover = true;
    }
    ctx.data_mut(|d| d.insert_temp(by_hover_id, by_hover));

    let mut config = theme::menu_config();
    if keep_open {
        config = config.close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
    }
    let inner = Popup::menu(&resp)
        .open_memory(cmd)
        .close_behavior(config.close_behavior)
        .style(config.style.clone())
        .info(egui::UiStackInfo::new(egui::UiKind::Menu).with_tag_value(MenuConfig::MENU_CONFIG_TAG, config))
        .show(|ui| {
            let t = ui.ctx().animate_bool_with_time(fade_id, true, 0.14);
            ui.multiply_opacity(t);
            if t < 1.0 {
                ui.ctx().request_repaint();
            }
            add(ui)
        });
    if inner.is_none() {
        ctx.animate_bool_with_time(fade_id, false, 0.0);
        ctx.data_mut(|d| d.remove::<f64>(away_id));
        return resp;
    }
    if by_hover {
        // Курсор над заголовком или над любым всплывающим слоем (само меню и подменю).
        let inside = ctx.pointer_hover_pos().is_some_and(|p| {
            resp.rect.expand(4.0).contains(p) || ctx.layer_id_at(p).is_some_and(|l| l.order == egui::Order::Foreground)
        });
        let now = ctx.input(|i| i.time);
        if inside {
            ctx.data_mut(|d| d.remove::<f64>(away_id));
        } else {
            let since: f64 = ctx.data_mut(|d| *d.get_temp_mut_or_insert_with(away_id, || now));
            if now - since > 0.45 {
                Popup::close_id(&ctx, popup_id);
                ctx.data_mut(|d| d.remove::<f64>(away_id));
                // Без событий мыши кадр сам не обновится — просим перерисовать.
                ctx.request_repaint();
            } else {
                ctx.request_repaint_after_secs(0.05);
            }
        }
    }
    resp
}

