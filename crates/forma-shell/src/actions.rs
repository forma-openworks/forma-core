//! Общие действия программ Forma: настройки, масштаб, тема, окна, «Найти действие».
//! Свои действия программа регистрирует сама — в те же группы («Файл», «Вид»…) или в свои.

use forma_actions::{Action, Context, Registry};

pub const SETTINGS: &str = "file.settings";
pub const EXPORT_SETTINGS: &str = "file.exportSettings";
pub const IMPORT_SETTINGS: &str = "file.importSettings";
pub const QUIT: &str = "file.quit";

pub const ZOOM_IN: &str = "view.zoomIn";
pub const ZOOM_OUT: &str = "view.zoomOut";
pub const ZOOM_RESET: &str = "view.zoomReset";
pub const QUICK_THEME: &str = "view.quickSwitchTheme";

pub const MAXIMIZE: &str = "window.maximizeArea";
pub const DETACH: &str = "window.detachTab";
pub const RETURN_WINDOWS: &str = "window.returnAll";

pub const FIND_ACTION: &str = "help.findAction";
pub const ABOUT: &str = "help.about";

pub const GROUP_FILE: &str = "action-group-file";
pub const GROUP_VIEW: &str = "action-group-view";
pub const GROUP_WINDOW: &str = "action-group-window";
pub const GROUP_HELP: &str = "action-group-help";

/// Все действия: сначала программы (`app`), потом общие. Так в раскладке клавиш действия
/// программы стоят в начале своих групп, а общие — в конце.
pub fn registry(app: impl FnOnce(&mut Registry)) -> Registry {
    let mut r = Registry::default();
    app(&mut r);
    let mut add = |id, group, title, context, keys: &[&str]| r.register(Action::new(id, group, title, context, keys));
    use Context::Global;
    add(SETTINGS, GROUP_FILE, "action-settings", Global, &["Ctrl+Alt+S"]);
    add(EXPORT_SETTINGS, GROUP_FILE, "action-export-settings", Global, &[]);
    add(IMPORT_SETTINGS, GROUP_FILE, "action-import-settings", Global, &[]);
    add(QUIT, GROUP_FILE, "action-quit", Global, &[]);
    add(ZOOM_IN, GROUP_VIEW, "action-zoom-in", Global, &["Ctrl+=", "Ctrl++"]);
    add(ZOOM_OUT, GROUP_VIEW, "action-zoom-out", Global, &["Ctrl+-"]);
    add(ZOOM_RESET, GROUP_VIEW, "action-zoom-reset", Global, &["Ctrl+0"]);
    add(QUICK_THEME, GROUP_VIEW, "action-quick-theme", Global, &["Ctrl+`"]);
    add(MAXIMIZE, GROUP_WINDOW, "dock-maximize", Context::Layout, &["Ctrl+Shift+F12"]);
    add(DETACH, GROUP_WINDOW, "dock-detach", Global, &[]);
    add(RETURN_WINDOWS, GROUP_WINDOW, "action-return-windows", Global, &[]);
    add(FIND_ACTION, GROUP_HELP, "action-find-action", Global, &["Ctrl+Shift+A"]);
    add(ABOUT, GROUP_HELP, "action-about", Global, &[]);
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_shortcuts_do_not_clash() {
        let r = registry(|_| {});
        let keymap = forma_actions::Keymap::default();
        for a in r.iter() {
            for key in keymap.shortcuts(&r, &a.id) {
                assert!(keymap.conflicts(&r, *key, &a.id).is_empty(), "{key} у {} занято ещё кем-то", a.id);
            }
        }
    }

    #[test]
    fn every_title_is_translated() {
        forma_i18n::init_with(&[crate::LOCALES], None);
        let r = registry(|_| {});
        for lang in forma_i18n::builtin_languages() {
            for a in r.iter() {
                let forma_actions::Title::Key(key) = &a.title else { continue };
                assert!(forma_i18n::has_message(lang, key), "нет перевода {key} ({lang})");
                assert!(forma_i18n::has_message(lang, &a.group), "нет перевода {} ({lang})", a.group);
            }
        }
    }
}
