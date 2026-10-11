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

/// Выпадающий список: поле с текущим значением, по щелчку — список вариантов с отметкой
/// у выбранного. Для выборов, которые со временем растут: язык, тема, шрифт.
/// Возвращает выбранный вариант, если он сменился.
pub fn dropdown<T: PartialEq + Clone>(ui: &mut Ui, pal: &Palette, id: Id, width: f32, current: &T, options: &[(T, String)]) -> Option<T> {
    let options: Vec<(T, String, Option<&str>)> = options.iter().map(|(v, l)| (v.clone(), l.clone(), None)).collect();
    dropdown_with_icons(ui, pal, id, width, current, &options)
}

/// Как [`dropdown`], но у вариантов могут быть значки (имена из [`crate::icons`]).
pub fn dropdown_with_icons<T: PartialEq + Clone>(ui: &mut Ui, pal: &Palette, id: Id, width: f32, current: &T, options: &[(T, String, Option<&str>)]) -> Option<T> {
    let chosen = options.iter().find(|(v, ..)| v == current);
    let label = chosen.map(|(_, l, _)| l.as_str()).unwrap_or("—");
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
    if let Some(icon) = chosen.and_then(|(_, _, i)| *i) {
        crate::icons::paint(painter, Rect::from_center_size(pos2(x + 7.0, rect.center().y), vec2(15.0, 15.0)), icon, pal.text);
        x += 22.0;
    }
    let galley = elide(ui, label, FontId::proportional(13.5), pal.text, rect.right() - 24.0 - x);
    painter.galley(pos2(x, rect.center().y - galley.size().y / 2.0), galley, pal.text);
    let arrow = Rect::from_center_size(pos2(rect.right() - 13.0, rect.center().y), vec2(14.0, 14.0));
    crate::icons::paint(painter, arrow, if open { "chevron-up" } else { "chevron-down" }, pal.text_muted);

    let mut picked = None;
    egui::Popup::menu(&resp).style(theme::menu_style).width(width).show(|ui| {
        ui.set_min_width(width - 12.0);
        egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
            for (value, text, icon) in options {
                let selected = value == current;
                let button = match icon {
                    Some(icon) => {
                        let color = if selected { ui.visuals().selection.stroke.color } else { ui.visuals().widgets.inactive.fg_stroke.color };
                        egui::Button::new((crate::icons::image(ui.ctx(), icon, crate::icons::SIZE - 2.0, color), text.as_str()))
                    }
                    None => egui::Button::new(text.as_str()),
                };
                let resp = ui.add(button.selected(selected).min_size(vec2(ui.available_width(), 0.0)));
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

