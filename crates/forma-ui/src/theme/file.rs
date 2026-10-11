//! Файл темы (`*.toml`):
//!
//! ```toml
//! [theme]
//! name = "Midnight"
//! id = "midnight"
//! dark = true
//! extends = "mycelium-dark"      # чего нет здесь, берётся оттуда
//!
//! [palette]                      # свои именованные цвета: $blue
//! blue = "#3794ff"
//!
//! [colors]                       # ключи темы, их список — в шаблоне
//! accent = "$blue"
//! "menubar.bg" = "#ff69b4"
//!
//! [syntax]                       # подсветка кода
//! keyword = { color = "$blue", bold = true }
//! comment = { color = "#6a9955", italic = true }
//! string = "#ce9178"
//!
//! [fonts]                        # шрифты темы (необязательно)
//! editor = "Cascadia Mono"
//!
//! [background]                   # фоновая картинка (необязательно)
//! image = "wolf.png"             # путь от папки темы
//! opacity = 0.12
//! fit = "cover"
//!
//! [icons]                        # свои значки (необязательно), пути от папки темы
//! folder = "icons"               # каждый `имя.svg` в этой папке заменяет значок «имя»
//! run = "my-run.svg"             # или по одному
//! ```

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Meta {
    pub name: String,
    pub id: String,
    /// Тёмная ли тема; если не указано — как у родителя, а без родителя — тёмная.
    pub dark: Option<bool>,
    pub extends: Option<String>,
    pub author: Option<String>,
}

/// Начертание в подсветке: цвет строкой или таблица с жирным, курсивом, подчёркиванием.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum SyntaxSpec {
    Color(String),
    Style {
        color: Option<String>,
        #[serde(default)]
        bold: bool,
        #[serde(default)]
        italic: bool,
        #[serde(default)]
        underline: bool,
    },
}

/// Шрифты темы: имена семейств, установленных в системе.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct FontsSpec {
    pub ui: Option<String>,
    pub editor: Option<String>,
    pub console: Option<String>,
}

/// Как картинка заполняет окно.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Fit {
    /// Закрывает всё окно, лишнее обрезается.
    #[default]
    Cover,
    /// Целиком помещается в окно.
    Contain,
    /// Растягивается по окну без сохранения пропорций.
    Stretch,
    /// Повторяется плиткой.
    Tile,
    /// По центру в своём размере.
    Center,
}

impl Fit {
    pub const ALL: [Fit; 5] = [Fit::Cover, Fit::Contain, Fit::Stretch, Fit::Tile, Fit::Center];
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(default)]
pub struct BackgroundSpec {
    pub image: String,
    pub opacity: f32,
    pub fit: Fit,
}

impl Default for BackgroundSpec {
    fn default() -> Self {
        Self { image: String::new(), opacity: 0.12, fit: Fit::Cover }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct ThemeFile {
    pub theme: Meta,
    pub palette: BTreeMap<String, String>,
    pub colors: BTreeMap<String, String>,
    pub syntax: BTreeMap<String, SyntaxSpec>,
    pub fonts: FontsSpec,
    pub background: Option<BackgroundSpec>,
    /// Свои значки: имя значка → путь к SVG; `folder` — папка, где `имя.svg` заменяет значок.
    pub icons: BTreeMap<String, String>,
    /// Папка файла темы: от неё считаются пути к картинкам.
    #[serde(skip)]
    pub dir: Option<PathBuf>,
}

impl ThemeFile {
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut f: ThemeFile = toml::from_str(text).map_err(|e| e.to_string())?;
        if f.theme.id.trim().is_empty() {
            f.theme.id = slug(&f.theme.name);
        }
        if f.theme.id.is_empty() {
            return Err("у темы нет ни id, ни названия".into());
        }
        if f.theme.name.trim().is_empty() {
            f.theme.name = f.theme.id.clone();
        }
        Ok(f)
    }

    /// Тема поверх родителя: всё, что задано здесь, заменяет родительское.
    pub fn over(&self, parent: &ThemeFile) -> ThemeFile {
        let mut out = parent.clone();
        out.theme = Meta { dark: self.theme.dark.or(parent.theme.dark), ..self.theme.clone() };
        out.palette.extend(self.palette.clone());
        out.colors.extend(self.colors.clone());
        out.syntax.extend(self.syntax.clone());
        let f = &self.fonts;
        out.fonts = FontsSpec {
            ui: f.ui.clone().or(out.fonts.ui),
            editor: f.editor.clone().or(out.fonts.editor),
            console: f.console.clone().or(out.fonts.console),
        };
        if self.background.is_some() {
            out.background = self.background.clone();
            out.dir = self.dir.clone();
        }
        out.icons.extend(self.icons.clone());
        out
    }

    /// Пути к значкам — от папки темы, папка `folder` раскрыта в отдельные значки.
    /// Вызывается, когда известна папка файла: после этого тему можно наследовать
    /// из другой папки, и пути не собьются.
    pub fn resolve_icon_paths(&mut self) {
        let Some(dir) = self.dir.clone() else { return };
        let abs = |p: &str| {
            let path = PathBuf::from(p);
            if path.is_relative() { dir.join(path) } else { path }
        };
        let mut icons = BTreeMap::new();
        if let Some(folder) = self.icons.get("folder") {
            for e in std::fs::read_dir(abs(folder)).into_iter().flatten().flatten() {
                let path = e.path();
                if path.extension().is_some_and(|x| x.eq_ignore_ascii_case("svg")) {
                    if let Some(name) = path.file_stem() {
                        icons.insert(name.to_string_lossy().into_owned(), path.display().to_string());
                    }
                }
            }
        }
        for (name, path) in &self.icons {
            if name != "folder" {
                icons.insert(name.clone(), abs(path).display().to_string());
            }
        }
        self.icons = icons;
    }
}

/// id из названия: «Visual Studio Dark» → `visual-studio-dark`.
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.trim().chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_all_sections() {
        let f = ThemeFile::parse(
            r##"
[theme]
name = "Pink Hat"
dark = true
extends = "mycelium-dark"

[palette]
pink = "#ff69b4"

[colors]
"menubar.bg" = "$pink"

[syntax]
keyword = { color = "#c586c0", bold = true }
string = "#ce9178"

[fonts]
editor = "Cascadia Mono"

[background]
image = "wolf.png"
fit = "tile"
"##,
        )
        .unwrap();
        assert_eq!(f.theme.id, "pink-hat");
        assert_eq!(f.colors["menubar.bg"], "$pink");
        assert_eq!(f.syntax["keyword"], SyntaxSpec::Style { color: Some("#c586c0".into()), bold: true, italic: false, underline: false });
        assert_eq!(f.syntax["string"], SyntaxSpec::Color("#ce9178".into()));
        assert_eq!(f.fonts.editor.as_deref(), Some("Cascadia Mono"));
        let bg = f.background.unwrap();
        assert_eq!((bg.image.as_str(), bg.fit, bg.opacity), ("wolf.png", Fit::Tile, 0.12));
    }

    #[test]
    fn icon_paths_resolve_from_theme_folder() {
        let dir = std::env::temp_dir().join(format!("forma-theme-icons-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("icons")).unwrap();
        std::fs::write(dir.join("icons").join("trash.svg"), "<svg/>").unwrap();
        std::fs::write(dir.join("icons").join("readme.txt"), "").unwrap();
        let mut f = ThemeFile::parse("[theme]\nid = \"t\"\n[icons]\nfolder = \"icons\"\nrun = \"my-run.svg\"").unwrap();
        f.dir = Some(dir.clone());
        f.resolve_icon_paths();
        assert_eq!(f.icons.keys().collect::<Vec<_>>(), vec!["run", "trash"]);
        assert_eq!(PathBuf::from(&f.icons["run"]), dir.join("my-run.svg"));
        assert_eq!(PathBuf::from(&f.icons["trash"]), dir.join("icons").join("trash.svg"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn child_overrides_parent() {
        let parent = ThemeFile::parse("[theme]\nid = \"base\"\n[colors]\na = \"#111111\"\nb = \"#222222\"\n[fonts]\nui = \"Segoe UI\"").unwrap();
        let child = ThemeFile::parse("[theme]\nid = \"kid\"\nextends = \"base\"\n[colors]\nb = \"#333333\"").unwrap();
        let m = child.over(&parent);
        assert_eq!(m.theme.id, "kid");
        assert_eq!(m.colors["a"], "#111111");
        assert_eq!(m.colors["b"], "#333333");
        assert_eq!(m.fonts.ui.as_deref(), Some("Segoe UI"));
        assert!(ThemeFile::parse("[theme]\n").is_err());
        assert_eq!(slug("  Visual Studio Dark! "), "visual-studio-dark");
    }
}
