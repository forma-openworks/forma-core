//! Вшивает все файлы переводов из `locales/<язык>/*.ftl` в библиотеку.
//! Новый файл или язык подхватывается сам — код менять не нужно.

use std::fmt::Write as _;
use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let locales = manifest.join("../../locales");
    println!("cargo:rerun-if-changed={}", locales.display());

    let mut langs: Vec<(String, Vec<PathBuf>)> = Vec::new();
    let dirs = std::fs::read_dir(&locales).expect("нет папки locales");
    for dir in dirs.flatten() {
        let path = dir.path();
        if !path.is_dir() {
            continue;
        }
        let code = path.file_name().unwrap().to_string_lossy().into_owned();
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
    out.push_str("pub(crate) static BUILTIN: &[(&str, &[(&str, &str)])] = &[\n");
    for (code, files) in &langs {
        let _ = writeln!(out, "    ({code:?}, &[");
        for file in files {
            println!("cargo:rerun-if-changed={}", file.display());
            let name = file.file_name().unwrap().to_string_lossy();
            let abs = std::path::absolute(file).expect("путь к .ftl");
            let _ = writeln!(out, "        ({name:?}, include_str!({:?})),", abs.display().to_string());
        }
        out.push_str("    ]),\n");
    }
    out.push_str("];\n");

    let dest = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR")).join("locales.rs");
    std::fs::write(dest, out).expect("не удалось записать locales.rs");
}
