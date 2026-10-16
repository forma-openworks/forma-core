//! SDK для code плагинов Forma. Плагин — crate `cdylib`, который собирается под
//! `wasm32-wasip2` и получается компонентом WebAssembly. Он реализует [`Plugin`] и
//! регистрирует себя макросом [`register_plugin!`]:
//!
//! ```ignore
//! use forma_plugin_api::{Context, HoverRequest, HoverResult, Plugin, register_plugin};
//!
//! struct Hello;
//!
//! impl Plugin for Hello {
//!     fn new(_cx: Context) -> Result<Self, String> {
//!         Ok(Hello)
//!     }
//!
//!     fn hover(&mut self, req: &HoverRequest) -> Option<HoverResult> {
//!         (req.text == "привет").then(|| HoverResult { start: 0, end: 6, markdown: "**Привет!**".into() })
//!     }
//! }
//!
//! register_plugin!(Hello);
//! ```
//!
//! Что программа даёт плагину (файлы хранилища, своё хранилище «ключ — значение»,
//! уведомления, журнал, открыть панель или ссылку), — в модуле [`host`]; права на файлы и
//! данные объявляются в манифесте.
//!
//! Интерфейс (блоки заметок, панели) плагин описывает деревом элементов из модуля [`ui`]:
//! `ui::column([ui::heading("…"), ui::button("id", "…")])`. Рисует его программа.

/// Привязки к миру WIT `forma:plugin/plugin` (сгенерированы wit-bindgen).
#[doc(hidden)]
pub mod bindings {
    wit_bindgen::generate!({
        path: "../forma-plugin-host/wit",
        world: "plugin",
        pub_export_macro: true,
        default_bindings_module: "forma_plugin_api::bindings",
    });
}

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Дерево элементов интерфейса: что плагин просит нарисовать и события от программы.
pub use forma_plugin_ui as ui;

/// Что программа даёт плагину.
pub use bindings::forma::plugin::host;
pub use bindings::forma::plugin::types::{Entry, HoverRequest, HoverResult, Level};

/// С чем плагин запущен (`activate`).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Context {
    /// Программа: `holotype`, `instar`, `mycelium`.
    pub app: String,
    /// Язык интерфейса: `ru`, `en`.
    pub language: String,
    /// Открыто ли хранилище или проект (иначе файлов нет).
    pub root: bool,
}

/// Где и с чем вызвали команду (`args` команды).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CommandContext {
    /// Откуда: `menu`, `key`, `block`, `panel`, `find-action`.
    pub from: String,
    /// Заметка или файл (путь от корня), если вызвали из неё.
    pub note: String,
    /// Выделенный текст.
    pub selection: String,
    /// Заголовок раздела, в котором выделение или курсор.
    pub heading: String,
    /// Абзац вокруг выделения или курсора.
    pub paragraph: String,
    /// `args` кнопки, которая запустила команду.
    #[serde(skip_serializing_if = "Value::is_null")]
    pub args: Value,
}

impl CommandContext {
    pub fn from_json(json: &str) -> Self {
        serde_json::from_str(json).unwrap_or_default()
    }
}

/// С чем рисуется панель.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PanelContext {
    /// Текущая заметка или файл (путь от корня).
    pub note: String,
    /// Тёмная ли тема.
    pub dark: bool,
}

impl PanelContext {
    pub fn from_json(json: &str) -> Self {
        serde_json::from_str(json).unwrap_or_default()
    }
}

/// Плагин: создаётся при первом вызове (`activate`) и живёт, пока работает программа.
/// Всё, кроме [`Plugin::new`], необязательно: плагин отвечает только на то, что объявил в
/// манифесте.
pub trait Plugin: 'static {
    fn new(cx: Context) -> Result<Self, String>
    where
        Self: Sized;

    /// В хранилище поменялись файлы (пути от корня).
    fn changed(&mut self, _paths: &[String]) {}

    /// Подсказка при наведении на символ `req.offset` текста `req.text`.
    fn hover(&mut self, _req: &HoverRequest) -> Option<HoverResult> {
        None
    }

    /// Блок заметки на языке `lang` → дерево элементов.
    fn render_block(&mut self, lang: &str, _source: &str, _context: &str) -> Result<ui::Element, String> {
        Err(format!("this plugin has no block {lang:?}"))
    }

    /// Команда `id` из манифеста; ответ — JSON (или пустая строка).
    fn command(&mut self, id: &str, _cx: &CommandContext) -> Result<String, String> {
        Err(format!("this plugin has no command {id:?}"))
    }

    /// Панель `panel` из манифеста → дерево элементов.
    fn render_panel(&mut self, panel: &str, _cx: &PanelContext) -> Result<ui::Element, String> {
        Err(format!("this plugin has no panel {panel:?}"))
    }

    /// Событие элемента панели. После него программа рисует панель заново
    /// ([`Plugin::render_panel`]).
    fn panel_event(&mut self, _panel: &str, _event: &ui::Event) {}
}

/// Запись в журнал программы.
pub fn log(level: Level, message: impl AsRef<str>) {
    #[cfg(target_arch = "wasm32")]
    host::log(level, message.as_ref());
    #[cfg(not(target_arch = "wasm32"))]
    let _ = (level, message);
}

/// Значение своей настройки (`[[contributes.settings]]` манифеста, API 1.1): из
/// `settings.json`, а если там нет или не подходит — по умолчанию. Человек может поменять
/// её в любой момент, поэтому её читают там, где она нужна, а не один раз при запуске.
///
/// ```ignore
/// if !forma_plugin_api::setting::<bool>("terms.hovers").unwrap_or(true) { return None; }
/// ```
pub fn setting<T: serde::de::DeserializeOwned>(key: &str) -> Option<T> {
    #[cfg(target_arch = "wasm32")]
    return host::setting(key).and_then(|json| serde_json::from_str(&json).ok());
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = key;
        None
    }
}

/// Как плагин живёт внутри компонента: один экземпляр на модуль.
#[doc(hidden)]
pub mod rt {
    use std::cell::RefCell;

    use super::{Context, Plugin};

    /// Создаёт плагин по контексту (его даёт [`register_plugin!`](crate::register_plugin)).
    pub type Factory = fn(Context) -> Result<Box<dyn Plugin>, String>;

    thread_local! {
        static FACTORY: RefCell<Option<Factory>> = const { RefCell::new(None) };
        static PLUGIN: RefCell<Option<Box<dyn Plugin>>> = RefCell::new(None);
    }

    pub fn set_factory(f: Factory) {
        FACTORY.with(|c| *c.borrow_mut() = Some(f));
    }

    pub fn activate(context: &str) -> Result<(), String> {
        let cx: Context = serde_json::from_str(context).unwrap_or_default();
        let factory = FACTORY.with(|c| *c.borrow()).ok_or("the plugin did not register itself (register_plugin!)")?;
        let plugin = factory(cx)?;
        PLUGIN.with(|p| *p.borrow_mut() = Some(plugin));
        Ok(())
    }

    pub fn with<R>(f: impl FnOnce(&mut dyn Plugin) -> R) -> Option<R> {
        PLUGIN.with(|p| p.borrow_mut().as_mut().map(|p| f(p.as_mut())))
    }

    /// Панель: событие (если есть) и новое дерево — JSON для программы.
    pub fn panel(panel: &str, event: Option<&str>, context: &str) -> Result<String, String> {
        with(|p| {
            if let Some(e) = event {
                let e = crate::ui::Event::from_json(e)?;
                p.panel_event(panel, &e);
            }
            p.render_panel(panel, &crate::PanelContext::from_json(context)).map(|t| t.to_json())
        })
        .unwrap_or_else(|| Err("the plugin is not activated".into()))
    }
}

/// Регистрирует тип плагина. Вызывается один раз в crate плагина.
#[macro_export]
macro_rules! register_plugin {
    ($plugin:ty) => {
        struct __FormaPluginComponent;

        impl $crate::bindings::Guest for __FormaPluginComponent {
            fn activate(context: String) -> Result<(), String> {
                $crate::rt::set_factory(|cx| <$plugin as $crate::Plugin>::new(cx).map(|p| Box::new(p) as Box<dyn $crate::Plugin>));
                $crate::rt::activate(&context)
            }

            fn changed(paths: Vec<String>) {
                $crate::rt::with(|p| p.changed(&paths));
            }

            fn hover(request: $crate::HoverRequest) -> Option<$crate::HoverResult> {
                $crate::rt::with(|p| p.hover(&request)).flatten()
            }

            fn render_block(lang: String, source: String, context: String) -> Result<String, String> {
                $crate::rt::with(|p| p.render_block(&lang, &source, &context).map(|t| t.to_json())).unwrap_or_else(|| Err("the plugin is not activated".into()))
            }

            fn command(id: String, args: String) -> Result<String, String> {
                $crate::rt::with(|p| p.command(&id, &$crate::CommandContext::from_json(&args))).unwrap_or_else(|| Err("the plugin is not activated".into()))
            }

            fn render_panel(panel: String, context: String) -> Result<String, String> {
                $crate::rt::panel(&panel, None, &context)
            }

            fn panel_event(panel: String, event: String, context: String) -> Result<String, String> {
                $crate::rt::panel(&panel, Some(&event), &context)
            }
        }

        $crate::bindings::export!(__FormaPluginComponent);
    };
}

/// Символы `text` с `start` до `end` (номера символов Unicode, как в [`HoverRequest`]).
pub fn chars(text: &str, start: usize, end: usize) -> String {
    text.chars().skip(start).take(end.saturating_sub(start)).collect()
}
