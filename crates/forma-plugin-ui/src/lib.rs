//! Дерево элементов интерфейса плагинов Forma.
//!
//! Плагин не рисует egui сам: он описывает, что показать, деревом готовых элементов, а
//! рисует программа — так все плагины выглядят родными и следуют теме. Это та же мысль,
//! что у `UILayout` аддонов Blender (`row`, `column`, `box`, `label`, `operator`, `prop`)
//! и у Kotlin UI DSL в IntelliJ (`panel`, `row`, `group`).
//!
//! Дерево ходит между плагином и программой JSON-строкой (`render-block`, `render-panel`):
//!
//! ```json
//! {"type": "column", "children": [
//!   {"type": "heading", "text": "Глоссарий"},
//!   {"type": "text-input", "id": "query", "value": "", "placeholder": "Найти term"},
//!   {"type": "list", "id": "terms", "items": [{"id": "uv", "text": "UV"}]}
//! ]}
//! ```
//!
//! Элемент незнакомого вида (из будущей версии API) программа не роняет, а показывает
//! пометкой «не поддерживается» — так новые плагины не ломают старые программы.
//!
//! Что пользователь сделал с элементами (нажал, ввёл, выбрал), программа сообщает плагину
//! событиями [`Event`]; кнопка с `command` вместо события запускает команду плагина.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Оттенок текста или значка: обычный, приглушённый, акцентный, «хорошо» и так далее.
/// Цвета берутся из темы программы.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Tone {
    #[default]
    Normal,
    Muted,
    Faint,
    Accent,
    Success,
    Warning,
    Danger,
}

/// Размер текста.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Size {
    Small,
    #[default]
    Normal,
    Large,
}

fn yes() -> bool {
    true
}

fn is_true(b: &bool) -> bool {
    *b
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// Вариант выпадающего списка.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Choice {
    pub value: String,
    pub text: String,
}

/// Строка списка.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Item {
    pub id: String,
    pub text: String,
    /// Приглушённая строка под текстом.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub detail: String,
    /// Значок (имя из набора значков Forma).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Пометка справа.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub badge: String,
    #[serde(skip_serializing_if = "is_normal")]
    pub tone: Tone,
}

fn is_normal(t: &Tone) -> bool {
    *t == Tone::Normal
}

/// Элемент интерфейса.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Element {
    /// Элементы сверху вниз.
    Column {
        #[serde(default)]
        children: Vec<Element>,
        /// Зазор между элементами (в точках); пусто — обычный.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        gap: Option<f32>,
    },
    /// Элементы слева направо; не помещаются — переносятся на следующую строку.
    Row {
        #[serde(default)]
        children: Vec<Element>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        gap: Option<f32>,
    },
    /// Группа в рамке с заголовком, как панель в Blender; `collapsible` — сворачивается.
    Group {
        #[serde(default, skip_serializing_if = "String::is_empty")]
        title: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        icon: Option<String>,
        #[serde(default)]
        children: Vec<Element>,
        #[serde(default, skip_serializing_if = "is_false")]
        collapsible: bool,
        /// Открыта ли свёрнутая группа в первый раз (дальше её помнит программа).
        #[serde(default = "yes", skip_serializing_if = "is_true")]
        open: bool,
    },
    Separator,
    /// Пустое место по вертикали.
    Space {
        #[serde(default)]
        size: f32,
    },
    Label {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        icon: Option<String>,
        #[serde(default, skip_serializing_if = "is_normal")]
        tone: Tone,
        #[serde(default, skip_serializing_if = "is_normal_size")]
        size: Size,
        #[serde(default, skip_serializing_if = "is_false")]
        bold: bool,
    },
    /// Заголовок: 1 — крупный, 3 — как у группы.
    Heading {
        text: String,
        #[serde(default = "two")]
        level: u8,
    },
    /// Markdown — его рисует тот же renderer, что и заметки (ссылки, code, плашки).
    Markdown { text: String },
    /// Code с подсветкой.
    Code {
        text: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        lang: String,
    },
    /// Небольшая плашка с текстом.
    Badge {
        text: String,
        #[serde(default, skip_serializing_if = "is_normal")]
        tone: Tone,
    },
    /// Кнопка: без `command` сообщает плагину событие `click`, с `command` — запускает
    /// команду (с `args`).
    Button {
        #[serde(default, skip_serializing_if = "String::is_empty")]
        id: String,
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        icon: Option<String>,
        #[serde(default, skip_serializing_if = "is_false")]
        primary: bool,
        #[serde(default = "yes", skip_serializing_if = "is_true")]
        enabled: bool,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        tooltip: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        command: Option<String>,
        #[serde(default, skip_serializing_if = "Value::is_null")]
        args: Value,
    },
    /// Поле ввода: событие `change` на каждую правку, `submit` — по Enter.
    TextInput {
        id: String,
        #[serde(default)]
        value: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        placeholder: String,
        #[serde(default, skip_serializing_if = "is_false")]
        multiline: bool,
        /// Значок лупы слева: поле поиска.
        #[serde(default, skip_serializing_if = "is_false")]
        search: bool,
    },
    /// Галочка: событие `change` с `true` или `false`.
    Checkbox {
        id: String,
        text: String,
        #[serde(default)]
        value: bool,
    },
    /// Выпадающий список: событие `change` со значением варианта.
    Select {
        id: String,
        #[serde(default)]
        value: String,
        #[serde(default)]
        options: Vec<Choice>,
    },
    /// Число, как поля Blender: тянешь — меняется, щёлкнешь — вводишь. Событие `change`.
    Number {
        id: String,
        value: f64,
        #[serde(default)]
        min: f64,
        #[serde(default = "hundred")]
        max: f64,
        #[serde(default = "one")]
        step: f64,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        unit: String,
    },
    /// Список строк: событие `select` с id строки, двойной щелчок — `activate`.
    List {
        id: String,
        #[serde(default)]
        items: Vec<Item>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        selected: Option<String>,
    },
    Table {
        #[serde(default)]
        columns: Vec<String>,
        #[serde(default)]
        rows: Vec<Vec<String>>,
    },
    /// Полоса хода: `value` от 0 до 1, пусто — «идёт, сколько осталось — неизвестно».
    Progress {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        value: Option<f32>,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        text: String,
    },
    /// Картинка из хранилища или проекта (путь от корня).
    Image {
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        width: Option<f32>,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        alt: String,
    },
    /// Элемент из более новой версии API: программа показывает пометку вместо него.
    #[serde(other)]
    Unknown,
}

fn two() -> u8 {
    2
}

fn one() -> f64 {
    1.0
}

fn hundred() -> f64 {
    100.0
}

fn is_normal_size(s: &Size) -> bool {
    *s == Size::Normal
}

impl Element {
    /// Разбирает дерево из JSON.
    pub fn from_json(json: &str) -> Result<Element, String> {
        serde_json::from_str(json).map_err(|e| e.to_string())
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

/// Что пользователь сделал с элементом.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EventKind {
    /// Нажата кнопка.
    Click,
    /// Изменено значение (поле, галочка, список, число).
    Change,
    /// Enter в поле ввода.
    Submit,
    /// Выбрана строка списка.
    Select,
    /// Двойной щелчок по строке списка.
    Activate,
}

/// Событие элемента `id`: что произошло и новое значение (строка, число, `true`/`false`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    pub kind: EventKind,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub value: Value,
}

impl Event {
    pub fn from_json(json: &str) -> Result<Event, String> {
        serde_json::from_str(json).map_err(|e| e.to_string())
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// Значение строкой (для полей ввода, списков, выпадающих списков).
    pub fn text(&self) -> &str {
        self.value.as_str().unwrap_or_default()
    }
}

// --------------------------------------------------------------------------------------------
// Короткие конструкторы — чтобы дерево в плагине читалось, как разметка.
// --------------------------------------------------------------------------------------------

pub fn column(children: impl IntoIterator<Item = Element>) -> Element {
    Element::Column { children: children.into_iter().collect(), gap: None }
}

pub fn row(children: impl IntoIterator<Item = Element>) -> Element {
    Element::Row { children: children.into_iter().collect(), gap: None }
}

pub fn group(title: impl Into<String>, children: impl IntoIterator<Item = Element>) -> Element {
    Element::Group { title: title.into(), icon: None, children: children.into_iter().collect(), collapsible: false, open: true }
}

pub fn label(text: impl Into<String>) -> Element {
    Element::Label { text: text.into(), icon: None, tone: Tone::Normal, size: Size::Normal, bold: false }
}

/// Подпись приглушённым цветом.
pub fn hint(text: impl Into<String>) -> Element {
    Element::Label { text: text.into(), icon: None, tone: Tone::Muted, size: Size::Small, bold: false }
}

pub fn heading(text: impl Into<String>) -> Element {
    Element::Heading { text: text.into(), level: 2 }
}

pub fn markdown(text: impl Into<String>) -> Element {
    Element::Markdown { text: text.into() }
}

pub fn code(text: impl Into<String>, lang: impl Into<String>) -> Element {
    Element::Code { text: text.into(), lang: lang.into() }
}

pub fn button(id: impl Into<String>, text: impl Into<String>) -> Element {
    Element::Button { id: id.into(), text: text.into(), icon: None, primary: false, enabled: true, tooltip: String::new(), command: None, args: Value::Null }
}

/// Кнопка, которая запускает команду плагина.
pub fn command_button(text: impl Into<String>, command: impl Into<String>, args: Value) -> Element {
    Element::Button { id: String::new(), text: text.into(), icon: None, primary: false, enabled: true, tooltip: String::new(), command: Some(command.into()), args }
}

pub fn text_input(id: impl Into<String>, value: impl Into<String>, placeholder: impl Into<String>) -> Element {
    Element::TextInput { id: id.into(), value: value.into(), placeholder: placeholder.into(), multiline: false, search: false }
}

/// Поле поиска (с лупой).
pub fn search(id: impl Into<String>, value: impl Into<String>, placeholder: impl Into<String>) -> Element {
    Element::TextInput { id: id.into(), value: value.into(), placeholder: placeholder.into(), multiline: false, search: true }
}

pub fn checkbox(id: impl Into<String>, text: impl Into<String>, value: bool) -> Element {
    Element::Checkbox { id: id.into(), text: text.into(), value }
}

pub fn list(id: impl Into<String>, items: impl IntoIterator<Item = Item>, selected: Option<String>) -> Element {
    Element::List { id: id.into(), items: items.into_iter().collect(), selected }
}

pub fn item(id: impl Into<String>, text: impl Into<String>) -> Item {
    Item { id: id.into(), text: text.into(), ..Default::default() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_goes_through_json_both_ways() {
        let tree = column([
            heading("Глоссарий"),
            search("query", "uv", "Найти term"),
            list("terms", [Item { detail: "развёртка".into(), ..item("uv", "UV") }], Some("uv".into())),
            row([button("more", "Ещё"), command_button("Найти", "terms.find", serde_json::json!({"q": 1}))]),
            Element::Group { title: "Свёрнуто".into(), icon: None, children: vec![markdown("**жирный**")], collapsible: true, open: false },
        ]);
        let json = tree.to_json();
        assert!(json.contains(r#""type":"text-input""#) && json.contains(r#""search":true"#), "{json}");
        assert!(!json.contains("enabled"), "значения по умолчанию не пишутся: {json}");
        assert_eq!(Element::from_json(&json).unwrap(), tree);
    }

    #[test]
    fn short_json_fills_defaults_and_unknown_elements_survive() {
        let tree = Element::from_json(r#"{"type":"column","children":[{"type":"button","text":"Ок"},{"type":"hologram","depth":3},{"type":"group","collapsible":true}]}"#).unwrap();
        let Element::Column { children, .. } = tree else { panic!() };
        assert!(matches!(&children[0], Element::Button { enabled: true, primary: false, command: None, .. }));
        assert_eq!(children[1], Element::Unknown, "незнакомый элемент не роняет дерево");
        assert!(matches!(&children[2], Element::Group { open: true, collapsible: true, .. }));
    }

    #[test]
    fn events_read_their_values() {
        let e = Event::from_json(r#"{"id":"query","kind":"change","value":"edge"}"#).unwrap();
        assert_eq!((e.kind, e.text()), (EventKind::Change, "edge"));
        let click = Event { id: "more".into(), kind: EventKind::Click, value: Value::Null };
        assert_eq!(click.to_json(), r#"{"id":"more","kind":"click"}"#);
    }
}
