//! Оболочка программ Forma — общая основа Mycelium, Holotype и следующих программ.
//!
//! Здесь живёт всё, что у программ Forma одинаково:
//! - запуск окна на Vulkan со своей шапкой вместо системной рамки ([`run`], [`caption`]);
//! - верхняя строка с меню и рабочими пространствами, строка состояния, экран приветствия,
//!   уведомления ([`chrome`]);
//! - рабочие пространства и раскладка областей с отдельными окнами ([`workspace`]);
//! - окно настроек с черновиком, «Найти действие», экспорт и импорт настроек, быстрая смена
//!   темы, «О программе» ([`Shell`]);
//! - фоновая картинка темы ([`look`]), подсветка кода цветами темы ([`highlight`]);
//! - общие действия и их сочетания ([`actions`]), общие плагины ([`plugins`]).
//!
//! Программа описывает себя в [`Spec`], свои настройки — через [`AppSettings`], вкладки и
//! встроенные рабочие пространства — через [`ShellTab`] и [`Builtin`].

pub mod actions;
pub mod caption;
pub mod chrome;
mod dialogs;
pub mod highlight;
pub mod look;
pub mod plugins;
pub mod settings;
mod shell;
pub mod workspace;

pub use settings::{AppSettings, PlatformSettings};
pub use shell::{Shell, ShellCmd, Spec, Toast, font_choice, open_folder, reveal_in_explorer};
pub use workspace::{Builtin, Ids, Layout, ShellTab, Workspace};

use eframe::{egui, egui_wgpu, wgpu};
use forma_actions::Registry;
use forma_i18n::t;
use forma_settings::Store;

// Строки оболочки: `pub static LOCALES`.
include!(concat!(env!("OUT_DIR"), "/locales.rs"));

/// Загружает переводы: платформы, оболочки, программы и пользовательские из папки настроек.
pub fn init_i18n(spec: &Spec) {
    forma_i18n::init_with(&[LOCALES, spec.locales], spec.user_locales_dir().as_deref());
}

/// Запускает программу: читает настройки, включает язык, открывает окно на Vulkan без
/// системной рамки и передаёт готовую оболочку в `make`, которая создаёт программу.
/// `actions` регистрирует действия программы (общие оболочка добавит сама).
pub fn run<S, B, A>(spec: &'static Spec, actions: impl FnOnce(&mut Registry), make: impl FnOnce(&eframe::CreationContext<'_>, Shell<S, B>) -> A + 'static)
where
    S: AppSettings,
    B: Builtin,
    A: eframe::App + 'static,
{
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();

    let mut store = Store::new(spec.config_dir());
    // Сначала настройки программы: при первом запуске она может перенести свои старые
    // настройки, в том числе в компоненты платформы (тему, масштаб).
    let app_settings = S::load(&mut store);
    let settings = PlatformSettings::load(&mut store);
    init_i18n(spec);
    forma_i18n::set_language(&settings.appearance.effective_language());
    let registry = actions::registry(actions);

    // Разрешаем только Vulkan: весь интерфейс рисуется через Vulkan-бэкенд wgpu.
    let mut setup = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    setup.instance_descriptor.backends = wgpu::Backends::VULKAN;
    setup.power_preference = wgpu::PowerPreference::HighPerformance;

    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: egui::ViewportBuilder::default()
            .with_title(spec.app.title)
            .with_app_id(spec.app_id)
            // Системной рамки нет: шапку с кнопками окна рисует сама программа.
            .with_decorations(false)
            .with_inner_size(spec.size)
            .with_min_inner_size(spec.min_size)
            .with_icon(spec.icon()),
        wgpu_options: egui_wgpu::WgpuConfiguration { wgpu_setup: setup.into(), ..Default::default() },
        // Размер и положение окна eframe хранит рядом с остальными настройками.
        persistence_path: spec.config_dir().map(|d| d.join("window")),
        ..Default::default()
    };

    let result = eframe::run_native(
        spec.app.title,
        options,
        Box::new(move |cc| {
            let shell = Shell::new(cc, spec, settings, app_settings, store, registry);
            Ok(Box::new(make(cc, shell)))
        }),
    );

    if let Err(err) = result {
        let text = t!("shell-error-vulkan", app = spec.app.title, details = err.to_string());
        log::error!("{text}");
        rfd::MessageDialog::new().set_title(spec.app.title).set_description(&text).set_level(rfd::MessageLevel::Error).show();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::path::Path;

    #[test]
    fn all_languages_have_the_same_keys() {
        let reference = forma_i18n::message_ids(&[LOCALES], forma_i18n::FALLBACK);
        assert!(!reference.is_empty());
        for (code, _) in LOCALES {
            let ids = forma_i18n::message_ids(&[LOCALES], code);
            let missing: Vec<_> = reference.iter().filter(|k| !ids.contains(k)).collect();
            let extra: Vec<_> = ids.iter().filter(|k| !reference.contains(k)).collect();
            assert!(missing.is_empty(), "в языке «{code}» не хватает строк: {missing:?}");
            assert!(extra.is_empty(), "в языке «{code}» лишние строки: {extra:?}");
        }
    }

    #[test]
    fn keys_do_not_repeat_platform_or_each_other() {
        for (code, _) in LOCALES {
            let ids = forma_i18n::message_ids(&[forma_i18n::PLATFORM, LOCALES], code);
            let mut seen = HashSet::new();
            let dups: Vec<_> = ids.iter().filter(|k| !seen.insert(k.as_str())).collect();
            assert!(dups.is_empty(), "в языке «{code}» повторяются ключи: {dups:?}");
        }
    }

    /// Каждый ключ макроса `t!` в оболочке переведён (в строках оболочки или платформы).
    /// Строки программы (`app-tagline`, `app-description`) приносит программа.
    #[test]
    fn every_used_key_exists() {
        let used = forma_i18n::used_keys(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"));
        assert!(!used.is_empty());
        let ids = forma_i18n::message_ids(&[forma_i18n::PLATFORM, LOCALES], forma_i18n::FALLBACK);
        let app_keys = ["app-tagline", "app-description"];
        let missing: Vec<_> = used.iter().filter(|(k, _)| !ids.contains(k) && !app_keys.contains(&k.as_str())).collect();
        assert!(missing.is_empty(), "ключи используются в коде, но не переведены: {missing:?}");
    }
}
