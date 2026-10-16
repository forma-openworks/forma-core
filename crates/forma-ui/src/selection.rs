//! Выделенный текст и контекстное меню над текстом (режим чтения заметки, редактор).
//!
//! Две беды egui, которые здесь обходятся:
//! - выделение в подписях egui наружу не отдаёт — только кладёт текст в буфер обмена по
//!   Ctrl+C. Поэтому мы «нажимаем Ctrl+C» сами ([`request`]) и перехватываем текст раньше,
//!   чем он попадёт в буфер обмена: буфер пользователя не меняется, текст — в [`take`];
//! - любое нажатие мыши, и правой кнопкой тоже, сбрасывает выделение в подписи или поле
//!   ввода. В IDE правый щелчок по выделенному оставляет его выделенным, поэтому внутри
//!   областей [`menu_area`] правый щелчок до egui не доходит: выделение остаётся, текст
//!   выделения перехватывается, а область узнаёт, что пора открыть своё меню.

use std::collections::HashMap;

use egui::{Context, FullOutput, Id, OutputCommand, PointerButton, Pos2, RawInput, Rect, Ui, ViewportId};

#[derive(Default, PartialEq)]
enum Grab {
    #[default]
    Idle,
    /// Скопировать выделение окна `ViewportId` в следующем кадре.
    Asked(ViewportId),
    /// Копирование идёт: перехватить текст из вывода кадра.
    Copying,
}

#[derive(Default)]
struct SelectionGrab {
    state: Grab,
    text: Option<String>,
    /// Области с собственным меню по окнам: заполняются за кадр, читаются в начале
    /// следующего кадра этого окна.
    areas: HashMap<ViewportId, Vec<(Id, Rect)>>,
    /// В какой области и где был правый щелчок этого кадра.
    clicked: Option<(Id, Pos2)>,
    /// Отпускание правой кнопки после спрятанного нажатия тоже прячем.
    swallow_release: bool,
}

impl egui::Plugin for SelectionGrab {
    fn debug_name(&self) -> &'static str {
        "forma-selection-grab"
    }

    fn on_begin_pass(&mut self, ui: &mut Ui) {
        self.areas.insert(ui.ctx().viewport_id(), Vec::new());
    }

    fn input_hook(&mut self, _ctx: &Context, input: &mut RawInput) {
        let viewport = input.viewport_id;
        let areas = self.areas.get(&viewport).cloned().unwrap_or_default();
        let mut hit = None;
        let mut swallow_release = self.swallow_release;
        input.events.retain(|e| match e {
            egui::Event::PointerButton { pos, button: PointerButton::Secondary, pressed: true, .. } => match areas.iter().find(|(_, r)| r.contains(*pos)) {
                Some((id, _)) => {
                    hit = Some((*id, *pos));
                    swallow_release = true;
                    false
                }
                None => true,
            },
            egui::Event::PointerButton { button: PointerButton::Secondary, pressed: false, .. } if swallow_release => {
                swallow_release = false;
                false
            }
            _ => true,
        });
        self.swallow_release = swallow_release;
        if hit.is_some() {
            self.clicked = hit;
        }
        let asked = matches!(self.state, Grab::Asked(v) if v == viewport);
        if hit.is_some() || asked {
            input.events.push(egui::Event::Copy);
            self.state = Grab::Copying;
            self.text = None;
        }
    }

    fn output_hook(&mut self, _ctx: &Context, output: &mut FullOutput) {
        // Щелчок, который за кадр никто не забрал (область пропала), — забыт.
        self.clicked = None;
        if self.state != Grab::Copying {
            return;
        }
        self.state = Grab::Idle;
        let commands = &mut output.platform_output.commands;
        let text = commands.iter().rev().find_map(|c| match c {
            OutputCommand::CopyText(t) => Some(t.clone()),
            _ => None,
        });
        commands.retain(|c| !matches!(c, OutputCommand::CopyText(_)));
        self.text = Some(text.unwrap_or_default());
    }
}

/// Есть ли выделение в подписях (в любом окне).
pub fn has_selection(ctx: &Context) -> bool {
    ctx.with_plugin::<egui::text_selection::LabelSelectionState, _>(|s| s.has_selection()).unwrap_or(false)
}

/// Попросить текст выделения окна `viewport`: он будет готов через кадр-другой ([`take`]).
pub fn request(ctx: &Context, viewport: ViewportId) {
    ctx.plugin_or_default::<SelectionGrab>().lock().state = Grab::Asked(viewport);
    ctx.request_repaint_of(viewport);
}

/// Текст выделения, если он уже готов (пустой — выделения не было).
pub fn take(ctx: &Context) -> Option<String> {
    ctx.plugin_opt::<SelectionGrab>().and_then(|p| p.lock().text.take())
}

/// Область `rect` с собственным контекстным меню; вызывать каждый кадр, до содержимого.
/// Возвращает место правого щелчка в том кадре, когда он случился: тогда пора открыть меню
/// (например, [`crate::theme::context_menu_at`]), а текст выделения будет в [`take`] со
/// следующего кадра.
pub fn menu_area(ui: &Ui, id: Id, rect: Rect) -> Option<Pos2> {
    let plugin = ui.ctx().plugin_or_default::<SelectionGrab>();
    let mut g = plugin.lock();
    g.areas.entry(ui.ctx().viewport_id()).or_default().push((id, rect.intersect(ui.clip_rect())));
    match g.clicked {
        Some((hit, pos)) if hit == id => {
            g.clicked = None;
            Some(pos)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use egui::{Event, Modifiers, pos2, vec2};

    use super::*;

    /// Кадр без окна: текстуры кадра никуда не идут, их нужно просто забыть.
    fn frame(ctx: &Context, input: RawInput, f: impl FnMut(&mut Ui)) -> FullOutput {
        let mut out = ctx.run_ui(input, f);
        out.textures_delta.clear();
        out
    }

    fn copied(out: &FullOutput) -> Vec<String> {
        out.platform_output
            .commands
            .iter()
            .filter_map(|c| match c {
                OutputCommand::CopyText(t) => Some(t.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn grabbed_text_never_reaches_the_clipboard() {
        let ctx = Context::default();
        request(&ctx, ViewportId::ROOT);
        // Кадр с перехватом: в ввод добавлен Copy, подпись «копирует» — текст наш.
        let out = frame(&ctx, RawInput::default(), |ui| {
            assert!(ui.input(|i| i.events.contains(&Event::Copy)), "в ввод добавлен Copy");
            ui.ctx().copy_text("Edit Mode".into());
        });
        assert!(copied(&out).is_empty(), "буфер обмена не тронут");
        assert_eq!(take(&ctx).as_deref(), Some("Edit Mode"));
        assert_eq!(take(&ctx), None);
        // Обычное копирование проходит как было.
        let out = frame(&ctx, RawInput::default(), |ui| ui.ctx().copy_text("x".into()));
        assert_eq!(copied(&out), ["x"]);
    }

    #[test]
    fn right_click_in_a_menu_area_keeps_the_selection_and_opens_the_menu() {
        let ctx = Context::default();
        let id = Id::new("note");
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(200.0, 200.0));
        let press = |pos, pressed| Event::PointerButton { pos, button: PointerButton::Secondary, pressed, modifiers: Modifiers::NONE };
        frame(&ctx, RawInput::default(), |ui| assert_eq!(menu_area(ui, id, rect), None));
        // Нажатие внутри области egui не видит (иначе он сбросил бы выделение), а область
        // узнаёт, где открыть меню.
        let input = RawInput { events: vec![Event::PointerMoved(pos2(10.0, 10.0)), press(pos2(10.0, 10.0), true)], ..Default::default() };
        let out = frame(&ctx, input, |ui| {
            assert!(!ui.input(|i| i.pointer.secondary_pressed()));
            assert_eq!(menu_area(ui, id, rect), Some(pos2(10.0, 10.0)));
        });
        assert!(copied(&out).is_empty());
        assert_eq!(take(&ctx).as_deref(), Some(""), "выделения не было — пустой текст");
        // Отпускание тоже спрятано, а щелчок мимо области доходит до egui.
        let input = RawInput { events: vec![press(pos2(10.0, 10.0), false), press(pos2(500.0, 10.0), true)], ..Default::default() };
        frame(&ctx, input, |ui| {
            assert!(!ui.input(|i| i.pointer.secondary_released()));
            assert!(ui.input(|i| i.pointer.secondary_pressed()));
            assert_eq!(menu_area(ui, id, rect), None);
        });
    }
}
