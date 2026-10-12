//! Диалоги оболочки: окно настроек, «Найти действие», быстрая смена темы, экспорт и импорт
//! настроек, «О программе», название рабочего пространства.
//!
//! Окно настроек работает с черновиком: изменения видны в нём сразу, а к программе
//! применяются кнопками «Применить» и «ОК». «Отмена» и Escape их отбрасывают.

use std::path::Path;

use eframe::egui::{self, Align, Key, Layout, Margin, RichText, TextEdit, Ui, vec2};
use forma_actions::{Context as ActionContext, Keymap};
use forma_i18n::t;
use forma_plugins::PluginSettings;
use forma_settings::export::{self, Archive, Status};
use forma_settings::{Component, decode};
use forma_ui::appearance::{self, Appearance, PageContext, ThemeOption};
use forma_ui::find_action::{FindAction, Item};
use forma_ui::font_page::{self, FontPage};
use forma_ui::fonts::Fonts;
use forma_ui::keymap_page::{self, KeymapPage};
use forma_ui::plugins_page::{self, PluginsPage};
use forma_ui::settings_window::{self as sw, Footer, PageInfo, SettingsWindow};
use forma_ui::theme::{Origin, Palette};
use forma_ui::theme_tools;

use crate::actions;
use crate::chrome::{self, modal_max_height, modal_title, modal_width};
use crate::settings::{AppSettings, PlatformSettings};
use crate::shell::{Modal, Shell, ShellCmd, open_folder, reveal_in_explorer};
use crate::workspace::Builtin;

/// Пункты «Найти действие», которые открывают страницу настроек.
const SETTINGS_ITEM: &str = "settings:";

pub(crate) struct SettingsDialog<S> {
    window: SettingsWindow,
    draft: PlatformSettings,
    app: S,
    keymap: KeymapPage,
    fonts: FontPage,
    plugins: PluginsPage,
}

/// Компоненты платформы, которые можно перенести экспортом: id, название, что внутри.
fn platform_exportable() -> Vec<(&'static str, String, String)> {
    vec![
        (Appearance::ID, t!("settings-page-appearance"), t!("settings-component-appearance")),
        (Fonts::ID, t!("settings-page-fonts"), t!("settings-component-fonts")),
        (Keymap::ID, t!("settings-page-keymap"), t!("settings-component-keymap")),
        (PluginSettings::ID, t!("settings-page-plugins"), t!("settings-component-plugins")),
    ]
}

fn exportable<S: AppSettings>(app: &S) -> Vec<(&'static str, String, String)> {
    let mut all = platform_exportable();
    all.extend(app.exportable());
    all
}

pub(crate) struct ExportDialog {
    checked: Vec<(&'static str, bool)>,
}

impl ExportDialog {
    pub(crate) fn new<S: AppSettings>(app: &S) -> Self {
        Self { checked: exportable(app).into_iter().map(|(id, ..)| (id, true)).collect() }
    }
}

struct ImportItem {
    id: String,
    status: Status,
    checked: bool,
}

pub(crate) struct ImportDialog {
    archive: Archive,
    items: Vec<ImportItem>,
    /// Плагины из архива, которых здесь нет: после импорта их предлагается поставить.
    missing_plugins: Vec<String>,
    /// Итог после импорта: что применилось и что пропущено.
    report: Option<Vec<(String, Status)>>,
}

/// Команда для нажатого на странице «Плагины».
pub(crate) fn plugin_cmd<B>(action: plugins_page::Action) -> ShellCmd<B> {
    match action {
        plugins_page::Action::Install(id) => ShellCmd::InstallPlugin(id),
        plugins_page::Action::InstallFile => ShellCmd::InstallPluginFile,
        plugins_page::Action::Uninstall(id) => ShellCmd::UninstallPlugin(id),
        plugins_page::Action::OpenFolder => ShellCmd::RevealPlugins,
    }
}

impl<S: AppSettings, B: Builtin> Shell<S, B> {
    /// Открывает окно настроек на странице `page` (или на последней открытой).
    pub fn open_settings(&mut self, page: Option<String>) {
        let page = page.unwrap_or_else(|| self.settings_page.clone());
        self.modal = Some(Modal::Settings(Box::new(SettingsDialog {
            window: SettingsWindow::new(page),
            draft: self.settings.clone(),
            app: self.app_settings.clone(),
            keymap: KeymapPage::default(),
            fonts: FontPage::default(),
            plugins: PluginsPage::default(),
        })));
    }

    /// Что тема может поменять в программе: ключи платформы и программы, классы подсветки.
    pub fn theme_kit(&self) -> theme_tools::Kit {
        let mut groups = vec![(t!("settings-section-platform"), forma_ui::theme::Palette::TOKENS)];
        if !self.spec.colors.is_empty() {
            groups.push((self.spec.app.title.to_owned(), self.spec.colors));
        }
        theme_tools::Kit { groups, syntax: crate::highlight::CLASS_SELECTORS.iter().map(|(class, _)| *class).collect() }
    }

    fn settings_pages(&self) -> Vec<PageInfo> {
        let platform = t!("settings-section-platform");
        let mut pages = vec![
            PageInfo { id: appearance::PAGE.into(), section: platform.clone(), title: t!("settings-page-appearance"), keywords: appearance::keywords() },
            PageInfo { id: theme_tools::INSPECTOR_PAGE.into(), section: platform.clone(), title: t!("settings-page-theme-tokens"), keywords: theme_tools::keywords(&self.theme_kit()) },
            PageInfo { id: font_page::PAGE.into(), section: platform.clone(), title: t!("settings-page-fonts"), keywords: font_page::keywords() },
            PageInfo { id: keymap_page::PAGE.into(), section: platform.clone(), title: t!("settings-page-keymap"), keywords: keymap_page::keywords(&self.actions) },
            PageInfo { id: plugins_page::PAGE.into(), section: platform, title: t!("settings-page-plugins"), keywords: plugins_page::keywords() },
        ];
        pages.extend(self.app_settings.pages());
        pages
    }

    /// Применяет черновик настроек к программе и сохраняет его.
    fn apply_settings(&mut self, d: &SettingsDialog<S>) {
        let language_changed = d.draft.appearance.effective_language() != self.settings.appearance.effective_language();
        self.settings.appearance = d.draft.appearance.clone();
        self.settings.fonts = d.draft.fonts.clone();
        self.settings.keymap = d.draft.keymap.clone();
        // Список установленных плагинов ведёт сама программа — из черновика берём только выключенные.
        self.settings.plugins.disabled = d.draft.plugins.disabled.clone();
        self.app_settings = d.app.clone();
        if language_changed {
            forma_i18n::set_language(&self.settings.appearance.effective_language());
        }
        self.ctx.set_zoom_factor(self.settings.appearance.zoom);
        self.settings.save(&mut self.store);
        self.app_settings.save(&mut self.store);
    }

    fn draft_dirty(&self, d: &SettingsDialog<S>) -> bool {
        d.draft.appearance != self.settings.appearance
            || d.draft.fonts != self.settings.fonts
            || d.draft.keymap != self.settings.keymap
            || d.draft.plugins.disabled != self.settings.plugins.disabled
            || d.app != self.app_settings
    }

    /// Окно настроек. Возвращает `true`, когда его пора закрыть.
    fn settings_ui(&mut self, ui: &mut Ui, pal: &Palette, d: &mut SettingsDialog<S>) -> bool {
        let pages = self.settings_pages();
        let live = &self.settings;
        let live_app = &self.app_settings;
        let modified = |id: &str| match id {
            appearance::PAGE => d.draft.appearance != live.appearance,
            font_page::PAGE => d.draft.fonts != live.fonts,
            keymap_page::PAGE => d.draft.keymap != live.keymap,
            plugins_page::PAGE => d.draft.plugins.disabled != live.plugins.disabled,
            other => d.app.page_modified(live_app, other),
        };
        // Флаги страниц считаются до отрисовки: черновик меняется внутри страниц.
        let flags: Vec<(String, bool)> = pages.iter().map(|p| (p.id.clone(), modified(&p.id))).collect();
        let dirty = flags.iter().any(|(_, m)| *m);
        let modified = |id: &str| flags.iter().any(|(p, m)| p == id && *m);
        let max = ui.ctx().content_rect().size() - vec2(48.0, 48.0);
        let themes: Vec<ThemeOption> = self.themes.list().iter().map(|t| ThemeOption { id: t.id.clone(), name: t.name.clone(), builtin: t.origin == Origin::Builtin }).collect();
        let mut theme_errors = self.theme.errors.clone();
        theme_errors.extend(self.themes.broken.iter().map(|(path, e)| format!("{}: {e}", path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())));
        let cx = PageContext { themes_dir: self.themes.dir(), themes: &themes, default_theme: self.spec.default_theme, theme_errors: &theme_errors };
        // Тема, выбранная в черновике: от неё зависят шрифты, инспектор и новая тема.
        let draft_id = if d.draft.appearance.theme.is_empty() { self.spec.default_theme.to_owned() } else { d.draft.appearance.theme.clone() };
        let draft_theme = self.themes.resolve(&draft_id, self.spec.default_theme);
        let draft_theme_fonts = draft_theme.fonts.clone();
        let SettingsDialog { window, draft, app, keymap, fonts, plugins: plugins_ui } = d;
        let actions = &self.actions;
        let plugins = &self.plugins;
        let catalog = self.spec.catalog();
        let mut plugin_actions = Vec::new();
        let mut appearance_action = None;
        let kit = self.theme_kit();
        let footer = window.show(ui, pal, max, &pages, &modified, dirty, &mut |ui, page, search| match page {
            appearance::PAGE => appearance_action = appearance::page(ui, pal, search, &mut draft.appearance, &cx).or(appearance_action),
            theme_tools::INSPECTOR_PAGE => theme_tools::inspector(ui, pal, search, &draft_theme, &kit),
            font_page::PAGE => fonts.ui(ui, pal, search, &mut draft.fonts, &draft_theme_fonts),
            keymap_page::PAGE => keymap.ui(ui, pal, actions, &mut draft.keymap, search),
            plugins_page::PAGE => plugins_ui.ui(ui, pal, search, plugins, &catalog, &mut draft.plugins, &mut plugin_actions),
            other => app.page_ui(ui, pal, other, search),
        });
        for action in plugin_actions {
            self.cmds.push(plugin_cmd(action));
        }
        match appearance_action {
            Some(appearance::PageAction::CreateTheme) => match self.create_theme(&draft_theme, &kit) {
                Ok((id, path)) => {
                    draft.appearance.theme = id;
                    reveal_in_explorer(&path);
                    self.toast(t!("toast-theme-created", path = path.display().to_string()), false);
                }
                Err(e) => self.toast(t!("toast-theme-create-failed", error = e), true),
            },
            Some(appearance::PageAction::OpenThemesFolder) => {
                if let Some(dir) = self.themes.dir() {
                    let _ = std::fs::create_dir_all(dir);
                    open_folder(dir);
                }
            }
            None => {}
        }
        self.settings_page = d.window.page.clone();
        match footer {
            Some(Footer::Ok) => {
                self.apply_settings(d);
                true
            }
            Some(Footer::Apply) => {
                self.apply_settings(d);
                false
            }
            Some(Footer::Cancel) => true,
            Some(f @ (Footer::Export | Footer::Import)) => {
                // Сначала применяем черновик: экспорт должен взять то, что видно в окне.
                if self.draft_dirty(d) {
                    self.apply_settings(d);
                }
                self.cmds.push(if f == Footer::Export { ShellCmd::ExportSettings } else { ShellCmd::ImportSettings });
                true
            }
            None => false,
        }
    }

    /// Создаёт свою тему из шаблона на основе `base`: «Моя тема», «Моя тема 2»…
    /// Возвращает id и путь к файлу.
    fn create_theme(&mut self, base: &forma_ui::theme::Theme, kit: &theme_tools::Kit) -> Result<(String, std::path::PathBuf), String> {
        let dir = self.themes.dir().ok_or("no themes folder")?.to_path_buf();
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let base_name = t!("theme-new-name");
        let (name, id, path) = (1..)
            .map(|n| if n == 1 { base_name.clone() } else { format!("{base_name} {n}") })
            .map(|name| {
                let id = forma_ui::theme::slug(&name);
                let path = dir.join(format!("{id}.toml"));
                (name, id, path)
            })
            .find(|(_, id, path)| !path.exists() && self.themes.get(id).is_none())
            .ok_or("no free name")?;
        std::fs::write(&path, theme_tools::template(&name, &id, base, kit)).map_err(|e| format!("{}: {e}", path.display()))?;
        self.themes.scan();
        Ok((id, path))
    }

    /// Пункты «Найти действие»: общие действия и страницы настроек.
    fn find_items(&self) -> Vec<Item> {
        let mut items: Vec<Item> = self
            .actions
            .iter()
            .filter(|a| a.context != ActionContext::Editor && a.id != actions::FIND_ACTION)
            .map(|a| Item { id: a.id.clone(), title: keymap_page::title(a), group: forma_i18n::tr(&a.group), shortcut: self.keys(&a.id) })
            .collect();
        let settings = t!("find-action-settings");
        items.extend(self.settings_pages().into_iter().map(|p| Item { id: format!("{SETTINGS_ITEM}{}", p.id), title: p.title, group: settings.clone(), shortcut: String::new() }));
        items
    }

    fn find_action_ui(&mut self, ui: &mut Ui, pal: &Palette, f: &mut FindAction) -> bool {
        ui.set_width(modal_width(ui.ctx(), 560.0));
        let items = self.find_items();
        let Some(id) = f.ui(ui, pal, &items) else { return false };
        match id.strip_prefix(SETTINGS_ITEM) {
            Some(page) => self.cmds.push(ShellCmd::OpenSettingsPage(page.to_owned())),
            None => self.cmds.push(ShellCmd::Action(id)),
        }
        true
    }

    fn export_ui(&mut self, ui: &mut Ui, pal: &Palette, e: &mut ExportDialog) -> bool {
        ui.set_width(modal_width(ui.ctx(), 460.0));
        modal_title(ui, pal, &t!("settings-export-title"));
        ui.add(egui::Label::new(RichText::new(t!("settings-export-hint", app = self.spec.app.title)).size(12.5).color(pal.text_muted)).wrap());
        ui.add_space(10.0);
        let infos = exportable(&self.app_settings);
        for (id, checked) in &mut e.checked {
            let Some((_, name, about)) = infos.iter().find(|(i, ..)| i == id) else { continue };
            ui.checkbox(checked, RichText::new(name).color(pal.text));
            ui.horizontal(|ui| {
                ui.add_space(26.0);
                ui.label(RichText::new(about).size(11.5).color(pal.text_faint));
            });
            ui.add_space(2.0);
        }
        ui.add_space(14.0);
        let mut close = false;
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let any = e.checked.iter().any(|(_, c)| *c);
            if ui.add_enabled_ui(any, |ui| sw::primary_button(ui, pal, &t!("settings-export-button"))).inner.clicked() {
                close = self.export_settings(e);
            }
            if ui.add(egui::Button::new(t!("settings-cancel")).min_size(vec2(84.0, 30.0))).clicked() {
                close = true;
            }
        });
        close
    }

    /// Спрашивает, куда сохранить архив, и пишет его. `true` — диалог можно закрыть.
    fn export_settings(&mut self, e: &ExportDialog) -> bool {
        let mut archive = Archive::new(&self.spec.app);
        for (id, on) in &e.checked {
            if !*on {
                continue;
            }
            match *id {
                Appearance::ID => archive.add(&self.settings.appearance),
                Fonts::ID => archive.add(&self.settings.fonts),
                Keymap::ID => archive.add(&self.settings.keymap),
                PluginSettings::ID => archive.add(&self.settings.plugins),
                other => self.app_settings.export(other, &mut archive),
            }
        }
        let name = export::default_file_name(&self.spec.app, archive.manifest.created_unix);
        let Some(path) = rfd::FileDialog::new().set_title(t!("dialog-export-settings")).set_file_name(name).add_filter("zip", &["zip"]).save_file() else {
            return false;
        };
        match archive.write(&path) {
            Ok(()) => self.toast(t!("toast-settings-exported", path = path.display().to_string()), false),
            Err(err) => self.toast(t!("toast-settings-export-failed", error = err.to_string()), true),
        }
        true
    }

    /// Спрашивает архив и показывает, что из него применится.
    pub(crate) fn import_settings(&mut self) {
        let Some(path) = rfd::FileDialog::new().set_title(t!("dialog-import-settings")).add_filter("zip", &["zip"]).pick_file() else { return };
        match self.read_import(&path) {
            Ok(dialog) => self.modal = Some(Modal::Import(Box::new(dialog))),
            Err(err) => self.toast(t!("toast-settings-import-failed", error = err), true),
        }
    }

    fn read_import(&self, path: &Path) -> Result<ImportDialog, String> {
        let archive = Archive::read(path).map_err(|e| e.to_string())?;
        let mut items: Vec<ImportItem> = archive
            .manifest
            .components
            .iter()
            .map(|id| {
                let status = match archive.components.get(id) {
                    Some(v) => self.import_status(id, v),
                    None => Status::Invalid(t!("settings-import-missing")),
                };
                ImportItem { id: id.clone(), checked: status.applicable(), status }
            })
            .collect();
        // Сначала то, что применится.
        items.sort_by_key(|i| !i.status.applicable());
        // Какие плагины стояли там, где делали архив, и не стоят здесь.
        let missing_plugins = archive
            .components
            .get(PluginSettings::ID)
            .and_then(|v| decode::<PluginSettings>(v).ok())
            .map(|(p, _)| p.installed.into_iter().filter(|id| self.plugins.get(id).is_none()).collect())
            .unwrap_or_default();
        Ok(ImportDialog { archive, items, missing_plugins, report: None })
    }

    /// Что будет с компонентом архива. Раскладка клавиш сверяется со списком действий:
    /// сочетания для действий, которых здесь нет, пропускаются.
    fn import_status(&self, id: &str, value: &serde_json::Value) -> Status {
        let partial = |skipped: Vec<String>| if skipped.is_empty() { Status::Apply } else { Status::Partial(skipped) };
        match id {
            Appearance::ID => decode::<Appearance>(value).map_or_else(Status::Invalid, |(_, s)| partial(s)),
            Fonts::ID => decode::<Fonts>(value).map_or_else(Status::Invalid, |(_, s)| partial(s)),
            PluginSettings::ID => decode::<PluginSettings>(value).map_or_else(Status::Invalid, |(_, s)| partial(s)),
            Keymap::ID => match decode::<Keymap>(value) {
                Ok((mut k, mut skipped)) => {
                    skipped.extend(k.retain_known(&self.actions));
                    partial(skipped)
                }
                Err(e) => Status::Invalid(e),
            },
            other => self.app_settings.import_status(other, value).unwrap_or(Status::Unsupported),
        }
    }

    fn import_apply(&mut self, id: &str, value: &serde_json::Value) {
        match id {
            Appearance::ID => {
                if let Ok((a, _)) = decode::<Appearance>(value) {
                    let language_changed = a.effective_language() != self.settings.appearance.effective_language();
                    self.settings.appearance = a;
                    if language_changed {
                        forma_i18n::set_language(&self.settings.appearance.effective_language());
                    }
                    self.ctx.set_zoom_factor(self.settings.appearance.zoom);
                }
            }
            PluginSettings::ID => {
                // Переносится, что выключено; недостающие плагины предлагаются к установке.
                if let Ok((p, _)) = decode::<PluginSettings>(value) {
                    self.settings.plugins.disabled = p.disabled;
                }
            }
            Fonts::ID => {
                if let Ok((f, _)) = decode::<Fonts>(value) {
                    self.settings.fonts = f;
                }
            }
            Keymap::ID => {
                if let Ok((mut k, _)) = decode::<Keymap>(value) {
                    k.retain_known(&self.actions);
                    self.settings.keymap = k;
                }
            }
            other => self.app_settings.import_apply(other, value),
        }
    }

    fn component_title(&self, id: &str) -> String {
        exportable(&self.app_settings).into_iter().find(|(i, ..)| *i == id).map(|(_, title, _)| title).unwrap_or_else(|| id.to_owned())
    }

    fn import_ui(&mut self, ui: &mut Ui, pal: &Palette, d: &mut ImportDialog) -> bool {
        ui.set_width(modal_width(ui.ctx(), 500.0));
        if let Some(report) = &d.report {
            modal_title(ui, pal, &t!("settings-import-done-title"));
            for (id, status) in report {
                status_row(ui, pal, &self.component_title(id), status, None, true);
            }
            if !d.missing_plugins.is_empty() {
                ui.add_space(8.0);
                let catalog = self.spec.catalog();
                let names: Vec<String> = d.missing_plugins.iter().map(|id| catalog.iter().find(|e| e.id == id).map(|e| e.name.clone()).unwrap_or_else(|| id.clone())).collect();
                forma_ui::widgets::icon_label(ui, "plugin", t!("settings-import-plugins-missing", items = names.join(", ")), 12.5, pal.text_muted);
            }
            ui.add_space(14.0);
            let mut close = false;
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                close = sw::primary_button(ui, pal, &t!("action-done")).clicked();
                if !d.missing_plugins.is_empty() && forma_ui::icons::text_button(ui, "plugin", t!("settings-import-open-plugins")).clicked() {
                    self.cmds.push(ShellCmd::OpenSettingsPage(plugins_page::PAGE.into()));
                    close = true;
                }
            });
            return close;
        }
        modal_title(ui, pal, &t!("settings-import-title"));
        let m = &d.archive.manifest;
        let date = export::date_text(m.created_unix);
        ui.label(RichText::new(t!("settings-import-from", app = m.app_title.clone(), version = m.app_version.clone(), date = date)).size(12.5).color(pal.text_muted));
        ui.add_space(10.0);
        for item in &mut d.items {
            let applicable = item.status.applicable();
            let check = applicable.then_some(&mut item.checked);
            status_row(ui, pal, &self.component_title(&item.id), &item.status, check, false);
        }
        ui.add_space(14.0);
        let mut close = false;
        let mut import = false;
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let any = d.items.iter().any(|i| i.checked && i.status.applicable());
            import = ui.add_enabled_ui(any, |ui| sw::primary_button(ui, pal, &t!("settings-import-button"))).inner.clicked();
            if ui.add(egui::Button::new(t!("settings-cancel")).min_size(vec2(84.0, 30.0))).clicked() {
                close = true;
            }
        });
        if import {
            let mut report = Vec::new();
            for item in &d.items {
                if item.checked && item.status.applicable() {
                    if let Some(v) = d.archive.components.get(&item.id) {
                        self.import_apply(&item.id, v);
                    }
                    report.push((item.id.clone(), item.status.clone()));
                } else if !item.status.applicable() {
                    report.push((item.id.clone(), item.status.clone()));
                }
            }
            self.settings.save(&mut self.store);
            self.app_settings.save(&mut self.store);
            d.report = Some(report);
        }
        close
    }

    /// Диалоги оболочки.
    pub(crate) fn shell_modals(&mut self, ctx: &egui::Context) {
        // Диалог вынимаем на время отрисовки: внутри него меняются настройки программы.
        let Some(mut modal) = self.modal.take() else { return };
        let pal = self.pal.clone();
        let focus = std::mem::take(&mut self.modal_focus);
        let max_h = modal_max_height(ctx);
        // Окно настроек и импорт не закрываются щелчком мимо: там легко потерять изменения.
        let sticky = matches!(modal, Modal::Settings(_) | Modal::Import(_));
        let margin = if matches!(modal, Modal::Settings(_)) { 0 } else { 22 };
        let mut rename_done: Option<(usize, String)> = None;
        let mut theme = self.settings.appearance.theme.clone();
        let (close, outside) = chrome::modal(ctx, &pal, sticky, margin, |ui| match &mut modal {
            Modal::Settings(dialog) => self.settings_ui(ui, &pal, dialog),
            Modal::FindAction(find) => self.find_action_ui(ui, &pal, find),
            Modal::ThemeSwitcher(switcher) => switcher.ui(ui, &pal, &mut theme),
            Modal::Export(export) => egui::ScrollArea::vertical().max_height(max_h).show(ui, |ui| self.export_ui(ui, &pal, export)).inner,
            Modal::Import(import) => egui::ScrollArea::vertical().max_height(max_h).show(ui, |ui| self.import_ui(ui, &pal, import)).inner,
            Modal::About => {
                ui.set_width(modal_width(ui.ctx(), 380.0));
                let logo = self.logo_texture(ui.ctx(), 104.0);
                let mut close = false;
                egui::ScrollArea::vertical().max_height(max_h).show(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add(egui::Image::new(egui::load::SizedTexture::new(logo.id(), vec2(72.0, 72.0))));
                        ui.add_space(8.0);
                        ui.label(RichText::new(self.spec.app.title).size(20.0).family(forma_ui::fonts::bold()).color(pal.heading));
                        ui.label(RichText::new(t!("about-version", version = self.spec.app.version)).size(12.5).color(pal.text_faint));
                        ui.add_space(8.0);
                        forma_ui::widgets::centered_text(ui, &t!("app-description"), 13.5, pal.text_muted, 340.0);
                        ui.add_space(8.0);
                        ui.label(RichText::new(format!("{} · {}", t!("status-renderer"), self.gpu_info)).size(12.0).color(pal.text_faint));
                        ui.label(RichText::new(t!("about-forma")).size(12.0).color(pal.accent));
                        ui.add_space(12.0);
                        close = sw::primary_button(ui, &pal, &t!("action-close")).clicked();
                    });
                });
                close
            }
            Modal::Rename { index, name } => {
                ui.set_width(modal_width(ui.ctx(), 380.0));
                modal_title(ui, &pal, &t!("workspace-rename-title"));
                let r = ui.add(TextEdit::singleline(name).desired_width(f32::INFINITY).margin(Margin::symmetric(10, 7)));
                if focus {
                    r.request_focus();
                }
                let mut ok = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                let mut close = false;
                ui.add_space(16.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ok |= sw::primary_button(ui, &pal, &t!("action-rename")).clicked();
                    if ui.add(egui::Button::new(t!("action-cancel")).min_size(vec2(84.0, 30.0))).clicked() {
                        close = true;
                    }
                });
                if ok {
                    rename_done = Some((*index, name.trim().to_owned()));
                }
                close
            }
        });
        let mut close = close || outside;
        if self.settings.appearance.theme != theme {
            self.settings.appearance.theme = theme;
        }
        if let Modal::ThemeSwitcher(sw) = &modal
            && close
        {
            if sw.committed {
                self.settings.save(&mut self.store);
            } else {
                // Быструю смену темы закрыли без выбора — возвращаем прежнюю тему.
                self.settings.appearance.theme = sw.original().to_owned();
            }
        }
        if let Some((index, name)) = rename_done {
            if let Some(ws) = self.layout.workspaces.get_mut(index) {
                // Пустое имя возвращает название шаблона на текущем языке.
                ws.name = if name.is_empty() || ws.builtin.is_some_and(|b| b.title() == name) { None } else { Some(name) };
            }
            close = true;
        }
        // Пока шёл кадр, мог открыться другой диалог — он важнее.
        if !close && self.modal.is_none() {
            self.modal = Some(modal);
        }
    }
}

/// Строка компонента при импорте: галочка (если применимо), название и что с ним будет.
/// В отчёте после импорта (`done`) — что с ним стало.
fn status_row(ui: &mut Ui, pal: &Palette, name: &str, status: &Status, check: Option<&mut bool>, done: bool) {
    match check {
        Some(c) => {
            ui.checkbox(c, RichText::new(name).color(pal.text));
        }
        None => {
            ui.horizontal(|ui| {
                ui.add_space(26.0);
                ui.label(RichText::new(name).color(pal.text_muted));
            });
        }
    }
    let (icon, text, color) = match status {
        Status::Apply if done => ("success", t!("settings-import-applied"), pal.success),
        Status::Apply => ("success", t!("settings-import-apply"), pal.success),
        Status::Partial(skipped) if done => ("warning", t!("settings-import-applied-partial", items = skipped.join(", ")), pal.warning),
        Status::Partial(skipped) => ("warning", t!("settings-import-partial", items = skipped.join(", ")), pal.warning),
        Status::Unsupported => ("error", t!("settings-import-unsupported"), pal.text_faint),
        Status::Invalid(e) => ("error", t!("settings-import-invalid", error = e.clone()), pal.danger),
    };
    ui.horizontal(|ui| {
        ui.add_space(26.0);
        forma_ui::widgets::icon_label(ui, icon, text, 11.5, color);
    });
    ui.add_space(4.0);
}
