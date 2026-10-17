//! Манифест плагина — `forma-plugin.toml` в корне папки или архива `.fpl`. Как мета-файл мода
//! Minecraft (`fabric.mod.json`): что это за плагин, для каких программ и какой версии API,
//! от чего зависит, что ему разрешено и что он приносит.
//!
//! ```toml
//! [plugin]
//! id = "myuri-graphics"              # латиница, цифры, дефис и точка; это же имя папки
//! name = "Мьюри: графика"
//! version = "0.1.0"
//! authors = ["Мьюри", "Холо"]
//! description = "Разбор рендеров и сетки для курса по графике"
//! apps = ["holotype"]                # пусто — для всех программ Forma
//! api = "^1.0"                       # какую версию API плагинов понимает
//!
//! [dependencies]
//! myuri = "^0.1"
//! blender = { version = "^0.1", optional = true }
//!
//! [permissions]
//! process = ["claude"]               # какие программы плагин может запускать
//! apps = ["instar"]                  # какие программы Forma может открыть (API 1.3)
//! files = ["vault:read"]             # что читать и писать
//!
//! [contributes]
//! themes = "themes"                  # папка тем (*.toml)
//! blocks = [{ lang = "pipeline", title = "Карта pipeline" }]
//! commands = [{ id = "myuri.review-render", title = "Разобрать рендер" }]
//!
//! [code]
//! wasm = "plugin.wasm"
//! activate = ["command:myuri.review-render"]
//! ```
//!
//! Прежний `plugin.toml` (с `kind` и одной папкой тем) читается по-прежнему.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Имя манифеста.
pub const MANIFEST: &str = "forma-plugin.toml";
/// Имя манифеста прежнего формата.
pub const LEGACY_MANIFEST: &str = "plugin.toml";

/// Версия API плагинов, которую понимает эта платформа.
pub fn api_version() -> semver::Version {
    semver::Version::new(1, 3, 0)
}

/// Вид плагина — категория на странице «Плагины». В новом манифесте её можно не писать:
/// она выводится из вкладов.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// Программа или языковой сервер (jdtls, kotlin-lsp, Python).
    #[default]
    Tool,
    /// Набор тем (со своими картинками).
    Themes,
    /// Набор значков (§6.11 дизайна: только значки, без цветов).
    Icons,
    /// Скрипт (появится вместе с плагином Python).
    Script,
    /// Расширение с кодом: команды, блоки, панели, подсказки.
    Extension,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Meta {
    pub id: String,
    pub name: String,
    pub version: String,
    /// Категория; в новом манифесте — `category`, в прежнем — `kind`.
    #[serde(alias = "category", skip_serializing_if = "Option::is_none")]
    pub kind: Option<Kind>,
    pub description: String,
    /// Автор в прежнем формате.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub author: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub authors: Vec<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub license: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub homepage: String,
    /// Значок: файл внутри плагина (`icon.svg`) или имя значка платформы.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Для каких программ (`holotype`, `instar`, `mycelium`); пусто — для всех.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub apps: Vec<String>,
    /// Какие версии API плагинов понимает (`^1.0`); без этого — любая 1.x.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api: Option<String>,
    /// Когда установлен (секунды Unix).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installed: Option<u64>,
}

/// Зависимость: строка версий (`"^0.1"`) или с пометкой «необязательная».
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Dependency {
    Version(String),
    Detailed {
        version: String,
        #[serde(default)]
        optional: bool,
    },
}

impl Dependency {
    pub fn version(&self) -> &str {
        match self {
            Dependency::Version(v) | Dependency::Detailed { version: v, .. } => v,
        }
    }

    pub fn optional(&self) -> bool {
        matches!(self, Dependency::Detailed { optional: true, .. })
    }
}

/// Что плагину разрешено. Встроенные плагины не спрашивают, остальные — при установке.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Permissions {
    /// Какие программы можно запускать (`claude`, `blender`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub process: Vec<String>,
    /// Доступ к файлам: `vault:read`, `vault:write` (хранилище или проект), `linked:read`
    /// (связанные папки хранилища: практика курса, связанные курсы), `data` (своя папка).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
    /// Какие программы Forma плагин может открыть на проекте или заметке (`instar`,
    /// `holotype`; API 1.3).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub apps: Vec<String>,
}

impl Permissions {
    pub fn is_empty(&self) -> bool {
        self.process.is_empty() && self.files.is_empty() && self.apps.is_empty()
    }
}

/// Новый блок заметок: блок кода с этим языком (` ```pipeline `) рисует плагин.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BlockDecl {
    pub lang: String,
    pub title: String,
}

/// Вставка файла: `![[model.glb]]` с этими расширениями рисует плагин.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EmbedDecl {
    pub extensions: Vec<String>,
    pub title: String,
}

/// Команда: в «Поиске действий», в меню и с сочетанием клавиш.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CommandDecl {
    pub id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

/// Пункт меню: где (`selection`, `note`, `file`) и какая команда.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MenuDecl {
    #[serde(rename = "where")]
    pub place: String,
    pub command: String,
}

/// Панель плагина — вкладка области.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PanelDecl {
    pub id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

/// Подсказки при наведении (на слово в заметке или code).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HoverDecl {
    pub id: String,
    pub title: String,
}

/// Что плагин добавляет в программу (вклады, как `contributes` расширений VS Code).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Contributes {
    /// Папка с темами внутри плагина.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub themes: Option<String>,
    /// Папка набора значков.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icons: Option<String>,
    /// Папка переводов: `<папка>/<язык>/*.ftl`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locales: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<BlockDecl>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub embeds: Vec<EmbedDecl>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub commands: Vec<CommandDecl>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub menus: Vec<MenuDecl>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub panels: Vec<PanelDecl>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hovers: Vec<HoverDecl>,
    /// Настройки плагина (API 1.1): окно настроек строит по ним страницу, значения лежат в
    /// `settings.json` программы, code читает их функцией `setting`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub settings: Vec<forma_settings::schema::Setting>,
    /// Вклады, которых эта версия платформы не знает: плагин ставится, они пропускаются.
    #[serde(flatten)]
    pub unknown: BTreeMap<String, toml::Value>,
}

impl Contributes {
    /// Есть ли что-то, что работает через code плагина.
    pub fn needs_code(&self) -> bool {
        !(self.blocks.is_empty() && self.embeds.is_empty() && self.commands.is_empty() && self.panels.is_empty() && self.hovers.is_empty())
    }
}

/// Code плагина: модуль WebAssembly и события, по которым он запускается.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Code {
    pub wasm: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub activate: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Manifest {
    pub plugin: Meta,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub dependencies: BTreeMap<String, Dependency>,
    #[serde(skip_serializing_if = "Permissions::is_empty")]
    pub permissions: Permissions,
    pub contributes: Contributes,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<Code>,
}

/// Годится ли строка в id плагина: латиница в нижнем регистре, цифры, дефис и точка.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.') && !id.starts_with('.')
}

impl Manifest {
    pub fn parse(text: &str) -> Result<Self, String> {
        let m: Manifest = toml::from_str(text).map_err(|e| e.to_string())?;
        if !valid_id(&m.plugin.id) {
            return Err(format!("bad plugin id {:?}: use lowercase latin letters, digits and dashes", m.plugin.id));
        }
        // Темы и значки — разные плагины (§6.11 дизайна): тему выбирают вместе с цветами, а
        // набор значков — отдельно в настройках, с любой темой.
        if m.contributes.themes.is_some() && m.contributes.icons.is_some() {
            return Err("a plugin cannot bring both themes and icons: split it into a themes plugin and an icons plugin".into());
        }
        for (id, dep) in &m.dependencies {
            if semver::VersionReq::parse(dep.version()).is_err() {
                return Err(format!("dependency {id}: bad version range {:?}", dep.version()));
            }
        }
        if let Some(api) = &m.plugin.api
            && semver::VersionReq::parse(api).is_err()
        {
            return Err(format!("bad api range {api:?}"));
        }
        forma_settings::schema::validate(&m.plugin.id, &m.contributes.settings)?;
        if m.contributes.needs_code() && m.code.is_none() {
            return Err("the plugin brings blocks, commands, panels or hovers but has no [code]".into());
        }
        Ok(m)
    }

    pub fn to_toml(&self) -> String {
        toml::to_string_pretty(self).unwrap_or_default()
    }

    /// Название для людей: имя, а если его нет — id.
    pub fn title(&self) -> &str {
        if self.plugin.name.trim().is_empty() { &self.plugin.id } else { &self.plugin.name }
    }

    /// Категория: указанная, иначе по вкладам.
    pub fn kind(&self) -> Kind {
        if let Some(k) = self.plugin.kind {
            return k;
        }
        let c = &self.contributes;
        if self.code.is_some() {
            Kind::Extension
        } else if c.icons.is_some() {
            Kind::Icons
        } else if c.themes.is_some() {
            Kind::Themes
        } else {
            Kind::Tool
        }
    }

    /// Авторы: новый список, иначе прежний один автор.
    pub fn authors(&self) -> Vec<String> {
        if self.plugin.authors.is_empty() && !self.plugin.author.is_empty() { vec![self.plugin.author.clone()] } else { self.plugin.authors.clone() }
    }

    /// Версия плагина (нестрогая: `1.0` читается как `1.0.0`).
    pub fn version(&self) -> Option<semver::Version> {
        parse_version(&self.plugin.version)
    }

    /// Подходит ли плагин программе `app` (без `apps` — подходит всем).
    pub fn for_app(&self, app: &str) -> bool {
        self.plugin.apps.is_empty() || self.plugin.apps.iter().any(|a| a == app)
    }

    /// Понимает ли плагин API этой платформы (без `api` — любая версия 1.x).
    pub fn api_ok(&self) -> bool {
        let req = self.plugin.api.as_deref().unwrap_or("^1.0");
        semver::VersionReq::parse(req).is_ok_and(|r| r.matches(&api_version()))
    }
}

/// Версия из строки, мягко: `1` и `1.2` дополняются нулями, `latest` — нет версии.
pub fn parse_version(s: &str) -> Option<semver::Version> {
    let s = s.trim();
    semver::Version::parse(s).ok().or_else(|| {
        let parts: Vec<&str> = s.split('.').collect();
        let nums: Option<Vec<u64>> = parts.iter().map(|p| p.parse().ok()).collect();
        match nums?.as_slice() {
            [a] => Some(semver::Version::new(*a, 0, 0)),
            [a, b] => Some(semver::Version::new(*a, *b, 0)),
            _ => None,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_checks_id() {
        let m = Manifest::parse("[plugin]\nid = \"pink-themes\"\nname = \"Pink\"\nkind = \"themes\"\n[contributes]\nthemes = \"themes\"").unwrap();
        assert_eq!(m.kind(), Kind::Themes);
        assert_eq!(m.contributes.themes.as_deref(), Some("themes"));
        assert_eq!(Manifest::parse(&m.to_toml()).unwrap(), m, "запись и чтение сходятся");
        assert!(Manifest::parse("[plugin]\nid = \"../evil\"").is_err());
        assert!(Manifest::parse("[plugin]\nid = \"Big\"").is_err());
        assert!(!valid_id(".hidden"));
    }

    #[test]
    fn new_manifest_with_everything() {
        let text = r#"
[plugin]
id = "myuri-graphics"
name = "Мьюри: графика"
version = "0.1.0"
authors = ["Мьюри", "Холо"]
apps = ["holotype"]
api = "^1.0"

[dependencies]
myuri = "^0.1"
blender = { version = "^0.1", optional = true }

[permissions]
process = ["claude"]
files = ["vault:read"]

[contributes]
blocks = [{ lang = "pipeline", title = "Карта pipeline" }]
menus = [{ where = "selection", command = "myuri.ask" }]
future-thing = ["что-то новое"]

[code]
wasm = "plugin.wasm"
activate = ["block:pipeline"]
"#;
        let m = Manifest::parse(text).unwrap();
        assert_eq!(m.kind(), Kind::Extension);
        assert!(m.for_app("holotype") && !m.for_app("instar"));
        assert!(m.api_ok());
        assert_eq!(m.dependencies["blender"].version(), "^0.1");
        assert!(m.dependencies["blender"].optional() && !m.dependencies["myuri"].optional());
        assert_eq!(m.permissions.process, ["claude"]);
        assert_eq!(m.contributes.menus[0].place, "selection");
        assert!(m.contributes.unknown.contains_key("future-thing"), "незнакомый вклад не ломает манифест");
        assert_eq!(Manifest::parse(&m.to_toml()).unwrap(), m);
    }

    #[test]
    fn rules_of_the_format() {
        let both = "[plugin]\nid = \"x\"\n[contributes]\nthemes = \"t\"\nicons = \"i\"";
        assert!(Manifest::parse(both).unwrap_err().contains("split"), "темы и значки — разные плагины");
        let no_code = "[plugin]\nid = \"x\"\n[contributes]\nblocks = [{ lang = \"a\" }]";
        assert!(Manifest::parse(no_code).is_err(), "блоки рисует code");
        assert!(Manifest::parse("[plugin]\nid = \"x\"\napi = \"не версия\"").is_err());
        let future = Manifest::parse("[plugin]\nid = \"x\"\napi = \"^2.0\"").unwrap();
        assert!(!future.api_ok(), "плагин для API 2 этой платформе не подходит");
        assert_eq!(parse_version("1.2"), Some(semver::Version::new(1, 2, 0)));
        assert_eq!(parse_version("latest"), None);
        assert!(Manifest::parse("[plugin]\nid = \"x\"\napi = \"^1.0\"").unwrap().api_ok(), "плагины для API 1.0 работают и на 1.1");
    }

    #[test]
    fn settings_are_declared_like_vs_code_configuration() {
        let text = r#"
[plugin]
id = "terms"

[[contributes.settings]]
key = "terms.hovers"
type = "boolean"
default = true
title = "Подсказки при наведении"

[[contributes.settings]]
key = "terms.style"
type = "string"
default = "dotted"
enum = [{ value = "dotted", title = "Точки" }, { value = "none", title = "Без линии" }]
"#;
        let m = Manifest::parse(text).unwrap();
        let s = &m.contributes.settings;
        assert_eq!((s[0].key.as_str(), s[0].default.clone()), ("terms.hovers", serde_json::json!(true)));
        assert_eq!(s[1].choices[1].title, "Без линии");
        assert_eq!(Manifest::parse(&m.to_toml()).unwrap(), m, "манифест с настройками пишется и читается");
        let foreign = "[plugin]\nid = \"terms\"\n[[contributes.settings]]\nkey = \"rust.x\"\ntype = \"boolean\"\ndefault = true";
        assert!(Manifest::parse(foreign).is_err(), "чужой ключ");
        let bad_default = "[plugin]\nid = \"terms\"\n[[contributes.settings]]\nkey = \"terms.n\"\ntype = \"integer\"\ndefault = \"x\"";
        assert!(Manifest::parse(bad_default).is_err());
    }
}
