//! Плагины в оболочке: каталог программы вместе с общими плагинами платформы, папки
//! установленных тем и плагин темы Visual Studio Dark, который программы Forma везут в себе.

use std::path::PathBuf;

use forma_i18n::t;
use forma_plugins::{CatalogEntry, Kind, PluginSettings, Plugins, Report, Source};

pub const VS_DARK: &str = "visual-studio-dark";

/// Плагины, общие для всех программ Forma.
pub fn shared() -> Vec<CatalogEntry> {
    vec![CatalogEntry {
        id: VS_DARK,
        name: t!("plugin-vs-dark-name"),
        description: t!("plugin-vs-dark-about"),
        kind: Kind::Themes,
        size: None,
        author: "FORMA",
        license: "MIT",
        homepage: "",
        icon: "palette",
        source: Source::Bundled { files: &[("themes/vs-dark.toml", include_str!("../plugins/visual-studio-dark/themes/vs-dark.toml"))] },
        // Тема — не первая необходимость: на экране приветствия её не предлагаем.
        recommended: false,
        validate: None,
        themes: Some("themes"),
    }]
}

/// Папка плагина, если он установлен и включён.
pub fn active_dir(plugins: &Plugins, settings: &PluginSettings, id: &str) -> Option<PathBuf> {
    plugins.get(id).filter(|_| settings.enabled(id)).map(|p| p.dir.clone())
}

/// Проверка плагинов программы `app`: что работает и чего не хватает остальным.
pub fn report(plugins: &Plugins, settings: &PluginSettings, app: &str) -> Report {
    forma_plugins::check(plugins.installed(), app, |id| settings.enabled(id))
}

/// Папки тем из работающих плагинов: включённых, подходящих программе и со всеми
/// зависимостями. У плагина, найденного дважды, не работает ни одна копия.
pub fn theme_dirs(plugins: &Plugins, report: &Report) -> Vec<PathBuf> {
    plugins.installed().iter().filter(|p| report.works(p.id())).filter_map(|p| p.themes_dir()).collect()
}
