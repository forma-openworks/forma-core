//! `settings.json` в папке настроек программы (`%APPDATA%\Forma\Instar\settings.json`) —
//! настройки плагинов, как пользовательский `settings.json` у VS Code. Плоские ключи
//! (`"rust.inlayHints.types": false`), комментарии разрешены, в файле только то, что
//! отличается от значения по умолчанию.
//!
//! Файл правят двумя способами, и оба равноправны: окно настроек ([`UserSettings::apply`]
//! меняет только свои ключи, не трогая комментарии) и человек в редакторе — тогда
//! [`UserSettings::poll`] замечает новый текст и перечитывает его. Пока в файле ошибка,
//! действуют прежние значения, а окно настроек в него не пишет.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde_json::Value;

use crate::jsonc::{self, ParseError};

/// Как часто проверять, не поменялся ли файл на диске.
const POLL_EVERY: f64 = 1.0;

/// Что нашёл опрос файла.
#[derive(Clone, Debug, PartialEq)]
pub enum Poll {
    /// Ничего не поменялось.
    Same,
    /// Перечитали: значения другие.
    Changed,
    /// Файл поменялся, но в нём ошибка: действуют прежние значения.
    Broken(ParseError),
}

/// Почему не получилось записать.
#[derive(Clone, Debug, PartialEq)]
pub enum WriteError {
    /// В файле ошибка: сначала её надо исправить (как в VS Code).
    Broken(ParseError),
    Io(String),
    /// Папки настроек нет.
    NoPath,
}

pub struct UserSettings {
    path: Option<PathBuf>,
    values: BTreeMap<String, Value>,
    error: Option<ParseError>,
    /// Время изменения и размер файла при последнем чтении.
    stamp: Option<(Option<SystemTime>, u64)>,
    /// Растёт, когда меняются значения.
    rev: u64,
    next_poll: f64,
}

fn stamp(path: &Path) -> Option<(Option<SystemTime>, u64)> {
    std::fs::metadata(path).ok().map(|m| (m.modified().ok(), m.len()))
}

impl UserSettings {
    /// Читает файл `path` (его может ещё не быть).
    pub fn open(path: Option<PathBuf>) -> Self {
        let mut s = Self { path, values: BTreeMap::new(), error: None, stamp: None, rev: 0, next_poll: 0.0 };
        s.reload();
        s
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Значения из файла, как есть (и чужих, и неизвестных ключей).
    pub fn values(&self) -> &BTreeMap<String, Value> {
        &self.values
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.values.get(key)
    }

    /// Ошибка в файле, если он сейчас не разбирается.
    pub fn error(&self) -> Option<&ParseError> {
        self.error.as_ref()
    }

    /// Номер изменения значений: по нему программа узнаёт, что пора перечитать настройки.
    pub fn rev(&self) -> u64 {
        self.rev
    }

    /// Перечитывает файл.
    fn reload(&mut self) -> Poll {
        let Some(path) = &self.path else { return Poll::Same };
        self.stamp = stamp(path);
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => {
                log::warn!("{}: {e}", path.display());
                return Poll::Same;
            }
        };
        match jsonc::parse(&text) {
            Ok(doc) => {
                self.error = None;
                let values = doc.values();
                if values == self.values {
                    return Poll::Same;
                }
                self.values = values;
                self.rev += 1;
                Poll::Changed
            }
            Err(e) => {
                self.error = Some(e.clone());
                Poll::Broken(e)
            }
        }
    }

    /// Раз в секунду (`now` — время кадра): не поменял ли кто-то файл.
    pub fn poll(&mut self, now: f64) -> Poll {
        if now < self.next_poll {
            return Poll::Same;
        }
        self.next_poll = now + POLL_EVERY;
        match &self.path {
            Some(path) if stamp(path) != self.stamp => self.reload(),
            _ => Poll::Same,
        }
    }

    /// Меняет ключи (`None` — убрать ключ: снова значение по умолчанию) и записывает файл.
    /// Текст берётся с диска, чтобы не затереть то, что человек поправил только что.
    pub fn apply(&mut self, changes: &[(String, Option<Value>)]) -> Result<(), WriteError> {
        if changes.is_empty() {
            return Ok(());
        }
        let path = self.path.clone().ok_or(WriteError::NoPath)?;
        let mut text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(WriteError::Io(e.to_string())),
        };
        for (key, value) in changes {
            text = jsonc::set(&text, key, value.as_ref()).map_err(WriteError::Broken)?;
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| WriteError::Io(e.to_string()))?;
        }
        crate::write_atomic(&path, text.as_bytes()).map_err(|e| WriteError::Io(e.to_string()))?;
        self.reload();
        Ok(())
    }

    /// Файл, а если его ещё нет — новый `{}` (чтобы открыть в редакторе).
    pub fn ensure_file(&mut self) -> Result<PathBuf, WriteError> {
        let path = self.path.clone().ok_or(WriteError::NoPath)?;
        if !path.exists() {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir).map_err(|e| WriteError::Io(e.to_string()))?;
            }
            crate::write_atomic(&path, b"{\n}\n").map_err(|e| WriteError::Io(e.to_string()))?;
            self.reload();
        }
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_file_is_edited_by_the_window_and_by_hand() {
        let dir = std::env::temp_dir().join(format!("forma-user-settings-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("settings.json");
        let mut s = UserSettings::open(Some(path.clone()));
        assert!(s.values().is_empty(), "файла ещё нет");
        s.apply(&[("rust.inlayHints.types".into(), Some(json!(false)))]).unwrap();
        assert_eq!(s.get("rust.inlayHints.types"), Some(&json!(false)));
        let rev = s.rev();

        // Человек дописал комментарий и ключ — опрос это видит.
        std::fs::write(&path, "{\n    // мои\n    \"rust.inlayHints.types\": false,\n    \"terms.hovers\": false\n}\n").unwrap();
        s.next_poll = 0.0;
        s.stamp = None;
        assert_eq!(s.poll(10.0), Poll::Changed);
        assert!(s.rev() > rev);
        assert_eq!(s.get("terms.hovers"), Some(&json!(false)));

        // Окно настроек возвращает значение по умолчанию — ключ уходит, комментарий остаётся.
        s.apply(&[("rust.inlayHints.types".into(), None)]).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("// мои") && !text.contains("inlayHints"), "{text}");

        // Сломанный файл: действуют прежние значения, а записать в него нельзя.
        std::fs::write(&path, "{ \"terms.hovers\": tru }").unwrap();
        s.next_poll = 0.0;
        s.stamp = None;
        assert!(matches!(s.poll(20.0), Poll::Broken(_)));
        assert_eq!(s.get("terms.hovers"), Some(&json!(false)));
        assert!(matches!(s.apply(&[("terms.hovers".into(), Some(json!(true)))]), Err(WriteError::Broken(_))));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
