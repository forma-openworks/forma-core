//! Чтение картинок в изображение egui и загрузчик для адресов `file:///…` (фича `decode`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;

use egui::load::{ImageLoadResult, ImageLoader, ImagePoll, LoadError, SizeHint};
use egui::{ColorImage, Context};
use image::{DynamicImage, RgbaImage};

use crate::{EXTENSIONS, Info, extension, path_of};

/// Форматы, которые читают только кодеки Windows.
const SYSTEM: &[&str] = &["heic", "heif", "hif", "avif", "jxr", "wdp", "hdp", "dng", "cr2", "cr3", "crw", "nef", "nrw", "arw", "srf", "sr2", "orf", "rw2", "raf", "pef", "srw", "rwl", "3fr", "erf", "kdc", "mrw", "mos", "iiq", "x3f", "raw"];

/// Прочитанная картинка.
pub struct Decoded {
    pub image: ColorImage,
    pub info: Info,
}

/// Название формата для людей.
fn format_name(ext: &str) -> String {
    match ext {
        "jpg" | "jpeg" | "jpe" | "jfif" => "JPEG",
        "tif" | "tiff" => "TIFF",
        "exr" => "OpenEXR",
        "hdr" => "Radiance HDR",
        "psd" => "Photoshop",
        "jxl" => "JPEG XL",
        "heic" | "heif" | "hif" => "HEIF",
        "jxr" | "wdp" | "hdp" => "JPEG XR",
        "dib" => "BMP",
        "ff" => "farbfeld",
        "pbm" | "pgm" | "ppm" | "pnm" | "pam" => "PNM",
        "dng" | "cr2" | "cr3" | "crw" | "nef" | "nrw" | "arw" | "srf" | "sr2" | "orf" | "rw2" | "raf" | "pef" | "srw" | "rwl" | "3fr" | "erf" | "kdc" | "mrw" | "mos" | "iiq" | "x3f" | "raw" => return format!("RAW ({})", ext.to_uppercase()),
        other => return other.to_uppercase(),
    }
    .to_owned()
}

/// Читает картинку `path`; больше `max_side` по стороне — уменьшается.
pub fn decode(path: &Path, max_side: u32) -> Result<Decoded, String> {
    let ext = extension(path);
    let bytes = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
    let (rgba, width, height) = match ext.as_str() {
        "svg" => svg(path, max_side)?,
        "psd" => psd(path)?,
        "jxl" => jxl(path)?,
        e if SYSTEM.contains(&e) => system(path)?,
        _ => match crate_image(path) {
            Ok(r) => r,
            // Расширение не то или формат крейту не по силам — пробуем кодеки системы.
            Err(e) => system(path).map_err(|_| e)?,
        },
    };
    let rgba = fit(rgba, max_side);
    let image = ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw());
    let info = Info { width, height, format: format_name(&ext), bytes };
    Ok(Decoded { image, info })
}

/// Уменьшает картинку, чтобы большая сторона была не больше `max_side`.
fn fit(rgba: RgbaImage, max_side: u32) -> RgbaImage {
    let (w, h) = rgba.dimensions();
    if w.max(h) <= max_side || max_side == 0 {
        return rgba;
    }
    let k = max_side as f64 / w.max(h) as f64;
    let (nw, nh) = (((w as f64 * k).round() as u32).max(1), ((h as f64 * k).round() as u32).max(1));
    image::imageops::resize(&rgba, nw, nh, image::imageops::FilterType::Triangle)
}

type Pixels = (RgbaImage, u32, u32);

/// Крейт image: формат — по содержимому, а не только по расширению.
fn crate_image(path: &Path) -> Result<Pixels, String> {
    let img = image::ImageReader::open(path).map_err(|e| e.to_string())?.with_guessed_format().map_err(|e| e.to_string())?.decode().map_err(|e| e.to_string())?;
    let (w, h) = (img.width(), img.height());
    Ok((display_rgba(img), w, h))
}

/// В 8 бит на канал. Картинки с плавающей точкой (HDR, EXR) хранят линейный свет: на экран —
/// через кривую sRGB, иначе они выглядят слишком тёмными.
fn display_rgba(img: DynamicImage) -> RgbaImage {
    match img {
        DynamicImage::ImageRgb32F(_) | DynamicImage::ImageRgba32F(_) => {
            let f = img.to_rgba32f();
            let mut out = RgbaImage::new(f.width(), f.height());
            for (o, p) in out.pixels_mut().zip(f.pixels()) {
                let c = |v: f32| (srgb(v.clamp(0.0, 1.0)) * 255.0).round() as u8;
                *o = image::Rgba([c(p[0]), c(p[1]), c(p[2]), (p[3].clamp(0.0, 1.0) * 255.0).round() as u8]);
            }
            out
        }
        other => other.to_rgba8(),
    }
}

/// Линейный свет → sRGB.
fn srgb(v: f32) -> f32 {
    if v <= 0.003_130_8 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 }
}

/// SVG: растр в четыре раза больше своего размера (чтобы при приближении оставался чётким),
/// но не больше `max_side` и не больше 4096 по стороне.
fn svg(path: &Path, max_side: u32) -> Result<Pixels, String> {
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    let mut options = resvg::usvg::Options { resources_dir: path.parent().map(Path::to_path_buf), ..Default::default() };
    options.fontdb = fonts();
    let tree = resvg::usvg::Tree::from_data(&data, &options).map_err(|e| e.to_string())?;
    let size = tree.size();
    let (w, h) = (size.width().max(1.0), size.height().max(1.0));
    let longest = w.max(h);
    let target = (longest * 4.0).min(4096.0).min(if max_side == 0 { 4096.0 } else { max_side as f32 }).max(longest.min(max_side.max(1) as f32));
    let k = target / longest;
    let (pw, ph) = ((w * k).round().max(1.0) as u32, (h * k).round().max(1.0) as u32);
    let mut pixmap = resvg::tiny_skia::Pixmap::new(pw, ph).ok_or("empty SVG")?;
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(k, k), &mut pixmap.as_mut());
    // tiny-skia хранит цвет, умноженный на прозрачность.
    let mut rgba = RgbaImage::new(pw, ph);
    for (o, p) in rgba.pixels_mut().zip(pixmap.pixels()) {
        let c = p.demultiply();
        *o = image::Rgba([c.red(), c.green(), c.blue(), c.alpha()]);
    }
    Ok((rgba, w.round() as u32, h.round() as u32))
}

/// Системные шрифты для текста в SVG — загружаются один раз.
fn fonts() -> Arc<resvg::usvg::fontdb::Database> {
    static FONTS: OnceLock<Arc<resvg::usvg::fontdb::Database>> = OnceLock::new();
    FONTS
        .get_or_init(|| {
            let mut db = resvg::usvg::fontdb::Database::new();
            db.load_system_fonts();
            Arc::new(db)
        })
        .clone()
}

/// Photoshop: сведённое изображение всех слоёв.
fn psd(path: &Path) -> Result<Pixels, String> {
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    let psd = psd::Psd::from_bytes(&data).map_err(|e| e.to_string())?;
    let (w, h) = (psd.width(), psd.height());
    let rgba = RgbaImage::from_raw(w, h, psd.rgba()).ok_or("PSD: wrong pixel count")?;
    Ok((rgba, w, h))
}

/// JPEG XL: первый кадр.
fn jxl(path: &Path) -> Result<Pixels, String> {
    let image = jxl_oxide::JxlImage::builder().open(path).map_err(|e| e.to_string())?;
    let render = image.render_frame(0).map_err(|e| e.to_string())?;
    let mut stream = render.stream();
    let (w, h, ch) = (stream.width(), stream.height(), stream.channels() as usize);
    let mut buf = vec![0u8; w as usize * h as usize * ch];
    stream.write_to_buffer(&mut buf);
    let mut rgba = RgbaImage::new(w, h);
    for (o, p) in rgba.pixels_mut().zip(buf.chunks_exact(ch.max(1))) {
        *o = match ch {
            1 => image::Rgba([p[0], p[0], p[0], 255]),
            2 => image::Rgba([p[0], p[0], p[0], p[1]]),
            3 => image::Rgba([p[0], p[1], p[2], 255]),
            _ => image::Rgba([p[0], p[1], p[2], p[3]]),
        };
    }
    Ok((rgba, w, h))
}

/// Кодеки Windows (WIC): первый кадр в RGBA.
#[cfg(windows)]
fn system(path: &Path) -> Result<Pixels, String> {
    use std::os::windows::ffi::OsStrExt;

    use windows::Win32::Foundation::GENERIC_READ;
    use windows::Win32::Graphics::Imaging::{CLSID_WICImagingFactory, GUID_WICPixelFormat32bppRGBA, IWICImagingFactory, WICConvertBitmapSource, WICDecodeMetadataCacheOnDemand};
    use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx};
    use windows::core::PCWSTR;

    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    // SAFETY: обычная последовательность WIC; строка пути живёт до конца функции, буфер
    // пикселей ровно такого размера, какой просим скопировать.
    unsafe {
        // Поток мог уже войти в COM с другой моделью — это не мешает.
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let factory: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).map_err(|e| e.message())?;
        let decoder = factory.CreateDecoderFromFilename(PCWSTR(wide.as_ptr()), None, GENERIC_READ, WICDecodeMetadataCacheOnDemand).map_err(|e| e.message())?;
        let frame = decoder.GetFrame(0).map_err(|e| e.message())?;
        let (mut w, mut h) = (0u32, 0u32);
        frame.GetSize(&mut w, &mut h).map_err(|e| e.message())?;
        let rgba = WICConvertBitmapSource(&GUID_WICPixelFormat32bppRGBA, &frame).map_err(|e| e.message())?;
        let mut buf = vec![0u8; w as usize * h as usize * 4];
        rgba.CopyPixels(std::ptr::null(), w * 4, &mut buf).map_err(|e| e.message())?;
        Ok((RgbaImage::from_raw(w, h, buf).ok_or("WIC: wrong pixel count")?, w, h))
    }
}

#[cfg(not(windows))]
fn system(_path: &Path) -> Result<Pixels, String> {
    Err("no system image codecs on this platform".into())
}

/// Что известно о картинках, прочитанных загрузчиком (по пути файла).
fn infos() -> &'static Mutex<HashMap<PathBuf, Info>> {
    static INFOS: OnceLock<Mutex<HashMap<PathBuf, Info>>> = OnceLock::new();
    INFOS.get_or_init(Default::default)
}

/// Размер, формат и вес картинки, которую загрузчик уже прочитал.
pub fn info(path: &Path) -> Option<Info> {
    infos().lock().ok()?.get(path).cloned()
}

enum State {
    Pending,
    Ready(Arc<ColorImage>),
    Failed(String),
}

struct Entry {
    state: State,
    /// Когда файл менялся: изменился на диске — читается заново.
    modified: Option<SystemTime>,
}

/// Загрузчик картинок egui для адресов `file:///…` с расширением из [`EXTENSIONS`]. Читает в
/// фоне; файл изменился — читает снова.
#[derive(Default)]
pub struct FileLoader {
    cache: Arc<Mutex<HashMap<String, Entry>>>,
    /// Какие форматы оставить другим загрузчикам: анимированные GIF и WebP читает
    /// `egui_extras`, если он установлен (он умеет кадры).
    skip: Vec<&'static str>,
}

impl ImageLoader for FileLoader {
    fn id(&self) -> &str {
        egui::generate_loader_id!(FileLoader)
    }

    fn load(&self, ctx: &Context, uri: &str, _size_hint: SizeHint) -> ImageLoadResult {
        let Some(path) = path_of(uri) else { return Err(LoadError::NotSupported) };
        let ext = extension(&path);
        if !EXTENSIONS.contains(&ext.as_str()) || self.skip.contains(&ext.as_str()) {
            return Err(LoadError::NotSupported);
        }
        let modified = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        let mut cache = self.cache.lock().map_err(|_| LoadError::Loading("cache".into()))?;
        if let Some(entry) = cache.get(uri)
            && entry.modified == modified
        {
            return match &entry.state {
                State::Pending => Ok(ImagePoll::Pending { size: None }),
                State::Ready(image) => Ok(ImagePoll::Ready { image: image.clone() }),
                State::Failed(e) => Err(LoadError::Loading(e.clone())),
            };
        }
        cache.insert(uri.to_owned(), Entry { state: State::Pending, modified });
        drop(cache);
        let max_side = ctx.input(|i| i.max_texture_side) as u32;
        let (cache, uri, ctx) = (self.cache.clone(), uri.to_owned(), ctx.clone());
        std::thread::Builder::new()
            .name(format!("image {uri}"))
            .spawn(move || {
                let state = match decode(&path, max_side) {
                    Ok(d) => {
                        if let Ok(mut i) = infos().lock() {
                            i.insert(path.clone(), d.info);
                        }
                        State::Ready(Arc::new(d.image))
                    }
                    Err(e) => {
                        log::warn!("{}: {e}", path.display());
                        State::Failed(e)
                    }
                };
                if let Ok(mut c) = cache.lock()
                    && let Some(entry) = c.get_mut(&uri)
                {
                    entry.state = state;
                }
                ctx.request_repaint();
            })
            .map_err(|e| LoadError::Loading(e.to_string()))?;
        Ok(ImagePoll::Pending { size: None })
    }

    fn forget(&self, uri: &str) {
        if let Ok(mut c) = self.cache.lock() {
            c.remove(uri);
        }
    }

    fn forget_all(&self) {
        if let Ok(mut c) = self.cache.lock() {
            c.clear();
        }
    }

    fn byte_size(&self) -> usize {
        self.cache.lock().map_or(0, |c| c.values().map(|e| if let State::Ready(i) = &e.state { i.pixels.len() * 4 } else { 0 }).sum())
    }

    fn has_pending(&self) -> bool {
        self.cache.lock().is_ok_and(|c| c.values().any(|e| matches!(e.state, State::Pending)))
    }
}

/// Ставит загрузчик: он спрашивается раньше уже установленных. Анимированные GIF и WebP он
/// не берёт, если раньше поставлен `egui_extras` (тот умеет кадры).
pub fn install(ctx: &Context) {
    if ctx.is_loader_installed(egui::generate_loader_id!(FileLoader)) {
        return;
    }
    let mut skip = Vec::new();
    if ctx.is_loader_installed("egui_extras::loaders::gif_loader::GifLoader") {
        skip.push("gif");
    }
    if ctx.is_loader_installed("egui_extras::loaders::webp_loader::WebPLoader") {
        skip.push("webp");
    }
    ctx.add_image_loader(Arc::new(FileLoader { skip, ..Default::default() }));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("forma-images-test");
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    /// Каждый формат крейта image читается, и размер — исходный.
    #[test]
    fn crate_formats_decode() {
        let img = RgbaImage::from_fn(5, 3, |x, y| image::Rgba([(x * 40) as u8, (y * 80) as u8, 100, 255]));
        for ext in ["png", "jpg", "bmp", "tiff", "tga", "qoi", "ff", "ico", "pnm"] {
            let path = temp(&format!("a.{ext}"));
            let dynamic = DynamicImage::ImageRgba8(img.clone());
            // JPEG и PNM без прозрачности, farbfeld — 16 бит на канал.
            let save = match ext {
                "jpg" | "pnm" => dynamic.to_rgb8().save(&path),
                "ff" => dynamic.to_rgba16().save(&path),
                _ => img.save(&path),
            };
            save.unwrap_or_else(|e| panic!("{ext}: {e}"));
            let d = decode(&path, 4096).unwrap_or_else(|e| panic!("{ext}: {e}"));
            assert_eq!((d.info.width, d.info.height), (5, 3), "{ext}");
            assert_eq!(d.image.size, [5, 3]);
        }
    }

    /// Линейный свет из EXR показывается через sRGB: 0.5 становится светлее 127.
    #[test]
    fn float_images_go_through_srgb() {
        let img = image::Rgba32FImage::from_pixel(2, 2, image::Rgba([0.5, 0.5, 0.5, 1.0]));
        let path = temp("a.exr");
        DynamicImage::ImageRgba32F(img).save(&path).unwrap();
        let d = decode(&path, 4096).unwrap();
        assert!(d.image.pixels[0].r() > 180, "{:?}", d.image.pixels[0]);
        assert_eq!(d.info.format, "OpenEXR");
    }

    /// Большая картинка уменьшается до предела видеокарты, размер в сведениях — исходный.
    #[test]
    fn big_images_fit_the_texture_limit() {
        let path = temp("big.png");
        RgbaImage::new(300, 100).save(&path).unwrap();
        let d = decode(&path, 120).unwrap();
        assert_eq!(d.image.size, [120, 40]);
        assert_eq!((d.info.width, d.info.height), (300, 100));
    }

    #[test]
    fn svg_renders_sharper_than_its_size() {
        let path = temp("a.svg");
        std::fs::write(&path, r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><rect width="20" height="10" fill="#f00"/></svg>"##).unwrap();
        let d = decode(&path, 4096).unwrap();
        assert_eq!(d.image.size, [80, 40]);
        assert_eq!((d.info.width, d.info.height), (20, 10));
        assert_eq!(d.image.pixels[0], egui::Color32::from_rgb(255, 0, 0));
    }

    /// Кодеки Windows читают обычный PNG — значит, путь через WIC работает.
    #[cfg(windows)]
    #[test]
    fn windows_codecs_read_png() {
        let path = temp("wic.png");
        RgbaImage::from_pixel(4, 2, image::Rgba([10, 20, 30, 255])).save(&path).unwrap();
        let (rgba, w, h) = system(&path).unwrap();
        assert_eq!((w, h), (4, 2));
        assert_eq!(rgba.get_pixel(0, 0).0, [10, 20, 30, 255]);
    }

}
