//! Шрифты: три независимых шрифта (интерфейс, редактор кода, консоль) из установленных
//! в системе. Если семейство не выбрано, берутся привычные шрифты системы (Segoe UI и
//! Cascadia Mono в Windows), а если нет и их — встроенные шрифты egui.
//!
//! В egui у каждого шрифта свои семейства: интерфейс — `Proportional`, `bold`,
//! `semibold`, `italic`, `bolditalic`; редактор — `Monospace`, `editor`, `editor-bold`,
//! `editor-italic`, `editor-bolditalic`; консоль — `console`, `console-bold`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

use egui::{self, FontData, FontDefinitions, FontFamily};
use serde::{Deserialize, Serialize};

pub fn bold() -> FontFamily {
    FontFamily::Name("bold".into())
}

pub fn semibold() -> FontFamily {
    FontFamily::Name("semibold".into())
}

/// Шрифт редактора кода: обычный, жирный, курсив.
pub fn editor(bold: bool, italic: bool) -> FontFamily {
    FontFamily::Name(
        match (bold, italic) {
            (false, false) => "editor",
            (true, false) => "editor-bold",
            (false, true) => "editor-italic",
            (true, true) => "editor-bolditalic",
        }
        .into(),
    )
}

static EDITOR_ITALIC: AtomicBool = AtomicBool::new(false);

/// Есть ли у шрифта редактора настоящий курсив (иначе egui наклоняет обычное начертание).
pub fn editor_has_italic() -> bool {
    EDITOR_ITALIC.load(Ordering::Relaxed)
}

/// Шрифт консоли.
pub fn console(bold: bool) -> FontFamily {
    FontFamily::Name(if bold { "console-bold" } else { "console" }.into())
}

/// Один из трёх шрифтов в настройках.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FontSlot {
    /// Свой шрифт вместо шрифта темы.
    pub custom: bool,
    /// Семейство; пусто — по умолчанию.
    pub family: String,
    /// Размер текста в пикселях.
    pub size: f32,
    /// Высота строки относительно размера.
    pub line_height: f32,
}

impl Default for FontSlot {
    fn default() -> Self {
        Self::new(14.0, 1.4)
    }
}

impl FontSlot {
    fn new(size: f32, line_height: f32) -> Self {
        Self { custom: false, family: String::new(), size, line_height }
    }

    /// Какое семейство показывать: своё (если выбрано), шрифт темы или по умолчанию (`None`).
    pub fn effective(&self, theme: Option<&str>) -> Option<String> {
        let own = Some(self.family.trim()).filter(|f| !f.is_empty()).map(str::to_owned);
        match theme {
            Some(t) if !self.custom => Some(t.to_owned()),
            _ => own,
        }
    }

    /// Высота строки в пикселях.
    pub fn row_height(&self) -> f32 {
        (self.size * self.line_height).round()
    }
}

/// Компонент настроек «Шрифты».
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Fonts {
    pub ui: FontSlot,
    pub editor: FontSlot,
    pub console: FontSlot,
    /// Ctrl+колёсико меняет размер шрифта в редакторе и консоли.
    pub ctrl_wheel: bool,
}

impl Default for Fonts {
    fn default() -> Self {
        Self { ui: FontSlot::new(14.5, 1.3), editor: FontSlot::new(14.0, 1.45), console: FontSlot::new(12.0, 1.5), ctrl_wheel: true }
    }
}

impl forma_settings::Component for Fonts {
    const ID: &'static str = "fonts";
}

pub const SIZE_RANGE: std::ops::RangeInclusive<f32> = 9.0..=32.0;

/// Что устанавливать: семейства трёх шрифтов (`None` — по умолчанию).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Choice {
    pub ui: Option<String>,
    pub editor: Option<String>,
    pub console: Option<String>,
}

static DB: OnceLock<fontdb::Database> = OnceLock::new();

/// Шрифты системы. Загружаются один раз; [`warm_up`] начинает это заранее в фоне.
fn database() -> &'static fontdb::Database {
    DB.get_or_init(|| {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        db
    })
}

pub fn warm_up() {
    std::thread::spawn(|| {
        database();
    });
}

/// Загружены ли уже шрифты системы (чтобы не ждать их в кадре интерфейса).
pub fn database_ready() -> bool {
    DB.get().is_some()
}

/// Семейства шрифтов системы по алфавиту. `monospace` — только моноширинные.
pub fn families(monospace: bool) -> Vec<String> {
    let db = database();
    let mut out: Vec<String> = db.faces().filter(|f| !monospace || f.monospaced).filter_map(|f| f.families.first().map(|(n, _)| n.clone())).collect();
    out.sort_by_key(|n| n.to_lowercase());
    out.dedup();
    out
}

/// Данные начертания семейства (`None`, если такого семейства нет).
fn load_face(family: &str, weight: u16, italic: bool) -> Option<FontData> {
    let db = database();
    let query = fontdb::Query {
        families: &[fontdb::Family::Name(family)],
        weight: fontdb::Weight(weight),
        stretch: fontdb::Stretch::Normal,
        style: if italic { fontdb::Style::Italic } else { fontdb::Style::Normal },
    };
    let id = db.query(&query)?;
    // Запрос подбирает ближайшее начертание: проверяем, что это то самое семейство.
    let face = db.face(id)?;
    if !face.families.iter().any(|(n, _)| n.eq_ignore_ascii_case(family)) {
        return None;
    }
    db.with_face_data(id, |data, index| {
        let mut d = FontData::from_owned(data.to_vec());
        d.index = index;
        d
    })
}

/// Привычные шрифты системы по путям — без загрузки всей базы шрифтов.
fn load_first(paths: &[&str]) -> Option<FontData> {
    paths.iter().find_map(|p| std::fs::read(p).ok().map(FontData::from_owned))
}

const UI_REGULAR: &[&str] = &["C:/Windows/Fonts/segoeui.ttf", "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf", "/usr/share/fonts/noto/NotoSans-Regular.ttf", "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"];
const UI_BOLD: &[&str] = &["C:/Windows/Fonts/segoeuib.ttf", "/usr/share/fonts/truetype/noto/NotoSans-Bold.ttf", "/usr/share/fonts/noto/NotoSans-Bold.ttf", "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf"];
const UI_SEMIBOLD: &[&str] = &["C:/Windows/Fonts/seguisb.ttf", "/usr/share/fonts/truetype/noto/NotoSans-SemiBold.ttf", "/usr/share/fonts/noto/NotoSans-SemiBold.ttf"];
const UI_ITALIC: &[&str] = &["C:/Windows/Fonts/segoeuii.ttf", "/usr/share/fonts/truetype/noto/NotoSans-Italic.ttf", "/usr/share/fonts/noto/NotoSans-Italic.ttf", "/usr/share/fonts/truetype/dejavu/DejaVuSans-Oblique.ttf"];
const UI_BOLD_ITALIC: &[&str] = &["C:/Windows/Fonts/segoeuiz.ttf", "/usr/share/fonts/truetype/noto/NotoSans-BoldItalic.ttf", "/usr/share/fonts/noto/NotoSans-BoldItalic.ttf"];
const MONO_REGULAR: &[&str] = &["C:/Windows/Fonts/CascadiaMono.ttf", "C:/Windows/Fonts/consola.ttf", "/usr/share/fonts/truetype/jetbrains-mono/JetBrainsMono-Regular.ttf", "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"];
/// Жирное и курсив моноширинного шрифта по умолчанию не подбираются из другого семейства:
/// у него другая ширина символов, и колонки кода разъехались бы. Без них жирное
/// остаётся обычным, а курсив egui делает наклоном.
const NONE: &[&str] = &[];
const SYMBOLS: &[&str] = &["C:/Windows/Fonts/seguisym.ttf", "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"];

/// Начертание: из выбранного семейства, а если его нет — из привычных шрифтов системы.
fn face(family: Option<&str>, weight: u16, italic: bool, fallback: &[&str]) -> Option<FontData> {
    family.and_then(|f| load_face(f, weight, italic)).or_else(|| load_first(fallback))
}

/// Устанавливает шрифты в egui.
pub fn install(ctx: &egui::Context, choice: &Choice) {
    let mut defs = FontDefinitions::default();
    let default_prop = defs.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let default_mono = defs.families.get(&FontFamily::Monospace).cloned().unwrap_or_default();
    let mut add = |name: &str, data: Option<FontData>| -> Option<String> {
        let data = data?;
        defs.font_data.insert(name.to_owned(), Arc::new(data));
        Some(name.to_owned())
    };

    let ui = choice.ui.as_deref();
    let ui_regular = add("ui-regular", face(ui, 400, false, UI_REGULAR));
    let ui_bold = add("ui-bold", face(ui, 700, false, UI_BOLD));
    let ui_semibold = add("ui-semibold", face(ui, 600, false, UI_SEMIBOLD));
    let ui_italic = add("ui-italic", face(ui, 400, true, UI_ITALIC));
    let ui_bold_italic = add("ui-bolditalic", face(ui, 700, true, UI_BOLD_ITALIC));

    let ed = choice.editor.as_deref();
    let ed_regular = add("editor-regular", face(ed, 400, false, MONO_REGULAR));
    let ed_bold = add("editor-bold", face(ed, 700, false, NONE));
    let ed_italic = add("editor-italic", face(ed, 400, true, NONE));
    let ed_bold_italic = add("editor-bolditalic", face(ed, 700, true, NONE));
    EDITOR_ITALIC.store(ed_italic.is_some(), Ordering::Relaxed);

    let co = choice.console.as_deref();
    let co_regular = add("console-regular", face(co, 400, false, MONO_REGULAR));
    let co_bold = add("console-bold", face(co, 700, false, NONE));

    let symbols = add("ui-symbols", load_first(SYMBOLS));

    // Цепочка: нужное начертание, обычное, затем символы и встроенные шрифты egui
    // (эмодзи, значки), а для моноширинных — ещё и шрифт интерфейса (кириллица).
    let chain = |first: &[&Option<String>], mono: bool| -> Vec<String> {
        let mut v: Vec<String> = first.iter().filter_map(|f| (*f).clone()).collect();
        let tail = if mono { default_mono.iter().chain(ui_regular.iter()) } else { default_prop.iter().chain(None.iter()) };
        for f in symbols.iter().chain(tail).chain(default_prop.iter()) {
            if !v.contains(f) {
                v.push(f.clone());
            }
        }
        v
    };
    let fam = |n: &str| FontFamily::Name(n.into());
    defs.families.insert(FontFamily::Proportional, chain(&[&ui_regular], false));
    defs.families.insert(bold(), chain(&[&ui_bold, &ui_semibold, &ui_regular], false));
    defs.families.insert(semibold(), chain(&[&ui_semibold, &ui_bold, &ui_regular], false));
    defs.families.insert(fam("italic"), chain(&[&ui_italic, &ui_regular], false));
    defs.families.insert(fam("bolditalic"), chain(&[&ui_bold_italic, &ui_bold, &ui_regular], false));
    defs.families.insert(FontFamily::Monospace, chain(&[&ed_regular], true));
    defs.families.insert(editor(false, false), chain(&[&ed_regular], true));
    defs.families.insert(editor(true, false), chain(&[&ed_bold, &ed_regular], true));
    defs.families.insert(editor(false, true), chain(&[&ed_italic, &ed_regular], true));
    defs.families.insert(editor(true, true), chain(&[&ed_bold_italic, &ed_bold, &ed_italic, &ed_regular], true));
    defs.families.insert(console(false), chain(&[&co_regular], true));
    defs.families.insert(console(true), chain(&[&co_bold, &co_regular], true));
    ctx.set_fonts(defs);
}
