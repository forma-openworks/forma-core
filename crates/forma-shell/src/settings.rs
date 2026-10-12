//! Настройки программы: компоненты платформы (внешний вид, шрифты, раскладка клавиш,
//! плагины) и компоненты самой программы. Каждый компонент лежит в своём файле
//! `options/<id>.json` папки настроек.

use std::path::Path;

use eframe::egui::Ui;
use forma_actions::Keymap;
use forma_plugins::PluginSettings;
use forma_settings::Store;
use forma_settings::export::{Archive, Status};
use forma_ui::appearance::Appearance;
use forma_ui::fonts::Fonts;
use forma_ui::settings_window::PageInfo;
use forma_ui::theme::Palette;
use serde::{Deserialize, Serialize};

/// Настройки платформы: одинаковы у всех программ Forma и переносятся между ними экспортом.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlatformSettings {
    pub appearance: Appearance,
    pub fonts: Fonts,
    pub keymap: Keymap,
    pub plugins: PluginSettings,
}

impl PlatformSettings {
    pub fn load(store: &mut Store) -> Self {
        Self { appearance: store.load(), fonts: store.load(), keymap: store.load(), plugins: store.load() }
    }

    /// Записывает изменившиеся компоненты.
    pub fn save(&self, store: &mut Store) {
        store.save(&self.appearance);
        store.save(&self.fonts);
        store.save(&self.keymap);
        store.save(&self.plugins);
    }
}

/// Свои настройки программы: компоненты, страницы в окне настроек, экспорт и импорт.
///
/// Окно настроек работает с черновиком: страницы программы меняют копию, а к программе
/// она применяется кнопками «Применить» и «ОК».
pub trait AppSettings: Clone + PartialEq + 'static {
    /// Читает компоненты программы. Вызывается до чтения настроек платформы: при первом
    /// запуске программа может записать в `store` и компоненты платформы (перенос старых настроек).
    fn load(store: &mut Store) -> Self;
    /// Записывает изменившиеся компоненты.
    fn save(&self, store: &mut Store);

    /// Страницы программы в окне настроек (раздел — название программы).
    fn pages(&self) -> Vec<PageInfo> {
        Vec::new()
    }
    /// Есть ли на странице `page` изменения относительно настроек `live`.
    fn page_modified(&self, _live: &Self, _page: &str) -> bool {
        false
    }
    /// Рисует страницу программы `page`, меняя черновик `self`.
    fn page_ui(&mut self, _ui: &mut Ui, _pal: &Palette, _page: &str, _search: &str) {}

    /// Компоненты программы, которые переносятся экспортом: id, название, что внутри.
    fn exportable(&self) -> Vec<(&'static str, String, String)> {
        Vec::new()
    }
    /// Кладёт компонент `id` в архив экспорта.
    fn export(&self, _id: &str, _archive: &mut Archive) {}
    /// Что будет с компонентом `id` из архива; `None` — это не компонент программы.
    fn import_status(&self, _id: &str, _value: &serde_json::Value) -> Option<Status> {
        None
    }
    /// Применяет компонент `id` из архива.
    fn import_apply(&mut self, _id: &str, _value: &serde_json::Value) {}
}

/// Программа без своих настроек.
impl AppSettings for () {
    fn load(_store: &mut Store) -> Self {}
    fn save(&self, _store: &mut Store) {}
}

pub fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    let text = std::fs::read_to_string(path).ok()?;
    match serde_json::from_str(&text) {
        Ok(v) => Some(v),
        Err(err) => {
            log::warn!("Failed to parse {}: {err}", path.display());
            None
        }
    }
}

/// Пишет JSON через временный файл, чтобы при сбое не остался обрезанный файл.
pub fn write_json<T: Serialize>(path: &Path, value: &T) {
    let result = serde_json::to_string_pretty(value).map_err(std::io::Error::other).and_then(|json| forma_settings::write_atomic(path, json.as_bytes()));
    if let Err(err) = result {
        log::warn!("Failed to save {}: {err}", path.display());
    }
}
