//! Палитры, стиль egui и мелкие виджеты в фирменном стиле (меню, галочки).
//! Основа взята из Forma: Holotype, чтобы программы FORMA выглядели одной семьёй.

use std::sync::Arc;

use egui::{
    self, Color32, CornerRadius, FontFamily, FontId, Shadow, Stroke, TextStyle, Theme, Visuals, style::ScrollStyle,
};
use forma_i18n::t;
use forma_layout::DockStyle;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ThemeKind {
    Dark,
    Light,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Accent {
    Violet,
    Blue,
    Teal,
    Green,
    Amber,
    Rose,
}

impl Accent {
    pub const ALL: [Accent; 6] = [Accent::Violet, Accent::Blue, Accent::Teal, Accent::Green, Accent::Amber, Accent::Rose];

    pub fn label(self) -> String {
        match self {
            Accent::Violet => t!("accent-violet"),
            Accent::Blue => t!("accent-blue"),
            Accent::Teal => t!("accent-teal"),
            Accent::Green => t!("accent-green"),
            Accent::Amber => t!("accent-amber"),
            Accent::Rose => t!("accent-rose"),
        }
    }

    pub fn color(self, dark: bool) -> Color32 {
        let (d, l) = match self {
            Accent::Violet => ((150, 132, 255), (108, 84, 230)),
            Accent::Blue => ((96, 165, 250), (37, 99, 235)),
            Accent::Teal => ((45, 212, 191), (13, 148, 136)),
            Accent::Green => ((74, 222, 128), (22, 163, 74)),
            Accent::Amber => ((251, 191, 36), (202, 138, 4)),
            Accent::Rose => ((251, 113, 133), (225, 29, 72)),
        };
        let (r, g, b) = if dark { d } else { l };
        Color32::from_rgb(r, g, b)
    }
}

/// Все цвета приложения. Вычисляются из типа темы и акцентного цвета.
#[derive(Clone, Debug, PartialEq)]
pub struct Palette {
    pub dark: bool,
    /// Фон содержимого областей.
    pub bg: Color32,
    /// Верхняя строка меню и строка состояния.
    pub bg_panel: Color32,
    /// Строка вкладок внутри области.
    pub bg_header: Color32,
    /// Зазоры между областями — самый глубокий тон.
    pub bg_deep: Color32,
    /// Всплывающие окна и меню.
    pub bg_elevated: Color32,
    pub bg_hover: Color32,
    pub border: Color32,
    pub text: Color32,
    pub text_muted: Color32,
    pub text_faint: Color32,
    pub heading: Color32,
    pub accent: Color32,
    pub accent_soft: Color32,
    pub selection: Color32,
    pub success: Color32,
    pub warning: Color32,
    pub danger: Color32,
}

fn with_alpha(c: Color32, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}

impl Palette {
    pub fn new(kind: ThemeKind, accent: Accent) -> Self {
        let dark = kind == ThemeKind::Dark;
        let accent_c = accent.color(dark);
        if dark {
            Self {
                dark,
                bg: Color32::from_rgb(28, 30, 36),
                bg_panel: Color32::from_rgb(19, 20, 25),
                bg_header: Color32::from_rgb(23, 25, 31),
                bg_deep: Color32::from_rgb(13, 14, 18),
                bg_elevated: Color32::from_rgb(36, 39, 47),
                bg_hover: Color32::from_rgb(44, 48, 58),
                border: Color32::from_rgb(42, 45, 54),
                text: Color32::from_rgb(220, 223, 228),
                text_muted: Color32::from_rgb(160, 166, 178),
                text_faint: Color32::from_rgb(108, 115, 128),
                heading: Color32::from_rgb(244, 245, 248),
                accent: accent_c,
                accent_soft: with_alpha(accent_c, 48),
                selection: with_alpha(accent_c, 80),
                success: Color32::from_rgb(74, 222, 128),
                warning: Color32::from_rgb(251, 191, 36),
                danger: Color32::from_rgb(248, 113, 113),
            }
        } else {
            Self {
                dark,
                bg: Color32::from_rgb(255, 255, 255),
                bg_panel: Color32::from_rgb(246, 247, 249),
                bg_header: Color32::from_rgb(241, 242, 246),
                bg_deep: Color32::from_rgb(222, 225, 231),
                bg_elevated: Color32::from_rgb(255, 255, 255),
                bg_hover: Color32::from_rgb(232, 234, 240),
                border: Color32::from_rgb(214, 218, 226),
                text: Color32::from_rgb(40, 44, 52),
                text_muted: Color32::from_rgb(92, 99, 112),
                text_faint: Color32::from_rgb(150, 156, 168),
                heading: Color32::from_rgb(17, 19, 24),
                accent: accent_c,
                accent_soft: with_alpha(accent_c, 36),
                selection: with_alpha(accent_c, 60),
                success: Color32::from_rgb(22, 163, 74),
                warning: Color32::from_rgb(202, 138, 4),
                danger: Color32::from_rgb(220, 38, 38),
            }
        }
    }

    /// Оформление областей и вкладок для составного интерфейса.
    /// Иконку передаём готовой: рисовать её на каждом кадре дорого.
    pub fn dock_style(&self, app_title: &str, icon: Arc<egui::IconData>, maximize_keys: Vec<egui::KeyboardShortcut>, maximize_text: String) -> DockStyle {
        DockStyle {
            gap_bg: self.bg_deep,
            area_bg: self.bg,
            header_bg: self.bg_header,
            hover_bg: self.bg_hover,
            text: self.text,
            text_muted: self.text_muted,
            text_strong: self.heading,
            accent: self.accent,
            border: self.border,
            menu_style,
            icon: Some(icon),
            app_title: app_title.to_owned(),
            maximize_keys,
            maximize_text,
        }
    }
}

/// Устанавливает визуальный стиль для палитры в контекст egui.
pub fn apply(ctx: &egui::Context, pal: &Palette, font_size: f32) {
    let theme = if pal.dark { Theme::Dark } else { Theme::Light };
    ctx.set_theme(theme);

    let mut visuals = if pal.dark { Visuals::dark() } else { Visuals::light() };
    visuals.panel_fill = pal.bg_panel;
    visuals.window_fill = pal.bg_elevated;
    visuals.extreme_bg_color = pal.bg_deep;
    visuals.text_edit_bg_color = Some(pal.bg);
    visuals.faint_bg_color = if pal.dark { Color32::from_rgba_unmultiplied(255, 255, 255, 6) } else { Color32::from_rgba_unmultiplied(0, 0, 0, 8) };
    visuals.code_bg_color = pal.bg_hover;
    visuals.hyperlink_color = pal.accent;
    visuals.override_text_color = None;
    visuals.selection.bg_fill = pal.selection;
    visuals.selection.stroke = Stroke::new(1.0, pal.accent);
    visuals.window_corner_radius = CornerRadius::same(12);
    visuals.menu_corner_radius = CornerRadius::same(8);
    visuals.window_stroke = Stroke::new(1.0, pal.border);
    visuals.window_shadow = Shadow { offset: [0, 8], blur: 28, spread: 0, color: Color32::from_black_alpha(if pal.dark { 110 } else { 40 }) };
    visuals.popup_shadow = Shadow { offset: [0, 4], blur: 16, spread: 0, color: Color32::from_black_alpha(if pal.dark { 90 } else { 30 }) };
    visuals.text_cursor.stroke = Stroke::new(2.0, pal.accent);
    visuals.indent_has_left_vline = false;
    visuals.collapsing_header_frame = false;

    let radius = CornerRadius::same(6);
    let w = &mut visuals.widgets;
    w.noninteractive.bg_stroke = Stroke::new(1.0, pal.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0, pal.text);
    w.noninteractive.corner_radius = radius;
    w.inactive.weak_bg_fill = Color32::TRANSPARENT;
    w.inactive.bg_fill = pal.bg_hover;
    w.inactive.bg_stroke = Stroke::NONE;
    w.inactive.fg_stroke = Stroke::new(1.0, pal.text_muted);
    w.inactive.corner_radius = radius;
    w.hovered.weak_bg_fill = pal.bg_hover;
    w.hovered.bg_fill = pal.bg_hover;
    w.hovered.bg_stroke = Stroke::NONE;
    w.hovered.fg_stroke = Stroke::new(1.5, pal.text);
    w.hovered.corner_radius = radius;
    w.hovered.expansion = 0.0;
    w.active.weak_bg_fill = pal.accent_soft;
    w.active.bg_fill = pal.accent_soft;
    w.active.bg_stroke = Stroke::NONE;
    w.active.fg_stroke = Stroke::new(1.5, pal.heading);
    w.active.corner_radius = radius;
    w.active.expansion = 0.0;
    w.open.weak_bg_fill = pal.bg_hover;
    w.open.bg_fill = pal.bg_hover;
    w.open.bg_stroke = Stroke::NONE;
    w.open.fg_stroke = Stroke::new(1.0, pal.text);
    w.open.corner_radius = radius;

    ctx.set_visuals_of(theme, visuals);

    ctx.style_mut_of(theme, |style| {
        let s = font_size;
        style.text_styles = [
            (TextStyle::Small, FontId::new(s * 0.78, FontFamily::Proportional)),
            (TextStyle::Body, FontId::new(s * 0.9, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(s * 0.88, FontFamily::Proportional)),
            (TextStyle::Heading, FontId::new(s * 1.3, crate::fonts::semibold())),
            (TextStyle::Monospace, FontId::new(s * 0.85, FontFamily::Monospace)),
        ]
        .into();
        let sp = &mut style.spacing;
        sp.item_spacing = egui::vec2(8.0, 5.0);
        sp.button_padding = egui::vec2(8.0, 4.0);
        sp.menu_margin = egui::Margin::same(6);
        sp.interact_size.y = 24.0;
        sp.indent = 16.0;
        sp.scroll = ScrollStyle { bar_width: 8.0, floating_width: 3.0, ..ScrollStyle::floating() };
        style.interaction.selectable_labels = false;
        style.url_in_tooltip = false;
    });
}

/// Поля пунктов выпадающих меню по горизонтали.
pub const MENU_PAD_X: f32 = 10.0;

/// Стиль выпадающих меню: как у egui, но с нормальными полями у пунктов —
/// иначе текст прилипает к краю подсветки.
pub fn menu_style(style: &mut egui::Style) {
    egui::containers::menu::menu_style(style);
    style.spacing.button_padding = egui::vec2(MENU_PAD_X, 5.0);
    style.spacing.item_spacing.y = 2.0;
}

/// Кнопки верхней строки меню («Файл», «Вид» …).
pub fn menu_bar_style(style: &mut egui::Style) {
    egui::containers::menu::menu_style(style);
    style.spacing.button_padding = egui::vec2(9.0, 3.0);
}

pub fn menu_config() -> egui::containers::menu::MenuConfig {
    egui::containers::menu::MenuConfig::new().style(menu_style)
}

/// Контекстное меню по правому щелчку в нашем стиле (вложенные меню наследуют его).
pub fn context_menu(resp: &egui::Response, add: impl FnOnce(&mut egui::Ui)) {
    use egui::containers::menu::MenuConfig;
    egui::Popup::context_menu(resp)
        .style(menu_style)
        .info(egui::UiStackInfo::new(egui::UiKind::Menu).with_tag_value(MenuConfig::MENU_CONFIG_TAG, menu_config()))
        .show(add);
}
