//! Схема настроек плагина — как `contributes.configuration` у расширений VS Code: ключ,
//! тип, значение по умолчанию, подпись, пояснение, варианты. По схеме окно настроек само
//! строит страницу плагина, а `settings.json` проверяется на неизвестные ключи и не тот тип.
//!
//! Ключ полный и плоский, с id плагина в начале: `rust.inlayHints.types`. В манифесте
//! плагина настройки — массив таблиц:
//!
//! ```toml
//! [[contributes.settings]]
//! key = "terms.hovers"
//! type = "boolean"
//! default = true
//! title = "Подсказки при наведении"
//! description = "Определение термина над словом из глоссария."
//! ```

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Тип значения настройки.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    #[default]
    Boolean,
    Number,
    Integer,
    String,
    /// Список: в окне настроек — ссылка «Изменить в settings.json».
    Array,
    /// Объект: тоже только в `settings.json`.
    Object,
}

/// Вариант значения (выпадающий список).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Choice {
    pub value: Value,
    pub title: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
}

/// Настройка плагина.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Setting {
    /// Полный ключ: `rust.inlayHints.types`.
    pub key: String,
    #[serde(rename = "type")]
    pub kind: Kind,
    #[serde(skip_serializing_if = "Value::is_null")]
    pub default: Value,
    /// Подпись в окне настроек; пусто — из ключа («Inlay Hints: Types»).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub title: String,
    /// Пояснение под настройкой.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Карточка на странице плагина, куда настройка попадает (в порядке первого появления).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub section: String,
    /// Допустимые значения — выпадающий список вместо поля.
    #[serde(rename = "enum", skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<Choice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimum: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maximum: Option<f64>,
    /// Шаг поля числа.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<f64>,
    /// Подпись единицы после числа: «с», «px».
    #[serde(skip_serializing_if = "String::is_empty")]
    pub unit: String,
    /// Ключ галочки, без которой эта настройка не действует: пока галочка снята, строка в
    /// окне настроек неактивна (как группы подсказок под «Показывать подсказки»).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub requires: String,
}

/// Что не так со значением настройки.
#[derive(Clone, Debug, PartialEq)]
pub enum Invalid {
    /// Другой тип: какой нужен.
    Type(Kind),
    /// Не из списка вариантов.
    NotAChoice,
    /// Меньше наименьшего.
    Below(f64),
    /// Больше наибольшего.
    Above(f64),
}

impl Setting {
    pub fn new(key: impl Into<String>, kind: Kind, default: impl Into<Value>) -> Self {
        Self { key: key.into(), kind, default: default.into(), ..Self::default() }
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    pub fn description(mut self, text: impl Into<String>) -> Self {
        self.description = text.into();
        self
    }

    pub fn section(mut self, section: impl Into<String>) -> Self {
        self.section = section.into();
        self
    }

    pub fn choice(mut self, value: impl Into<Value>, title: impl Into<String>) -> Self {
        self.choices.push(Choice { value: value.into(), title: title.into(), description: String::new() });
        self
    }

    pub fn range(mut self, min: f64, max: f64, step: f64) -> Self {
        (self.minimum, self.maximum, self.step) = (Some(min), Some(max), Some(step));
        self
    }

    pub fn unit(mut self, unit: impl Into<String>) -> Self {
        self.unit = unit.into();
        self
    }

    pub fn requires(mut self, key: impl Into<String>) -> Self {
        self.requires = key.into();
        self
    }

    /// Подпись: заданная, иначе из ключа без id плагина, как в VS Code:
    /// `rust.inlayHints.closingBraces` → «Inlay Hints: Closing Braces».
    pub fn label(&self) -> String {
        if !self.title.trim().is_empty() {
            return self.title.clone();
        }
        let rest = self.key.split_once('.').map_or(self.key.as_str(), |(_, r)| r);
        rest.split('.').map(words).collect::<Vec<_>>().join(": ")
    }

    /// Годится ли значение.
    pub fn check(&self, v: &Value) -> Result<(), Invalid> {
        let typed = match self.kind {
            Kind::Boolean => v.is_boolean(),
            Kind::Number => v.is_number(),
            Kind::Integer => v.is_i64() || v.is_u64(),
            Kind::String => v.is_string(),
            Kind::Array => v.is_array(),
            Kind::Object => v.is_object(),
        };
        if !typed {
            return Err(Invalid::Type(self.kind));
        }
        if !self.choices.is_empty() && !self.choices.iter().any(|c| same(&c.value, v)) {
            return Err(Invalid::NotAChoice);
        }
        if let Some(n) = v.as_f64() {
            if let Some(min) = self.minimum.filter(|m| n < *m) {
                return Err(Invalid::Below(min));
            }
            if let Some(max) = self.maximum.filter(|m| n > *m) {
                return Err(Invalid::Above(max));
            }
        }
        Ok(())
    }

    /// Значение, которое действует: заданное, если оно годится, иначе по умолчанию.
    pub fn effective(&self, v: Option<&Value>) -> Value {
        match v {
            Some(v) if self.check(v).is_ok() => v.clone(),
            _ => self.default.clone(),
        }
    }

    /// Значение по умолчанию ли это (числа сравниваются как числа: `1` и `1.0` — одно).
    pub fn is_default(&self, v: &Value) -> bool {
        same(&self.default, v)
    }
}

/// Одинаковые ли значения: числа — по величине.
pub fn same(a: &Value, b: &Value) -> bool {
    match (a.as_f64(), b.as_f64()) {
        (Some(x), Some(y)) if a.is_number() && b.is_number() => x == y,
        _ => a == b,
    }
}

/// `inlayHints` → «Inlay Hints», `closing_braces` → «Closing Braces».
fn words(part: &str) -> String {
    let mut out = String::new();
    let mut prev_lower = false;
    for c in part.chars() {
        if c == '_' || c == '-' {
            out.push(' ');
            prev_lower = false;
            continue;
        }
        if c.is_uppercase() && prev_lower {
            out.push(' ');
        }
        let first = out.is_empty() || out.ends_with(' ');
        if first {
            out.extend(c.to_uppercase());
        } else {
            out.push(c);
        }
        prev_lower = c.is_lowercase() || c.is_ascii_digit();
    }
    out
}

/// Проверка схемы плагина `plugin`: ключи начинаются с `plugin.`, не повторяются и не
/// вложены друг в друга (у VS Code ключ не может быть началом другого ключа), значения по
/// умолчанию подходят своим настройкам.
pub fn validate(plugin: &str, settings: &[Setting]) -> Result<(), String> {
    let prefix = format!("{plugin}.");
    for (i, s) in settings.iter().enumerate() {
        if !s.key.starts_with(&prefix) || s.key.len() == prefix.len() {
            return Err(format!("setting {:?} must start with {prefix:?}", s.key));
        }
        if s.key.contains(char::is_whitespace) || s.key.ends_with('.') || s.key.contains("..") {
            return Err(format!("bad setting key {:?}", s.key));
        }
        for other in &settings[..i] {
            if other.key == s.key {
                return Err(format!("setting {:?} is declared twice", s.key));
            }
            if other.key.starts_with(&format!("{}.", s.key)) || s.key.starts_with(&format!("{}.", other.key)) {
                return Err(format!("setting {:?} and {:?}: one key cannot start another", other.key, s.key));
            }
        }
        if s.default.is_null() {
            return Err(format!("setting {:?} has no default", s.key));
        }
        if let Err(e) = s.check(&s.default) {
            return Err(format!("setting {:?}: the default does not fit ({e:?})", s.key));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn values_are_checked_against_the_schema() {
        let mode = Setting::new("rust.check.command", Kind::String, "clippy").choice("clippy", "Clippy").choice("check", "cargo check");
        assert_eq!(mode.check(&json!("check")), Ok(()));
        assert_eq!(mode.check(&json!("build")), Err(Invalid::NotAChoice));
        assert_eq!(mode.check(&json!(1)), Err(Invalid::Type(Kind::String)));
        let len = Setting::new("rust.inlayHints.maxLength", Kind::Integer, 30).range(5.0, 120.0, 1.0);
        assert_eq!(len.check(&json!(2)), Err(Invalid::Below(5.0)));
        assert_eq!(len.check(&json!(1.5)), Err(Invalid::Type(Kind::Integer)));
        // Не годится — действует значение по умолчанию.
        assert_eq!(len.effective(Some(&json!(500))), json!(30));
        assert_eq!(len.effective(Some(&json!(40))), json!(40));
        assert!(len.is_default(&json!(30.0)), "числа сравниваются по величине");
    }

    #[test]
    fn labels_come_from_keys_like_vs_code() {
        assert_eq!(Setting::new("rust.inlayHints.closingBraces", Kind::Boolean, true).label(), "Inlay Hints: Closing Braces");
        assert_eq!(Setting::new("terms.show_hovers", Kind::Boolean, true).label(), "Show Hovers");
        assert_eq!(Setting::new("terms.x", Kind::Boolean, true).title("Своя подпись").label(), "Своя подпись");
    }

    #[test]
    fn schemas_are_validated() {
        let ok = [Setting::new("terms.hovers", Kind::Boolean, true)];
        assert!(validate("terms", &ok).is_ok());
        assert!(validate("other", &ok).unwrap_err().contains("must start"));
        let twice = [Setting::new("a.b", Kind::Boolean, true), Setting::new("a.b", Kind::Boolean, false)];
        assert!(validate("a", &twice).unwrap_err().contains("twice"));
        let nested = [Setting::new("a.b", Kind::Boolean, true), Setting::new("a.b.c", Kind::Boolean, false)];
        assert!(validate("a", &nested).is_err());
        let wrong = [Setting::new("a.b", Kind::Integer, "x")];
        assert!(validate("a", &wrong).is_err());
    }

    #[test]
    fn settings_read_from_a_toml_manifest() {
        let text = "key = \"terms.hovers\"\ntype = \"boolean\"\ndefault = true\ntitle = \"Подсказки\"\n";
        let s: Setting = toml_like(text);
        assert_eq!(s.kind, Kind::Boolean);
        assert_eq!(s.default, json!(true));
        assert_eq!(s.title, "Подсказки");
    }

    /// Разбор простого TOML (`ключ = значение`) без зависимости от toml: значения — JSON.
    fn toml_like(text: &str) -> Setting {
        let mut map = serde_json::Map::new();
        for line in text.lines() {
            let (k, v) = line.split_once(" = ").unwrap();
            map.insert(k.to_owned(), serde_json::from_str(v).unwrap());
        }
        serde_json::from_value(Value::Object(map)).unwrap()
    }
}
