//! Интерфейсная основа платформы Forma: тема и палитра, шрифты, фирменные виджеты.
//! Всё, что выглядит одинаково во всех программах Forma, живёт здесь.

/// egui для макроса `theme_tokens!` в других крейтах.
pub use egui;

pub mod appearance;
pub mod elements;
pub mod find_action;
pub mod font_page;
pub mod fonts;
pub mod icons;
pub mod keymap_page;
pub mod plugins_page;
pub mod selection;
pub mod settings_window;
pub mod theme;
pub mod theme_tools;
pub mod widgets;
