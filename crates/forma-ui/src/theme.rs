//! Темы платформы Forma.
//!
//! Тема — файл TOML ([`file::ThemeFile`]): палитра, ключи цветов, подсветка кода, шрифты
//! и фоновая картинка. Каждый ключ темы описан в таблице ([`theme_tokens!`]) вместе со
//! значением по умолчанию, которое ссылается на другие ключи (`menubar.bg` → `$bg.panel`).
//! Поэтому теме достаточно задать то, что она меняет: розовая шапка — это одна строка
//! `"menubar.bg" = "#ff69b4"`, а всё остальное возьмётся из основы.
//!
//! Таблицы ключей бывают у платформы ([`Palette`]) и у программы (цвета происхождения,
//! уровней лога в Mycelium) — они вычисляются одним [`Resolver`] и могут ссылаться друг на друга.

pub mod color;
pub mod file;
mod registry;

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Arc;

use egui::{self, Color32, CornerRadius, FontFamily, FontId, Shadow, Stroke, TextStyle, Visuals, style::ScrollStyle};
use forma_i18n::t;
use forma_layout::DockStyle;
use serde::{Deserialize, Serialize};

pub use file::{BackgroundSpec, Fit, FontsSpec, SyntaxSpec, ThemeFile, slug};
pub use registry::{ThemeEntry, Themes};

/// Акцентный цвет, которым можно перекрасить любую тему.
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

/// Ключ темы: имя в файле, значение по умолчанию и описание (для шаблона новой темы).
#[derive(Clone, Copy, Debug)]
pub struct TokenInfo {
    pub key: &'static str,
    pub default: &'static str,
    pub doc: &'static str,
}

/// Таблица ключей темы: из каждой строки получается поле структуры, ключ в файле,
/// значение по умолчанию и описание.
///
/// ```ignore
/// forma_ui::theme_tokens! {
///     pub struct ModColors {
///         /// Классы и ресурсы Minecraft.
///         minecraft = "origin.minecraft" => "#6abe30",
///     }
/// }
/// ```
#[macro_export]
macro_rules! theme_tokens {
    ($(#[$m:meta])* $vis:vis struct $name:ident { $( #[doc = $doc:literal] $field:ident = $key:literal => $default:literal, )* }) => {
        $(#[$m])*
        #[derive(Clone, Debug, PartialEq)]
        $vis struct $name {
            /// Тёмная ли тема.
            pub dark: bool,
            $( #[doc = $doc] pub $field: $crate::egui::Color32, )*
        }

        impl $name {
            pub const TOKENS: &'static [$crate::theme::TokenInfo] = &[$($crate::theme::TokenInfo { key: $key, default: $default, doc: $doc },)*];

            pub fn resolve(r: &$crate::theme::Resolver) -> Self {
                Self { dark: r.dark(), $($field: r.color($key),)* }
            }
        }
    };
}

/// Вычисляет ключи темы: подставляет ссылки `$имя` (палитра, ключи темы, значения по
/// умолчанию), запоминает вычисленное и находит циклы. Ошибки копятся и показываются
/// в настройках, а сломанный ключ получает значение по умолчанию.
pub struct Resolver<'a> {
    file: &'a ThemeFile,
    defaults: HashMap<&'static str, &'static str>,
    overrides: HashMap<&'static str, Color32>,
    cache: RefCell<HashMap<String, Color32>>,
    stack: RefCell<Vec<String>>,
    errors: RefCell<Vec<String>>,
}

impl<'a> Resolver<'a> {
    pub fn new(file: &'a ThemeFile, groups: &[&'static [TokenInfo]]) -> Self {
        let defaults = groups.iter().flat_map(|g| g.iter()).map(|t| (t.key, t.default)).collect();
        Self { file, defaults, overrides: HashMap::new(), cache: RefCell::default(), stack: RefCell::default(), errors: RefCell::default() }
    }

    /// Значение, которое важнее темы (например, акцент из настроек).
    pub fn set(&mut self, key: &'static str, c: Color32) {
        self.overrides.insert(key, c);
    }

    pub fn dark(&self) -> bool {
        self.file.theme.dark.unwrap_or(true)
    }

    /// Цвет ключа. При ошибке — значение по умолчанию, а если сломано и оно — пурпурный.
    pub fn color(&self, key: &str) -> Color32 {
        match self.lookup(key) {
            Ok(c) => c,
            Err(e) => {
                self.error(format!("{key}: {e}"));
                Color32::from_rgb(255, 0, 255)
            }
        }
    }

    pub fn errors(&self) -> Vec<String> {
        self.errors.borrow().clone()
    }

    fn error(&self, e: String) {
        let mut errors = self.errors.borrow_mut();
        if !errors.contains(&e) {
            errors.push(e);
        }
    }

    /// Вычисляет выражение темы (для подсветки кода и прочих значений вне таблиц).
    pub fn eval(&self, expr: &str) -> Result<Color32, String> {
        color::eval(expr, &mut |name| self.lookup(name))
    }

    fn lookup(&self, name: &str) -> Result<Color32, String> {
        if let Some(c) = self.overrides.get(name) {
            return Ok(*c);
        }
        if let Some(c) = self.cache.borrow().get(name) {
            return Ok(*c);
        }
        if self.stack.borrow().iter().any(|n| n == name) {
            let chain = self.stack.borrow().join(" → ");
            return Err(format!("цикл ссылок: {chain} → {name}"));
        }
        let from_theme = self.file.palette.get(name).or_else(|| self.file.colors.get(name));
        let default = self.defaults.get(name).copied();
        let Some(expr) = from_theme.map(String::as_str).or(default) else {
            return Err(format!("нет цвета «{name}»"));
        };
        self.stack.borrow_mut().push(name.to_owned());
        let mut result = self.eval(expr);
        // Ошибка в теме — пробуем значение по умолчанию, а ошибку запоминаем.
        if let (Err(e), Some(_), Some(d)) = (&result, from_theme, default) {
            self.error(format!("{name}: {e}"));
            result = self.eval(d);
        }
        self.stack.borrow_mut().pop();
        let c = result?;
        self.cache.borrow_mut().insert(name.to_owned(), c);
        Ok(c)
    }
}

theme_tokens! {
    /// Цвета интерфейса платформы. Значения по умолчанию — тема Mycelium Dark Default.
    pub struct Palette {
        // Основа: на неё ссылаются все остальные ключи.
        /// Фон содержимого областей.
        bg = "bg" => "#1c1e24",
        /// Панели: строка меню, строка состояния, боковые панели.
        bg_panel = "bg.panel" => "#131419",
        /// Строка вкладок внутри области.
        bg_header = "bg.header" => "#17191f",
        /// Самый глубокий тон: зазоры между областями.
        bg_deep = "bg.deep" => "#0d0e12",
        /// Всплывающие окна, меню, диалоги.
        bg_elevated = "bg.elevated" => "#24272f",
        /// Подсветка под указателем.
        bg_hover = "bg.hover" => "#2c303a",
        /// Рамки и разделители.
        border = "border" => "#2a2d36",
        /// Более заметные рамки.
        border_strong = "border.strong" => "lighten($border, 12%)",
        /// Основной текст.
        text = "text" => "#dcdfe4",
        /// Второстепенный текст.
        text_muted = "text.muted" => "#a0a6b2",
        /// Едва заметный текст: подписи, номера строк.
        text_faint = "text.faint" => "#6c7380",
        /// Заголовки и выбранное.
        heading = "heading" => "#f4f5f8",
        /// Акцентный цвет.
        accent = "accent" => "#9684ff",
        /// Мягкая заливка акцентом: выбранная вкладка, строка.
        accent_soft = "accent.soft" => "alpha($accent, 19%)",
        /// Текст на заливке акцентом.
        accent_text = "accent.text" => "$bg",
        /// Выделение текста и выбранные пункты.
        selection = "selection" => "alpha($accent, 31%)",
        /// Текст на выделении и в выбранных пунктах.
        selection_text = "selection.text" => "$heading",
        /// Подсветка совпадений поиска.
        highlight = "highlight" => "#facc153c",
        /// Успех.
        success = "success" => "#4ade80",
        /// Предупреждение.
        warning = "warning" => "#fbbf24",
        /// Ошибка.
        danger = "danger" => "#f87171",
        /// Тени всплывающих окон.
        shadow = "shadow" => "#0000006e",

        // Окно.
        /// Фон окна под всем остальным.
        window_bg = "window.bg" => "$bg.deep",
        /// Строка меню («Файл», «Вид»…) и рабочих пространств.
        menubar_bg = "menubar.bg" => "$bg.panel",
        /// Пункты строки меню.
        menubar_text = "menubar.text" => "$text",
        /// Пункт строки меню под указателем.
        menubar_hover = "menubar.hover" => "$bg.hover",
        /// Вкладка рабочего пространства.
        workspace_tab_text = "workspace.tab.text" => "mix($menubar.text, $menubar.bg, 30%)",
        /// Вкладка рабочего пространства под указателем.
        workspace_tab_hover = "workspace.tab.hover" => "$menubar.hover",
        /// Выбранное рабочее пространство.
        workspace_tab_active = "workspace.tab.active" => "$accent.soft",
        /// Текст выбранного рабочего пространства.
        workspace_tab_active_text = "workspace.tab.active.text" => "$menubar.text",
        /// Строка состояния.
        statusbar_bg = "statusbar.bg" => "$bg.panel",
        /// Текст строки состояния.
        statusbar_text = "statusbar.text" => "$text.faint",
        /// Заметный текст строки состояния (имя проекта).
        statusbar_text_strong = "statusbar.text.strong" => "$text.muted",

        // Области и вкладки.
        /// Зазоры между областями.
        dock_gap = "dock.gap" => "$window.bg",
        /// Фон области.
        area_bg = "area.bg" => "$bg",
        /// Строка вкладок области.
        area_header = "area.header" => "$bg.header",
        /// Рамка области и зоны броска вкладки.
        area_border = "area.border" => "$border",
        /// Подпись вкладки.
        tab_text = "tab.text" => "$text.muted",
        /// Подпись активной вкладки.
        tab_active_text = "tab.active.text" => "$heading",
        /// Полоска под активной вкладкой.
        tab_indicator = "tab.indicator" => "$accent",
        /// Вкладка под указателем.
        tab_hover = "tab.hover" => "$bg.hover",

        // Всплывающее.
        /// Меню, подсказки, списки.
        popup_bg = "popup.bg" => "$bg.elevated",
        /// Рамка меню и подсказок.
        popup_border = "popup.border" => "$border",
        /// Пункт меню под указателем.
        popup_hover = "popup.hover" => "$bg.hover",
        /// Диалоги.
        modal_bg = "modal.bg" => "$bg.elevated",
        /// Рамка диалога.
        modal_border = "modal.border" => "$border",
        /// Затемнение под диалогом.
        modal_backdrop = "modal.backdrop" => "#0000006e",
        /// Уведомления внизу окна.
        toast_bg = "toast.bg" => "$bg.elevated",
        /// Текст уведомлений.
        toast_text = "toast.text" => "$text",

        // Элементы управления.
        /// Кнопки, галочки, ползунки, полосы прокрутки.
        control_bg = "control.bg" => "$bg.hover",
        /// Рамка кнопок, галочек и полей ввода.
        control_border = "control.border" => "transparent",
        /// Текст и значки элементов.
        control_text = "control.text" => "$text.muted",
        /// Элемент под указателем.
        control_hover = "control.hover" => "$bg.hover",
        /// Текст элемента под указателем.
        control_hover_text = "control.hover.text" => "$text",
        /// Нажатый элемент.
        control_active = "control.active" => "$accent.soft",
        /// Главная кнопка диалога («ОК», «Готово»).
        button_primary = "button.primary" => "$accent",
        /// Текст главной кнопки.
        button_primary_text = "button.primary.text" => "$accent.text",
        /// Фон полей ввода.
        input_bg = "input.bg" => "$bg",
        /// Рамка поля ввода в фокусе.
        input_focus = "input.focus" => "$accent",
        /// Дорожка переключателя-сегментов.
        segmented_bg = "segmented.bg" => "darken($bg, 15%)",
        /// Выбранный сегмент.
        segmented_selected = "segmented.selected" => "$bg.hover",
        /// Курсор в полях ввода.
        cursor = "cursor" => "$accent",
        /// Ссылки.
        link = "link" => "$accent",
        /// Строка списка под указателем (дерево, консоль, списки).
        list_hover = "list.hover" => "alpha($bg.hover, 60%)",
        /// Выбранная строка списка.
        list_selected = "list.selected" => "$accent.soft",
        /// Плашки: сочетания клавиш, метки.
        chip_bg = "chip.bg" => "$bg.elevated",
        /// Рамка плашек.
        chip_border = "chip.border" => "$border",
        /// Боковая панель окна настроек.
        sidebar_bg = "sidebar.bg" => "$bg.panel",

        // Редактор кода.
        /// Фон редактора.
        editor_bg = "editor.bg" => "$area.bg",
        /// Текст без подсветки.
        editor_text = "editor.text" => "$text",
        /// Полоса номеров строк.
        editor_gutter = "editor.gutter" => "$editor.bg",
        /// Номера строк.
        editor_line_number = "editor.line-number" => "$text.faint",
        /// Номер строки с курсором.
        editor_line_number_current = "editor.line-number.current" => "$text",
        /// Подсветка строки с курсором.
        editor_current_line = "editor.current-line" => "alpha($bg.hover, 45%)",
        /// Выделение в редакторе.
        editor_selection = "editor.selection" => "$selection",
        /// Текст на выделении в редакторе.
        editor_selection_text = "editor.selection.text" => "$selection.text",
        /// Курсор в редакторе.
        editor_cursor = "editor.cursor" => "$cursor",
        /// Строка пути над редактором.
        editor_header = "editor.header" => "alpha($bg.header, 60%)",
        /// Текст строки пути.
        editor_header_text = "editor.header.text" => "$text.muted",
        /// Подчёркивание ошибок.
        editor_error = "editor.error" => "$danger",
        /// Подчёркивание предупреждений.
        editor_warning = "editor.warning" => "$warning",
        /// Подсказки и замечания.
        editor_info = "editor.info" => "$text.faint",
        /// Пунктир под именем при Ctrl+наведении.
        editor_link = "editor.link" => "$accent",
        /// Выбранный вариант автодополнения.
        completion_selected = "completion.selected" => "$accent.soft",
        /// Вариант автодополнения.
        completion_text = "completion.text" => "$heading",
        /// Подпись варианта справа.
        completion_detail = "completion.detail" => "$text.faint",
        /// Значок метода.
        completion_method = "completion.method" => "#c084fc",
        /// Значок поля и переменной.
        completion_field = "completion.field" => "#fb923c",
        /// Значок класса.
        completion_class = "completion.class" => "#60a5fa",
        /// Значок интерфейса.
        completion_interface = "completion.interface" => "#2dd4bf",
        /// Значок перечисления и константы.
        completion_enum = "completion.enum" => "#facc15",
        /// Значок ключевого слова и модуля.
        completion_keyword = "completion.keyword" => "$text.muted",

        // Экран приветствия.
        /// Фон экрана приветствия.
        welcome_bg = "welcome.bg" => "$bg",
        /// Название программы.
        welcome_title = "welcome.title" => "$heading",
        /// Подпись «FORMA».
        welcome_brand = "welcome.brand" => "$accent",
    }
}

/// Начертание в подсветке кода.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SyntaxStyle {
    pub color: Color32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

/// Фоновая картинка, готовая к показу.
#[derive(Clone, Debug, PartialEq)]
pub struct Background {
    pub image: PathBuf,
    pub opacity: f32,
    pub fit: Fit,
}

/// Вычисленная тема.
#[derive(Clone, Debug)]
pub struct Theme {
    pub id: String,
    pub name: String,
    pub pal: Palette,
    /// Подсветка кода: класс → начертание.
    pub syntax: BTreeMap<String, SyntaxStyle>,
    pub fonts: FontsSpec,
    pub background: Option<Background>,
    /// Свои значки темы: имя значка → SVG (см. [`crate::icons`]).
    pub icons: BTreeMap<String, PathBuf>,
    /// Цвета значков из ключей `icon.<имя>`: главнее цвета, которым значок рисуется по месту.
    pub icon_colors: BTreeMap<String, Color32>,
    /// Ошибки в файле темы — показываются в настройках.
    pub errors: Vec<String>,
    /// Файл темы вместе с родителями: из него программа вычисляет свои таблицы ключей.
    pub file: Arc<ThemeFile>,
    accent: Option<Color32>,
}

impl Theme {
    /// `accent` — акцент из настроек поверх акцента темы.
    pub fn resolve(file: ThemeFile, accent: Option<Accent>) -> Self {
        let accent = accent.map(|a| a.color(file.theme.dark.unwrap_or(true)));
        let mut r = Resolver::new(&file, &[Palette::TOKENS]);
        if let Some(a) = accent {
            r.set("accent", a);
        }
        let pal = Palette::resolve(&r);
        let mut syntax = BTreeMap::new();
        for (class, spec) in &file.syntax {
            let (expr, bold, italic, underline) = match spec {
                SyntaxSpec::Color(c) => (Some(c.as_str()), false, false, false),
                SyntaxSpec::Style { color, bold, italic, underline } => (color.as_deref(), *bold, *italic, *underline),
            };
            let color = match expr.map(|e| r.eval(e)) {
                Some(Ok(c)) => c,
                Some(Err(e)) => {
                    r.error(format!("syntax.{class}: {e}"));
                    pal.editor_text
                }
                None => pal.editor_text,
            };
            syntax.insert(class.clone(), SyntaxStyle { color, bold, italic, underline });
        }
        // Цвет значка: «icon.имя» в [colors] — например, "icon.run" = "$green".
        let mut icon_colors = BTreeMap::new();
        for (key, expr) in &file.colors {
            if let Some(name) = key.strip_prefix("icon.") {
                match r.eval(expr) {
                    Ok(c) => {
                        icon_colors.insert(name.to_owned(), c);
                    }
                    Err(e) => r.error(format!("{key}: {e}")),
                }
            }
        }
        let background = file.background.as_ref().filter(|b| !b.image.trim().is_empty()).map(|b| {
            let path = PathBuf::from(&b.image);
            let image = if path.is_relative() { file.dir.as_ref().map(|d| d.join(&path)).unwrap_or(path) } else { path };
            Background { image, opacity: b.opacity.clamp(0.0, 1.0), fit: b.fit }
        });
        let errors = r.errors();
        for e in &errors {
            log::warn!("Theme {}: {e}", file.theme.id);
        }
        Self {
            id: file.theme.id.clone(),
            name: file.theme.name.clone(),
            pal,
            syntax,
            fonts: file.fonts.clone(),
            background,
            icons: file.icons.iter().map(|(name, path)| (name.clone(), PathBuf::from(path))).collect(),
            icon_colors,
            errors,
            file: Arc::new(file),
            accent,
        }
    }

    /// Вычислитель ключей этой темы для групп `groups` (для инструментов авторов тем).
    pub fn with_resolver<R>(&self, groups: &[&'static [TokenInfo]], f: impl FnOnce(&Resolver) -> R) -> R {
        let mut all = vec![Palette::TOKENS];
        all.extend(groups.iter().copied());
        let mut r = Resolver::new(&self.file, &all);
        if let Some(a) = self.accent {
            r.set("accent", a);
        }
        f(&r)
    }

    /// Своя таблица ключей программы, вычисленная по этой же теме.
    pub fn group<T>(&self, tokens: &'static [TokenInfo], make: fn(&Resolver) -> T) -> T {
        let mut r = Resolver::new(&self.file, &[Palette::TOKENS, tokens]);
        if let Some(a) = self.accent {
            r.set("accent", a);
        }
        let out = make(&r);
        for e in r.errors() {
            log::warn!("Theme {}: {e}", self.id);
        }
        out
    }

    /// Начертание класса подсветки, а если тема его не задала — обычный текст.
    pub fn syntax(&self, class: &str) -> SyntaxStyle {
        self.syntax.get(class).copied().unwrap_or(SyntaxStyle { color: self.pal.editor_text, bold: false, italic: false, underline: false })
    }
}

impl Palette {
    /// Оформление областей и вкладок для составного интерфейса.
    /// Иконку передаём готовой: рисовать её на каждом кадре дорого.
    pub fn dock_style(&self, app_title: &str, icon: Arc<egui::IconData>, maximize_keys: Vec<egui::KeyboardShortcut>, maximize_text: String) -> DockStyle {
        DockStyle {
            gap_bg: self.dock_gap,
            area_bg: self.area_bg,
            header_bg: self.area_header,
            hover_bg: self.tab_hover,
            text: self.text,
            text_muted: self.tab_text,
            text_strong: self.tab_active_text,
            accent: self.tab_indicator,
            border: self.area_border,
            menu_style,
            paint_icon: crate::icons::paint,
            icon: Some(icon),
            app_title: app_title.to_owned(),
            maximize_keys,
            maximize_text,
        }
    }
}

/// Устанавливает визуальный стиль темы в контекст egui.
pub fn apply(ctx: &egui::Context, pal: &Palette, font_size: f32) {
    let theme = if pal.dark { egui::Theme::Dark } else { egui::Theme::Light };
    ctx.set_theme(theme);

    let mut visuals = if pal.dark { Visuals::dark() } else { Visuals::light() };
    visuals.panel_fill = pal.bg_panel;
    visuals.window_fill = pal.popup_bg;
    visuals.window_stroke = Stroke::new(1.0, pal.popup_border);
    visuals.extreme_bg_color = pal.input_bg;
    visuals.text_edit_bg_color = Some(pal.input_bg);
    visuals.faint_bg_color = color::with_alpha(pal.text, 0.03);
    visuals.code_bg_color = pal.bg_hover;
    visuals.hyperlink_color = pal.link;
    visuals.override_text_color = None;
    visuals.selection.bg_fill = pal.selection;
    // egui красит этим цветом и выделенный текст, и текст выбранных пунктов — нужен
    // читаемый на выделении цвет, а не акцент.
    visuals.selection.stroke = Stroke::new(1.0, pal.selection_text);
    visuals.window_corner_radius = CornerRadius::same(12);
    visuals.menu_corner_radius = CornerRadius::same(8);
    visuals.window_shadow = Shadow { offset: [0, 8], blur: 28, spread: 0, color: pal.shadow };
    visuals.popup_shadow = Shadow { offset: [0, 4], blur: 16, spread: 0, color: color::with_alpha(pal.shadow, pal.shadow.a() as f32 / 255.0 * 0.8) };
    visuals.text_cursor.stroke = Stroke::new(2.0, pal.cursor);
    visuals.indent_has_left_vline = false;
    visuals.collapsing_header_frame = false;

    let radius = CornerRadius::same(6);
    let border = Stroke::new(1.0, pal.control_border);
    let w = &mut visuals.widgets;
    w.noninteractive.bg_stroke = Stroke::new(1.0, pal.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0, pal.text);
    w.noninteractive.corner_radius = radius;
    w.inactive.weak_bg_fill = Color32::TRANSPARENT;
    w.inactive.bg_fill = pal.control_bg;
    w.inactive.bg_stroke = border;
    w.inactive.fg_stroke = Stroke::new(1.0, pal.control_text);
    w.inactive.corner_radius = radius;
    w.hovered.weak_bg_fill = pal.control_hover;
    w.hovered.bg_fill = pal.control_hover;
    w.hovered.bg_stroke = border;
    w.hovered.fg_stroke = Stroke::new(1.5, pal.control_hover_text);
    w.hovered.corner_radius = radius;
    w.hovered.expansion = 0.0;
    w.active.weak_bg_fill = pal.control_active;
    w.active.bg_fill = pal.control_active;
    w.active.bg_stroke = border;
    w.active.fg_stroke = Stroke::new(1.5, pal.heading);
    w.active.corner_radius = radius;
    w.active.expansion = 0.0;
    w.open.weak_bg_fill = pal.popup_hover;
    w.open.bg_fill = pal.popup_hover;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_resolve_without_errors() {
        let file = ThemeFile::parse("[theme]\nid = \"empty\"\ndark = true").unwrap();
        let t = Theme::resolve(file, None);
        assert!(t.errors.is_empty(), "{:?}", t.errors);
        assert_eq!(t.pal.menubar_bg, t.pal.bg_panel);
        assert_eq!(t.pal.dock_gap, t.pal.bg_deep);
        assert_eq!(color::to_hex(t.pal.accent_soft), "#9585ff30");
    }

    #[test]
    fn one_line_recolors_a_region() {
        let file = ThemeFile::parse("[theme]\nid = \"pink\"\n[palette]\npink = \"#ff69b4\"\n[colors]\n\"menubar.bg\" = \"$pink\"").unwrap();
        let t = Theme::resolve(file, None);
        assert_eq!(t.pal.menubar_bg, Color32::from_rgb(255, 105, 180));
        assert_eq!(t.pal.statusbar_bg, t.pal.bg_panel, "остальное — как было");
    }

    #[test]
    fn accent_override_flows_into_derived_keys() {
        let file = ThemeFile::parse("[theme]\nid = \"t\"\ndark = true").unwrap();
        let t = Theme::resolve(file, Some(Accent::Teal));
        assert_eq!(t.pal.accent, Accent::Teal.color(true));
        assert_eq!(t.pal.tab_indicator, t.pal.accent);
        assert_eq!(t.pal.selection, color::with_alpha(t.pal.accent, 0.31));
    }

    #[test]
    fn broken_values_fall_back_and_report() {
        let file = ThemeFile::parse("[theme]\nid = \"b\"\n[colors]\n\"menubar.bg\" = \"$nope\"\n\"tab.hover\" = \"$tab.hover\"\nbg = \"#zzzzzz\"").unwrap();
        let t = Theme::resolve(file, None);
        assert_eq!(t.pal.menubar_bg, t.pal.bg_panel, "сломанный ключ — по умолчанию");
        assert_eq!(t.pal.bg, Color32::from_rgb(28, 30, 36));
        assert!(t.errors.iter().any(|e| e.contains("nope")), "{:?}", t.errors);
        assert!(t.errors.iter().any(|e| e.contains("цикл")), "{:?}", t.errors);
        assert!(t.errors.iter().any(|e| e.contains("zzzzzz")), "{:?}", t.errors);
    }

    #[test]
    fn syntax_styles_resolve_references() {
        let file = ThemeFile::parse("[theme]\nid = \"s\"\n[palette]\nblue = \"#569cd6\"\n[syntax]\nkeyword = { color = \"$blue\", bold = true }\nstring = \"#ce9178\"").unwrap();
        let t = Theme::resolve(file, None);
        assert_eq!(t.syntax("keyword"), SyntaxStyle { color: Color32::from_rgb(0x56, 0x9c, 0xd6), bold: true, italic: false, underline: false });
        assert_eq!(t.syntax("string").color, Color32::from_rgb(0xce, 0x91, 0x78));
        assert_eq!(t.syntax("unknown").color, t.pal.editor_text);
    }
}
