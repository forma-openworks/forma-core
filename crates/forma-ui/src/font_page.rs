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

/// Пределы высоты строки и её шаг.
pub const LINE_HEIGHT_RANGE: std::ops::RangeInclusive<f32> = 1.0..=2.2;

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

#[derive(Debug, Default)]
pub struct FontPage;

impl FontPage {
    pub fn ui(&mut self, ui: &mut Ui, pal: &Palette, search: &str, f: &mut Fonts, theme: &FontsSpec) {
        let d = Fonts::default();
        sw::section(ui, pal, &t!("fonts-ui"), |ui| slot_rows(ui, pal, search, PAGE, "ui", &mut f.ui, &d.ui, theme.ui.as_deref(), false, false));
        sw::section(ui, pal, &t!("fonts-editor"), |ui| slot_rows(ui, pal, search, PAGE, "editor", &mut f.editor, &d.editor, theme.editor.as_deref(), true, true));
        sw::section(ui, pal, &t!("fonts-console"), |ui| slot_rows(ui, pal, search, PAGE, "console", &mut f.console, &d.console, theme.console.as_deref(), true, true));
        sw::section(ui, pal, "", |ui| {
            let modified = f.ctrl_wheel != d.ctrl_wheel;
            if sw::check(ui, pal, search, &t!("fonts-ctrl-wheel"), &mut f.ctrl_wheel, modified) {
                f.ctrl_wheel = d.ctrl_wheel;
            }
        });
    }
}

/// Строки одного шрифта в карточке настроек: семейство, «свой шрифт вместо шрифта темы»
/// (если тема задаёт шрифт), размер и высота строки. `page` и `key` — для id элементов,
/// `mono` — в списке только моноширинные шрифты. Пригодится и программам: например,
/// шрифт консоли на странице «Запуск и терминал».
#[allow(clippy::too_many_arguments)]
pub fn slot_rows(ui: &mut Ui, pal: &Palette, search: &str, page: &str, key: &str, s: &mut FontSlot, d: &FontSlot, theme: Option<&str>, mono: bool, line_height: bool) {
    let lists = fonts::family_lists();
    if lists.is_none() {
        // Список шрифтов системы грузится в фоне — заглянем, когда будет готов.
        ui.ctx().request_repaint_after_secs(0.2);
    }
    let families = lists.as_ref().map(|(all, monos)| if mono { monos.as_slice() } else { all.as_slice() });
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
            pick = widgets::dropdown(ui, pal, sw::id(page, key), 300.0, &shown, &options);
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
        widgets::number_field(ui, pal, sw::id(page, &format!("{key}-size")), &mut s.size, SIZE_RANGE, 0.5, " px", 150.0);
    }) {
        s.size = d.size;
    }
    if line_height
        && row(ui, pal, search, &t!("fonts-line-height"), s.line_height != d.line_height, |ui| {
            widgets::number_field(ui, pal, sw::id(page, &format!("{key}-line-height")), &mut s.line_height, LINE_HEIGHT_RANGE, 0.05, "", 150.0);
        })
    {
        s.line_height = d.line_height;
    }
}
