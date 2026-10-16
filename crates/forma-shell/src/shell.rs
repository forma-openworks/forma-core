//! Оболочка программы Forma: всё, что одинаково у Mycelium, Holotype и следующих программ.
//!
//! [`Shell`] хранит настройки платформы и программы, темы, плагины, раскладку рабочих
//! пространств и диалоги платформы, а программа рисует внутри оболочки свои меню, панели
//! и вкладки. Кадр программы выглядит так:
//!
//! ```text
//! shell.begin_frame(ctx);           // тема, шрифты, плагины
//! shell.app_modal = свой диалог открыт;
//! shell.handle_keys(ctx);           // общие сочетания клавиш
//! shell.top_bar(…) / status_bar(…) / workspace::show(…)   — или welcome_ui(…)
//! shell.end_frame(ctx, |ctx, shell| свои диалоги);
//! for id in shell.process(ctx) { свои действия }
//! ```

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui;
use forma_actions::{Context as ActionContext, Registry, Shortcut};

/// Группа действий с командами плагинов (ключ перевода).
pub const PLUGIN_ACTIONS: &str = "action-group-plugins";
use forma_i18n::t;
use forma_layout::DockStyle;
use forma_plugins::{CatalogEntry, Manifest, Plugins};
use forma_settings::schema::Setting;
use forma_settings::user::{Poll, UserSettings};
use forma_settings::{AppInfo, Store};
use forma_ui::appearance::ZOOM_RANGE;
use forma_ui::find_action::FindAction;
use forma_ui::fonts::Choice;
use forma_ui::theme::{self, Background, Palette, Theme, Themes, TokenInfo};

use crate::actions;
use crate::dialogs::{ExportDialog, ImportDialog, SettingsDialog};
use crate::look::{Backdrop, ThemeSwitcher};
use crate::settings::{AppSettings, PlatformSettings};
use crate::workspace::{self, Builtin, Layout, Workspace};

/// Всё, что оболочке нужно знать о программе.
pub struct Spec {
    /// id, папка настроек, название и версия: `Forma Holotype`, `%APPDATA%\Forma\Holotype`.
    pub app: AppInfo,
    /// Название под «F O R M A» на экране приветствия: «Holotype».
    pub short_name: &'static str,
    /// id окна для системы (Wayland, X11): `forma-holotype`.
    pub app_id: &'static str,
    /// Тексты встроенных тем (TOML). Первая — запасная.
    pub themes: &'static [&'static str],
    /// Тема по умолчанию (когда в настройках пусто).
    pub default_theme: &'static str,
    /// Свои ключи темы программы — для инспектора темы и шаблона новой темы.
    pub colors: &'static [TokenInfo],
    /// Подробный логотип для больших размеров (экран приветствия, «О программе»): RGBA
    /// картинка `size`×`size` без предумножения альфы.
    pub logo: fn(u32) -> Vec<u8>,
    /// Упрощённый логотип для маленьких размеров (шапка окна, значок окна).
    pub mark: fn(u32) -> Vec<u8>,
    /// Встроенные переводы программы (`forma_build::locales`).
    pub locales: forma_i18n::Locales,
    /// Свои плагины программы; общие плагины платформы оболочка добавит сама.
    pub catalog: fn() -> Vec<CatalogEntry>,
    /// Размер окна при первом запуске и наименьший размер.
    pub size: [f32; 2],
    pub min_size: [f32; 2],
}

impl Spec {
    /// Папка настроек: `%APPDATA%\Forma\<программа>`.
    pub fn config_dir(&self) -> Option<PathBuf> {
        self.app.config_dir()
    }

    /// Кеши и плагины: `%LOCALAPPDATA%\Forma\<программа>`.
    pub fn cache_dir(&self) -> Option<PathBuf> {
        self.app.cache_dir()
    }

    /// Пользовательские переводы: `<настройки>\locales\<язык>\*.ftl`.
    pub fn user_locales_dir(&self) -> Option<PathBuf> {
        self.config_dir().map(|d| d.join("locales"))
    }

    /// Свои темы: `<настройки>\themes\*.toml`.
    pub fn user_themes_dir(&self) -> Option<PathBuf> {
        self.config_dir().map(|d| d.join("themes"))
    }

    /// Папка плагинов.
    pub fn plugins_dir(&self) -> Option<PathBuf> {
        self.cache_dir().map(|d| d.join("plugins"))
    }

    /// Как программа называется в манифестах плагинов (`apps = ["holotype"]`): id окна без
    /// `forma-`.
    pub fn plugin_app(&self) -> &'static str {
        self.app_id.strip_prefix("forma-").unwrap_or(self.app_id)
    }

    /// Настройки плагинов: `<настройки>\settings.json`, как у VS Code.
    pub fn user_settings_path(&self) -> Option<PathBuf> {
        self.config_dir().map(|d| d.join("settings.json"))
    }

    /// Раскладка, когда программа не указала свой файл раскладки.
    pub fn global_layout_path(&self) -> Option<PathBuf> {
        self.config_dir().map(|d| d.join("layout.json"))
    }

    /// Стандартная раскладка («Сохранить раскладку как стандартную»).
    pub fn default_layout_path(&self) -> Option<PathBuf> {
        self.config_dir().map(|d| d.join("default-layout.json"))
    }

    /// Иконка окна и панели задач. Там она маленькая — поэтому упрощённый рисунок.
    pub fn icon(&self) -> egui::IconData {
        let size = 256;
        egui::IconData { rgba: (self.mark)(size), width: size, height: size }
    }

    /// Каталог плагинов: свои плагины программы и общие плагины платформы.
    pub fn catalog(&self) -> Vec<CatalogEntry> {
        let mut all = (self.catalog)();
        let shared: Vec<CatalogEntry> = crate::plugins::shared().into_iter().filter(|s| !all.iter().any(|e| e.id == s.id)).collect();
        all.extend(shared);
        all
    }
}

/// Команды оболочки: копятся за кадр и выполняются в [`Shell::process`].
pub enum ShellCmd<B> {
    SwitchWorkspace(usize),
    NewWorkspace(Option<B>),
    DuplicateWorkspace(usize),
    RenameWorkspace(usize),
    ResetWorkspace(usize),
    DeleteWorkspace(usize),
    ResetAllWorkspaces,
    SaveDefaultLayout,
    ApplyDefaultLayout,
    ResetDefaultLayout,
    DetachFocused,
    ReturnAllWindows,
    ToggleMaximize,
    Settings,
    /// Открыть настройки на странице.
    OpenSettingsPage(String),
    ExportSettings,
    ImportSettings,
    FindAction,
    /// Выполнить действие по id (из «Найти действие» или отдельного окна).
    Action(String),
    ZoomIn,
    ZoomOut,
    ZoomReset,
    /// Быстрая смена темы с предпросмотром.
    QuickSwitchTheme,
    /// Ctrl+колёсико в редакторе или консоли: размер шрифта на `step` пикселей.
    FontZoom { console: bool, step: f32 },
    InstallPlugin(String),
    /// Поставить плагин из `.fpl` или `.zip` (спросить файл).
    InstallPluginFile,
    /// Показать, что приносит и что просит плагин из этого файла, и поставить с согласия.
    OfferPlugin(PathBuf),
    UninstallPlugin(String),
    /// Открыть папку плагинов.
    RevealPlugins,
    /// Подключить папку плагина в разработке (спросить папку).
    AddPluginFolder,
    RemovePluginFolder(PathBuf),
    /// Перечитать плагины.
    ReloadPlugins,
    /// Открыть `settings.json` (в программе, если она умеет, иначе в программе по умолчанию).
    OpenSettingsJson,
    About,
    Quit,
}

/// Команда общего действия. `None` — действие программы.
fn standard_cmd<B>(id: &str) -> Option<ShellCmd<B>> {
    use actions::*;
    Some(match id {
        SETTINGS => ShellCmd::Settings,
        EXPORT_SETTINGS => ShellCmd::ExportSettings,
        IMPORT_SETTINGS => ShellCmd::ImportSettings,
        QUIT => ShellCmd::Quit,
        ZOOM_IN => ShellCmd::ZoomIn,
        ZOOM_OUT => ShellCmd::ZoomOut,
        ZOOM_RESET => ShellCmd::ZoomReset,
        QUICK_THEME => ShellCmd::QuickSwitchTheme,
        MAXIMIZE => ShellCmd::ToggleMaximize,
        DETACH => ShellCmd::DetachFocused,
        RETURN_WINDOWS => ShellCmd::ReturnAllWindows,
        FIND_ACTION => ShellCmd::FindAction,
        ABOUT => ShellCmd::About,
        _ => return None,
    })
}

/// Диалоги оболочки.
pub(crate) enum Modal<S> {
    Settings(Box<SettingsDialog<S>>),
    FindAction(FindAction),
    ThemeSwitcher(ThemeSwitcher),
    Export(ExportDialog),
    Import(Box<ImportDialog>),
    About,
    Rename { index: usize, name: String },
    /// Поставить плагин из файла: что приносит и что просит.
    PluginConsent(Box<forma_plugins::Pending>),
}

/// Уведомление внизу окна.
pub struct Toast {
    pub text: String,
    pub error: bool,
    pub until: f64,
}

/// От чего зависит оформление: при изменении тема вычисляется заново.
#[derive(Clone, Debug, PartialEq)]
struct StyleKey {
    theme: String,
    themes_rev: u64,
    ui_size: u32,
    max_keys: Vec<Shortcut>,
    overrides: BTreeMap<String, String>,
}

/// Какие шрифты ставить: свои из настроек или шрифты темы.
pub fn font_choice(f: &forma_ui::fonts::Fonts, theme: &forma_ui::theme::FontsSpec) -> Choice {
    Choice { ui: f.ui.effective(theme.ui.as_deref()), editor: f.editor.effective(theme.editor.as_deref()), console: f.console.effective(theme.console.as_deref()) }
}

pub struct Shell<S: AppSettings, B: Builtin> {
    pub spec: &'static Spec,
    /// Контекст egui — фоновые задачи будят им интерфейс.
    pub ctx: egui::Context,
    /// Настройки платформы (внешний вид, шрифты, раскладка клавиш, плагины).
    pub settings: PlatformSettings,
    /// Свои настройки программы.
    pub app_settings: S,
    /// Файлы компонентов настроек (`options/*.json`).
    pub store: Store,
    /// Все действия программы — для раскладки клавиш, меню и «Найти действие».
    pub actions: Registry,
    /// Встроенные и свои темы.
    pub themes: Themes,
    /// Номер изменения своих тем: растёт, когда файл темы поменяли на диске.
    themes_rev: u64,
    /// Текущая тема и её цвета интерфейса.
    pub theme: Theme,
    pub pal: Palette,
    /// Номер темы: растёт при каждой смене темы. По нему программа пересчитывает свои цвета.
    pub theme_gen: u64,
    /// Свои значения ключей поверх темы (например, акцентный цвет из настроек программы).
    pub theme_overrides: BTreeMap<String, String>,
    style_key: Option<StyleKey>,
    fonts_key: Option<Choice>,
    backdrop: Backdrop,
    pub icon: Arc<egui::IconData>,
    pub dock_style: DockStyle,
    pub layout: Layout<B>,
    /// Файл текущей раскладки; `None` — общая раскладка в папке настроек.
    pub layout_file: Option<PathBuf>,
    /// Что программа ещё помнит об интерфейсе рядом с раскладкой.
    pub view: serde_json::Value,
    /// Установленные плагины и идущие установки.
    pub plugins: Plugins,
    /// Что из плагинов работает и чего не хватает остальным (проверка загрузчика).
    pub plugin_report: forma_plugins::Report,
    /// Code плагинов (песочница WebAssembly); `None` — песочница не запустилась.
    pub plugin_runtime: Option<forma_plugin_host::Runtime>,
    /// Команды плагинов, которые сейчас зарегистрированы как действия.
    plugin_commands: Vec<forma_plugin_host::CommandInfo>,
    /// `settings.json`: значения настроек плагинов.
    pub user: UserSettings,
    /// С какими значениями и плагинами code плагинов последний раз получил настройки.
    runtime_settings: Option<(u64, Vec<String>)>,
    /// Программа сама открывает текстовые файлы (Instar — в своём редакторе): тогда
    /// `settings.json` ждёт её в [`Shell::take_open_files`], а не открывается в системе.
    pub edit_in_app: bool,
    open_files: Vec<PathBuf>,
    /// Экран приветствия вместо рабочего интерфейса.
    pub welcome: bool,
    pub(crate) modal: Option<Modal<S>>,
    /// У программы открыт свой диалог (общие сочетания клавиш в это время не работают).
    pub app_modal: bool,
    pub(crate) modal_focus: bool,
    toast: Option<Toast>,
    pub cmds: Vec<ShellCmd<B>>,
    /// Сработавшие общие сочетания клавиш в отдельных окнах (см. [`Shell::fire_global`]).
    pub fired: Vec<String>,
    /// Действия программы, сработавшие за кадр, — отдаются из [`Shell::process`].
    app_actions: Vec<String>,
    pub gpu_info: String,
    window_title: String,
    pub(crate) logo: Option<(u32, egui::TextureHandle)>,
    pub(crate) mark: Option<(u32, egui::TextureHandle)>,
    /// Высота верхней строки на прошлом кадре — под неё подстраиваются кнопки окна.
    pub caption_h: f32,
    /// Последняя открытая страница настроек.
    pub(crate) settings_page: String,
}

impl<S: AppSettings, B: Builtin> Shell<S, B> {
    pub fn new(cc: &eframe::CreationContext<'_>, spec: &'static Spec, settings: PlatformSettings, app_settings: S, store: Store, actions: Registry) -> Self {
        let ctx = &cc.egui_ctx;
        // Системной рамки у окна нет — шапку рисует программа (см. `caption`).
        crate::caption::round_corners(cc);
        crate::highlight::warm_up();
        // Список шрифтов системы нужен странице «Шрифты» и темам со своими шрифтами.
        forma_ui::fonts::warm_up();
        // Масштаб и выход — действия из раскладки клавиш, а не встроенные сочетания egui:
        // иначе случайное Ctrl+Q молча закрывало бы программу.
        ctx.options_mut(|o| {
            o.zoom_with_keyboard = false;
            o.quit_shortcuts.clear();
        });
        ctx.set_zoom_factor(settings.appearance.zoom.clamp(*ZOOM_RANGE.start(), *ZOOM_RANGE.end()));
        let mut plugins = Plugins::new(spec.plugins_dir());
        plugins.set_dev_dirs(settings.plugins.dev.clone());
        let plugin_report = crate::plugins::report(&plugins, &settings.plugins, spec.plugin_app());
        let plugin_runtime = match forma_plugin_host::Runtime::new(spec.plugin_app(), spec.plugins_dir().map(|d| d.join(".data"))) {
            Ok(mut rt) => {
                rt.set_language(&settings.appearance.effective_language());
                rt.sync(plugins.installed(), &plugin_report, false);
                Some(rt)
            }
            Err(e) => {
                log::error!("Plugin sandbox failed to start: {e}");
                None
            }
        };
        let mut themes = Themes::new(spec.themes, spec.user_themes_dir());
        themes.set_plugin_dirs(crate::plugins::theme_dirs(&plugins, &plugin_report));
        let theme = themes.resolve(&settings.appearance.theme, spec.default_theme);
        let pal = theme.pal.clone();
        // Шрифты нужны уже в первом кадре: egui применяет новые шрифты только со следующего.
        let fonts_choice = font_choice(&settings.fonts, &theme.fonts);
        forma_ui::fonts::install(ctx, &fonts_choice);
        let icon = Arc::new(spec.icon());
        let dock_style = pal.dock_style(spec.app.title, icon.clone(), Vec::new(), String::new());
        let gpu_info = cc
            .wgpu_render_state
            .as_ref()
            .map(|rs| {
                let info = rs.adapter.get_info();
                format!("{:?} · {}", info.backend, info.name)
            })
            .unwrap_or_else(|| "Vulkan".into());
        let (layout, view) = spec.global_layout_path().and_then(|p| workspace::read_layout(&p)).unwrap_or_else(|| (default_layout(spec), serde_json::Value::Null));
        Self {
            spec,
            ctx: ctx.clone(),
            settings,
            app_settings,
            store,
            actions,
            themes,
            themes_rev: 0,
            theme,
            pal,
            theme_gen: 0,
            theme_overrides: BTreeMap::new(),
            style_key: None,
            fonts_key: Some(fonts_choice),
            backdrop: Backdrop::default(),
            icon,
            dock_style,
            layout,
            layout_file: None,
            view,
            plugins,
            plugin_report,
            plugin_runtime,
            plugin_commands: Vec::new(),
            user: UserSettings::open(spec.user_settings_path()),
            runtime_settings: None,
            edit_in_app: false,
            open_files: Vec::new(),
            welcome: true,
            modal: None,
            app_modal: false,
            modal_focus: false,
            toast: None,
            cmds: Vec::new(),
            fired: Vec::new(),
            app_actions: Vec::new(),
            gpu_info,
            window_title: String::new(),
            logo: None,
            mark: None,
            caption_h: crate::caption::WELCOME_H,
            settings_page: forma_ui::appearance::PAGE.to_owned(),
        }
    }

    pub fn now(&self) -> f64 {
        self.ctx.input(|i| i.time)
    }

    /// Уведомление внизу окна.
    pub fn toast(&mut self, text: impl Into<String>, error: bool) {
        let now = self.now();
        self.toast = Some(Toast { text: text.into(), error, until: now + if error { 5.0 } else { 2.8 } });
    }

    pub(crate) fn current_toast(&self) -> Option<&Toast> {
        self.toast.as_ref()
    }

    pub(crate) fn clear_toast(&mut self) {
        self.toast = None;
    }

    /// Открыт ли какой-нибудь диалог: оболочки или программы.
    pub fn modal_open(&self) -> bool {
        self.modal.is_some() || self.app_modal
    }

    /// Текст сочетания действия для меню.
    pub fn keys(&self, id: &str) -> String {
        self.settings.keymap.text(&self.actions, id)
    }

    /// Тема из настроек, а если там пусто — тема по умолчанию.
    pub fn theme_id(&self) -> String {
        let id = &self.settings.appearance.theme;
        if id.is_empty() { self.spec.default_theme.to_owned() } else { id.clone() }
    }

    /// Начало кадра: тема, шрифты, плагины, проверка раскладки.
    pub fn begin_frame(&mut self, ctx: &egui::Context) {
        crate::caption::finish_start(ctx);
        self.apply_style(ctx);
        self.poll_plugins(ctx);
        self.poll_user_settings();
        // Плагин `.fpl`, брошенный в окно, ставится через окно согласия; программа его не видит.
        let dropped: Vec<PathBuf> = ctx.input_mut(|i| {
            let (fpl, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut i.raw.dropped_files).into_iter().partition(|f| f.path().extension().is_some_and(|e| e.eq_ignore_ascii_case("fpl")));
            i.raw.dropped_files = rest;
            fpl.into_iter().map(|f| f.path().to_path_buf()).collect()
        });
        if let Some(path) = dropped.into_iter().next() {
            self.cmds.push(ShellCmd::OfferPlugin(path));
        }
        if self.layout.active >= self.layout.workspaces.len() {
            self.layout.active = 0;
        }
    }

    /// Общие сочетания клавиш главного окна. Пока открыт диалог, они не работают, как в
    /// JetBrains: иначе, например, запись сочетания в настройках запускала бы его.
    pub fn handle_keys(&mut self, ctx: &egui::Context) {
        if self.modal_open() {
            return;
        }
        let fired = ctx.input_mut(|i| forma_actions::triggered(i, &self.actions, &self.settings.keymap, ActionContext::Global));
        for id in fired {
            self.route(id);
        }
    }

    /// Общие сочетания клавиш в отдельном окне ОС (из `EditorViewer::window_input`).
    pub fn fire_global(ui: &mut egui::Ui, actions: &Registry, keymap: &forma_actions::Keymap, fired: &mut Vec<String>) {
        fired.extend(ui.input_mut(|i| forma_actions::triggered(i, actions, keymap, ActionContext::Global)));
    }

    fn route(&mut self, id: String) {
        match standard_cmd(&id) {
            Some(cmd) => self.cmds.push(cmd),
            None => self.app_actions.push(id),
        }
    }

    /// Конец кадра: фоновая картинка, диалоги оболочки и программы, уведомление, кнопки
    /// и края окна. Свои диалоги программа рисует в `app_modals` (если открыт диалог
    /// оболочки, они ждут).
    pub fn end_frame(&mut self, ctx: &egui::Context, app_modals: impl FnOnce(&egui::Context, &mut Self)) {
        let bg = self.background();
        self.backdrop.paint(ctx, bg);
        self.shell_modals(ctx);
        if self.modal.is_none() {
            app_modals(ctx, self);
        }
        if !self.modal_open() {
            crate::chrome::reset_modal_fade(ctx);
        }
        self.toast_ui(ctx);
        let height = if self.welcome { crate::caption::WELCOME_H } else { self.caption_h.max(24.0) };
        crate::caption::window_frame(ctx, &self.pal, height, self.modal_open());
    }

    /// Выполняет команды оболочки и возвращает id действий программы, сработавших за кадр.
    pub fn process(&mut self, ctx: &egui::Context) -> Vec<String> {
        for id in std::mem::take(&mut self.fired) {
            if !self.modal_open() {
                self.route(id);
            }
        }
        // Команда может породить новую (`Action` → команда действия), поэтому — до пустой очереди.
        while !self.cmds.is_empty() {
            for cmd in std::mem::take(&mut self.cmds) {
                self.run(ctx, cmd);
            }
        }
        std::mem::take(&mut self.app_actions)
    }

    fn run(&mut self, ctx: &egui::Context, cmd: ShellCmd<B>) {
        match cmd {
            ShellCmd::SwitchWorkspace(i) => {
                if i < self.layout.workspaces.len() {
                    self.layout.active = i;
                }
            }
            ShellCmd::NewWorkspace(template) => {
                let ws = match template {
                    Some(b) => Workspace { builtin: Some(b), name: None, surface: b.surface(&mut self.layout.ids()) },
                    None => self.layout.copy_workspace(self.layout.active),
                };
                self.layout.workspaces.push(ws);
                self.layout.active = self.layout.workspaces.len() - 1;
            }
            ShellCmd::DuplicateWorkspace(i) => {
                if i < self.layout.workspaces.len() {
                    let ws = self.layout.copy_workspace(i);
                    self.layout.workspaces.insert(i + 1, ws);
                    self.layout.active = i + 1;
                }
            }
            ShellCmd::RenameWorkspace(i) => {
                if let Some(ws) = self.layout.workspaces.get(i) {
                    self.modal = Some(Modal::Rename { index: i, name: ws.title() });
                    self.modal_focus = true;
                }
            }
            ShellCmd::ResetWorkspace(i) => {
                if let Some(b) = self.layout.workspaces.get(i).and_then(|w| w.builtin) {
                    let surface = b.surface(&mut self.layout.ids());
                    self.layout.workspaces[i].surface = surface;
                }
            }
            ShellCmd::DeleteWorkspace(i) => {
                let ws = &mut self.layout.workspaces;
                if ws.len() > 1 && i < ws.len() {
                    ws.remove(i);
                    if self.layout.active > i || self.layout.active >= ws.len() {
                        self.layout.active = self.layout.active.saturating_sub(1);
                    }
                }
            }
            ShellCmd::ResetAllWorkspaces => {
                self.layout.workspaces = workspace::default_workspaces(&mut self.layout.ids());
                self.layout.active = 0;
            }
            ShellCmd::SaveDefaultLayout => match (self.layout.for_default(), self.spec.default_layout_path()) {
                (Some(copy), Some(path)) => {
                    workspace::write_layout(&path, &copy, &serde_json::Value::Null);
                    self.toast(t!("toast-default-layout-saved"), false);
                }
                _ => self.toast(t!("toast-default-layout-failed"), true),
            },
            ShellCmd::ApplyDefaultLayout => {
                self.layout = default_layout(self.spec);
                self.toast(t!("toast-default-layout-applied"), false);
            }
            ShellCmd::ResetDefaultLayout => {
                if let Some(path) = self.spec.default_layout_path() {
                    let _ = std::fs::remove_file(path);
                }
                self.toast(t!("toast-default-layout-reset"), false);
            }
            ShellCmd::DetachFocused => {
                let i = self.layout.active;
                self.layout.dock.detach_focused(&self.layout.workspaces[i].surface);
            }
            ShellCmd::ReturnAllWindows => self.layout.dock.return_all(),
            ShellCmd::ToggleMaximize => {
                let i = self.layout.active;
                self.layout.dock.toggle_maximize_focused(&self.layout.workspaces[i].surface);
            }
            ShellCmd::Settings => self.open_settings(None),
            ShellCmd::OpenSettingsPage(page) => self.open_settings(Some(page)),
            ShellCmd::ExportSettings => self.modal = Some(Modal::Export(ExportDialog::new(&self.app_settings))),
            ShellCmd::ImportSettings => self.import_settings(),
            ShellCmd::FindAction => self.modal = Some(Modal::FindAction(FindAction::default())),
            ShellCmd::Action(id) => self.route(id),
            ShellCmd::ZoomIn => self.set_zoom(ctx, self.settings.appearance.zoom + 0.1),
            ShellCmd::ZoomOut => self.set_zoom(ctx, self.settings.appearance.zoom - 0.1),
            ShellCmd::ZoomReset => self.set_zoom(ctx, 1.0),
            ShellCmd::QuickSwitchTheme => {
                self.modal = Some(Modal::ThemeSwitcher(ThemeSwitcher::new(&self.settings.appearance.theme, self.spec.default_theme, &self.themes)));
            }
            ShellCmd::FontZoom { console, step } => {
                let slot = if console { &mut self.settings.fonts.console } else { &mut self.settings.fonts.editor };
                // Размер — с шагом в полпикселя, как у ползунка в настройках.
                let size = ((slot.size + step) * 2.0).round() / 2.0;
                slot.size = size.clamp(*forma_ui::fonts::SIZE_RANGE.start(), *forma_ui::fonts::SIZE_RANGE.end());
            }
            ShellCmd::InstallPlugin(id) => {
                if let Some(entry) = self.spec.catalog().into_iter().find(|e| e.id == id) {
                    self.plugins.install(&entry);
                }
            }
            ShellCmd::InstallPluginFile => {
                let file = rfd::FileDialog::new().set_title(t!("dialog-install-plugin")).add_filter("Forma", &["fpl", "zip"]).pick_file();
                if let Some(file) = file {
                    self.offer_plugin(&file);
                }
            }
            ShellCmd::OfferPlugin(file) => self.offer_plugin(&file),
            ShellCmd::AddPluginFolder => {
                if let Some(dir) = rfd::FileDialog::new().set_title(t!("dialog-plugin-folder")).pick_folder()
                    && !self.settings.plugins.dev.contains(&dir)
                {
                    match forma_plugins::read_manifest(&dir) {
                        Ok(_) => {
                            self.settings.plugins.dev.push(dir);
                            self.plugins.set_dev_dirs(self.settings.plugins.dev.clone());
                            self.settings.save(&mut self.store);
                        }
                        Err(e) => self.toast(t!("toast-plugin-failed", error = e), true),
                    }
                }
            }
            ShellCmd::RemovePluginFolder(dir) => {
                self.settings.plugins.dev.retain(|d| *d != dir);
                self.plugins.set_dev_dirs(self.settings.plugins.dev.clone());
                self.settings.save(&mut self.store);
            }
            ShellCmd::ReloadPlugins => {
                self.plugins.scan();
                self.themes.scan();
                self.themes_rev += 1;
                // Пересобранные модули плагинов в папках разработчика — заново.
                let report = crate::plugins::report(&self.plugins, &self.settings.plugins, self.spec.plugin_app());
                if let Some(rt) = &mut self.plugin_runtime {
                    rt.sync(self.plugins.installed(), &report, true);
                }
            }
            ShellCmd::UninstallPlugin(id) => {
                // Выбранная тема из удаляемого плагина пропадёт — возвращаем тему по умолчанию.
                if let Some(dir) = self.plugins.get(&id).map(|p| p.dir.clone())
                    && self.themes.get(&self.settings.appearance.theme).is_some_and(|t| matches!(&t.origin, theme::Origin::Plugin(p) if p.starts_with(&dir)))
                {
                    self.settings.appearance.theme = String::new();
                }
                if let Err(e) = self.plugins.uninstall(&id) {
                    self.toast(t!("toast-plugin-failed", error = e), true);
                }
            }
            ShellCmd::RevealPlugins => {
                if let Some(root) = self.plugins.root() {
                    let _ = std::fs::create_dir_all(root);
                    open_folder(root);
                }
            }
            ShellCmd::OpenSettingsJson => self.open_settings_json(),
            ShellCmd::About => self.modal = Some(Modal::About),
            ShellCmd::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
        }
    }

    fn set_zoom(&mut self, ctx: &egui::Context, zoom: f32) {
        let zoom = ((zoom * 10.0).round() / 10.0).clamp(*ZOOM_RANGE.start(), *ZOOM_RANGE.end());
        self.settings.appearance.zoom = zoom;
        ctx.set_zoom_factor(zoom);
    }

    /// Сохраняет настройки (платформы и программы) и раскладку.
    pub fn persist(&mut self) {
        self.settings.save(&mut self.store);
        self.app_settings.save(&mut self.store);
        self.save_layout();
    }

    /// Записывает раскладку в её файл.
    pub fn save_layout(&self) {
        if let Some(path) = self.layout_file.clone().or_else(|| self.spec.global_layout_path()) {
            workspace::write_layout(&path, &self.layout, &self.view);
        }
    }

    /// Переключается на раскладку из файла `path` (например, раскладку открытого проекта):
    /// прежняя сохраняется на своё место. Нет файла — стандартная раскладка, а если нет и
    /// её — заводская. `None` — общая раскладка в папке настроек.
    pub fn switch_layout(&mut self, path: Option<PathBuf>) {
        self.save_layout();
        let file = path.clone().or_else(|| self.spec.global_layout_path());
        let (layout, view) = file.and_then(|p| workspace::read_layout(&p)).unwrap_or_else(|| (default_layout(self.spec), serde_json::Value::Null));
        self.layout = layout;
        self.view = view;
        self.layout_file = path;
    }

    /// Заголовок окна (меняется, только если текст другой).
    pub fn set_title(&mut self, ctx: &egui::Context, title: &str) {
        if title != self.window_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.to_owned()));
            self.window_title = title.to_owned();
        }
    }

    /// Фоновая картинка: своя из настроек, иначе картинка темы.
    fn background(&self) -> Option<Background> {
        let a = &self.settings.appearance;
        match &a.background {
            Some(image) => Some(Background { image: image.clone(), opacity: a.background_opacity, fit: a.background_fit }),
            None => self.theme.background.clone(),
        }
    }

    /// Тема, цвета, шрифты: пересчитываются, когда меняется что-то, от чего они зависят
    /// (в том числе файл своей темы на диске).
    fn apply_style(&mut self, ctx: &egui::Context) {
        if self.themes.poll(ctx.input(|i| i.time)) {
            self.themes_rev += 1;
        }
        let a = &self.settings.appearance;
        let key = StyleKey {
            theme: a.theme.clone(),
            themes_rev: self.themes_rev,
            ui_size: self.settings.fonts.ui.size.to_bits(),
            max_keys: self.settings.keymap.shortcuts(&self.actions, actions::MAXIMIZE).to_vec(),
            overrides: self.theme_overrides.clone(),
        };
        if self.style_key.as_ref() != Some(&key) {
            let mut theme = self.themes.resolve(&key.theme, self.spec.default_theme);
            if !key.overrides.is_empty() {
                let mut file = (*theme.file).clone();
                file.colors.extend(key.overrides.iter().map(|(k, v)| (k.clone(), v.clone())));
                theme = Theme::resolve(file);
            }
            self.theme = theme;
            self.pal = self.theme.pal.clone();
            self.theme_gen += 1;
            crate::highlight::set_theme(&self.theme, self.theme_gen);
            forma_ui::icons::set_theme(&self.theme.icons, &self.theme.icon_colors);
            let text = key.max_keys.first().map(ToString::to_string).unwrap_or_default();
            self.dock_style = self.pal.dock_style(self.spec.app.title, self.icon.clone(), key.max_keys.iter().map(Shortcut::to_egui).collect(), text);
            theme::apply(ctx, &self.pal, self.settings.fonts.ui.size);
            self.style_key = Some(key);
        }
        let choice = font_choice(&self.settings.fonts, &self.theme.fonts);
        if self.fonts_key.as_ref() != Some(&choice) {
            forma_ui::fonts::install(ctx, &choice);
            self.fonts_key = Some(choice);
        }
    }

    /// Установки плагинов: что поставилось или удалилось — к тому подключаются темы и
    /// список плагинов в настройках (для экспорта).
    fn poll_plugins(&mut self, ctx: &egui::Context) {
        if self.plugins.poll() {
            ctx.request_repaint_after_secs(0.2);
        }
        for event in self.plugins.take_events() {
            let title = |id: &str| self.spec.catalog().into_iter().find(|e| e.id == id).map(|e| e.name).unwrap_or_else(|| id.to_owned());
            match event {
                forma_plugins::Event::Installed(id) => {
                    let name = self.plugins.get(&id).map(|p| p.manifest.title().to_owned()).unwrap_or_else(|| title(&id));
                    self.toast(t!("toast-plugin-installed", name = name), false);
                }
                forma_plugins::Event::Failed { id, error } => self.toast(t!("toast-plugin-install-failed", name = title(&id), error = error), true),
                forma_plugins::Event::Removed(_) => {}
            }
        }
        // Список установленных — в настройках, чтобы экспорт знал, какие плагины переносить.
        let installed: Vec<String> = self.plugins.installed().iter().filter(|p| !p.builtin).map(|p| p.id().to_owned()).collect();
        if self.settings.plugins.installed != installed {
            self.settings.plugins.installed = installed;
        }
        // Проверка загрузчика — каждый кадр: плагинов немного, а включения, установки и папки
        // разработчика меняют её в разных местах.
        self.plugin_report = crate::plugins::report(&self.plugins, &self.settings.plugins, self.spec.plugin_app());
        if self.themes.set_plugin_dirs(crate::plugins::theme_dirs(&self.plugins, &self.plugin_report)) {
            self.themes_rev += 1;
        }
        // Code плагинов — за работающими плагинами; что плагины сообщают — внизу окна.
        let language = self.settings.appearance.effective_language();
        let mut notices = Vec::new();
        let mut commands = None;
        // Настройки — code тех плагинов, что работают; заново, когда поменялись значения или
        // набор плагинов.
        let settings_key = (self.user.rev(), self.plugin_report.order.clone());
        let settings = (self.runtime_settings.as_ref() != Some(&settings_key)).then(|| self.effective_settings());
        if let Some(rt) = &mut self.plugin_runtime {
            rt.set_language(&language);
            rt.sync(self.plugins.installed(), &self.plugin_report, false);
            if let Some(values) = settings {
                rt.set_settings(values);
                self.runtime_settings = Some(settings_key);
            }
            notices = rt.take_notices();
            commands = Some(rt.commands());
        }
        if let Some(commands) = commands.filter(|c| *c != self.plugin_commands) {
            self.register_plugin_commands(commands);
        }
        for n in notices {
            let name = self.plugins.get(&n.plugin).map(|p| p.manifest.title().to_owned()).unwrap_or(n.plugin);
            self.toast(format!("{name}: {}", n.text), n.error);
        }
    }

    /// Команды плагинов — действиями в группе «Плагины»: их видно в «Найти действие» и в
    /// раскладке клавиш, у них сочетания из манифеста (`key`), которые можно переназначить.
    /// Сработавшую команду программа получает, как свои действия, из [`Shell::process`].
    fn register_plugin_commands(&mut self, commands: Vec<forma_plugin_host::CommandInfo>) {
        self.actions.retain(|a| a.group != PLUGIN_ACTIONS);
        for c in &commands {
            if self.actions.get(&c.id).is_some() {
                log::warn!("Plugin {}: command {} has the id of an app action", c.plugin, c.id);
                continue;
            }
            self.actions.register(forma_actions::Action {
                id: c.id.clone(),
                group: PLUGIN_ACTIONS.to_owned(),
                title: forma_actions::Title::Text(c.title.clone()),
                defaults: c.key.as_deref().and_then(Shortcut::parse).into_iter().collect(),
                context: ActionContext::Global,
            });
        }
        self.plugin_commands = commands;
    }

    /// Команда плагина ли это (id действия).
    pub fn is_plugin_command(&self, id: &str) -> bool {
        self.plugin_commands.iter().any(|c| c.id == id)
    }

    /// Встроенные плагины программы (их манифесты, обычно без `[code]`: код у них свой, в самой
    /// программе). Они видны на странице «Плагины», их можно выключить, но не удалить, а их
    /// настройки — в окне настроек и `settings.json`. Работать ли их коду, программа узнаёт
    /// у [`Shell::plugin_works`].
    pub fn set_builtin_plugins(&mut self, manifests: Vec<Manifest>) {
        self.plugins.set_builtin(manifests);
        self.plugin_report = crate::plugins::report(&self.plugins, &self.settings.plugins, self.spec.plugin_app());
    }

    /// Работает ли плагин: установлен (или встроен), включён и всего ему хватает.
    pub fn plugin_works(&self, id: &str) -> bool {
        self.plugin_report.works(id)
    }

    /// Настройки работающих плагинов: id плагина, его название и схема.
    pub fn plugin_settings(&self) -> Vec<(String, String, Vec<Setting>)> {
        self.plugins
            .installed()
            .iter()
            .filter(|p| !p.manifest.contributes.settings.is_empty() && self.plugin_report.works(p.id()))
            .map(|p| (p.id().to_owned(), p.manifest.title().to_owned(), p.manifest.contributes.settings.clone()))
            .collect()
    }

    /// Схема настройки по ключу (у работающих плагинов).
    pub fn setting_schema(&self, key: &str) -> Option<Setting> {
        let plugin = key.split('.').next()?;
        let p = self.plugins.installed().iter().find(|p| p.id() == plugin && self.plugin_report.works(p.id()))?;
        p.manifest.contributes.settings.iter().find(|s| s.key == key).cloned()
    }

    /// Значение настройки плагина: из `settings.json`, если оно там есть и подходит, иначе
    /// по умолчанию. Незнакомый ключ — то, что в файле (или `null`).
    pub fn setting(&self, key: &str) -> serde_json::Value {
        match self.setting_schema(key) {
            Some(s) => s.effective(self.user.get(key)),
            None => self.user.get(key).cloned().unwrap_or_default(),
        }
    }

    pub fn setting_bool(&self, key: &str) -> bool {
        self.setting(key).as_bool().unwrap_or(false)
    }

    pub fn setting_f64(&self, key: &str) -> f64 {
        self.setting(key).as_f64().unwrap_or(0.0)
    }

    pub fn setting_str(&self, key: &str) -> String {
        self.setting(key).as_str().unwrap_or_default().to_owned()
    }

    /// Номер изменения `settings.json`: вырос — настройки плагинов стоит перечитать.
    pub fn settings_rev(&self) -> u64 {
        self.user.rev()
    }

    /// Все действующие значения настроек работающих плагинов.
    fn effective_settings(&self) -> BTreeMap<String, serde_json::Value> {
        self.plugin_settings().into_iter().flat_map(|(_, _, list)| list).map(|s| (s.key.clone(), s.effective(self.user.get(&s.key)))).collect()
    }

    /// Раз в секунду: не поправили ли `settings.json` руками. Ошибка в нём — уведомление со
    /// строкой и столбцом, а действуют прежние значения.
    fn poll_user_settings(&mut self) {
        if let Poll::Broken(e) = self.user.poll(self.now()) {
            let error = forma_ui::plugin_settings::parse_error_text(&e);
            self.toast(t!("toast-settings-json-broken", line = e.line, col = e.col, error = error), true);
        }
    }

    /// Открывает `settings.json` (новый — с `{}`): в программе, если она открывает файлы сама
    /// ([`Shell::edit_in_app`]), иначе в программе по умолчанию.
    pub fn open_settings_json(&mut self) {
        match self.user.ensure_file() {
            Ok(path) if self.edit_in_app => self.open_files.push(path),
            Ok(path) => open_folder(&path),
            Err(e) => self.toast(t!("toast-settings-json-write-failed", error = forma_ui::plugin_settings::write_error_text(&e)), true),
        }
    }

    /// Файлы, которые оболочка просит программу открыть (`settings.json`).
    pub fn take_open_files(&mut self) -> Vec<PathBuf> {
        std::mem::take(&mut self.open_files)
    }

    /// Распаковывает плагин из файла и показывает окно согласия: что он приносит и что просит.
    fn offer_plugin(&mut self, file: &std::path::Path) {
        match self.plugins.prepare(file) {
            Ok(pending) => {
                self.modal = Some(Modal::PluginConsent(Box::new(pending)));
                self.modal_focus = true;
                // Файл бросили в окно без движения мыши — окно должно показаться сразу.
                self.ctx.request_repaint();
            }
            Err(e) => self.toast(t!("toast-plugin-failed", error = e), true),
        }
    }
}

/// Стандартная раскладка, а если её нет — заводская.
fn default_layout<B: Builtin>(spec: &Spec) -> Layout<B> {
    spec.default_layout_path().and_then(|p| workspace::read_layout(&p)).map(|(l, _)| l).unwrap_or_else(Layout::factory)
}

/// Открывает папку в проводнике.
pub fn open_folder(path: &std::path::Path) {
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("explorer").arg(path).spawn();
    }
    #[cfg(not(windows))]
    {
        let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
        let _ = std::process::Command::new(opener).arg(path).spawn();
    }
}

/// Показывает файл или папку в проводнике.
pub fn reveal_in_explorer(path: &std::path::Path) {
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("explorer").arg(format!("/select,{}", path.display())).spawn();
    }
    #[cfg(not(windows))]
    {
        let dir = if path.is_dir() { path.to_path_buf() } else { path.parent().map(std::path::Path::to_path_buf).unwrap_or_default() };
        let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
        let _ = std::process::Command::new(opener).arg(dir).spawn();
    }
}
