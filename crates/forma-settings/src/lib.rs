//! Настройки платформы Forma, как в JetBrains.
//!
//! Настройки разбиты на компоненты ([`Component`]): внешний вид, раскладка клавиш, Discord…
//! Каждый лежит в своём файле `options/<id>.json` в папке настроек программы. У каждой
//! программы Forma свои настройки. Переносить их между программами и компьютерами можно
//! архивом ([`export`]): при импорте применяется только то, что программа понимает,
//! а остальное перечисляется в отчёте.

pub mod export;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;

/// Что платформа знает о программе.
#[derive(Clone, Copy, Debug)]
pub struct AppInfo {
    /// Постоянный id: `mycelium`.
    pub id: &'static str,
    /// Папка внутри `Forma`: `Mycelium`.
    pub dir_name: &'static str,
    /// Название для людей: `Forma Mycelium`.
    pub title: &'static str,
    pub version: &'static str,
}

/// Переменная окружения с другой папкой настроек (вместо `%APPDATA%\Forma`):
/// для переносной установки и для проверок, которые не должны трогать настоящие настройки.
pub const CONFIG_HOME_VAR: &str = "FORMA_CONFIG_HOME";

impl AppInfo {
    /// Настройки: `%APPDATA%\Forma\<программа>` или `$FORMA_CONFIG_HOME\<программа>`.
    pub fn config_dir(&self) -> Option<PathBuf> {
        let home = std::env::var_os(CONFIG_HOME_VAR).filter(|v| !v.is_empty()).map(PathBuf::from);
        home.or_else(|| dirs::config_dir().map(|d| d.join("Forma"))).map(|d| d.join(self.dir_name))
    }

    /// Кеши, индексы, плагины: `%LOCALAPPDATA%\Forma\<программа>`, а с `FORMA_CONFIG_HOME` —
    /// `$FORMA_CONFIG_HOME\<программа>\cache` (переносная установка и проверки не трогают
    /// настоящие плагины и кеши).
    pub fn cache_dir(&self) -> Option<PathBuf> {
        if let Some(home) = std::env::var_os(CONFIG_HOME_VAR).filter(|v| !v.is_empty()) {
            return Some(PathBuf::from(home).join(self.dir_name).join("cache"));
        }
        dirs::data_local_dir().map(|d| d.join("Forma").join(self.dir_name))
    }
}

/// Компонент настроек: одна тема, один файл.
///
/// Поля компонента должны переживать отсутствие в файле (`#[serde(default)]`) и всегда
/// сериализоваться (без `skip_serializing_if`): по разнице между входным и записанным
/// JSON импорт узнаёт, какие поля программа не поняла.
pub trait Component: Serialize + DeserializeOwned + Default + Clone + PartialEq {
    /// Имя файла и ключ в архиве экспорта.
    const ID: &'static str;
    /// Можно ли переносить экспортом. Недавние проекты, например, нельзя.
    const EXPORTABLE: bool = true;
}

/// Папка `options` с компонентами. Помнит, что записано, чтобы не писать лишний раз.
#[derive(Debug)]
pub struct Store {
    dir: Option<PathBuf>,
    saved: HashMap<&'static str, String>,
}

impl Store {
    /// `dir` — папка настроек программы; `None`, если её нет (тогда ничего не сохраняется).
    pub fn new(config_dir: Option<PathBuf>) -> Self {
        Self { dir: config_dir.map(|d| d.join("options")), saved: HashMap::new() }
    }

    fn path(&self, id: &str) -> Option<PathBuf> {
        self.dir.as_ref().map(|d| d.join(format!("{id}.json")))
    }

    /// Есть ли уже папка компонентов (иначе — первый запуск или старый формат настроек).
    pub fn initialized(&self) -> bool {
        self.dir.as_ref().is_some_and(|d| d.is_dir())
    }

    /// Есть ли уже файл компонента.
    pub fn exists<C: Component>(&self) -> bool {
        self.path(C::ID).is_some_and(|p| p.is_file())
    }

    pub fn load<C: Component>(&mut self) -> C {
        let Some(path) = self.path(C::ID) else { return C::default() };
        let Ok(text) = std::fs::read_to_string(&path) else { return C::default() };
        match serde_json::from_str::<C>(&text) {
            Ok(c) => {
                self.saved.insert(C::ID, text);
                c
            }
            Err(err) => {
                log::warn!("Failed to parse {}: {err}", path.display());
                C::default()
            }
        }
    }

    /// Записывает компонент, если он изменился с прошлой записи.
    pub fn save<C: Component>(&mut self, c: &C) {
        let Some(path) = self.path(C::ID) else { return };
        let Ok(json) = serde_json::to_string_pretty(c) else { return };
        if self.saved.get(C::ID) == Some(&json) {
            return;
        }
        match write_atomic(&path, json.as_bytes()) {
            Ok(()) => {
                self.saved.insert(C::ID, json);
            }
            Err(err) => log::warn!("Failed to save {}: {err}", path.display()),
        }
    }
}

/// Пишет файл через временный, чтобы при сбое не остался обрезанный.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

/// Значение из архива или старого файла → компонент. Вторым возвращаются поля,
/// которых программа не знает: при импорте они пропускаются и попадают в отчёт.
pub fn decode<C: Component>(value: &serde_json::Value) -> Result<(C, Vec<String>), String> {
    let c: C = serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
    let back = serde_json::to_value(&c).map_err(|e| e.to_string())?;
    let mut skipped = Vec::new();
    unknown_keys(value, &back, "", &mut skipped);
    Ok((c, skipped))
}

fn unknown_keys(input: &serde_json::Value, known: &serde_json::Value, prefix: &str, out: &mut Vec<String>) {
    let (Some(input), Some(known)) = (input.as_object(), known.as_object()) else { return };
    for (key, value) in input {
        let path = if prefix.is_empty() { key.clone() } else { format!("{prefix}.{key}") };
        match known.get(key) {
            None => out.push(path),
            Some(k) => unknown_keys(value, k, &path, out),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::*;

    #[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
    #[serde(default)]
    struct Look {
        theme: String,
        size: f32,
        inner: Inner,
    }

    #[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
    #[serde(default)]
    struct Inner {
        a: bool,
    }

    impl Component for Look {
        const ID: &'static str = "look";
    }

    #[test]
    fn decode_reports_unknown_fields() {
        let v = serde_json::json!({ "theme": "dark", "glow": 3, "inner": { "a": true, "b": 1 } });
        let (look, skipped) = decode::<Look>(&v).unwrap();
        assert_eq!(look.theme, "dark");
        assert!(look.inner.a);
        assert_eq!(skipped, vec!["glow".to_owned(), "inner.b".to_owned()]);
        assert!(decode::<Look>(&serde_json::json!({ "size": "big" })).is_err());
    }

    #[test]
    fn store_writes_only_changes() {
        let dir = std::env::temp_dir().join(format!("forma-settings-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut store = Store::new(Some(dir.clone()));
        assert!(!store.initialized());
        let mut look = Look { theme: "dark".into(), ..Default::default() };
        store.save(&look);
        let file = dir.join("options").join("look.json");
        let first = std::fs::metadata(&file).unwrap().modified().unwrap();
        store.save(&look);
        assert_eq!(std::fs::metadata(&file).unwrap().modified().unwrap(), first, "без изменений файл не переписывается");
        look.size = 14.0;
        store.save(&look);
        let mut again = Store::new(Some(dir.clone()));
        assert!(again.initialized());
        assert_eq!(again.load::<Look>(), look);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
