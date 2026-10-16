//! Страница настроек плагина в окне настроек, построенная по его схеме
//! ([`forma_settings::schema`]), — как раздел расширения в Settings у VS Code: галочки,
//! выпадающие списки, числа и строки, «↺» у изменённых. Списки и объекты правятся только в
//! `settings.json` — у них ссылка «Изменить в settings.json».
//!
//! Страница работает с черновиком значений `settings.json` (в нём только то, что отличается
//! от значения по умолчанию): выбрал значение по умолчанию — ключ из черновика уходит.

use std::collections::BTreeMap;

use egui::{RichText, Ui};
use forma_i18n::t;
use forma_settings::jsonc::{ErrorKind, ParseError};
use forma_settings::schema::{Invalid, Kind, Setting, same};
use forma_settings::user::WriteError;
use serde_json::Value;

use crate::settings_window as sw;
use crate::theme::Palette;

/// Начало id страницы плагина: `plugin:rust`.
pub const PREFIX: &str = "plugin:";

pub fn page_id(plugin: &str) -> String {
    format!("{PREFIX}{plugin}")
}

/// Плагин страницы, если это страница плагина.
pub fn plugin_of(page: &str) -> Option<&str> {
    page.strip_prefix(PREFIX)
}

/// Подписи настроек и их вариантов — для поиска по окну настроек.
pub fn keywords(settings: &[Setting]) -> Vec<String> {
    let mut out: Vec<String> = settings.iter().map(Setting::label).collect();
    out.extend(settings.iter().map(|s| s.key.clone()));
    out.extend(settings.iter().flat_map(|s| s.choices.iter().map(|c| c.title.clone())));
    out
}

/// Что нажали на странице.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageAction {
    /// Открыть `settings.json`.
    OpenJson,
}

/// Почему значение из `settings.json` не действует — словами.
pub fn invalid_text(why: &Invalid) -> String {
    match why {
        Invalid::Type(kind) => t!("settings-invalid-type", kind = kind_text(*kind)),
        Invalid::NotAChoice => t!("settings-invalid-choice"),
        Invalid::Below(min) => t!("settings-invalid-below", min = min.to_string()),
        Invalid::Above(max) => t!("settings-invalid-above", max = max.to_string()),
    }
}

/// Что не так в тексте `settings.json` — словами (без места: строку и столбец добавляет тот,
/// кто показывает).
pub fn parse_error_text(e: &ParseError) -> String {
    match &e.kind {
        ErrorKind::End => t!("json-error-end"),
        ErrorKind::Expected(c) => t!("json-error-expected", what = c.to_string()),
        ErrorKind::BadString => t!("json-error-string"),
        ErrorKind::BadNumber => t!("json-error-number"),
        ErrorKind::BadValue => t!("json-error-value"),
        ErrorKind::NotObject => t!("json-error-not-object"),
        ErrorKind::Trailing => t!("json-error-trailing"),
    }
}

/// Почему не получилось записать `settings.json` — словами.
pub fn write_error_text(e: &WriteError) -> String {
    match e {
        WriteError::Broken(p) => t!("settings-json-fix-first", line = p.line, col = p.col, error = parse_error_text(p)),
        WriteError::Io(e) => e.clone(),
        WriteError::NoPath => t!("settings-json-no-folder"),
    }
}

/// Тип значения словами: «да или нет», «целое число».
pub fn kind_text(kind: Kind) -> String {
    t!(match kind {
        Kind::Boolean => "settings-kind-boolean",
        Kind::Number => "settings-kind-number",
        Kind::Integer => "settings-kind-integer",
        Kind::String => "settings-kind-string",
        Kind::Array => "settings-kind-array",
        Kind::Object => "settings-kind-object",
    })
}

/// Записывает значение в черновик: значение по умолчанию — ключ уходит.
fn put(draft: &mut BTreeMap<String, Value>, s: &Setting, v: Value) {
    if s.is_default(&v) {
        draft.remove(&s.key);
    } else {
        draft.insert(s.key.clone(), v);
    }
}

/// Рисует страницу плагина `plugin` с настройками `settings` над черновиком `draft`.
pub fn page(ui: &mut Ui, pal: &Palette, search: &str, plugin: &str, settings: &[Setting], draft: &mut BTreeMap<String, Value>) -> Option<PageAction> {
    let mut action = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.label(RichText::new(t!("settings-json-note")).size(12.0).color(pal.text_faint));
        if crate::icons::text_button(ui, "file-json", t!("settings-open-json")).clicked() {
            action = Some(PageAction::OpenJson);
        }
    });
    ui.add_space(10.0);
    // Карточки — в порядке первого появления раздела в схеме.
    let mut sections: Vec<&str> = Vec::new();
    for s in settings {
        if !sections.contains(&s.section.as_str()) {
            sections.push(&s.section);
        }
    }
    for section in sections {
        sw::section(ui, pal, section, |ui| {
            for s in settings.iter().filter(|s| s.section == section) {
                // Настройка, которая действует только вместе с галочкой (`requires`).
                let on = s.requires.is_empty() || settings.iter().find(|r| r.key == s.requires).is_none_or(|r| r.effective(draft.get(&r.key)) == Value::Bool(true));
                ui.add_enabled_ui(on, |ui| {
                    if row(ui, pal, search, plugin, s, draft) {
                        action = Some(PageAction::OpenJson);
                    }
                });
            }
        });
    }
    action
}

/// Строка настройки. `true` — нажали «Изменить в settings.json».
fn row(ui: &mut Ui, pal: &Palette, search: &str, plugin: &str, s: &Setting, draft: &mut BTreeMap<String, Value>) -> bool {
    let label = s.label();
    let given = draft.get(&s.key).cloned();
    let value = s.effective(given.as_ref());
    let modified = !same(&value, &s.default);
    let id = sw::id(plugin, &s.key);
    let mut open_json = false;
    let reset = if s.kind == Kind::Boolean && s.choices.is_empty() {
        let mut v = value.as_bool().unwrap_or(false);
        let reset = sw::check(ui, pal, search, &label, &mut v, modified);
        if Value::Bool(v) != value {
            put(draft, s, Value::Bool(v));
        }
        reset
    } else {
        sw::row(ui, pal, search, &label, modified, |ui| {
            if !s.choices.is_empty() {
                let current = s.choices.iter().position(|c| same(&c.value, &value));
                let options: Vec<(Option<usize>, String)> = s.choices.iter().enumerate().map(|(i, c)| (Some(i), c.title.clone())).collect();
                if let Some(Some(i)) = crate::widgets::dropdown(ui, pal, id, 280.0, &current, &options) {
                    put(draft, s, s.choices[i].value.clone());
                }
                return;
            }
            match s.kind {
                Kind::Number | Kind::Integer => {
                    let lo = s.minimum.unwrap_or(-1.0e6) as f32;
                    let hi = s.maximum.unwrap_or(1.0e6) as f32;
                    let step = s.step.unwrap_or(if s.kind == Kind::Integer { 1.0 } else { 0.1 }) as f32;
                    let mut n = value.as_f64().unwrap_or(0.0) as f32;
                    // Единица — через пробел: «30 симв.», «2 с».
                    let unit = if s.unit.is_empty() || s.unit.starts_with(' ') { s.unit.clone() } else { format!(" {}", s.unit) };
                    if crate::widgets::number_field(ui, pal, id, &mut n, lo..=hi, step, &unit, 160.0).changed() {
                        let v = if s.kind == Kind::Integer { Value::from(n.round() as i64) } else { serde_json::Number::from_f64(f64::from(n)).map_or(Value::Null, Value::Number) };
                        if !v.is_null() {
                            put(draft, s, v);
                        }
                    }
                }
                Kind::String => {
                    let mut text = value.as_str().unwrap_or_default().to_owned();
                    if ui.add(egui::TextEdit::singleline(&mut text).id(id).desired_width(260.0)).changed() {
                        put(draft, s, Value::String(text));
                    }
                }
                Kind::Boolean | Kind::Array | Kind::Object => {
                    if crate::icons::text_button(ui, "file-json", t!("settings-json-edit")).clicked() {
                        open_json = true;
                    }
                }
            }
        })
    };
    if reset {
        draft.remove(&s.key);
    }
    // Значение из файла не подходит — действует значение по умолчанию; видно, почему.
    if let Some(Err(why)) = given.as_ref().map(|v| s.check(v)) {
        sw::warning(ui, pal, &t!("settings-json-invalid", why = invalid_text(&why)));
    }
    if !s.description.is_empty() {
        sw::hint(ui, pal, &s.description);
    }
    open_json
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn default_values_leave_the_draft() {
        let s = Setting::new("rust.inlayHints.maxLength", Kind::Integer, 30);
        let mut draft = BTreeMap::new();
        put(&mut draft, &s, json!(40));
        assert_eq!(draft.get("rust.inlayHints.maxLength"), Some(&json!(40)));
        put(&mut draft, &s, json!(30.0));
        assert!(draft.is_empty(), "значение по умолчанию в settings.json не пишется");
        assert_eq!(plugin_of(&page_id("rust")), Some("rust"));
        assert!(keywords(&[s]).contains(&"Inlay Hints: Max Length".to_owned()));
    }
}
