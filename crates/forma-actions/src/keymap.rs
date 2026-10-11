//! Раскладка клавиш: правки пользователя поверх сочетаний по умолчанию.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{Registry, Shortcut};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Keymap {
    /// id действия → его сочетания. Пустой список значит «сочетания убраны».
    pub overrides: BTreeMap<String, Vec<Shortcut>>,
}

impl forma_settings::Component for Keymap {
    const ID: &'static str = "keymap";
}

impl Keymap {
    /// Сочетания действия с учётом правок.
    pub fn shortcuts<'a>(&'a self, registry: &'a Registry, id: &str) -> &'a [Shortcut] {
        match self.overrides.get(id) {
            Some(keys) => keys,
            None => registry.get(id).map(|a| a.defaults.as_slice()).unwrap_or(&[]),
        }
    }

    /// Первое сочетание действия текстом — для подписи в меню. Пусто, если сочетаний нет.
    pub fn text(&self, registry: &Registry, id: &str) -> String {
        self.shortcuts(registry, id).first().map(ToString::to_string).unwrap_or_default()
    }

    pub fn is_modified(&self, id: &str) -> bool {
        self.overrides.contains_key(id)
    }

    /// Задаёт сочетания действия. Совпадающие с сочетаниями по умолчанию правкой не считаются.
    pub fn set(&mut self, registry: &Registry, id: &str, keys: Vec<Shortcut>) {
        let defaults = registry.get(id).map(|a| a.defaults.as_slice()).unwrap_or(&[]);
        if keys == defaults {
            self.overrides.remove(id);
        } else {
            self.overrides.insert(id.to_owned(), keys);
        }
    }

    pub fn add(&mut self, registry: &Registry, id: &str, key: Shortcut) {
        let mut keys = self.shortcuts(registry, id).to_vec();
        if !keys.contains(&key) {
            keys.push(key);
        }
        self.set(registry, id, keys);
    }

    pub fn remove(&mut self, registry: &Registry, id: &str, key: Shortcut) {
        let keys: Vec<Shortcut> = self.shortcuts(registry, id).iter().copied().filter(|k| *k != key).collect();
        self.set(registry, id, keys);
    }

    pub fn reset(&mut self, id: &str) {
        self.overrides.remove(id);
    }

    /// Какие ещё действия заняли это сочетание.
    pub fn conflicts(&self, registry: &Registry, key: Shortcut, except: &str) -> Vec<String> {
        registry.iter().filter(|a| a.id != except && self.shortcuts(registry, &a.id).contains(&key)).map(|a| a.id.clone()).collect()
    }

    /// Убирает правки для действий, которых в программе нет (после импорта
    /// раскладки из другой программы Forma). Возвращает их id.
    pub fn retain_known(&mut self, registry: &Registry) -> Vec<String> {
        let unknown: Vec<String> = self.overrides.keys().filter(|id| registry.get(id).is_none()).cloned().collect();
        for id in &unknown {
            self.overrides.remove(id);
        }
        unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Action, Context};

    fn registry() -> Registry {
        let mut r = Registry::default();
        r.register(Action::new("file.save", "g-file", "save", Context::Global, &["Ctrl+S"]));
        r.register(Action::new("run.start", "g-run", "run", Context::Global, &["Shift+F10"]));
        r.register(Action::new("editor.complete", "g-editor", "complete", Context::Editor, &["Ctrl+Space"]));
        r
    }

    #[test]
    fn overrides_sit_on_top_of_defaults() {
        let r = registry();
        let mut k = Keymap::default();
        assert_eq!(k.text(&r, "file.save"), "Ctrl+S");
        k.add(&r, "file.save", Shortcut::parse("Ctrl+Alt+S").unwrap());
        assert_eq!(k.shortcuts(&r, "file.save").len(), 2);
        assert!(k.is_modified("file.save"));
        // Вернули как было — правка исчезает сама.
        k.remove(&r, "file.save", Shortcut::parse("Ctrl+Alt+S").unwrap());
        assert!(!k.is_modified("file.save"));
        // Все сочетания убраны — это тоже правка.
        k.remove(&r, "run.start", Shortcut::parse("Shift+F10").unwrap());
        assert!(k.shortcuts(&r, "run.start").is_empty());
        assert!(k.is_modified("run.start"));
        k.reset("run.start");
        assert_eq!(k.text(&r, "run.start"), "Shift+F10");
    }

    #[test]
    fn finds_conflicts_and_drops_unknown_actions() {
        let r = registry();
        let mut k = Keymap::default();
        let key = Shortcut::parse("Ctrl+S").unwrap();
        k.add(&r, "run.start", key);
        assert_eq!(k.conflicts(&r, key, "run.start"), vec!["file.save".to_owned()]);
        k.overrides.insert("holotype.note.new".into(), vec![Shortcut::parse("Ctrl+N").unwrap()]);
        assert_eq!(k.retain_known(&r), vec!["holotype.note.new".to_owned()]);
        assert!(k.is_modified("run.start"));
    }
}
