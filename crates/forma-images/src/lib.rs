//! Картинки любых форматов для программ Forma: файл → изображение ([`decode`]) и загрузчик
//! картинок egui ([`install`]) для адресов `file:///…`.
//!
//! Что читается:
//! - всё, что умеет крейт `image`: PNG, JPEG, GIF, WebP, BMP, ICO, TIFF, TGA, DDS, HDR, EXR,
//!   PNM, QOI, farbfeld; HDR и EXR (линейный свет) показываются через sRGB;
//! - SVG (resvg), Photoshop PSD (сведённое изображение), JPEG XL (jxl-oxide);
//! - на Windows — всё, для чего в системе есть кодек WIC: HEIC/HEIF, AVIF, JPEG XR, RAW с
//!   камер (DNG, CR2, NEF, ARW…); и то, что крейт `image` не прочитал.
//!
//! Картинка больше предела видеокарты по стороне уменьшается: иначе egui не создаст текстуру.
//! Анимированные GIF и WebP загрузчик оставляет `egui_extras` (он умеет кадры), если тот
//! установлен раньше.

use std::path::{Path, PathBuf};

#[cfg(feature = "decode")]
mod decode;
#[cfg(feature = "decode")]
pub use decode::{Decoded, FileLoader, decode, info, install};

/// Расширения, которые читаются как картинки (в нижнем регистре).
pub const EXTENSIONS: &[&str] = &[
    // Крейт image.
    "png", "apng", "jpg", "jpeg", "jpe", "jfif", "gif", "webp", "bmp", "dib", "ico", "tif", "tiff", "tga", "dds", "hdr", "exr", "pbm", "pgm", "ppm", "pnm", "pam", "qoi", "ff",
    // Свои декодеры.
    "svg", "psd", "jxl",
    // Кодеки Windows.
    "heic", "heif", "hif", "avif", "jxr", "wdp", "hdp", "dng", "cr2", "cr3", "crw", "nef", "nrw", "arw", "srf", "sr2", "orf", "rw2", "raf", "pef", "srw", "rwl", "3fr", "erf", "kdc", "mrw", "mos", "iiq", "x3f", "raw",
];

/// Расширение в нижнем регистре.
pub(crate) fn extension(path: &Path) -> String {
    path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

/// Картинка ли это (по расширению).
pub fn is_image(path: &Path) -> bool {
    EXTENSIONS.contains(&extension(path).as_str())
}

/// Что известно о картинке: исходный размер (до уменьшения), формат и вес файла.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Info {
    pub width: u32,
    pub height: u32,
    /// `PNG`, `JPEG`, `OpenEXR`…
    pub format: String,
    pub bytes: u64,
}


/// Путь файла из адреса `file:///C:/…` (или `file://…`).
pub fn path_of(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let rest = rest.split('#').next().unwrap_or(rest);
    // `/C:/…` → `C:/…`.
    let rest = if rest.len() > 2 && rest.starts_with('/') && rest.as_bytes()[2] == b':' { &rest[1..] } else { rest };
    Some(PathBuf::from(rest.replace("%20", " ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_uris_become_paths() {
        assert_eq!(path_of("file:///C:/Хранилище/Вложения/a b.png"), Some(PathBuf::from("C:/Хранилище/Вложения/a b.png")));
        assert_eq!(path_of("file:///C:/x%20y.tga"), Some(PathBuf::from("C:/x y.tga")));
        assert_eq!(path_of("http://x/a.png"), None);
        assert!(is_image(Path::new("photo.HEIC")) && is_image(Path::new("t.exr")) && !is_image(Path::new("a.md")));
    }
}
