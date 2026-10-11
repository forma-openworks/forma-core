//! Внешний вид — компонент настроек платформы и его страница: язык, тема, масштаб,
//! фоновая картинка. Акцент — внутренний цвет темы, в настройках его нет.

use std::path::{Path, PathBuf};

use egui::{RichText, Ui};
use forma_i18n::t;
use serde::{Deserialize, Deserializer, Serialize};

use crate::settings_window::{self as sw, row};
use crate::theme::{Fit, Palette};
use crate::widgets;

pub const PAGE: &str = "appearance";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Appearance {
    /// Язык интерфейса; `None` — как в системе.
    pub language: Option<String>,
    /// id темы; пусто — тема программы по умолчанию.
    #[serde(deserialize_with = "theme_id")]
    pub theme: String,
    /// Масштаб всего интерфейса.
    pub zoom: f32,
    /// Своя фоновая картинка поверх картинки темы.
    pub background: Option<PathBuf>,
    pub background_opacity: f32,
    pub background_fit: Fit,
    /// Размер текста из первых версий: переезжает в компонент «Шрифты».
    #[serde(rename = "font_size", skip_serializing)]
    pub legacy_font_size: Option<f32>,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            language: None,
            theme: String::new(),
            zoom: 1.0,
            background: None,
            background_opacity: 0.12,
            background_fit: Fit::Cover,
            legacy_font_size: None,
        }
    }
}

/// id темы. Первые версии хранили «Dark» и «Light» — это встроенные темы Mycelium.
fn theme_id<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let s = String::deserialize(d)?;
    Ok(match s.as_str() {
        "Dark" => "mycelium-dark".to_owned(),
        "Light" => "mycelium-light".to_owned(),
        _ => s,
    })
}

impl forma_settings::Component for Appearance {
    const ID: &'static str = "appearance";
}

impl Appearance {
    /// Код языка с учётом «как в системе».
    pub fn effective_language(&self) -> String {
        self.language.clone().unwrap_or_else(forma_i18n::system_language)
    }
}

pub const ZOOM_RANGE: std::ops::RangeInclusive<f32> = 0.5..=3.0;

/// Тема в списке выбора.
#[derive(Clone, Debug, PartialEq)]
pub struct ThemeOption {
    pub id: String,
    pub name: String,
    /// Встроенная тема программы — помечается «По умолчанию».
    pub builtin: bool,
}

/// Что странице нужно знать о программе.
pub struct PageContext<'a> {
    /// Папка своих тем.
    pub themes_dir: Option<&'a Path>,
    pub themes: &'a [ThemeOption],
    /// Тема по умолчанию (когда в настройках пусто).
    pub default_theme: &'a str,
    /// Ошибки в файле выбранной темы.
    pub theme_errors: &'a [String],
}

/// Подписи настроек страницы — для поиска.
pub fn keywords() -> Vec<String> {
    vec![t!("settings-language"), t!("settings-theme"), t!("settings-zoom"), t!("settings-background")]
}

/// Что нажали на странице «Внешний вид».
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageAction {
    /// Создать свою тему из шаблона на основе выбранной.
    CreateTheme,
    OpenThemesFolder,
}

/// Страница «Внешний вид».
pub fn page(ui: &mut Ui, pal: &Palette, search: &str, a: &mut Appearance, cx: &PageContext) -> Option<PageAction> {
    let mut action = None;
    let d = Appearance::default();

    sw::group(ui, pal, &t!("settings-group-language"));
    let mut languages: Vec<(Option<String>, String)> = vec![(None, t!("settings-language-system"))];
    languages.extend(forma_i18n::languages().into_iter().map(|(code, name)| (Some(code), name)));
    let mut pick = None;
    if row(ui, pal, search, &t!("settings-language"), a.language != d.language, |ui| {
        pick = widgets::dropdown(ui, pal, sw::id(PAGE, "language"), 300.0, &a.language, &languages);
    }) {
        a.language = d.language.clone();
    }
    if let Some(l) = pick {
        a.language = l;
    }

    ui.add_space(8.0);
    sw::group(ui, pal, &t!("settings-group-look"));
    let current = if a.theme.is_empty() { cx.default_theme.to_owned() } else { a.theme.clone() };
    let themes: Vec<(String, String, Option<String>)> = cx.themes.iter().map(|t| (t.id.clone(), t.name.clone(), t.builtin.then(|| t!("theme-default")))).collect();
    let mut pick = None;
    if row(ui, pal, search, &t!("settings-theme"), !a.theme.is_empty() && a.theme != cx.default_theme, |ui| {
        pick = widgets::dropdown_tagged(ui, pal, sw::id(PAGE, "theme"), 300.0, &current, &themes);
    }) {
        a.theme = String::new();
    }
    if let Some(id) = pick {
        a.theme = id;
    }
    for e in cx.theme_errors.iter().take(5) {
        sw::warning(ui, pal, e);
    }
    if cx.themes_dir.is_some() {
        // Свои темы: шаблон на основе выбранной и папка, где они лежат.
        let indent = sw::label_indent(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            ui.add_space(indent);
            if crate::icons::text_button(ui, "palette", t!("settings-theme-create")).on_hover_text(t!("settings-theme-create-hint")).clicked() {
                action = Some(PageAction::CreateTheme);
            }
            if crate::icons::text_button(ui, "open", t!("settings-theme-open-folder")).clicked() {
                action = Some(PageAction::OpenThemesFolder);
            }
        });
        ui.add_space(4.0);
    }

    if row(ui, pal, search, &t!("settings-zoom"), a.zoom != d.zoom, |ui| {
        let mut percent = (a.zoom * 100.0).round();
        if ui.add(egui::Slider::new(&mut percent, 50.0..=300.0).step_by(10.0).suffix(" %")).changed() {
            a.zoom = percent / 100.0;
        }
    }) {
        a.zoom = d.zoom;
    }

    ui.add_space(8.0);
    sw::group(ui, pal, &t!("settings-background"));
    let mut choose = false;
    if row(ui, pal, search, &t!("settings-background-image"), a.background.is_some(), |ui| {
        let name = a.background.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| t!("settings-background-from-theme"));
        ui.label(RichText::new(name).color(pal.text));
        choose = ui.button(t!("settings-background-choose")).clicked();
    }) {
        a.background = None;
    }
    if choose {
        let file = rfd::FileDialog::new().set_title(t!("settings-background-choose")).add_filter("image", &["png", "jpg", "jpeg", "webp", "bmp", "gif"]).pick_file();
        if file.is_some() {
            a.background = file;
        }
    }
    if row(ui, pal, search, &t!("settings-background-opacity"), a.background_opacity != d.background_opacity, |ui| {
        let mut percent = (a.background_opacity * 100.0).round();
        if ui.add(egui::Slider::new(&mut percent, 0.0..=100.0).step_by(1.0).suffix(" %")).changed() {
            a.background_opacity = percent / 100.0;
        }
    }) {
        a.background_opacity = d.background_opacity;
    }
    let fits: Vec<(Fit, String)> = Fit::ALL.iter().map(|f| (*f, fit_label(*f))).collect();
    let mut pick = None;
    if row(ui, pal, search, &t!("settings-background-fit"), a.background_fit != d.background_fit, |ui| {
        pick = widgets::dropdown(ui, pal, sw::id(PAGE, "fit"), 220.0, &a.background_fit, &fits);
    }) {
        a.background_fit = d.background_fit;
    }
    if let Some(f) = pick {
        a.background_fit = f;
    }
    action
}

pub fn fit_label(f: Fit) -> String {
    match f {
        Fit::Cover => t!("fit-cover"),
        Fit::Contain => t!("fit-contain"),
        Fit::Stretch => t!("fit-stretch"),
        Fit::Tile => t!("fit-tile"),
        Fit::Center => t!("fit-center"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_files_migrate() {
        // Акцент из настроек первых версий больше не читается: это цвет темы.
        let a: Appearance = serde_json::from_str(r#"{"language":"ru","theme":"Light","accent":"Amber","font_size":16.0,"zoom":1.0}"#).unwrap();
        assert_eq!(a.theme, "mycelium-light");
        assert_eq!(a.legacy_font_size, Some(16.0));
        let back = serde_json::to_value(&a).unwrap();
        assert!(back.get("font_size").is_none(), "старое поле больше не пишется");
        assert!(back.get("accent").is_none());
        let fresh: Appearance = serde_json::from_str(r#"{"theme":"vs-dark"}"#).unwrap();
        assert_eq!(fresh.theme, "vs-dark");
    }
}
