//! Холст плагина ([`Element::Canvas`](forma_plugin_ui::Element::Canvas)) и сравнение «до и
//! после» ([`Element::Compare`](forma_plugin_ui::Element::Compare)).
//!
//! Холст вписывается в ширину места и не увеличивается. Холст с `pan_zoom` двигается,
//! если тянуть мышью, и приближается Ctrl и колесом — как редактор нод Blender; простое
//! колесо по-прежнему прокручивает заметку. Двойной щелчок возвращает вид как было.

use egui::epaint::{CubicBezierShape, Mesh, Vertex, WHITE_UV};
use egui::load::{SizeHint, TexturePoll};
use egui::{Align2, Color32, CornerRadius, CursorIcon, FontFamily, FontId, Id, Pos2, Rect, RichText, Sense, Shape as EShape, Stroke, TextureOptions, Ui, Vec2, pos2, vec2};
use forma_plugin_ui::{Align, Point, Shape};

use crate::elements::Hooks;
use crate::theme::Palette;

/// Цвет фигуры: `#rrggbb`, `#rrggbbaa` или имя цвета темы. Непонятное — цвет текста.
pub fn color(pal: &Palette, s: &str) -> Color32 {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix('#') {
        let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
        let short = |i: usize| u8::from_str_radix(hex.get(i..i + 1)?, 16).ok().map(|v| v * 17);
        let rgba = match hex.len() {
            8 => (byte(0), byte(2), byte(4), byte(6)),
            6 => (byte(0), byte(2), byte(4), Some(255)),
            3 => (short(0), short(1), short(2), Some(255)),
            _ => (None, None, None, None),
        };
        if let (Some(r), Some(g), Some(b), Some(a)) = rgba {
            return Color32::from_rgba_unmultiplied(r, g, b, a);
        }
        return pal.text;
    }
    match s {
        "muted" => pal.text_muted,
        "faint" => pal.text_faint,
        "heading" => pal.heading,
        "accent" => pal.accent,
        "success" => pal.success,
        "warning" => pal.warning,
        "danger" => pal.danger,
        "border" => pal.border,
        "bg" => pal.bg,
        "panel" => pal.bg_panel,
        "card" => pal.settings_card,
        _ => pal.text,
    }
}

/// Как холст сейчас показан: сдвиг (в точках экрана) и приближение.
#[derive(Clone, Copy, Debug, PartialEq)]
struct View {
    offset: Vec2,
    zoom: f32,
}

impl Default for View {
    fn default() -> Self {
        Self { offset: Vec2::ZERO, zoom: 1.0 }
    }
}

fn radius(r: &[f32], s: f32) -> CornerRadius {
    let c = |v: f32| (v * s).round().clamp(0.0, 255.0) as u8;
    match r {
        [] => CornerRadius::ZERO,
        [a] => CornerRadius::same(c(*a)),
        [nw, ne, sw, se, ..] => CornerRadius { nw: c(*nw), ne: c(*ne), sw: c(*sw), se: c(*se) },
        [a, ..] => CornerRadius::same(c(*a)),
    }
}

fn align2(h: Align, v: Align) -> Align2 {
    let x = match h {
        Align::Start => egui::Align::Min,
        Align::Center => egui::Align::Center,
        Align::End => egui::Align::Max,
    };
    let y = match v {
        Align::Start => egui::Align::Min,
        Align::Center => egui::Align::Center,
        Align::End => egui::Align::Max,
    };
    Align2([x, y])
}

/// Точка кубической кривой Безье.
fn bezier_at(p: [Pos2; 4], t: f32) -> Pos2 {
    let u = 1.0 - t;
    let w = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
    pos2(w.iter().zip(p).map(|(w, p)| w * p.x).sum(), w.iter().zip(p).map(|(w, p)| w * p.y).sum())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn canvas(ui: &mut Ui, pal: &Palette, id: Id, width: f32, height: f32, shapes: &[Shape], pan_zoom: bool, background: Option<&str>, hooks: &mut dyn Hooks) {
    let width = width.max(1.0);
    let height = height.max(1.0);
    // Холст вписывается в ширину; холст, который можно двигать, мельче 0,85 не становится —
    // иначе текст на нём не прочесть, а остальное видно, если подвинуть.
    let avail = ui.available_width();
    let fit = (avail / width).clamp(0.05, 1.0);
    let scale = if pan_zoom { fit.clamp(0.85, 1.0) } else { fit };
    let sense = if pan_zoom { Sense::click_and_drag() } else { Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(vec2((width * scale).min(avail), height * scale), sense);
    let view_id = id.with("canvas-view");
    let mut view: View = ui.data(|d| d.get_temp(view_id)).unwrap_or_default();
    if pan_zoom {
        if resp.dragged() {
            view.offset += resp.drag_delta();
        }
        let zoom = ui.input(|i| i.zoom_delta());
        if resp.hovered() && zoom != 1.0 {
            let p = ui.input(|i| i.pointer.hover_pos()).unwrap_or(rect.center()) - rect.min;
            let new = (view.zoom * zoom).clamp(0.2, 6.0);
            view.offset = p - (p - view.offset) * (new / view.zoom);
            view.zoom = new;
        }
        if resp.double_clicked() {
            view = View::default();
        }
        ui.data_mut(|d| d.insert_temp(view_id, view));
        if resp.hovered() {
            ui.ctx().set_cursor_icon(if resp.dragged() { CursorIcon::Grabbing } else { CursorIcon::Grab });
        }
    }
    if !ui.is_rect_visible(rect) {
        return;
    }
    let s = scale * view.zoom;
    let at = |p: Point| rect.min + view.offset + vec2(p[0], p[1]) * s;
    let painter = ui.painter_at(rect);
    if let Some(bg) = background {
        painter.rect_filled(rect, CornerRadius::same(8), color(pal, bg));
    }
    let pointer = resp.hover_pos();
    let mut tooltip: Option<&str> = None;
    for shape in shapes {
        match shape {
            Shape::Line { points, color: c, width, dashed } => {
                let pts: Vec<Pos2> = points.iter().map(|p| at(*p)).collect();
                let stroke = Stroke::new((width * s).max(0.5), color(pal, c));
                if *dashed {
                    painter.extend(EShape::dashed_line(&pts, stroke, 6.0 * s, 4.0 * s));
                } else if pts.len() >= 2 {
                    painter.add(EShape::line(pts, stroke));
                }
            }
            Shape::Bezier { from, c1, c2, to, color: c, color_to, width } => {
                let p = [at(*from), at(*c1), at(*c2), at(*to)];
                let a = color(pal, c);
                let w = (width * s).max(0.5);
                match color_to.as_deref().map(|c| color(pal, c)) {
                    // Переход цвета: кривая — короткими отрезками своего цвета.
                    Some(b) if b != a => {
                        let n = 24;
                        let pts: Vec<Pos2> = (0..=n).map(|i| bezier_at(p, i as f32 / n as f32)).collect();
                        for i in 0..n {
                            let t = (i as f32 + 0.5) / n as f32;
                            painter.line_segment([pts[i], pts[i + 1]], Stroke::new(w, crate::theme::color::mix(a, b, t)));
                        }
                    }
                    _ => {
                        painter.add(CubicBezierShape::from_points_stroke(p, false, Color32::TRANSPARENT, Stroke::new(w, a)));
                    }
                }
            }
            Shape::Rect { x, y, w, h, fill, stroke, stroke_width, radius: r, tooltip: tip } => {
                let r_screen = Rect::from_min_max(at([*x, *y]), at([x + w, y + h]));
                let corner = radius(r, s);
                if let Some(f) = fill {
                    painter.rect_filled(r_screen, corner, color(pal, f));
                }
                if let Some(st) = stroke {
                    painter.rect_stroke(r_screen, corner, Stroke::new((stroke_width * s).max(0.5), color(pal, st)), egui::StrokeKind::Inside);
                }
                if !tip.is_empty() && pointer.is_some_and(|p| r_screen.contains(p)) {
                    tooltip = Some(tip);
                }
            }
            Shape::Circle { x, y, r, fill, stroke, stroke_width, tooltip: tip } => {
                let c = at([*x, *y]);
                let rr = r * s;
                if let Some(f) = fill {
                    painter.circle_filled(c, rr, color(pal, f));
                }
                if let Some(st) = stroke {
                    painter.circle_stroke(c, rr, Stroke::new((stroke_width * s).max(0.5), color(pal, st)));
                }
                if !tip.is_empty() && pointer.is_some_and(|p| p.distance(c) <= rr) {
                    tooltip = Some(tip);
                }
            }
            Shape::Polygon { points, fill, stroke } => {
                let pts: Vec<Pos2> = points.iter().map(|p| at(*p)).collect();
                let fill = fill.as_deref().map_or(Color32::TRANSPARENT, |f| color(pal, f));
                let stroke = stroke.as_deref().map_or(Stroke::NONE, |c| Stroke::new(s.max(0.5), color(pal, c)));
                painter.add(EShape::convex_polygon(pts, fill, stroke));
            }
            Shape::Mesh { vertices, colors, indices } => {
                let colors: Vec<Color32> = colors.iter().map(|c| color(pal, c)).collect();
                let first = colors.first().copied().unwrap_or(pal.text);
                let vertices: Vec<Vertex> = vertices.iter().enumerate().map(|(i, p)| Vertex { pos: at(*p), uv: WHITE_UV, color: colors.get(i).copied().unwrap_or(first) }).collect();
                let n = vertices.len() as u32;
                let indices: Vec<u32> = indices.as_chunks::<3>().0.iter().filter(|t| t.iter().all(|&i| i < n)).flatten().copied().collect();
                painter.add(EShape::mesh(Mesh { vertices, indices, ..Default::default() }));
            }
            Shape::Text { x, y, text, size, color: c, align, valign, bold, mono } => {
                let family = if *mono {
                    FontFamily::Monospace
                } else if *bold {
                    crate::fonts::semibold()
                } else {
                    FontFamily::Proportional
                };
                painter.text(at([*x, *y]), align2(*align, *valign), text, FontId::new((size * s).max(4.0), family), color(pal, c));
            }
            Shape::Image { x, y, w, h, path } => {
                let r_screen = Rect::from_min_max(at([*x, *y]), at([x + w, y + h]));
                match hooks.image_uri(path) {
                    Some(uri) => egui::Image::new(uri).corner_radius(CornerRadius::same((4.0 * s) as u8)).paint_at(ui, r_screen),
                    None => {
                        painter.rect_stroke(r_screen, CornerRadius::same(4), Stroke::new(1.0, pal.border), egui::StrokeKind::Inside);
                    }
                }
            }
            Shape::Unknown => {}
        }
    }
    if let Some(tip) = tooltip {
        resp.on_hover_text_at_pointer(tip);
    }
}

/// Сохранённое положение границы «до и после» (доля ширины).
fn split_of(ui: &Ui, id: Id) -> f32 {
    ui.data(|d| d.get_temp(id)).unwrap_or(0.5)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn compare(ui: &mut Ui, pal: &Palette, id: Id, before: &str, after: &str, before_label: &str, after_label: &str, hooks: &mut dyn Hooks) {
    let uris = (hooks.image_uri(before), hooks.image_uri(after));
    let (Some(a), Some(b)) = uris else {
        let missing = if hooks.image_uri(before).is_none() { before } else { after };
        ui.label(RichText::new(format!("⚠ {missing}")).size(12.5).color(pal.warning));
        return;
    };
    let load = |uri: &str| ui.ctx().try_load_texture(uri, TextureOptions::LINEAR, SizeHint::default());
    let (ta, tb) = match (load(&a), load(&b)) {
        (Ok(TexturePoll::Ready { texture: ta }), Ok(TexturePoll::Ready { texture: tb })) => (ta, tb),
        (Err(e), _) | (_, Err(e)) => {
            ui.label(RichText::new(e.to_string()).size(12.5).color(pal.warning));
            return;
        }
        _ => {
            ui.add(egui::Spinner::new().size(16.0).color(pal.accent));
            ui.ctx().request_repaint_after_secs(0.05);
            return;
        }
    };
    let size = ta.size;
    let scale = (ui.available_width() / size.x.max(1.0)).min(1.0);
    let (rect, resp) = ui.allocate_exact_size(size * scale, Sense::click_and_drag());
    let split_id = id.with("compare-split");
    let mut t = split_of(ui, split_id);
    if (resp.dragged() || resp.clicked())
        && let Some(p) = resp.interact_pointer_pos()
    {
        t = ((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
        ui.data_mut(|d| d.insert_temp(split_id, t));
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
    }
    let x = rect.left() + rect.width() * t;
    let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
    let painter = ui.painter_at(rect);
    // «После» — целиком, «до» — слева от границы.
    painter.image(tb.id, rect, uv, Color32::WHITE);
    painter.with_clip_rect(Rect::from_min_max(rect.min, pos2(x, rect.bottom()))).image(ta.id, rect, uv, Color32::WHITE);
    painter.vline(x, rect.y_range(), Stroke::new(2.0, Color32::WHITE));
    let knob = pos2(x, rect.center().y);
    painter.circle(knob, 13.0, Color32::from_black_alpha(140), Stroke::new(2.0, Color32::WHITE));
    painter.text(knob, Align2::CENTER_CENTER, "‹ ›", FontId::proportional(13.0), Color32::WHITE);
    let chip = |text: &str, anchor: Pos2, align: Align2| {
        if text.is_empty() {
            return;
        }
        let g = painter.layout_no_wrap(text.to_owned(), FontId::proportional(12.0), Color32::WHITE);
        let r = align.anchor_size(anchor, g.size() + vec2(14.0, 6.0));
        painter.rect_filled(r, CornerRadius::same(5), Color32::from_black_alpha(150));
        painter.galley(r.min + vec2(7.0, 3.0), g, Color32::WHITE);
    };
    chip(before_label, rect.left_top() + vec2(10.0, 10.0), Align2::LEFT_TOP);
    chip(after_label, rect.right_top() + vec2(-10.0, 10.0), Align2::RIGHT_TOP);
    if tb.size != ta.size {
        // Картинки разного размера: «после» растянута под «до» — покажем размеры.
        resp.on_hover_text(format!("{}×{} / {}×{}", ta.size.x, ta.size.y, tb.size.x, tb.size.y));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_read_hex_and_theme_names() {
        let pal = crate::theme::Theme::resolve(crate::theme::ThemeFile::parse("[theme]\nid = \"t\"\ndark = true").unwrap()).pal;
        assert_eq!(color(&pal, "#ff8000"), Color32::from_rgb(255, 128, 0));
        assert_eq!(color(&pal, "#ffffff80"), Color32::from_rgba_unmultiplied(255, 255, 255, 128));
        assert_eq!(color(&pal, "#f80"), Color32::from_rgb(255, 136, 0));
        assert_eq!(color(&pal, "accent"), pal.accent);
        assert_eq!(color(&pal, "#zzz"), pal.text, "непонятное — цвет текста");
        assert_eq!(radius(&[2.0, 4.0, 6.0, 8.0], 1.0), CornerRadius { nw: 2, ne: 4, sw: 6, se: 8 });
        let p = [pos2(0.0, 0.0), pos2(0.0, 10.0), pos2(10.0, 10.0), pos2(10.0, 0.0)];
        assert_eq!(bezier_at(p, 0.0), p[0]);
        assert_eq!(bezier_at(p, 1.0), p[3]);
    }
}
