//! Локализация интерфейса программ Forma на Fluent.
//!
//! Строки самой платформы лежат в `locales/<язык>/*.ftl` этого крейта и вшиваются в него.
//! Каждая программа (и общие крейты со своими строками, вроде оболочки) вшивает свои
//! переводы сборочным скриптом (`forma_build::locales`) и передаёт их в [`init_with`].
//! Дополнительные языки можно положить в папку настроек (`locales/<язык>/*.ftl`): они
//! подхватываются при запуске без пересборки и могут переопределять встроенные строки.
//!
//! В коде интерфейса строки берутся только через [`t!`]:
//!
//! ```ignore
//! t!("menu-file");
//! t!("exceptions-count", count = 5);
//! ```

use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use std::sync::{LazyLock, Mutex, RwLock};

use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::FluentResource;
use unic_langid::LanguageIdentifier;

pub use fluent_bundle::{FluentArgs, FluentValue};

/// Встроенные переводы: код языка и его файлы `.ftl` (имя, содержимое). Такую таблицу
/// пишет `forma_build::locales`.
pub type Locales = &'static [(&'static str, &'static [(&'static str, &'static str)])];

// Строки платформы: `pub static PLATFORM`.
include!(concat!(env!("OUT_DIR"), "/locales.rs"));

/// Переводы программы, переданные в [`init_with`] (строки платформы к ним добавляются всегда).
static SOURCES: RwLock<Vec<Locales>> = RwLock::new(Vec::new());

/// Все встроенные переводы: сначала платформа, потом то, что передала программа.
fn sources() -> Vec<Locales> {
    let extra = SOURCES.read().unwrap_or_else(|e| e.into_inner());
    std::iter::once(PLATFORM as Locales).chain(extra.iter().copied()).collect()
}

/// Файлы языка `code` во всех наборах.
fn files_of<'a>(sources: &'a [Locales], code: &'a str) -> impl Iterator<Item = &'static (&'static str, &'static str)> + 'a {
    sources.iter().flat_map(move |set| set.iter().filter(move |(c, _)| *c == code).flat_map(|(_, files)| files.iter()))
}

/// Язык, к которому откатываемся, если строки нет в выбранном.
pub const FALLBACK: &str = "en";

/// Строка в текущем языке интерфейса.
///
/// `t!("ключ")` — простая строка, `t!("ключ", count = n, name = s)` — с аргументами.
#[macro_export]
macro_rules! t {
    ($key:expr) => {
        $crate::tr($key)
    };
    ($key:expr, $($name:ident = $value:expr),+ $(,)?) => {{
        let mut args = $crate::FluentArgs::new();
        $( args.set(stringify!($name), $value); )+
        $crate::tr_args($key, &args)
    }};
}

struct State {
    current: String,
    bundles: BTreeMap<String, FluentBundle<FluentResource>>,
}

static STATE: LazyLock<RwLock<State>> = LazyLock::new(|| RwLock::new(load(None)));

/// Ключи, о которых уже предупреждали в логе (чтобы не сыпать одно и то же каждый кадр).
static WARNED: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(Default::default);

fn new_bundle(code: &str) -> FluentBundle<FluentResource> {
    let id: LanguageIdentifier = code.parse().unwrap_or_else(|_| FALLBACK.parse().expect("en"));
    let mut bundle = FluentBundle::new_concurrent(vec![id]);
    // Без невидимых символов изоляции вокруг подстановок: egui рисовал бы их квадратиками.
    bundle.set_use_isolating(false);
    bundle
}

fn add_source(bundle: &mut FluentBundle<FluentResource>, code: &str, name: &str, source: String) {
    let resource = match FluentResource::try_new(source) {
        Ok(r) => r,
        Err((r, errors)) => {
            log::warn!("Errors in locale file {code}/{name}: {errors:?}");
            r
        }
    };
    bundle.add_resource_overriding(resource);
}

/// Смешанный язык: интерфейс и термины одного языка, подсказки — другого.
struct Blend {
    code: &'static str,
    /// Самоназвание в списке языков.
    name: &'static str,
    /// Язык интерфейса и терминов.
    base: &'static str,
    /// Язык подсказок.
    hints: &'static str,
}

/// Английский интерфейс с русскими подсказками: меню, кнопки и термины — как в
/// англоязычных руководствах по моддингу, а объяснения — по-русски.
const BLENDS: &[Blend] = &[Blend { code: "en-RU", name: "English · русские подсказки", base: "en", hints: "ru" }];

/// Подсказка ли это, а не элемент интерфейса или термин: пояснения, описания окон,
/// пустые состояния, подсказки в полях ввода, объяснения известных ошибок.
pub fn is_hint(key: &str) -> bool {
    const SUFFIXES: &[&str] = &["-hint", "-about", "-explain", "-description", "-tagline"];
    SUFFIXES.iter().any(|s| key.ends_with(s)) || key.starts_with("settings-component-")
}

/// Сообщения-подсказки из исходника `.ftl`: каждое сообщение — строка `id = …`
/// и строки продолжения с отступом.
fn hint_messages(source: &str) -> String {
    let mut out = String::new();
    let mut take = false;
    for line in source.lines() {
        let continues = line.starts_with([' ', '\t']) || (take && line.trim().is_empty());
        if !continues {
            take = line.split_once(" =").is_some_and(|(id, _)| is_hint(id.trim()));
        }
        if take {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

fn load(extra: Option<&Path>) -> State {
    let sources = sources();
    let mut bundles: BTreeMap<String, FluentBundle<FluentResource>> = BTreeMap::new();
    for set in &sources {
        for (code, files) in *set {
            let bundle = bundles.entry((*code).to_owned()).or_insert_with(|| new_bundle(code));
            for (name, source) in *files {
                add_source(bundle, code, name, (*source).to_owned());
            }
        }
    }
    for blend in BLENDS {
        let mut bundle = new_bundle(blend.base);
        for (name, source) in files_of(&sources, blend.base) {
            add_source(&mut bundle, blend.code, name, (*source).to_owned());
        }
        for (name, source) in files_of(&sources, blend.hints) {
            add_source(&mut bundle, blend.code, name, hint_messages(source));
        }
        add_source(&mut bundle, blend.code, "name", format!("language-name = {}\n", blend.name));
        bundles.insert(blend.code.to_owned(), bundle);
    }
    if let Some(dir) = extra {
        load_extra(&mut bundles, dir);
    }
    State { current: FALLBACK.to_owned(), bundles }
}

/// Пользовательские переводы: `<папка>/<язык>/*.ftl`.
fn load_extra(bundles: &mut BTreeMap<String, FluentBundle<FluentResource>>, dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let code = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
        if code.parse::<LanguageIdentifier>().is_err() {
            log::warn!("Skipping locale folder with invalid language code: {}", path.display());
            continue;
        }
        let Ok(files) = std::fs::read_dir(&path) else { continue };
        let mut files: Vec<_> = files.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "ftl")).collect();
        files.sort();
        let bundle = bundles.entry(code.clone()).or_insert_with(|| new_bundle(&code));
        for file in files {
            match std::fs::read_to_string(&file) {
                Ok(source) => {
                    let name = file.file_name().unwrap_or_default().to_string_lossy().into_owned();
                    add_source(bundle, &code, &name, source);
                }
                Err(err) => log::warn!("Failed to read locale file {}: {err}", file.display()),
            }
        }
    }
}

/// Загружает переводы программы: строки платформы, встроенные переводы `sources` (программы
/// и общих крейтов со своими строками) и пользовательские из папки `extra`.
/// Выбранный язык сохраняется.
pub fn init_with(sources: &[Locales], extra: Option<&Path>) {
    *SOURCES.write().unwrap_or_else(|e| e.into_inner()) = sources.to_vec();
    init(extra);
}

/// Загружает переводы заново: встроенные (платформы и переданные в [`init_with`]) и (если
/// указана папка) пользовательские. Выбранный язык сохраняется.
pub fn init(extra: Option<&Path>) {
    let mut state = STATE.write().unwrap_or_else(|e| e.into_inner());
    let current = std::mem::take(&mut state.current);
    *state = load(extra);
    if state.bundles.contains_key(&current) {
        state.current = current;
    }
}

/// Язык системы, если для него есть перевод; иначе [`FALLBACK`].
pub fn system_language() -> String {
    let available = available_codes();
    sys_locale::get_locales()
        .filter_map(|l| l.parse::<LanguageIdentifier>().ok())
        .map(|id| id.language.as_str().to_owned())
        .find(|code| available.contains(code))
        .unwrap_or_else(|| FALLBACK.to_owned())
}

fn available_codes() -> Vec<String> {
    STATE.read().unwrap_or_else(|e| e.into_inner()).bundles.keys().cloned().collect()
}

/// Доступные языки: код и самоназвание (строка `language-name` из перевода).
pub fn languages() -> Vec<(String, String)> {
    let state = STATE.read().unwrap_or_else(|e| e.into_inner());
    state
        .bundles
        .iter()
        .map(|(code, bundle)| (code.clone(), format_in(bundle, "language-name", None).unwrap_or_else(|| code.clone())))
        .collect()
}

/// Переключает язык интерфейса. Неизвестный код игнорируется.
pub fn set_language(code: &str) {
    let mut state = STATE.write().unwrap_or_else(|e| e.into_inner());
    if state.bundles.contains_key(code) {
        state.current = code.to_owned();
    } else {
        log::warn!("Unknown interface language: {code}");
    }
}

/// Текущий язык интерфейса.
pub fn language() -> String {
    STATE.read().unwrap_or_else(|e| e.into_inner()).current.clone()
}

fn format_in(bundle: &FluentBundle<FluentResource>, key: &str, args: Option<&FluentArgs>) -> Option<String> {
    let pattern = bundle.get_message(key)?.value()?;
    let mut errors = Vec::new();
    let text = bundle.format_pattern(pattern, args, &mut errors);
    if !errors.is_empty() {
        log::warn!("Errors formatting '{key}': {errors:?}");
    }
    Some(text.into_owned())
}

fn lookup(key: &str, args: Option<&FluentArgs>) -> String {
    let state = STATE.read().unwrap_or_else(|e| e.into_inner());
    let found = state
        .bundles
        .get(&state.current)
        .and_then(|b| format_in(b, key, args))
        .or_else(|| state.bundles.get(FALLBACK).and_then(|b| format_in(b, key, args)));
    match found {
        Some(text) => text,
        None => {
            let mut warned = WARNED.lock().unwrap_or_else(|e| e.into_inner());
            if warned.insert(key.to_owned()) {
                log::warn!("Missing translation for '{key}'");
            }
            key.to_owned()
        }
    }
}

/// Строка без аргументов. Обычно вызывается через [`t!`].
pub fn tr(key: &str) -> String {
    lookup(key, None)
}

/// Строка с аргументами. Обычно вызывается через [`t!`].
pub fn tr_args(key: &str, args: &FluentArgs) -> String {
    lookup(key, Some(args))
}

/// Есть ли такая строка в языке (для проверок).
pub fn has_message(code: &str, key: &str) -> bool {
    let state = STATE.read().unwrap_or_else(|e| e.into_inner());
    state.bundles.get(code).is_some_and(|b| b.has_message(key))
}

/// Все ключи встроенных переводов (платформы и программы) — для проверки полноты.
pub fn builtin_message_ids(code: &str) -> Vec<String> {
    message_ids(&sources(), code)
}

/// Ключи языка `code` в наборах переводов `sets` (повторы сохраняются — по ним видны дубли).
pub fn message_ids(sets: &[Locales], code: &str) -> Vec<String> {
    let mut ids = Vec::new();
    for (_, source) in files_of(sets, code) {
        let resource = FluentResource::try_new((*source).to_owned()).unwrap_or_else(|(r, _)| r);
        for entry in resource.entries() {
            if let fluent_syntax::ast::Entry::Message(m) = entry {
                ids.push(m.id.name.to_owned());
            }
        }
    }
    ids.sort();
    ids
}

/// Коды встроенных языков.
pub fn builtin_languages() -> Vec<&'static str> {
    let mut codes: Vec<&'static str> = sources().iter().flat_map(|set| set.iter().map(|(code, _)| *code)).collect();
    codes.sort();
    codes.dedup();
    codes
}

/// Ключи `t!("…")` в исходниках `.rs` папки `dir` (вместе с вложенными, кроме `target`):
/// ключ и файл. Для проверок, что каждый ключ переведён.
pub fn used_keys(dir: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    collect_keys(dir, &mut out);
    out
}

fn collect_keys(dir: &Path, out: &mut Vec<(String, String)>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        if path.is_dir() {
            if name != "target" {
                collect_keys(&path, out);
            }
        } else if name.ends_with(".rs") {
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            let mut from = 0;
            while let Some(i) = text[from..].find("t!(\"").map(|i| i + from) {
                from = i + 4;
                // `format!("…")` и подобные — не наш макрос.
                let before = text[..i].chars().next_back();
                if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                    continue;
                }
                if let Some(end) = text[from..].find('"') {
                    out.push((text[from..from + end].to_owned(), path.display().to_string()));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Тесты меняют общий язык, поэтому выполняются по очереди.
    static LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn builtin_locales_parse_without_errors() {
        for (code, files) in PLATFORM {
            for (name, source) in *files {
                if let Err((_, errors)) = FluentResource::try_new((*source).to_owned()) {
                    panic!("ошибки в {code}/{name}: {errors:?}");
                }
            }
        }
    }

    #[test]
    fn russian_and_english_are_builtin() {
        let langs = builtin_languages();
        assert!(langs.contains(&"ru") && langs.contains(&"en"), "встроены: {langs:?}");
    }

    #[test]
    fn all_languages_have_the_same_keys() {
        let reference = message_ids(&[PLATFORM], FALLBACK);
        assert!(!reference.is_empty());
        for (code, _) in PLATFORM {
            let ids = message_ids(&[PLATFORM], code);
            let missing: Vec<_> = reference.iter().filter(|k| !ids.contains(k)).collect();
            let extra: Vec<_> = ids.iter().filter(|k| !reference.contains(k)).collect();
            assert!(missing.is_empty(), "в языке «{code}» не хватает строк: {missing:?}");
            assert!(extra.is_empty(), "в языке «{code}» лишние строки (нет в {FALLBACK}): {extra:?}");
        }
    }

    #[test]
    fn duplicate_keys_are_not_allowed() {
        for (code, _) in PLATFORM {
            let ids = message_ids(&[PLATFORM], code);
            let mut seen = HashSet::new();
            let dups: Vec<_> = ids.iter().filter(|k| !seen.insert(k.as_str())).collect();
            assert!(dups.is_empty(), "в языке «{code}» повторяются ключи: {dups:?}");
        }
    }

    /// Каждый ключ из `t!("…")` в крейтах платформы должен быть в строках платформы.
    /// Крейты со своей папкой `locales` (например, оболочка) проверяют себя сами.
    #[test]
    fn every_used_key_exists() {
        let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let this_crate = std::path::absolute(env!("CARGO_MANIFEST_DIR")).unwrap_or_default();
        let mut used = Vec::new();
        for entry in std::fs::read_dir(&crates).into_iter().flatten().flatten() {
            let path = entry.path();
            let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
            let own_strings = path.join("locales").is_dir();
            // Сам крейт локализации пропускаем: в его документации ключи — только примеры.
            let skip = std::path::absolute(&path).is_ok_and(|p| p == this_crate);
            if name.starts_with("forma-") && !own_strings && !skip {
                used.extend(used_keys(&path));
            }
        }
        assert!(!used.is_empty(), "ключи t!(\"…\") не найдены — проверь сканер");
        let ids = message_ids(&[PLATFORM], FALLBACK);
        let missing: Vec<_> = used.iter().filter(|(k, _)| !ids.contains(k)).collect();
        assert!(missing.is_empty(), "ключи используются в коде, но не переведены: {missing:?}");
    }

    #[test]
    fn russian_plurals() {
        let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_language("ru");
        assert_eq!(t!("dock-more-tabs", count = 1), "Ещё 1 вкладка");
        assert_eq!(t!("dock-more-tabs", count = 3), "Ещё 3 вкладки");
        assert_eq!(t!("dock-more-tabs", count = 5), "Ещё 5 вкладок");
        assert_eq!(t!("dock-more-tabs", count = 21), "Ещё 21 вкладка");
        set_language("en");
    }

    #[test]
    fn blend_keeps_interface_english_and_hints_russian() {
        let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        assert!(languages().iter().any(|(code, name)| code == "en-RU" && name.contains("русские")));
        set_language("en-RU");
        // Интерфейс и термины — по-английски.
        assert_eq!(tr("settings-page-keymap"), "Keymap");
        // Подсказки — по-русски.
        set_language("ru");
        let ru_hint = tr("dock-empty-hint");
        set_language("en-RU");
        assert_eq!(tr("dock-empty-hint"), ru_hint);
        assert!(is_hint("known-mixin-explain") && is_hint("settings-component-keymap") && !is_hint("known-mixin-title"));
        set_language("en");
    }

    #[test]
    fn hint_messages_take_whole_multiline_entries() {
        let src = "menu-file = Файл\nsome-hint =\n    первая строка\n    вторая\n\n# комментарий\nother = нет\nlast-about = да\n";
        assert_eq!(hint_messages(src), "some-hint =\n    первая строка\n    вторая\n\nlast-about = да\n");
    }

    #[test]
    fn missing_key_falls_back_to_key() {
        let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_language("ru");
        assert_eq!(tr("no-such-key-anywhere"), "no-such-key-anywhere");
        assert_eq!(language(), "ru");
        set_language("xx-unknown");
        assert_eq!(language(), "ru", "неизвестный язык не выбирается");
    }
}
