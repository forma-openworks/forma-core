//! Страница «Шрифты»: интерфейс, редактор кода и консоль — каждый со своим семейством,
//! размером и высотой строки. Если шрифт задаёт тема, поле показывает его и заблокировано,
//! пока не отмечено «Использовать свой шрифт».

use egui::{RichText, Ui};
use forma_i18n::t;

use crate::fonts::{self, FontSlot, Fonts, SIZE_RANGE};
use crate::settings_window::{self as sw, row};
use crate::theme::{FontsSpec, Palette};
use crate::widgets;

pub const PAGE: &str = "fonts";

/// Подписи настроек страницы — для поиска.
pub fn keywords() -> Vec<String> {
    vec![
        t!("fonts-ui"),
        t!("fonts-editor"),
        t!("fonts-console"),
        t!("fonts-family"),
        t!("fonts-size"),
        t!("fonts-line-height"),
        t!("fonts-ctrl-wheel"),
    ]
}

/// Списки семейств загружаются один раз за открытие окна.
#[derive(Debug, Default)]
pub struct FontPage {
    all: Option<Vec<String>>,
    mono: Option<Vec<String>>,
}

impl FontPage {
    pub fn ui(&mut self, ui: &mut Ui, pal: &Palette, search: &str, f: &mut Fonts, theme: &FontsSpec) {
        // Пока база шрифтов системы грузится в фоне, списки не строим, чтобы не ждать в кадре.
        if self.all.is_none() && fonts::database_ready() {
            self.all = Some(fonts::families(false));
            self.mono = Some(fonts::families(true));
        }
        if self.all.is_none() {
            fonts::warm_up();
            ui.ctx().request_repaint_after_secs(0.2);
        }
        let d = Fonts::default();
        slot(ui, pal, search, "ui", &t!("fonts-ui"), &mut f.ui, &d.ui, theme.ui.as_deref(), self.all.as_deref(), false);
        ui.add_space(8.0);
        slot(ui, pal, search, "editor", &t!("fonts-editor"), &mut f.editor, &d.editor, theme.editor.as_deref(), self.mono.as_deref(), true);
        ui.add_space(8.0);
        slot(ui, pal, search, "console", &t!("fonts-console"), &mut f.console, &d.console, theme.console.as_deref(), self.mono.as_deref(), true);
        ui.add_space(8.0);
        let modified = f.ctrl_wheel != d.ctrl_wheel;
        if sw::check(ui, pal, search, &t!("fonts-ctrl-wheel"), &mut f.ctrl_wheel, modified) {
            f.ctrl_wheel = d.ctrl_wheel;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn slot(ui: &mut Ui, pal: &Palette, search: &str, key: &str, title: &str, s: &mut FontSlot, d: &FontSlot, theme: Option<&str>, families: Option<&[String]>, line_height: bool) {
    sw::group(ui, pal, title);
    let locked = theme.is_some() && !s.custom;
    let shown = s.effective(theme).unwrap_or_default();
    let mut options: Vec<(String, String)> = vec![(String::new(), t!("fonts-default"))];
    match families {
        Some(list) => options.extend(list.iter().map(|f| (f.clone(), f.clone()))),
        None => {
            if !shown.is_empty() {
                options.push((shown.clone(), shown.clone()));
            }
        }
    }
    if !shown.is_empty() && !options.iter().any(|(v, _)| *v == shown) {
        options.push((shown.clone(), shown.clone()));
    }
    let mut pick = None;
    if row(ui, pal, search, &t!("fonts-family"), !locked && !s.family.is_empty(), |ui| {
        ui.add_enabled_ui(!locked, |ui| {
            pick = widgets::dropdown(ui, pal, sw::id(PAGE, key), 300.0, &shown, &options);
        });
        if families.is_none() {
            ui.label(RichText::new(t!("fonts-loading")).size(11.5).color(pal.text_faint));
        }
    }) {
        s.family.clear();
    }
    if let Some(f) = pick {
        s.family = f;
    }
    if let Some(name) = theme {
        let modified = s.custom;
        let label = t!("fonts-custom", font = name.to_owned());
        if sw::check(ui, pal, search, &label, &mut s.custom, modified) {
            s.custom = false;
        }
    }
    if row(ui, pal, search, &t!("fonts-size"), s.size != d.size, |ui| {
        ui.add(egui::Slider::new(&mut s.size, SIZE_RANGE).step_by(0.5).suffix(" px"));
    }) {
        s.size = d.size;
    }
    if line_height
        && row(ui, pal, search, &t!("fonts-line-height"), s.line_height != d.line_height, |ui| {
            ui.add(egui::Slider::new(&mut s.line_height, 1.0..=2.2).step_by(0.05));
        })
    {
        s.line_height = d.line_height;
    }
}
