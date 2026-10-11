//! Виджеты в фирменном стиле FORMA (перенесены из Forma: Holotype).

use egui::{ Color32, CornerRadius, CursorIcon, FontId, Id, Rect, Sense, Stroke, Ui, pos2, vec2};

use crate::theme::{self, Palette};

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
    painter.rect_filled(rect, CornerRadius::same(9), if pal.dark { pal.bg.gamma_multiply(0.85) } else { pal.bg_hover });
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
        if pal.dark { pal.bg_hover } else { Color32::WHITE },
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

/// Пункт верхнего меню с плавным появлением содержимого.
///
/// Меню открывается и щелчком, и простым наведением. Открытое наведением меню само
/// закрывается, когда курсор надолго уходит из него, а щелчок по заголовку «закрепляет» его.
/// `keep_open` — меню не закрывается по щелчку внутри (для переключателей);
/// обычные пункты в таком меню закрывают его сами через `ui.close()`.
pub fn top_menu(ui: &mut Ui, title: &str, keep_open: bool, add: impl FnOnce(&mut Ui)) {
    use egui::containers::menu::MenuConfig;
    use egui::containers::SetOpenCommand;
    use egui::Popup;
    let ctx = ui.ctx().clone();
    let fade_id = ui.id().with(("menu-fade", title));
    // Заголовок открытого меню подсвечен.
    let resp = ui
        .scope(|ui| {
            if Popup::is_id_open(&ctx, ui.next_auto_id().with("popup")) {
                let w = &mut ui.style_mut().visuals.widgets;
                w.inactive.weak_bg_fill = w.open.weak_bg_fill;
            }
            ui.add(egui::Button::new(title))
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
        return;
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
}

