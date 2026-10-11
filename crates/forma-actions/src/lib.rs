//! Действия и раскладка клавиш платформы Forma, как в JetBrains.
//!
//! Каждая команда программы — [`Action`]: постоянный id, название (ключ перевода),
//! группа для дерева в настройках и сочетания по умолчанию. Программа и плагины
//! регистрируют действия в [`Registry`]. [`Keymap`] хранит только правки пользователя
//! поверх сочетаний по умолчанию, поэтому новые действия и новые стандартные сочетания
//! подхватываются сами, а экспорт раскладки остаётся маленьким.

mod keymap;
mod shortcut;

pub use keymap::Keymap;
pub use shortcut::Shortcut;

/// Где действие работает. Сочетания действий редактора ловит сам редактор,
/// пока он в фокусе; общие действия работают везде.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Context {
    Global,
    Editor,
    /// Сочетание ловит раскладка областей: действие зависит от области под курсором.
    Layout,
}

/// Название действия: ключ перевода или готовый текст (у плагинов).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Title {
    Key(String),
    Text(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Action {
    /// Постоянный id: `run.start`, `file.settings`. По нему хранятся правки раскладки.
    pub id: String,
    /// Группа в дереве раскладки (ключ перевода): `action-group-run`.
    pub group: String,
    pub title: Title,
    pub defaults: Vec<Shortcut>,
    pub context: Context,
}

impl Action {
    pub fn new(id: &str, group: &str, title_key: &str, context: Context, defaults: &[&str]) -> Self {
        Self {
            id: id.to_owned(),
            group: group.to_owned(),
            title: Title::Key(title_key.to_owned()),
            defaults: defaults.iter().map(|s| Shortcut::parse(s).unwrap_or_else(|| panic!("неверное сочетание «{s}» у действия {id}"))).collect(),
            context,
        }
    }
}

/// Все действия программы в порядке регистрации.
#[derive(Clone, Debug, Default)]
pub struct Registry {
    actions: Vec<Action>,
}

impl Registry {
    pub fn register(&mut self, action: Action) {
        debug_assert!(self.get(&action.id).is_none(), "действие {} зарегистрировано дважды", action.id);
        self.actions.push(action);
    }

    pub fn get(&self, id: &str) -> Option<&Action> {
        self.actions.iter().find(|a| a.id == id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Action> {
        self.actions.iter()
    }

    /// Группы в порядке первого появления.
    pub fn groups(&self) -> Vec<&str> {
        let mut out: Vec<&str> = Vec::new();
        for a in &self.actions {
            if !out.contains(&a.group.as_str()) {
                out.push(&a.group);
            }
        }
        out
    }
}

/// Какие действия контекста сработали в этом кадре. Их сочетания съедаются,
/// чтобы поле ввода или редактор не увидели их ещё раз.
pub fn triggered(input: &mut egui::InputState, registry: &Registry, keymap: &Keymap, context: Context) -> Vec<String> {
    let mut out = Vec::new();
    for action in registry.iter().filter(|a| a.context == context) {
        if keymap.shortcuts(registry, &action.id).iter().any(|s| s.consume(input)) {
            out.push(action.id.clone());
        }
    }
    out
}

/// Сработало ли конкретное действие (для редакторов, которые ловят свои сочетания сами).
pub fn consume(input: &mut egui::InputState, registry: &Registry, keymap: &Keymap, id: &str) -> bool {
    keymap.shortcuts(registry, id).iter().any(|s| s.consume(input))
}
