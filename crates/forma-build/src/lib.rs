//! Помощники сборочных скриптов (`build.rs`) программ Forma.
//!
//! - [`locales`] вшивает переводы из папки `locales/<язык>/*.ftl`: новый файл или язык
//!   подхватывается сам, код менять не нужно.
//! - [`windows_resources`] вшивает в `.exe` иконку нескольких размеров и сведения о продукте,
//!   чтобы проводник и диспетчер задач показывали название программы.
//!
//! ```text
//! // build.rs
//! fn main() {
//!     forma_build::locales("../../locales", "LOCALES");
//! }
//! // main.rs
//! include!(concat!(env!("OUT_DIR"), "/locales.rs"));
//! forma_i18n::init_with(&[LOCALES], None);
//! ```

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

fn out_dir() -> PathBuf {
    PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR: функция вызывается из build.rs"))
}

fn manifest_dir() -> PathBuf {
    PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"))
}

/// Вшивает переводы из `dir` (путь от папки крейта): пишет `$OUT_DIR/locales.rs` со
/// статической таблицей `pub static <name>` — её принимает `forma_i18n::init_with`.
pub fn locales(dir: impl AsRef<Path>, name: &str) {
    let locales = manifest_dir().join(dir.as_ref());
    println!("cargo:rerun-if-changed={}", locales.display());

    let mut langs: Vec<(String, Vec<PathBuf>)> = Vec::new();
    let dirs = std::fs::read_dir(&locales).unwrap_or_else(|e| panic!("нет папки переводов {}: {e}", locales.display()));
    for dir in dirs.flatten() {
        let path = dir.path();
        if !path.is_dir() {
            continue;
        }
        let code = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
        let mut files: Vec<PathBuf> = std::fs::read_dir(&path)
            .expect("не удалось прочитать папку языка")
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "ftl"))
            .collect();
        files.sort();
        langs.push((code, files));
    }
    langs.sort();

    let mut out = String::from("/// Встроенные переводы: код языка и его файлы `.ftl` (имя, содержимое).\n");
    let _ = writeln!(out, "pub static {name}: &[(&str, &[(&str, &str)])] = &[");
    for (code, files) in &langs {
        let _ = writeln!(out, "    ({code:?}, &[");
        for file in files {
            println!("cargo:rerun-if-changed={}", file.display());
            let file_name = file.file_name().unwrap_or_default().to_string_lossy();
            let abs = std::path::absolute(file).expect("путь к .ftl");
            let _ = writeln!(out, "        ({file_name:?}, include_str!({:?})),", abs.display().to_string());
        }
        out.push_str("    ]),\n");
    }
    out.push_str("];\n");
    std::fs::write(out_dir().join("locales.rs"), out).expect("не удалось записать locales.rs");
}

/// Что вшить в `.exe` под Windows.
pub struct WindowsResources<'a> {
    /// Название продукта: «Forma Mycelium».
    pub product: &'a str,
    /// Имя исполняемого файла без пробелов: `forma-mycelium`.
    pub internal_name: &'a str,
    /// Имя готового файла: «Forma Mycelium.exe».
    pub file_name: &'a str,
    /// Рисунок иконки размером `size`×`size` пикселей (RGBA, без предумножения альфы).
    pub icon: &'a dyn Fn(u32) -> Vec<u8>,
}

/// Размеры картинок в `.ico`: от значка в заголовке окна до крупных значков проводника.
pub const ICON_SIZES: [u32; 10] = [16, 20, 24, 32, 40, 48, 64, 96, 128, 256];

/// Вшивает иконку и сведения о продукте в `.exe` (только при сборке под Windows).
/// Если компилятор ресурсов не найден, программа просто собирается без иконки у файла.
pub fn windows_resources(res: &WindowsResources) {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = out_dir();
    let ico = out.join(format!("{}.ico", res.internal_name));
    std::fs::write(&ico, ico_file(&ICON_SIZES, res.icon)).expect("не удалось записать .ico");
    let rc = out.join("app.rc");
    std::fs::write(&rc, resource_script(&ico, res)).expect("не удалось записать .rc");
    if let Err(err) = embed_resource::compile(&rc, embed_resource::NONE).manifest_optional() {
        println!("cargo:warning=Иконка не вшита в .exe: {err}");
    }
}

/// Сценарий ресурсов: иконка и VERSIONINFO с названием продукта.
fn resource_script(ico: &Path, res: &WindowsResources) -> String {
    let ico_path = ico.display().to_string().replace('\\', "\\\\");
    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".into());
    let mut parts: Vec<u16> = version.split(['.', '-']).filter_map(|p| p.parse().ok()).collect();
    parts.resize(4, 0);
    let commas = format!("{},{},{},{}", parts[0], parts[1], parts[2], parts[3]);
    let (product, internal, file) = (res.product, res.internal_name, res.file_name);
    format!(
        r#"1 ICON "{ico_path}"

1 VERSIONINFO
FILEVERSION {commas}
PRODUCTVERSION {commas}
FILEOS 0x40004
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "CompanyName", "FORMA"
      VALUE "FileDescription", "{product}"
      VALUE "ProductName", "{product}"
      VALUE "FileVersion", "{version}"
      VALUE "ProductVersion", "{version}"
      VALUE "InternalName", "{internal}"
      VALUE "OriginalFilename", "{file}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#
    )
}

/// Файл .ico: заголовок, каталог и картинки в формате 32-битного BMP (с альфа-каналом).
pub fn ico_file(sizes: &[u32], icon: &dyn Fn(u32) -> Vec<u8>) -> Vec<u8> {
    let images: Vec<Vec<u8>> = sizes.iter().map(|&s| bmp_entry(s, &icon(s))).collect();
    let mut out = Vec::new();
    out.extend_from_slice(&0u16.to_le_bytes()); // зарезервировано
    out.extend_from_slice(&1u16.to_le_bytes()); // тип: иконка
    out.extend_from_slice(&(sizes.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * sizes.len() as u32;
    for (&s, img) in sizes.iter().zip(&images) {
        let dim = if s >= 256 { 0 } else { s as u8 };
        out.extend_from_slice(&[dim, dim, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes()); // плоскости
        out.extend_from_slice(&32u16.to_le_bytes()); // бит на пиксель
        out.extend_from_slice(&(img.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += img.len() as u32;
    }
    for img in images {
        out.extend_from_slice(&img);
    }
    out
}

/// Одна картинка .ico: BITMAPINFOHEADER, пиксели BGRA снизу вверх и пустая маска.
fn bmp_entry(size: u32, rgba: &[u8]) -> Vec<u8> {
    let mask_row = size.div_ceil(32) * 4;
    let mut out = Vec::new();
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(size as i32).to_le_bytes());
    out.extend_from_slice(&(size as i32 * 2).to_le_bytes()); // высота картинки и маски
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // без сжатия
    out.extend_from_slice(&(size * size * 4 + mask_row * size).to_le_bytes());
    out.extend_from_slice(&[0u8; 16]); // разрешение и палитра не нужны
    for y in (0..size).rev() {
        for x in 0..size {
            let i = ((y * size + x) * 4) as usize;
            out.extend_from_slice(&[rgba[i + 2], rgba[i + 1], rgba[i], rgba[i + 3]]);
        }
    }
    out.extend(std::iter::repeat_n(0u8, (mask_row * size) as usize));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ico_has_directory_for_every_size() {
        let ico = ico_file(&[16, 256], &|s| vec![255; (s * s * 4) as usize]);
        assert_eq!(u16::from_le_bytes([ico[4], ico[5]]), 2);
        assert_eq!(ico[6], 16);
        // 256 пикселей в каталоге записываются нулём.
        assert_eq!(ico[6 + 16], 0);
    }
}
