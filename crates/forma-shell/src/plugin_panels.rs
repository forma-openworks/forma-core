//! Панели плагинов в программе — одинаково во всех программах Forma: панель — вид редактора
//! области (как типы редакторов от аддонов в Blender и окна инструментов в IDE от
//! JetBrains), плагин описывает её деревом элементов, программа рисует его своими виджетами.
//!
//! Дерево плагина спрашивают, только когда нужно: панель открыли, плагин попросил
//! перерисовать, событие элемента, другая заметка или файл, другой набор плагинов
//! ([`Runtime::generation`]). Пока нового дерева нет, на экране остаётся старое — прокрутка
//! и свёрнутые группы не сбрасываются.
//!
//! Здесь же списки пунктов меню плагинов и то, что получает их команда ([`CommandCx`]).

use std::collections::{HashMap, HashSet};
use std::path::Path;

use eframe::egui::{self, Frame, Id, Margin, RichText, ScrollArea, Ui};
use forma_i18n::t;
use forma_plugin_host::{CommandInfo, PanelInfo, Runtime};
use forma_plugin_ui::{Element, Event};
use forma_ui::elements::{Hooks, Output};
use forma_ui::theme::Palette;
use serde::Serialize;
use serde_json::Value;

/// Ответ плагина: дерево элементов или почему его нет.
pub enum Tree {
    Ready(Element),
    Failed(String),
}

impl Tree {
    /// Ответ рантайма: JSON дерева или ошибка плагина.
    pub fn from_answer(answer: Result<String, String>) -> Tree {
        match answer.and_then(|json| Element::from_json(&json)) {
            Ok(e) => Tree::Ready(e),
            Err(e) => Tree::Failed(e),
        }
    }
}

/// Панели и пункты меню работающих плагинов (обновляются каждый кадр) и нарисованные деревья
/// панелей.
#[derive(Default)]
pub struct PluginPanels {
    /// Поколение плагинов, для которого нарисованы деревья.
    generation: u64,
    pub panels: Vec<PanelInfo>,
    /// Пункты меню выделенного текста и всей заметки (файла).
    pub selection_menu: Vec<CommandInfo>,
    pub note_menu: Vec<CommandInfo>,
    trees: HashMap<String, Tree>,
    /// Панели, которые плагин нарисует заново; пока нового дерева нет, видно старое.
    stale: HashSet<String>,
}

impl PluginPanels {
    /// Берёт вклады работающих плагинов. `true` — сменилось поколение: свои ответы плагинов
    /// (блоки заметок) программе пора забыть.
    pub fn sync(&mut self, rt: &Runtime) -> bool {
        let changed = rt.generation() != self.generation;
        if changed {
            self.generation = rt.generation();
            self.refresh_all();
        }
        self.panels = rt.panels();
        self.selection_menu = rt.menu("selection");
        self.note_menu = rt.menu("note");
        changed
    }

    pub fn panel(&self, key: &str) -> Option<&PanelInfo> {
        self.panels.iter().find(|p| p.key() == key)
    }

    /// Панель `key` — нарисовать заново (старое дерево видно, пока не придёт новое).
    pub fn refresh(&mut self, key: &str) {
        self.stale.insert(key.to_owned());
    }

    /// Все нарисованные панели — заново (другая заметка или файл).
    pub fn refresh_all(&mut self) {
        self.stale.extend(self.trees.keys().cloned());
    }

    /// Панель надо (пере)рисовать: дерева нет или оно устарело.
    pub fn needs_render(&self, key: &str) -> bool {
        !self.trees.contains_key(key) || self.stale.contains(key)
    }

    /// Спросить плагин: нарисовать панель `key` (если надо) или передать событие элемента.
    /// `context` — JSON для плагина (`note`, `dark`), `plugins` — откуда взять название
    /// плагина для сообщения об ошибке.
    pub fn render(&mut self, ctx: &egui::Context, rt: &mut Runtime, plugins: &forma_plugins::Plugins, key: &str, event: Option<&Event>, context: &str) {
        if event.is_none() && !self.needs_render(key) {
            return;
        }
        let Some((plugin, panel)) = split_key(key) else { return };
        let answer = match event {
            Some(e) => rt.panel_event(plugin, panel, &e.to_json(), context),
            None => rt.render_panel(plugin, panel, context),
        };
        match answer {
            Some(answer) => {
                self.trees.insert(key.to_owned(), Tree::from_answer(answer));
                self.stale.remove(key);
                ctx.request_repaint();
            }
            None => match rt.failure(plugin) {
                Some(error) => {
                    let name = plugins.get(plugin).map(|p| p.manifest.title().to_owned()).unwrap_or_else(|| plugin.to_owned());
                    self.trees.insert(key.to_owned(), Tree::Failed(t!("plugin-panel-stopped", plugin = name, error = error)));
                    self.stale.remove(key);
                }
                // Плагин ещё компилируется — спросить снова чуть позже.
                None => ctx.request_repaint_after_secs(0.1),
            },
        }
    }

    /// Рисует панель `key` в области: дерево элементов, ошибку или «Плагин готовится…».
    /// Возвращает, что пользователь сделал с элементами. Есть ли такая панель вообще,
    /// программа проверяет сама ([`Self::panel`]) — у неё своя заглушка.
    pub fn show(&self, ui: &mut Ui, pal: &Palette, key: &str, scroll: Id, hooks: &mut dyn Hooks) -> Vec<Output> {
        let mut out = Vec::new();
        ScrollArea::vertical().id_salt(scroll).auto_shrink([false, false]).show(ui, |ui| {
            Frame::new().inner_margin(Margin::same(12)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                match self.trees.get(key) {
                    Some(Tree::Ready(tree)) => out = forma_ui::elements::show(ui, pal, Id::new(("plugin-panel", key)), tree, hooks),
                    Some(Tree::Failed(error)) => {
                        ui.add(egui::Label::new(RichText::new(t!("plugin-panel-failed", error = error.clone())).size(13.0).color(pal.warning)).wrap());
                    }
                    None => {
                        ui.horizontal(|ui| {
                            ui.add(egui::Spinner::new().size(14.0).color(pal.accent));
                            ui.label(RichText::new(t!("plugin-panel-loading")).size(13.0).color(pal.text_muted));
                        });
                    }
                }
            });
        });
        out
    }
}

/// `плагин/панель` → (плагин, панель).
pub fn split_key(key: &str) -> Option<(&str, &str)> {
    key.split_once('/')
}

/// Где и с чем вызвали команду плагина — то, что SDK читает как `CommandContext`.
#[derive(Clone, Debug, Default, Serialize)]
pub struct CommandCx {
    /// `menu`, `key`, `block`, `panel`.
    pub from: String,
    /// Заметка или файл: путь от корня хранилища или проекта, через `/`.
    pub note: String,
    pub selection: String,
    pub heading: String,
    pub paragraph: String,
    #[serde(skip_serializing_if = "Value::is_null")]
    pub args: Value,
}

impl CommandCx {
    pub fn new(from: &str, note: Option<&Path>) -> Self {
        Self { from: from.to_owned(), note: note.map(note_path).unwrap_or_default(), ..Default::default() }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

/// Путь заметки или файла для плагина: от корня, через `/`.
pub fn note_path(rel: &Path) -> String {
    rel.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Устаревшее дерево остаётся, пока не придёт новое; перерисовать надо и новую панель.
    #[test]
    fn stale_trees_stay_until_replaced() {
        let mut p = PluginPanels::default();
        assert!(p.needs_render("a/b"));
        p.trees.insert("a/b".into(), Tree::Ready(forma_plugin_ui::label("x")));
        assert!(!p.needs_render("a/b"));
        p.refresh_all();
        assert!(p.needs_render("a/b") && p.trees.contains_key("a/b"), "старое дерево на месте, но его пора перерисовать");
        assert_eq!(CommandCx::new("menu", Some(Path::new("src\\main.rs"))).note, "src/main.rs");
        assert_eq!(split_key("terms/glossary"), Some(("terms", "glossary")));
    }
}
