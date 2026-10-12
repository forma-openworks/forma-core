//! Шапка окна вместо системной, как у Claude, VS Code и новых IDE JetBrains.
//!
//! Системная рамка у окна выключена (`with_decorations(false)`), а её работу делает сама
//! программа:
//! - кнопки «Свернуть», «Развернуть» и «Закрыть» в правом верхнем углу;
//! - пустое место верхней строки — заголовок окна: за него окно перетаскивают (с
//!   прилипанием к краям экрана, как обычно в Windows), двойной щелчок разворачивает;
//! - тонкие полоски по краям и углам меняют размер окна.
//!
//! Кнопки и края лежат в самом верхнем слое: окно можно свернуть, закрыть, подвинуть и
//! растянуть, даже когда открыт диалог.

use eframe::egui::{self, Color32, CornerRadius, CursorIcon, Id, Order, Pos2, Rect, ResizeDirection, Sense, Stroke, StrokeKind, Ui, ViewportCommand, pos2, vec2};
use forma_i18n::t;
use forma_ui::theme::Palette;

/// Ширина одной кнопки окна — как у Windows 11.
const BUTTON_W: f32 = 46.0;
/// Ширина всех трёх кнопок окна: верхняя строка оставляет под них место справа.
pub const BUTTONS_W: f32 = BUTTON_W * 3.0;
/// Высота шапки на экране приветствия (там нет строки меню).
pub const WELCOME_H: f32 = 32.0;
/// Толщина краёв, за которые меняют размер окна, и размер углов.
const EDGE: f32 = 4.0;
const CORNER: f32 = 10.0;

#[derive(Clone, Copy, PartialEq)]
enum Button {
    Minimize,
    Maximize,
    Close,
}

/// Пустое место шапки в прямоугольнике `rect`: перетаскивание окна и разворот двойным
/// щелчком. Вызывается до остальных виджетов шапки, чтобы они лежали поверх него и
/// щелчки по ним до шапки не доходили.
pub fn drag_area(ui: &Ui, rect: Rect, id: Id) {
    let resp = ui.interact(rect, id, Sense::click());
    let ctx = ui.ctx();
    if resp.double_clicked() {
        toggle_maximized(ctx);
        return;
    }
    // Нажатие запоминается сразу: как только указатель уходит дальше, чем для щелчка,
    // egui перестаёт считать кнопку нажатой на шапке (она ведь не тянется сама).
    // Перетаскивание начинается один раз за нажатие: повторная команда после того, как
    // Windows уже повела окно, запустила бы второе перетаскивание после отпускания кнопки.
    let state_id = id.with("drag-state");
    let (down, dragging) = ctx.input(|i| (i.pointer.primary_down(), i.pointer.is_decidedly_dragging()));
    let mut state: DragState = ctx.data(|d| d.get_temp(state_id)).unwrap_or_default();
    let before = state;
    if !down {
        state = DragState::Idle;
    } else if state == DragState::Idle && resp.is_pointer_button_down_on() && !dragging {
        state = DragState::Pressed;
    } else if state == DragState::Pressed && dragging {
        ctx.send_viewport_cmd(ViewportCommand::StartDrag);
        state = DragState::Moving;
    }
    if state != before {
        ctx.data_mut(|d| d.insert_temp(state_id, state));
    }
}

/// Перетаскивание окна за шапку в пределах одного нажатия кнопки.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum DragState {
    #[default]
    Idle,
    /// Кнопку нажали на пустом месте шапки.
    Pressed,
    /// Окно уже ведёт Windows.
    Moving,
}

fn maximized(ctx: &egui::Context) -> bool {
    ctx.input(|i| i.viewport().maximized.unwrap_or(false))
}

fn toggle_maximized(ctx: &egui::Context) {
    ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized(ctx)));
}

/// Кнопки окна и края для изменения размера — поверх всего, в конце кадра. `height` —
/// высота шапки, `modal_open` — открыт ли диалог (тогда за шапку тоже можно тянуть окно).
pub fn window_frame(ctx: &egui::Context, pal: &Palette, height: f32, modal_open: bool) {
    let screen = ctx.content_rect();
    // Пока открыт диалог, верхняя строка под затемнением — за неё всё равно можно
    // перетащить окно. В маленьком окне диалог заходит под строку: там главнее он.
    if modal_open {
        let mut rect = Rect::from_min_size(screen.min, vec2(screen.width() - BUTTONS_W, height));
        if let Some(dialog) = ctx.memory(|m| m.area_rect(Id::new(crate::chrome::MODAL_ID)))
            && dialog.intersects(rect)
        {
            rect.max.y = rect.max.y.min(dialog.top());
        }
        if rect.height() >= 2.0 {
            egui::Area::new(Id::new("forma-caption-modal")).order(Order::Tooltip).fixed_pos(rect.min).show(ctx, |ui| {
                ui.allocate_rect(rect, Sense::hover());
                drag_area(ui, rect, Id::new("forma-caption-modal-drag"));
            });
        }
    }
    window_buttons(ctx, pal, screen, height);
    if !maximized(ctx) && !ctx.input(|i| i.viewport().fullscreen.unwrap_or(false)) {
        resize_edges(ctx, screen);
    }
}

fn window_buttons(ctx: &egui::Context, pal: &Palette, screen: Rect, height: f32) {
    let is_max = maximized(ctx);
    egui::Area::new(Id::new("forma-window-buttons")).order(Order::Tooltip).fixed_pos(pos2(screen.right() - BUTTONS_W, screen.top())).show(ctx, |ui| {
        ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
        ui.horizontal(|ui| {
            for button in [Button::Minimize, Button::Maximize, Button::Close] {
                let (rect, resp) = ui.allocate_exact_size(vec2(BUTTON_W, height), Sense::click());
                let hover = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.1);
                let pressed = resp.is_pointer_button_down_on();
                let (fill, glyph) = match button {
                    Button::Close => (pal.window_close_hover, pal.menubar_text.lerp_to_gamma(pal.window_close_text, hover)),
                    _ => (pal.window_button_hover, pal.menubar_text),
                };
                if hover > 0.0 {
                    let fill = if pressed { fill.gamma_multiply(0.8) } else { fill };
                    ui.painter().rect_filled(rect, CornerRadius::ZERO, fill.gamma_multiply(hover));
                }
                paint_glyph(ui, rect.center(), button, is_max, glyph);
                let hint = match button {
                    Button::Minimize => t!("window-minimize"),
                    Button::Maximize if is_max => t!("window-restore"),
                    Button::Maximize => t!("window-maximize"),
                    Button::Close => t!("window-close"),
                };
                if resp.on_hover_text(hint).clicked() {
                    let ctx = ui.ctx();
                    match button {
                        Button::Minimize => ctx.send_viewport_cmd(ViewportCommand::Minimized(true)),
                        Button::Maximize => toggle_maximized(ctx),
                        Button::Close => ctx.send_viewport_cmd(ViewportCommand::Close),
                    }
                }
            }
        });
    });
}

/// Значок кнопки окна тонкими линиями, как у Windows 11: линии попадают точно в пиксели.
fn paint_glyph(ui: &Ui, center: Pos2, button: Button, is_max: bool, color: Color32) {
    let ppp = ui.ctx().pixels_per_point();
    // Линия в целое число пикселей экрана, а её середина — на середине пикселя.
    let width = ppp.round().max(1.0) / ppp;
    let snap = |v: f32| ((v * ppp).floor() + 0.5) / ppp;
    let stroke = Stroke::new(width, color);
    let c = pos2(snap(center.x), snap(center.y));
    let painter = ui.painter();
    match button {
        Button::Minimize => {
            painter.line_segment([pos2(c.x - 5.0, c.y), pos2(c.x + 5.0, c.y)], stroke);
        }
        Button::Maximize if is_max => {
            // «Восстановить»: передний квадрат и выглядывающий из-за него задний.
            let front = Rect::from_min_max(pos2(c.x - 5.0, c.y - 3.0), pos2(c.x + 3.0, c.y + 5.0));
            painter.rect_stroke(front, CornerRadius::same(1), stroke, StrokeKind::Middle);
            painter.add(egui::Shape::line(
                vec![pos2(c.x - 3.0, c.y - 3.0), pos2(c.x - 3.0, c.y - 5.0), pos2(c.x + 5.0, c.y - 5.0), pos2(c.x + 5.0, c.y + 3.0), pos2(c.x + 3.0, c.y + 3.0)],
                stroke,
            ));
        }
        Button::Maximize => {
            painter.rect_stroke(Rect::from_center_size(c, vec2(10.0, 10.0)), CornerRadius::same(1), stroke, StrokeKind::Middle);
        }
        Button::Close => {
            let d = 5.0;
            painter.line_segment([pos2(c.x - d, c.y - d), pos2(c.x + d, c.y + d)], stroke);
            painter.line_segment([pos2(c.x - d, c.y + d), pos2(c.x + d, c.y - d)], stroke);
        }
    }
}

/// Края и углы окна, за которые меняют его размер.
fn resize_edges(ctx: &egui::Context, screen: Rect) {
    use ResizeDirection::*;
    let (l, r, t, b) = (screen.left(), screen.right(), screen.top(), screen.bottom());
    let edges = [
        (North, Rect::from_min_max(pos2(l + CORNER, t), pos2(r - CORNER, t + EDGE)), CursorIcon::ResizeVertical),
        (South, Rect::from_min_max(pos2(l + CORNER, b - EDGE), pos2(r - CORNER, b)), CursorIcon::ResizeVertical),
        (West, Rect::from_min_max(pos2(l, t + CORNER), pos2(l + EDGE, b - CORNER)), CursorIcon::ResizeHorizontal),
        (East, Rect::from_min_max(pos2(r - EDGE, t + CORNER), pos2(r, b - CORNER)), CursorIcon::ResizeHorizontal),
        (NorthWest, Rect::from_min_max(pos2(l, t), pos2(l + CORNER, t + EDGE)), CursorIcon::ResizeNwSe),
        (NorthWest, Rect::from_min_max(pos2(l, t), pos2(l + EDGE, t + CORNER)), CursorIcon::ResizeNwSe),
        (NorthEast, Rect::from_min_max(pos2(r - CORNER, t), pos2(r, t + EDGE)), CursorIcon::ResizeNeSw),
        (NorthEast, Rect::from_min_max(pos2(r - EDGE, t), pos2(r, t + CORNER)), CursorIcon::ResizeNeSw),
        (SouthWest, Rect::from_min_max(pos2(l, b - EDGE), pos2(l + CORNER, b)), CursorIcon::ResizeNeSw),
        (SouthWest, Rect::from_min_max(pos2(l, b - CORNER), pos2(l + EDGE, b)), CursorIcon::ResizeNeSw),
        (SouthEast, Rect::from_min_max(pos2(r - CORNER, b - EDGE), pos2(r, b)), CursorIcon::ResizeNwSe),
        (SouthEast, Rect::from_min_max(pos2(r - EDGE, b - CORNER), pos2(r, b)), CursorIcon::ResizeNwSe),
    ];
    for (i, (dir, rect, cursor)) in edges.into_iter().enumerate() {
        egui::Area::new(Id::new(("forma-resize-edge", i))).order(Order::Tooltip).fixed_pos(rect.min).show(ctx, |ui| {
            let (_, resp) = ui.allocate_exact_size(rect.size(), Sense::drag());
            let resp = resp.on_hover_cursor(cursor);
            if resp.drag_started() {
                ui.ctx().send_viewport_cmd(ViewportCommand::BeginResize(dir));
            }
        });
    }
}

/// Скругляет углы окна без системной рамки, как у остальных окон Windows 11.
#[cfg(windows)]
pub fn round_corners(cc: &eframe::CreationContext<'_>) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::Graphics::Dwm::{DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmSetWindowAttribute};
    let Ok(handle) = cc.window_handle() else { return };
    if let RawWindowHandle::Win32(h) = handle.as_raw() {
        let preference = DWMWCP_ROUND;
        // Windows 10 этого атрибута не знает и просто вернёт ошибку — углы останутся прямыми.
        unsafe {
            DwmSetWindowAttribute(h.hwnd.get() as _, DWMWA_WINDOW_CORNER_PREFERENCE as u32, (&raw const preference).cast(), size_of_val(&preference) as u32);
        }
    }
}

#[cfg(not(windows))]
pub fn round_corners(_cc: &eframe::CreationContext<'_>) {}
