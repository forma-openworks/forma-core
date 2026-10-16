//! Просмотр 3D-модели плагина ([`Element::View3d`](forma_plugin_ui::Element::View3d)):
//! треугольники с простым светом, как режим Solid во viewport Blender.
//!
//! Тянуть мышью — вращать вокруг модели, Ctrl и колесо — приблизить, двойной щелчок —
//! вернуть вид. Модель рисуется на процессоре сеткой egui: треугольники сортируются по
//! глубине (дальние раньше), каждый закрашен по углу к свету. Сетка пересобирается, только
//! когда меняется вид или размер, — простой кадр её не трогает.

use std::sync::Arc;

use egui::epaint::{Mesh, Vertex, WHITE_UV};
use egui::{Color32, CornerRadius, CursorIcon, Id, Rect, Sense, Shape, Stroke, Ui, Vec2, pos2, vec2};

use crate::theme::Palette;

/// Больше треугольников рёбра не рисуются: сетка линий стала бы кашей и тормозила.
const WIRE_LIMIT: usize = 20_000;

/// Вид на модель: поворот вокруг вертикали и наклон (радианы), приближение.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Orbit {
    yaw: f32,
    pitch: f32,
    zoom: f32,
}

impl Default for Orbit {
    fn default() -> Self {
        // Чуть сбоку и сверху — как вид по умолчанию у просмотрщиков моделей.
        Self { yaw: 0.6, pitch: 0.35, zoom: 1.0 }
    }
}

/// Собранная сетка для вида и размера.
#[derive(Clone)]
struct Built {
    key: (u64, [u32; 3], [u32; 2]),
    mesh: Arc<Mesh>,
    wire: Arc<Vec<Shape>>,
}

/// Отпечаток данных модели (чтобы не пересобирать сетку от чужой модели с тем же id).
fn data_key(positions: &[f32], indices: &[u32]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (positions.len(), indices.len()).hash(&mut h);
    for p in positions.iter().step_by((positions.len() / 64).max(1)) {
        p.to_bits().hash(&mut h);
    }
    h.finish()
}

/// Центр и радиус модели.
fn bounds(positions: &[f32]) -> ([f32; 3], f32) {
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    for v in positions.as_chunks::<3>().0 {
        for k in 0..3 {
            lo[k] = lo[k].min(v[k]);
            hi[k] = hi[k].max(v[k]);
        }
    }
    if lo[0] > hi[0] {
        return ([0.0; 3], 1.0);
    }
    let c = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
    let r = positions.as_chunks::<3>().0.iter().map(|v| ((v[0] - c[0]).powi(2) + (v[1] - c[1]).powi(2) + (v[2] - c[2]).powi(2)).sqrt()).fold(0.0f32, f32::max);
    (c, r.max(1e-6))
}

/// Вершины в пространстве камеры: модель в центре, поворот вида.
fn to_view(positions: &[f32], center: [f32; 3], radius: f32, o: Orbit) -> Vec<[f32; 3]> {
    let (sy, cy) = o.yaw.sin_cos();
    let (sp, cp) = o.pitch.sin_cos();
    positions
        .as_chunks::<3>()
        .0
        .iter()
        .map(|v| {
            let (x, y, z) = ((v[0] - center[0]) / radius, (v[1] - center[1]) / radius, (v[2] - center[2]) / radius);
            // Поворот вокруг вертикали (Y), потом наклон вокруг X.
            let (x1, z1) = (x * cy + z * sy, -x * sy + z * cy);
            let (y2, z2) = (y * cp - z1 * sp, y * sp + z1 * cp);
            [x1, y2, z2]
        })
        .collect()
}

fn shade(base: Color32, k: f32) -> Color32 {
    let f = |c: u8| ((c as f32) * k).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgb(f(base.r()), f(base.g()), f(base.b()))
}

/// Собирает сетку egui для вида: перспектива, сортировка по глубине, свет.
fn build(rect: Rect, positions: &[f32], indices: &[u32], base: Color32, wire_color: Color32, wireframe: bool, o: Orbit) -> (Mesh, Vec<Shape>) {
    let (center, radius) = bounds(positions);
    let view = to_view(positions, center, radius, o);
    // Камера на расстоянии трёх радиусов, модель вписана в меньшую сторону.
    let dist = 3.0;
    let focal = 0.5 * rect.width().min(rect.height()) * dist * 0.8 * o.zoom;
    let project = |p: [f32; 3]| {
        let d = (dist - p[2]).max(0.05);
        pos2(rect.center().x + p[0] * focal / d, rect.center().y - p[1] * focal / d)
    };
    let screen: Vec<_> = view.iter().map(|p| project(*p)).collect();
    let n = view.len() as u32;
    let mut tris: Vec<(f32, [u32; 3])> = indices
        .as_chunks::<3>()
        .0
        .iter()
        .filter(|t| t.iter().all(|&i| i < n))
        .copied()
        .map(|t| ((view[t[0] as usize][2] + view[t[1] as usize][2] + view[t[2] as usize][2]) / 3.0, t))
        .collect();
    // Дальние — раньше: ближние ложатся поверх.
    tris.sort_by(|a, b| a.0.total_cmp(&b.0));
    let light = {
        let l = [-0.45f32, 0.55, 0.7];
        let n = (l[0] * l[0] + l[1] * l[1] + l[2] * l[2]).sqrt();
        [l[0] / n, l[1] / n, l[2] / n]
    };
    let mut mesh = Mesh::default();
    mesh.reserve_triangles(tris.len());
    mesh.reserve_vertices(tris.len() * 3);
    // Какие грани смотрят на камеру: рёбра рисуются только у них (задние не просвечивают).
    let mut front = Vec::with_capacity(tris.len());
    for (_, t) in &tris {
        let [a, b, c] = t.map(|i| view[i as usize]);
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let mut nrm = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
        let len = (nrm[0] * nrm[0] + nrm[1] * nrm[1] + nrm[2] * nrm[2]).sqrt().max(1e-12);
        nrm = nrm.map(|x| x / len);
        front.push(nrm[2] > 0.0);
        // Обе стороны освещены: у моделей бывают открытые края и вывернутые грани.
        if nrm[2] < 0.0 {
            nrm = nrm.map(|x| -x);
        }
        let diffuse = (nrm[0] * light[0] + nrm[1] * light[1] + nrm[2] * light[2]).max(0.0);
        let color = shade(base, 0.32 + 0.78 * diffuse + 0.12 * nrm[2]);
        let idx = mesh.vertices.len() as u32;
        for i in t {
            mesh.vertices.push(Vertex { pos: screen[*i as usize], uv: WHITE_UV, color });
        }
        mesh.indices.extend([idx, idx + 1, idx + 2]);
    }
    // Рёбра — отдельными отрезками: у замкнутой линии на почти вырожденном треугольнике
    // (видном с ребра) egui вытягивает острый угол в длинный луч. Общее ребро — один раз,
    // и только у граней, повёрнутых к камере, как каркас поверх Solid в Blender.
    let mut wire = Vec::new();
    if wireframe && tris.len() <= WIRE_LIMIT {
        let stroke = Stroke::new(0.6, wire_color);
        let mut edges: Vec<(u32, u32)> = tris.iter().zip(&front).filter(|(_, f)| **f).flat_map(|((_, t), _)| [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])]).map(|(a, b)| (a.min(b), a.max(b))).collect();
        edges.sort_unstable();
        edges.dedup();
        wire.extend(edges.into_iter().map(|(a, b)| Shape::line_segment([screen[a as usize], screen[b as usize]], stroke)));
    }
    (mesh, wire)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn view3d(ui: &mut Ui, pal: &Palette, id: Id, positions: &[f32], indices: &[u32], color: Option<Color32>, height: f32, wireframe: bool) {
    let size = vec2(ui.available_width(), height.clamp(80.0, 1200.0));
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
    let orbit_id = id.with("view3d-orbit");
    let mut o: Orbit = ui.data(|d| d.get_temp(orbit_id)).unwrap_or_default();
    if resp.dragged() {
        let d: Vec2 = resp.drag_delta();
        o.yaw += d.x * 0.01;
        o.pitch = (o.pitch + d.y * 0.01).clamp(-1.5, 1.5);
    }
    let zoom = ui.input(|i| i.zoom_delta());
    if resp.hovered() && zoom != 1.0 {
        o.zoom = (o.zoom * zoom).clamp(0.3, 8.0);
    }
    if resp.double_clicked() {
        o = Orbit::default();
    }
    ui.data_mut(|d| d.insert_temp(orbit_id, o));
    if resp.hovered() {
        ui.ctx().set_cursor_icon(if resp.dragged() { CursorIcon::Grabbing } else { CursorIcon::Grab });
    }
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter_at(rect);
    // Фон — как у viewport: темнее книзу.
    let top = pal.bg_panel;
    let bottom = pal.bg_deep;
    let mut bg = Mesh::default();
    bg.colored_vertex(rect.left_top(), top);
    bg.colored_vertex(rect.right_top(), top);
    bg.colored_vertex(rect.right_bottom(), bottom);
    bg.colored_vertex(rect.left_bottom(), bottom);
    bg.add_triangle(0, 1, 2);
    bg.add_triangle(0, 2, 3);
    painter.add(Shape::mesh(bg));
    painter.rect_stroke(rect, CornerRadius::same(8), Stroke::new(1.0, pal.border), egui::StrokeKind::Inside);

    let base = color.unwrap_or(Color32::from_gray(176));
    let key = (data_key(positions, indices), [o.yaw.to_bits(), o.pitch.to_bits(), o.zoom.to_bits()], [rect.width().to_bits(), rect.height().to_bits()]);
    let cache_id = id.with("view3d-mesh");
    let cached: Option<Built> = ui.data(|d| d.get_temp(cache_id));
    let built = match cached {
        Some(b) if b.key == key => b,
        _ => {
            // Сетку строим в координатах от угла области: при прокрутке её только сдвигаем.
            let local = Rect::from_min_size(pos2(0.0, 0.0), rect.size());
            let (mesh, wire) = build(local, positions, indices, base, pal.text.gamma_multiply(0.35), wireframe, o);
            let b = Built { key, mesh: Arc::new(mesh), wire: Arc::new(wire) };
            ui.data_mut(|d| d.insert_temp(cache_id, b.clone()));
            b
        }
    };
    let offset = rect.min.to_vec2();
    let mut mesh = (*built.mesh).clone();
    mesh.translate(offset);
    painter.add(Shape::mesh(mesh));
    for w in built.wire.iter() {
        let mut w = w.clone();
        w.translate(offset);
        painter.add(w);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Куб из 12 треугольников: все треугольники на месте, ближние нарисованы последними.
    #[test]
    fn a_cube_is_built_back_to_front_and_lit() {
        let p: Vec<f32> = vec![-1., -1., -1., 1., -1., -1., 1., 1., -1., -1., 1., -1., -1., -1., 1., 1., -1., 1., 1., 1., 1., -1., 1., 1.];
        let i: Vec<u32> = vec![0, 1, 2, 0, 2, 3, 4, 6, 5, 4, 7, 6, 0, 4, 5, 0, 5, 1, 3, 2, 6, 3, 6, 7, 1, 5, 6, 1, 6, 2, 0, 3, 7, 0, 7, 4];
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(200.0, 200.0));
        let (mesh, wire) = build(rect, &p, &i, Color32::from_gray(176), Color32::WHITE, true, Orbit::default());
        assert_eq!((mesh.vertices.len(), mesh.indices.len()), (36, 36));
        assert!((9..18).contains(&wire.len()), "видны рёбра только передних граней: {}", wire.len());
        assert!(mesh.vertices.iter().all(|v| rect.expand(1.0).contains(v.pos)), "модель вписана в область");
        let shades: std::collections::HashSet<u8> = mesh.vertices.iter().map(|v| v.color.r()).collect();
        assert!(shades.len() >= 3, "грани под разным углом к свету — разной яркости");
        assert_eq!(bounds(&p), ([0.0; 3], 3f32.sqrt()));
        // Неверные номера вершин пропускаются, а не роняют программу.
        let (mesh, _) = build(rect, &p, &[0, 1, 99], Color32::GRAY, Color32::WHITE, false, Orbit::default());
        assert!(mesh.indices.is_empty());
    }
}

