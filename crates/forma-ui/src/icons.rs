//! Значки платформы Forma. Основа — открытый набор [Lucide](https://lucide.dev) (лицензия ISC,
//! файлы и лицензия — в `icons/lucide/`), вшитый в программу. Тема может заменить любой
//! значок своим SVG (раздел `[icons]` файла темы).
//!
//! Значки одноцветные: в SVG цвет задан как `currentColor`. Цвет значка всегда берётся из
//! стиля: по умолчанию — цвет того места, где значок стоит (текст, акцент, предупреждение),
//! а тема может задать его прямо ключом `icon.<имя>` в `[colors]` — он главнее. Свой значок
//! темы без `currentColor` перекрашивать нельзя: он рисуется в своих цветах, цвет стиля
//! для него не применяется.
//!
//! Значки зовутся по смыслу («run», «trash», «file-java»), а не по рисунку: так тема
//! заменяет «кнопку запуска», не задумываясь, какой рисунок под ней сейчас.

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use egui::{Color32, ColorImage, Context, Id, Painter, Rect, Response, Sense, StrokeKind, TextureHandle, TextureId, TextureOptions, Ui, pos2, vec2};

macro_rules! lucide {
    ($($name:literal => $file:literal),* $(,)?) => {
        /// Встроенные значки: имя по смыслу и SVG из набора Lucide.
        pub const BUILTIN: &[(&str, &[u8])] = &[$(($name, include_bytes!(concat!("../icons/lucide/", $file, ".svg")))),*];
        /// Какой рисунок Lucide у встроенного значка — для шаблона темы.
        pub const LUCIDE_NAMES: &[(&str, &str)] = &[$(($name, $file)),*];
    };
}

lucide! {
    // Общие действия и состояния.
    "add" => "plus",
    "close" => "x",
    "more" => "ellipsis",
    "overflow" => "chevrons-right",
    "menu" => "menu",
    "search" => "search",
    "settings" => "settings",
    "reset" => "rotate-ccw",
    "check" => "check",
    "chevron-right" => "chevron-right",
    "chevron-down" => "chevron-down",
    "chevron-up" => "chevron-up",
    "warning" => "triangle-alert",
    "error" => "circle-x",
    "info" => "info",
    "success" => "circle-check",
    "hint" => "lightbulb",
    "edit" => "pencil",
    "copy" => "copy",
    "trash" => "trash",
    "show" => "eye",
    "hide" => "eye-off",
    "refresh" => "refresh-cw",
    "collapse-all" => "chevrons-down-up",
    "scroll-down" => "arrow-down-to-line",
    "filter" => "funnel",
    "lock" => "lock",
    "open" => "folder-open",
    "continue" => "arrow-right",
    "plugin" => "puzzle",
    "download" => "download",
    "external" => "external-link",
    "loading" => "loader-circle",
    "palette" => "palette",
    "keyboard" => "keyboard",
    "fonts" => "type",
    "all" => "asterisk",
    // Проблемы и логи.
    "crash" => "bomb",
    "noise" => "leaf",
    "jump" => "corner-down-right",
    // Файлы.
    "folder" => "folder",
    "folder-open" => "folder-open",
    "file" => "file",
    "file-text" => "file-text",
    "file-code" => "file-code",
    "file-java" => "coffee",
    "file-kotlin" => "diamond",
    "file-json" => "braces",
    "file-image" => "image",
    "file-config" => "file-cog",
    "file-model" => "box",
    "package" => "package",
    "library" => "library",
    "jar" => "archive",
    "jdk" => "coffee",
    // Редакторы.
    "editor-project" => "folder-tree",
    "editor-code" => "code-xml",
    "editor-map" => "network",
    "editor-exceptions" => "triangle-alert",
    "editor-console" => "square-terminal",
    "editor-model" => "box",
    // Запуск.
    "run" => "play",
    "stop" => "square",
    "stop-all" => "circle-stop",
    "restart" => "refresh-ccw",
    "run-client" => "gamepad-2",
    "run-server" => "server",
    "run-data" => "database",
    "run-build" => "hammer",
    "run-clean" => "brush-cleaning",
}

/// Значки и цвета из темы. `generation` растёт при смене самих значков: по нему
/// сбрасываются готовые картинки (смена цвета их не трогает — цвет накладывается при рисовании).
#[derive(Default)]
struct Overrides {
    files: HashMap<String, Vec<u8>>,
    colors: HashMap<String, Color32>,
    generation: u64,
}

fn overrides() -> &'static RwLock<Overrides> {
    static STATE: OnceLock<RwLock<Overrides>> = OnceLock::new();
    STATE.get_or_init(RwLock::default)
}

/// Значки и цвета значков темы. Вызывать при смене темы; одинаковые значки ничего не сбрасывают.
pub fn set_theme(paths: &BTreeMap<String, PathBuf>, colors: &BTreeMap<String, Color32>) {
    let files: HashMap<String, Vec<u8>> = paths
        .iter()
        .filter_map(|(name, path)| match std::fs::read(path) {
            Ok(bytes) => Some((name.clone(), bytes)),
            Err(e) => {
                log::warn!("Theme icon {name} ({}) is not readable: {e}", path.display());
                None
            }
        })
        .collect();
    let mut state = overrides().write().unwrap_or_else(|e| e.into_inner());
    if state.files != files {
        state.files = files;
        state.generation += 1;
    }
    state.colors = colors.iter().map(|(k, v)| (k.clone(), *v)).collect();
}

/// Цвет значка: из темы (`icon.<имя>`), а если там не задан — цвет по месту.
pub fn color_of(name: &str, fallback: Color32) -> Color32 {
    overrides().read().ok().and_then(|s| s.colors.get(name).copied()).unwrap_or(fallback)
}

/// Есть ли значок с таким именем (встроенный или из темы).
pub fn exists(name: &str) -> bool {
    BUILTIN.iter().any(|(n, _)| *n == name) || overrides().read().is_ok_and(|s| s.files.contains_key(name))
}

/// SVG значка: замена из темы или встроенный.
fn source(name: &str) -> Option<Cow<'static, [u8]>> {
    if let Some(bytes) = overrides().read().ok().and_then(|s| s.files.get(name).cloned()) {
        return Some(Cow::Owned(bytes));
    }
    BUILTIN.iter().find(|(n, _)| *n == name).map(|(_, b)| Cow::Borrowed(*b))
}

/// Рисует SVG в квадрат `px`×`px`. Второе значение — одноцветный ли значок (его можно перекрашивать).
fn rasterize(svg: &[u8], px: u32) -> Option<(ColorImage, bool)> {
    let text = std::str::from_utf8(svg).ok()?;
    let mono = text.contains("currentColor");
    let text = text.replace("currentColor", "#ffffff");
    let tree = resvg::usvg::Tree::from_str(&text, &resvg::usvg::Options::default()).ok()?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(px, px)?;
    let size = tree.size();
    let scale = px as f32 / size.width().max(size.height());
    let dx = (px as f32 - size.width() * scale) / 2.0;
    let dy = (px as f32 - size.height() * scale) / 2.0;
    resvg::render(&tree, resvg::tiny_skia::Transform::from_row(scale, 0.0, 0.0, scale, dx, dy), &mut pixmap.as_mut());
    Some((ColorImage::from_rgba_premultiplied([px as usize, px as usize], pixmap.data()), mono))
}

/// Готовые картинки значков в видеопамяти: по имени и размеру в пикселях.
#[derive(Clone, Default)]
struct Cache(Arc<Mutex<CacheInner>>);

#[derive(Default)]
struct CacheInner {
    generation: u64,
    textures: HashMap<(String, u32), Option<(TextureHandle, bool)>>,
}

/// Картинка значка размером `px` пикселей и одноцветный ли он. `None` — такого значка нет.
pub fn texture(ctx: &Context, name: &str, px: u32) -> Option<(TextureId, bool)> {
    let cache = ctx.data_mut(|d| d.get_temp_mut_or_default::<Cache>(Id::new("forma-icons")).clone());
    let generation = overrides().read().map(|s| s.generation).unwrap_or(0);
    let mut inner = cache.0.lock().unwrap_or_else(|e| e.into_inner());
    if inner.generation != generation {
        inner.textures.clear();
        inner.generation = generation;
    }
    let key = (name.to_owned(), px);
    if let Some(found) = inner.textures.get(&key) {
        return found.as_ref().map(|(t, mono)| (t.id(), *mono));
    }
    let made = source(name).and_then(|svg| rasterize(&svg, px)).map(|(img, mono)| (ctx.load_texture(format!("forma-icon-{name}-{px}"), img, TextureOptions::LINEAR), mono));
    if made.is_none() && !exists(name) {
        log::debug!("Unknown icon {name}");
    }
    let out = made.as_ref().map(|(t, mono)| (t.id(), *mono));
    inner.textures.insert(key, made);
    out
}

/// Рисует значок по центру `rect` (квадратом по меньшей стороне) цветом `color`.
/// Возвращает `false`, если такого значка нет — тогда можно нарисовать что-то своё.
pub fn paint(painter: &Painter, rect: Rect, name: &str, color: Color32) -> bool {
    let side = rect.width().min(rect.height());
    if side <= 0.0 {
        return true;
    }
    let ppp = painter.ctx().pixels_per_point();
    let px = (side * ppp).round().max(1.0) as u32;
    let Some((tex, mono)) = texture(painter.ctx(), name, px) else { return false };
    let color = color_of(name, color);
    // Квадрат ровно по пикселям экрана — так тонкие линии остаются чёткими.
    let corner = rect.center() - vec2(side, side) / 2.0;
    let min = pos2((corner.x * ppp).round() / ppp, (corner.y * ppp).round() / ppp);
    let r = Rect::from_min_size(min, vec2(px as f32, px as f32) / ppp);
    let tint = if mono { color } else { Color32::WHITE.gamma_multiply(color.a() as f32 / 255.0) };
    painter.image(tex, r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), tint);
    true
}

/// Значок как картинка egui — для кнопок и пунктов меню с подписью.
pub fn image(ctx: &Context, name: &str, size: f32, color: Color32) -> egui::Image<'static> {
    let px = (size * ctx.pixels_per_point()).round().max(1.0) as u32;
    match texture(ctx, name, px) {
        Some((tex, mono)) => egui::Image::new(egui::load::SizedTexture::new(tex, vec2(size, size))).tint(if mono { color_of(name, color) } else { Color32::WHITE }),
        None => egui::Image::new(egui::load::SizedTexture::new(TextureId::default(), vec2(size, size))).tint(Color32::TRANSPARENT),
    }
}

/// Размер значка в кнопках и строках.
pub const SIZE: f32 = 16.0;

/// Кнопка-значок в стиле обычной кнопки egui. `selected` — включённый переключатель.
pub fn button(ui: &mut Ui, name: &str, selected: bool) -> Response {
    let side = ui.spacing().interact_size.y;
    let (rect, resp) = ui.allocate_exact_size(vec2(side, side), Sense::click());
    if ui.is_rect_visible(rect) {
        let v = ui.style().interact_selectable(&resp, selected);
        ui.painter().rect(rect.expand(v.expansion), v.corner_radius, v.weak_bg_fill, v.bg_stroke, StrokeKind::Inside);
        let color = if ui.is_enabled() { v.fg_stroke.color } else { ui.visuals().widgets.noninteractive.fg_stroke.color };
        paint(ui.painter(), Rect::from_center_size(rect.center(), vec2(SIZE, SIZE)), name, color);
    }
    resp
}

/// Кнопка-значок без рамки: рамка появляется при наведении.
pub fn flat_button(ui: &mut Ui, name: &str, color: Color32) -> Response {
    let side = ui.spacing().interact_size.y;
    let (rect, resp) = ui.allocate_exact_size(vec2(side, side), Sense::click());
    if ui.is_rect_visible(rect) {
        if resp.hovered() {
            let v = ui.style().interact(&resp);
            ui.painter().rect_filled(rect, v.corner_radius, v.weak_bg_fill);
        }
        paint(ui.painter(), Rect::from_center_size(rect.center(), vec2(SIZE, SIZE)), name, color);
    }
    resp
}

/// Пункт меню (или кнопка) со значком и подписью; `selected` — отмеченный пункт.
pub fn item(ui: &mut Ui, name: &str, text: impl Into<egui::WidgetText>, selected: bool) -> Response {
    let color = if selected { ui.visuals().selection.stroke.color } else { ui.visuals().widgets.inactive.fg_stroke.color };
    let img = image(ui.ctx(), name, SIZE - 2.0, color);
    ui.add(egui::Button::new((img, text.into())).selected(selected))
}

/// Кнопка-значок цвета `color` (запуск — зелёный, остановка — красный).
pub fn colored_button(ui: &mut Ui, name: &str, color: Color32) -> Response {
    let side = ui.spacing().interact_size.y;
    let (rect, resp) = ui.allocate_exact_size(vec2(side, side), Sense::click());
    if ui.is_rect_visible(rect) {
        let v = ui.style().interact(&resp);
        ui.painter().rect(rect.expand(v.expansion), v.corner_radius, v.weak_bg_fill, v.bg_stroke, StrokeKind::Inside);
        let color = if ui.is_enabled() { color } else { color.gamma_multiply(0.4) };
        paint(ui.painter(), Rect::from_center_size(rect.center(), vec2(SIZE, SIZE)), name, color);
    }
    resp
}

/// Кнопка со значком и подписью.
pub fn text_button(ui: &mut Ui, name: &str, text: impl Into<egui::WidgetText>) -> Response {
    let color = ui.visuals().widgets.inactive.fg_stroke.color;
    let img = image(ui.ctx(), name, SIZE - 2.0, color);
    ui.add(egui::Button::new((img, text.into())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_builtin_icon_renders() {
        for (name, svg) in BUILTIN {
            let (img, mono) = rasterize(svg, 24).unwrap_or_else(|| panic!("значок {name} не рисуется"));
            assert!(mono, "{name}: встроенные значки одноцветные");
            assert!(img.pixels.iter().any(|p| p.a() > 0), "{name}: пустой значок");
        }
        let names: std::collections::HashSet<&str> = BUILTIN.iter().map(|(n, _)| *n).collect();
        assert_eq!(names.len(), BUILTIN.len(), "имена значков не повторяются");
    }

    #[test]
    fn theme_icons_replace_builtin_and_keep_their_colors() {
        let dir = std::env::temp_dir().join(format!("forma-icons-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("run.svg");
        std::fs::write(&path, r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect width="24" height="24" fill="#ff0000"/></svg>"##).unwrap();
        let before = overrides().read().unwrap().generation;
        set_theme(&BTreeMap::from([("run".to_owned(), path.clone())]), &BTreeMap::from([("trash".to_owned(), Color32::GREEN)]));
        assert_eq!(color_of("trash", Color32::RED), Color32::GREEN, "цвет из темы главнее цвета по месту");
        assert_eq!(color_of("add", Color32::RED), Color32::RED);
        let svg = source("run").unwrap();
        let (img, mono) = rasterize(&svg, 8).unwrap();
        assert!(!mono, "цветной значок темы не перекрашивается");
        assert_eq!(img.pixels[0], Color32::from_rgb(255, 0, 0));
        assert!(overrides().read().unwrap().generation > before);
        set_theme(&BTreeMap::new(), &BTreeMap::new());
        assert!(source("run").is_some_and(|s| s.len() > 100), "без темы — снова встроенный");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
