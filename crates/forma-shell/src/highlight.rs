//! Подсветка кода: около 170 языков из набора bat (через two-face). Цвета, жирный и
//! курсив берутся из раздела `[syntax]` текущей темы.

use std::str::FromStr;
use std::sync::{Arc, OnceLock, RwLock};

use eframe::egui::{
    self, Color32, FontId,
    text::{LayoutJob, TextFormat},
};
use syntect::easy::HighlightLines;
use syntect::highlighting::{Color, FontStyle, ScopeSelectors, StyleModifier, Theme, ThemeItem, ThemeSettings};
use syntect::parsing::{SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;

pub fn syntaxes() -> &'static SyntaxSet {
    static SS: OnceLock<SyntaxSet> = OnceLock::new();
    SS.get_or_init(two_face::syntax::extra_newlines)
}

/// Загружает синтаксисы в фоне, чтобы первый файл открылся без задержки.
pub fn warm_up() {
    std::thread::spawn(|| {
        syntaxes();
    });
}

/// Ищет синтаксис по обозначению языка из Markdown, учитывая привычные сокращения.
pub fn find(lang: &str) -> Option<&'static SyntaxReference> {
    let lower = lang.trim().to_lowercase();
    let token = match lower.as_str() {
        "" | "text" | "txt" | "plain" | "plaintext" | "output" | "log" => return None,
        // Шейдерные языки: HLSL и Cg очень похожи на GLSL.
        "hlsl" | "shader" | "frag" | "vert" | "comp" | "geom" | "tesc" | "tese" | "fx" | "fxh" | "cg" | "shaderlab" => "glsl",
        "metal" | "cuda" | "cu" | "hpp" | "cc" | "cxx" | "h++" | "c++" => "cpp",
        "h" => "c",
        "c#" | "csharp" => "cs",
        "f#" | "fsharp" => "fs",
        "js" | "mjs" | "cjs" | "node" | "javascript" => "js",
        "ts" | "typescript" => "ts",
        // Отдельной подсветки JSX в наборе нет, а TypeScriptReact разбирает его без ошибок.
        "jsx" | "react" | "javascriptreact" | "typescriptreact" => "tsx",
        "objc" | "objective-c" | "objectivec" => "m",
        "objc++" | "objective-c++" => "mm",
        "svg" | "xaml" | "plist" => "xml",
        "env" | "dotenv" => ".env",
        "ex" | "exs" | "elixir" => "ex",
        "gql" => "graphql",
        "conf" | "cfg" => "ini",
        "asm" | "nasm" | "assembly" => "asm",
        "tf" | "terraform" => "tf",
        "gd" | "gdscript" | "godot" => "gd",
        "py" | "python3" | "py3" => "py",
        "sh" | "shell" | "zsh" | "console" | "terminal" => "bash",
        "ps" | "ps1" | "pwsh" | "powershell" | "bat" | "cmd" => "bat",
        "yml" => "yaml",
        "md" => "markdown",
        "rs" => "rs",
        "kt" | "kts" => "kt",
        "golang" => "go",
        "docker" => "dockerfile",
        "make" => "makefile",
        "jsonc" | "json5" => "json",
        "vue" => "vue",
        other => other,
    };
    let ss = syntaxes();
    ss.find_syntax_by_token(token)
        .or_else(|| ss.find_syntax_by_name(token))
        .or_else(|| ss.syntaxes().iter().find(|s| s.name.eq_ignore_ascii_case(token)))
}

/// Классы подсветки, которые задаёт тема (раздел `[syntax]`), и их селекторы TextMate.
pub const CLASS_SELECTORS: &[(&str, &str)] = &[
    ("comment", "comment, punctuation.definition.comment"),
    ("keyword", "keyword, storage.modifier, storage.type.function, keyword.control, variable.language"),
    ("operator", "keyword.operator, punctuation.accessor, punctuation.separator.key-value"),
    ("string", "string, punctuation.definition.string, string.quoted"),
    ("escape", "constant.character.escape, string.regexp"),
    ("number", "constant.numeric, constant.language, support.constant, constant.other"),
    ("function", "entity.name.function, support.function, meta.function-call, variable.function, meta.method-call entity.name"),
    (
        "type",
        "storage.type, support.type, entity.name.type, entity.name.class, entity.name.struct, entity.name.enum, entity.name.trait, entity.other.inherited-class, support.class",
    ),
    ("parameter", "variable.parameter, entity.name.lifetime, storage.modifier.lifetime"),
    ("tag", "entity.name.tag, punctuation.definition.tag"),
    ("attribute", "entity.other.attribute-name"),
    ("preprocessor", "meta.preprocessor, keyword.control.directive, keyword.other.directive, entity.name.macro, support.macro"),
    ("annotation", "meta.annotation, meta.attribute, entity.name.decorator"),
    ("punctuation", "punctuation.section, punctuation.terminator, punctuation.separator"),
    ("heading", "markup.heading, entity.name.section"),
    ("bold", "markup.bold"),
    ("italic", "markup.italic"),
    ("inserted", "markup.inserted"),
    ("deleted", "markup.deleted, invalid"),
    ("raw", "markup.raw, markup.inline.raw"),
];

/// Текущая тема подсветки и её номер: номер меняется вместе с темой и входит в ключ кеша.
struct Current {
    generation: u64,
    theme: Arc<Theme>,
}

fn current() -> &'static RwLock<Current> {
    static CURRENT: OnceLock<RwLock<Current>> = OnceLock::new();
    CURRENT.get_or_init(|| RwLock::new(Current { generation: 0, theme: Arc::new(Theme::default()) }))
}

fn to_syntect(c: Color32) -> Color {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    Color { r, g, b, a }
}

/// Строит тему подсветки из темы Forma. `generation` должен меняться с каждой новой темой.
pub fn set_theme(theme: &forma_ui::theme::Theme, generation: u64) {
    let scopes = CLASS_SELECTORS
        .iter()
        .filter_map(|(class, sel)| {
            let style = theme.syntax(class);
            let mut font_style = FontStyle::empty();
            font_style.set(FontStyle::BOLD, style.bold);
            font_style.set(FontStyle::ITALIC, style.italic);
            font_style.set(FontStyle::UNDERLINE, style.underline);
            Some(ThemeItem {
                scope: ScopeSelectors::from_str(sel).ok()?,
                style: StyleModifier { foreground: Some(to_syntect(style.color)), background: None, font_style: Some(font_style) },
            })
        })
        .collect();
    let built = Theme {
        name: Some(theme.name.clone()),
        author: None,
        settings: ThemeSettings { foreground: Some(to_syntect(theme.pal.editor_text)), background: Some(to_syntect(theme.pal.editor_bg)), ..Default::default() },
        scopes,
    };
    let mut cur = current().write().unwrap_or_else(|e| e.into_inner());
    *cur = Current { generation, theme: Arc::new(built) };
}

fn to_color32(c: Color) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
}

#[derive(Default)]
struct CodeHighlighter;

#[derive(Clone, Copy, Hash)]
struct Key<'a> {
    code: &'a str,
    lang: &'a str,
    generation: u64,
    size_bits: u32,
    line_height_bits: u32,
}

impl egui::cache::ComputerMut<Key<'_>, LayoutJob> for CodeHighlighter {
    fn compute(&mut self, key: Key<'_>) -> LayoutJob {
        let theme = current().read().unwrap_or_else(|e| e.into_inner()).theme.clone();
        build_job(key.code, key.lang, &theme, f32::from_bits(key.size_bits), f32::from_bits(key.line_height_bits))
    }
}

type HighlightCache = egui::cache::FrameCache<LayoutJob, CodeHighlighter>;

/// Подсвеченный код для egui (с кэшированием между кадрами).
pub fn job(ctx: &egui::Context, code: &str, lang: &str, size: f32, line_height: f32) -> LayoutJob {
    let generation = current().read().unwrap_or_else(|e| e.into_inner()).generation;
    let key = Key { code, lang, generation, size_bits: size.to_bits(), line_height_bits: line_height.to_bits() };
    ctx.memory_mut(|m| m.caches.cache::<HighlightCache>().get(key).clone())
}

fn build_job(code: &str, lang: &str, theme: &Theme, size: f32, line_height: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    let fg = theme.settings.foreground.map(to_color32).unwrap_or(Color32::GRAY);
    let format = |color: Color32, style: FontStyle| {
        let (bold, italic) = (style.contains(FontStyle::BOLD), style.contains(FontStyle::ITALIC));
        TextFormat {
            font_id: FontId::new(size, forma_ui::fonts::editor(bold, italic)),
            color,
            line_height: Some(line_height),
            // Нет настоящего курсива у шрифта — egui наклоняет обычный.
            italics: italic && !forma_ui::fonts::editor_has_italic(),
            underline: if style.contains(FontStyle::UNDERLINE) { egui::Stroke::new(1.0, color) } else { egui::Stroke::NONE },
            ..Default::default()
        }
    };
    let Some(syntax) = find(lang) else {
        job.append(code, 0.0, format(fg, FontStyle::empty()));
        return job;
    };
    let mut h = HighlightLines::new(syntax, theme);
    for line in LinesWithEndings::from(code) {
        match h.highlight_line(line, syntaxes()) {
            Ok(ranges) => {
                for (style, text) in ranges {
                    job.append(text, 0.0, format(to_color32(style.foreground), style.font_style));
                }
            }
            Err(_) => job.append(line, 0.0, format(fg, FontStyle::empty())),
        }
    }
    job
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_common_languages() {
        for lang in ["java", "kt", "json", "toml", "rust", "python", "xml", "glsl", "md", "yaml"] {
            assert!(find(lang).is_some(), "не найден язык {lang}");
        }
        assert!(find("text").is_none());
    }

    #[test]
    fn highlights_with_colors() {
        let theme = forma_ui::theme::Theme::resolve(
            forma_ui::theme::ThemeFile::parse("[theme]
id = \"t\"
[syntax]
keyword = \"#ff0000\"
comment = \"#00ff00\"
number = \"#0000ff\"
function = \"#ffff00\"
").unwrap(),
        );
        set_theme(&theme, 1);
        let theme = current().read().unwrap().theme.clone();
        let job = build_job("fn main() { let x = 42; } // привет", "rust", &theme, 14.0, 20.0);
        let colors: std::collections::HashSet<_> = job.sections.iter().map(|s| s.format.color).collect();
        assert!(colors.len() >= 4, "слишком мало цветов: {}", colors.len());
    }
}
