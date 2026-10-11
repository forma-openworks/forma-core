//! `plugin.toml` — описание установленного плагина:
//!
//! ```toml
//! [plugin]
//! id = "pink-themes"                 # латиница, цифры и дефис; это же имя папки
//! name = "Pink themes"
//! version = "1.0"
//! kind = "themes"                    # tool — программа или сервер, themes — темы, script — скрипт
//! description = "Two pink themes"
//! author = "Holo"
//! license = "MIT"
//! homepage = "https://example.com"
//!
//! [contributes]
//! themes = "themes"                  # папка с темами (*.toml) внутри плагина
//! ```
//!
//! Плагинам из каталога программы `plugin.toml` пишет сама программа при установке.

use serde::{Deserialize, Serialize};

/// Вид плагина.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// Программа или языковой сервер (jdtls, kotlin-lsp, Python).
    #[default]
    Tool,
    /// Набор тем (со своими значками и картинками).
    Themes,
    /// Скрипт (появится вместе с плагином Python).
    Script,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Meta {
    pub id: String,
    pub name: String,
    pub version: String,
    pub kind: Kind,
    pub description: String,
    pub author: String,
    pub license: String,
    pub homepage: String,
    /// Когда установлен (секунды Unix).
    pub installed: Option<u64>,
}

/// Что плагин добавляет в программу.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Contributes {
    /// Папка с темами внутри плагина.
    pub themes: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Manifest {
    pub plugin: Meta,
    pub contributes: Contributes,
}

/// Годится ли строка в id плагина: латиница в нижнем регистре, цифры, дефис и точка.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.') && !id.starts_with('.')
}

impl Manifest {
    pub fn parse(text: &str) -> Result<Self, String> {
        let m: Manifest = toml::from_str(text).map_err(|e| e.to_string())?;
        if !valid_id(&m.plugin.id) {
            return Err(format!("bad plugin id {:?}: use lowercase latin letters, digits and dashes", m.plugin.id));
        }
        Ok(m)
    }

    pub fn to_toml(&self) -> String {
        toml::to_string_pretty(self).unwrap_or_default()
    }

    /// Название для людей: имя, а если его нет — id.
    pub fn title(&self) -> &str {
        if self.plugin.name.trim().is_empty() { &self.plugin.id } else { &self.plugin.name }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_checks_id() {
        let m = Manifest::parse("[plugin]\nid = \"pink-themes\"\nname = \"Pink\"\nkind = \"themes\"\n[contributes]\nthemes = \"themes\"").unwrap();
        assert_eq!(m.plugin.kind, Kind::Themes);
        assert_eq!(m.contributes.themes.as_deref(), Some("themes"));
        assert_eq!(Manifest::parse(&m.to_toml()).unwrap(), m, "запись и чтение сходятся");
        assert!(Manifest::parse("[plugin]\nid = \"../evil\"").is_err());
        assert!(Manifest::parse("[plugin]\nid = \"Big\"").is_err());
        assert!(!valid_id(".hidden"));
    }
}
