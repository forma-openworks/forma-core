//! Архив настроек для переноса: zip с `manifest.json` и `options/<id>.json`.
//!
//! Архив одной программы Forma можно импортировать в другую: компоненты, которых
//! она не знает, помечаются как неподдерживаемые и пропускаются.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{AppInfo, Component};

/// Версия формата архива.
const FORMAT: u32 = 1;
const MANIFEST: &str = "manifest.json";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Manifest {
    pub format: u32,
    /// Из какой программы архив: `mycelium`.
    pub app_id: String,
    pub app_title: String,
    pub app_version: String,
    pub created_unix: u64,
    pub components: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Archive {
    pub manifest: Manifest,
    pub components: BTreeMap<String, serde_json::Value>,
}

impl Archive {
    pub fn new(app: &AppInfo) -> Self {
        let created_unix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        Self {
            manifest: Manifest {
                format: FORMAT,
                app_id: app.id.to_owned(),
                app_title: app.title.to_owned(),
                app_version: app.version.to_owned(),
                created_unix,
                components: Vec::new(),
            },
            components: BTreeMap::new(),
        }
    }

    pub fn add<C: Component>(&mut self, c: &C) {
        if let Ok(v) = serde_json::to_value(c) {
            self.manifest.components.push(C::ID.to_owned());
            self.components.insert(C::ID.to_owned(), v);
        }
    }

    pub fn write(&self, path: &Path) -> std::io::Result<()> {
        let file = std::fs::File::create(path)?;
        let mut zip = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        zip.start_file(MANIFEST, opts).map_err(std::io::Error::other)?;
        zip.write_all(pretty(&self.manifest).as_bytes())?;
        for (id, value) in &self.components {
            zip.start_file(format!("options/{id}.json"), opts).map_err(std::io::Error::other)?;
            zip.write_all(pretty(value).as_bytes())?;
        }
        zip.finish().map_err(std::io::Error::other)?;
        Ok(())
    }

    pub fn read(path: &Path) -> std::io::Result<Self> {
        let file = std::fs::File::open(path)?;
        let mut zip = zip::ZipArchive::new(file).map_err(std::io::Error::other)?;
        let mut read = |name: &str| -> std::io::Result<String> {
            let mut entry = zip.by_name(name).map_err(std::io::Error::other)?;
            let mut text = String::new();
            entry.read_to_string(&mut text)?;
            Ok(text)
        };
        let manifest: Manifest = serde_json::from_str(&read(MANIFEST)?).map_err(std::io::Error::other)?;
        let mut components = BTreeMap::new();
        for id in &manifest.components {
            // Испорченный компонент не мешает остальным: он просто не попадёт в список.
            let Ok(text) = read(&format!("options/{id}.json")) else { continue };
            if let Ok(v) = serde_json::from_str(&text) {
                components.insert(id.clone(), v);
            }
        }
        Ok(Self { manifest, components })
    }
}

/// Имя файла по умолчанию: `Forma Mycelium settings 2026-10-11.zip`. Знаки, которых не
/// бывает в именах файлов Windows, убираются: «Forma: Holotype» → «Forma Holotype».
pub fn default_file_name(app: &AppInfo, unix: u64) -> String {
    let title: String = app.title.chars().filter(|c| !matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')).collect();
    let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
    format!("{title} settings {}.zip", date_text(unix))
}

/// Дата `2026-10-11` по времени Unix (UTC).
pub fn date_text(unix: u64) -> String {
    let (y, m, d) = civil_date(unix);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Дата по времени Unix (UTC), алгоритм Хиннанта.
fn civil_date(unix: u64) -> (i64, u32, u32) {
    let z = (unix / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

/// Что будет с компонентом архива при импорте.
#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    /// Применится целиком.
    Apply,
    /// Применится, но эти поля (или элементы) программа не знает и пропустит.
    Partial(Vec<String>),
    /// Такого компонента у программы нет.
    Unsupported,
    /// Компонент есть, но данные в архиве испорчены.
    Invalid(String),
}

impl Status {
    pub fn applicable(&self) -> bool {
        matches!(self, Status::Apply | Status::Partial(_))
    }
}

fn pretty<T: Serialize>(v: &T) -> String {
    serde_json::to_string_pretty(v).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::*;

    #[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
    #[serde(default)]
    struct Keys {
        map: BTreeMap<String, String>,
    }

    impl Component for Keys {
        const ID: &'static str = "keys";
    }

    const APP: AppInfo = AppInfo { id: "test", dir_name: "Test", title: "Forma Test", version: "0.1.0" };

    #[test]
    fn round_trip() {
        let path = std::env::temp_dir().join(format!("forma-export-{}.zip", std::process::id()));
        let mut a = Archive::new(&APP);
        let keys = Keys { map: [("file.save".to_owned(), "Ctrl+S".to_owned())].into() };
        a.add(&keys);
        a.write(&path).unwrap();
        let b = Archive::read(&path).unwrap();
        assert_eq!(b.manifest.app_id, "test");
        assert_eq!(b.manifest.components, vec!["keys".to_owned()]);
        assert_eq!(crate::decode::<Keys>(&b.components["keys"]).unwrap().0, keys);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn file_name_has_date() {
        assert_eq!(default_file_name(&APP, 1_791_676_800), "Forma Test settings 2026-10-11.zip");
        let colon = AppInfo { title: "Forma: Holotype", ..APP };
        assert_eq!(default_file_name(&colon, 1_791_676_800), "Forma Holotype settings 2026-10-11.zip");
        assert_eq!(civil_date(0), (1970, 1, 1));
        assert_eq!(civil_date(951_782_400), (2000, 2, 29));
    }
}
