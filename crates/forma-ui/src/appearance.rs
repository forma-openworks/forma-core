//! Внешний вид — компонент настроек платформы и его страница: язык, тема, акцент,
//! размер текста, масштаб.

use std::path::Path;

use egui::{CursorIcon, Sense, Stroke, Ui, vec2};
use forma_i18n::t;
use serde::{Deserialize, Serialize};

use crate::settings_window::{self as sw, row};
use crate::theme::{Accent, Palette, ThemeKind};
use crate::widgets;

pub const PAGE: &str = "appearance";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Appearance {
    /// Язык интерфейса; `None` — как в системе.
    pub language: Option<String>,
    pub theme: ThemeKind,
    pub accent: Accent,
    /// Размер текста интерфейса в пикселях.
    pub font_size: f32,
    /// Масштаб всего интерфейса.
    pub zoom: f32,
}

impl Default for Appearance {
    fn default() -> Self {
        Self { language: None, theme: ThemeKind::Dark, accent: Accent::Violet, font_size: 16.0, zoom: 1.0 }
    }
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

/// Подписи настроек страницы — для поиска.
pub fn keywords() -> Vec<String> {
    vec![t!("settings-language"), t!("settings-theme"), t!("settings-accent"), t!("settings-font-size"), t!("settings-zoom")]
}

/// Страница «Внешний вид». `locales_dir` — где лежат пользовательские переводы.
pub fn page(ui: &mut Ui, pal: &Palette, search: &str, a: &mut Appearance, locales_dir: Option<&Path>) {
    let d = Appearance::default();

    sw::group(ui, pal, &t!("settings-group-language"));
    let languages = forma_i18n::languages();
    let mut labels = vec![t!("settings-language-system")];
    labels.extend(languages.iter().map(|(_, name)| name.clone()));
    let selected = match &a.language {
        None => 0,
        Some(code) => languages.iter().position(|(c, _)| c == code).map_or(0, |i| i + 1),
    };
    let mut pick = None;
    if row(ui, pal, search, &t!("settings-language"), a.language != d.language, |ui| {
        ui.allocate_ui(vec2(320.0, 28.0), |ui| {
            let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
            pick = widgets::segmented(ui, pal, sw::id(PAGE, "language"), &refs, selected, true);
        });
    }) {
        a.language = d.language.clone();
    }
    if let Some(i) = pick {
        a.language = if i == 0 { None } else { Some(languages[i - 1].0.clone()) };
    }
    if let Some(dir) = locales_dir {
        sw::hint(ui, pal, &t!("settings-locales-hint", path = dir.display().to_string()));
    }

    ui.add_space(8.0);
    sw::group(ui, pal, &t!("settings-group-look"));
    let mut pick = None;
    if row(ui, pal, search, &t!("settings-theme"), a.theme != d.theme, |ui| {
        ui.allocate_ui(vec2(220.0, 28.0), |ui| {
            let themes = [t!("theme-dark"), t!("theme-light")];
            let refs: Vec<&str> = themes.iter().map(String::as_str).collect();
            pick = widgets::segmented(ui, pal, sw::id(PAGE, "theme"), &refs, usize::from(a.theme == ThemeKind::Light), true);
        });
    }) {
        a.theme = d.theme;
    }
    if let Some(i) = pick {
        a.theme = if i == 0 { ThemeKind::Dark } else { ThemeKind::Light };
    }

    if row(ui, pal, search, &t!("settings-accent"), a.accent != d.accent, |ui| accent_picker(ui, pal, &mut a.accent)) {
        a.accent = d.accent;
    }

    if row(ui, pal, search, &t!("settings-font-size"), a.font_size != d.font_size, |ui| {
        ui.add(egui::Slider::new(&mut a.font_size, 13.0..=22.0).step_by(0.5).suffix(" px"));
    }) {
        a.font_size = d.font_size;
    }

    if row(ui, pal, search, &t!("settings-zoom"), a.zoom != d.zoom, |ui| {
        let mut percent = (a.zoom * 100.0).round();
        if ui.add(egui::Slider::new(&mut percent, 50.0..=300.0).step_by(10.0).suffix(" %")).changed() {
            a.zoom = percent / 100.0;
        }
    }) {
        a.zoom = d.zoom;
    }
}

/// Кружки акцентных цветов.
pub fn accent_picker(ui: &mut Ui, pal: &Palette, accent: &mut Accent) {
    for a in Accent::ALL {
        let (rect, resp) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::click());
        let hover = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.12);
        let c = a.color(pal.dark);
        ui.painter().circle_filled(rect.center(), 8.0 + hover * 1.5, c);
        if *accent == a {
            ui.painter().circle_stroke(rect.center(), 12.0, Stroke::new(2.0, c));
        }
        if resp.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(a.label()).clicked() {
            *accent = a;
        }
    }
}
