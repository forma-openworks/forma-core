//! Инструменты для авторов тем: шаблон новой темы и инспектор ключей.
//!
//! Шаблон — файл темы, который наследует выбранную тему и перечисляет всё, что можно
//! поменять: ключи цветов с описаниями и текущими значениями, классы подсветки, шрифты,
//! фон и значки. Всё закомментировано: тема начинается как копия основы, а автор
//! раскомментирует и меняет нужное. Инспектор показывает те же ключи вживую — с образцами
//! цвета, значками и строкой для файла темы, которую можно скопировать.

use egui::{Color32, CornerRadius, FontId, Sense, Stroke, Ui, vec2};
use forma_i18n::t;

use crate::settings_window::{self as sw, matches};
use crate::theme::{Palette, Theme, TokenInfo};

pub const INSPECTOR_PAGE: &str = "theme-tokens";

/// Что тема может поменять в программе: группы ключей и классы подсветки кода.
pub struct Kit {
    /// Название группы (платформа, программа) и её ключи.
    pub groups: Vec<(String, &'static [TokenInfo])>,
    pub syntax: Vec<&'static str>,
}

/// Подписи страницы инспектора для поиска по настройкам: разделы, все ключи, классы и значки.
pub fn keywords(kit: &Kit) -> Vec<String> {
    let mut out = vec![t!("theme-tokens-colors"), t!("theme-tokens-icons"), t!("theme-tokens-syntax")];
    out.extend(kit.groups.iter().flat_map(|(_, tokens)| tokens.iter().map(|tk| tk.key.to_owned())));
    out.extend(kit.syntax.iter().map(|c| (*c).to_owned()));
    out.extend(crate::icons::LUCIDE_NAMES.iter().map(|(n, _)| (*n).to_owned()));
    out
}

/// Цвет как в файле темы: `#rrggbb` или `#rrggbbaa`.
pub fn hex(c: Color32) -> String {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    if a == 255 { format!("#{r:02x}{g:02x}{b:02x}") } else { format!("#{r:02x}{g:02x}{b:02x}{a:02x}") }
}

/// Текст новой темы `name` (`id`), наследующей `base`.
pub fn template(name: &str, id: &str, base: &Theme, kit: &Kit) -> String {
    let mut out = String::new();
    let mut line = |s: &str| {
        out.push_str(s);
        out.push('\n');
    };
    line(&format!("# {name} — своя тема Forma. Your own Forma theme."));
    line("#");
    line("# Строки с «#» не действуют: уберите «#», чтобы поменять ключ. Lines starting with # are off.");
    line("# Цвет — \"#rrggbb\", \"#rrggbbaa\" или ссылка \"$имя\" на палитру или другой ключ.");
    line("# Тема перечитывается сразу после сохранения файла. The theme reloads as soon as you save.");
    line("");
    line("[theme]");
    line(&format!("name = \"{name}\""));
    line(&format!("id = \"{id}\""));
    line(&format!("dark = {}", base.pal.dark));
    line(&format!("extends = \"{}\"            # чего нет здесь, берётся оттуда", base.id));
    line("author = \"\"");
    line("");
    line("[palette]                          # свои имена цветов: $pink");
    line("# pink = \"#ff69b4\"");
    line("");
    line("[colors]");
    let groups: Vec<&'static [TokenInfo]> = kit.groups.iter().map(|(_, g)| *g).collect();
    base.with_resolver(&groups, |r| {
        for (title, tokens) in &kit.groups {
            line(&format!("# ── {title} ──"));
            for tk in *tokens {
                line(&format!("# {}", tk.doc.trim()));
                line(&format!("# \"{}\" = \"{}\"", tk.key, hex(r.color(tk.key))));
            }
            line("");
        }
    });
    line("# ── Значки · Icons ──");
    line("# Цвет значка по имени главнее цвета по месту. Свой значок без currentColor цвет не меняет.");
    for (name, _) in crate::icons::LUCIDE_NAMES {
        line(&format!("# \"icon.{name}\" = \"$accent\""));
    }
    line("");
    line("[syntax]                           # подсветка кода: цвет или { color, bold, italic, underline }");
    for class in &kit.syntax {
        let s = base.syntax(class);
        line(&format!("# {class} = {{ color = \"{}\", bold = {}, italic = {}, underline = {} }}", hex(s.color), s.bold, s.italic, s.underline));
    }
    line("");
    line("[fonts]                            # шрифты темы (установленные в системе)");
    line("# ui = \"Segoe UI\"");
    line("# editor = \"JetBrains Mono\"");
    line("# console = \"JetBrains Mono\"");
    line("");
    line("# [background]                     # фоновая картинка, путь от папки темы");
    line("# image = \"picture.png\"");
    line("# opacity = 0.12");
    line("# fit = \"cover\"                    # cover, contain, stretch, tile, center");
    line("");
    line("[icons]                            # свои значки (SVG), пути от папки темы");
    line("# folder = \"icons\"                 # каждый «имя.svg» в этой папке заменяет значок «имя»");
    line("# Значки и рисунок Lucide (https://lucide.dev), которым каждый нарисован сейчас:");
    for (name, lucide) in crate::icons::LUCIDE_NAMES {
        line(&format!("# {name} = \"icons/{name}.svg\"      # {lucide}"));
    }
    out
}

/// Инспектор ключей темы `theme`: цвета, классы подсветки и значки.
pub fn inspector(ui: &mut Ui, pal: &Palette, search: &str, theme: &Theme, kit: &Kit) {
    let note = |ui: &mut Ui, text: String| {
        ui.add(egui::Label::new(egui::RichText::new(text).size(11.5).color(pal.text_faint)).wrap());
    };
    note(ui, t!("theme-tokens-hint", theme = theme.name.clone()));
    ui.add_space(6.0);
    // Страница нашлась по названию — показываем всё, иначе только подходящее.
    let filter = if matches(&t!("settings-page-theme-tokens"), search) { "" } else { search };
    let fits = |key: &str, doc: &str| matches(key, filter) || matches(doc, filter);

    let groups: Vec<&'static [TokenInfo]> = kit.groups.iter().map(|(_, g)| *g).collect();
    theme.with_resolver(&groups, |r| {
        for (title, tokens) in &kit.groups {
            let shown: Vec<&TokenInfo> = tokens.iter().filter(|tk| fits(tk.key, tk.doc)).collect();
            if shown.is_empty() {
                continue;
            }
            sw::group(ui, pal, &format!("{} · {}", t!("theme-tokens-colors"), title));
            for tk in shown {
                let color = r.color(tk.key);
                let set = theme.file.colors.contains_key(tk.key);
                token_row(ui, pal, tk.key, tk.doc.trim(), color, set, &format!("\"{}\" = \"{}\"", tk.key, hex(color)));
            }
            ui.add_space(6.0);
        }
    });

    let classes: Vec<&&str> = kit.syntax.iter().filter(|c| fits(c, "")).collect();
    if !classes.is_empty() {
        sw::group(ui, pal, &t!("theme-tokens-syntax"));
        for class in classes {
            let s = theme.syntax(class);
            let set = theme.file.syntax.keys().any(|k| k == *class);
            let line = format!("{class} = {{ color = \"{}\", bold = {}, italic = {} }}", hex(s.color), s.bold, s.italic);
            token_row(ui, pal, class, "", s.color, set, &line);
        }
        ui.add_space(6.0);
    }

    let icons: Vec<&(&str, &str)> = crate::icons::LUCIDE_NAMES.iter().filter(|(n, l)| fits(n, l)).collect();
    if !icons.is_empty() {
        sw::group(ui, pal, &t!("theme-tokens-icons"));
        note(ui, t!("theme-tokens-icons-hint"));
        ui.add_space(4.0);
        // Сетка: значок цветом из темы (или текста) и имя.
        let cell = vec2(150.0, 26.0);
        let cols = ((ui.available_width() / cell.x).floor() as usize).max(1);
        for row in icons.chunks(cols) {
            ui.horizontal(|ui| {
                for (name, lucide) in row {
                    let (rect, resp) = ui.allocate_exact_size(cell, Sense::click());
                    if resp.hovered() {
                        ui.painter().rect_filled(rect, CornerRadius::same(6), pal.list_hover);
                    }
                    let color = crate::icons::color_of(name, pal.text);
                    crate::icons::paint(ui.painter(), egui::Rect::from_center_size(rect.left_center() + vec2(14.0, 0.0), vec2(16.0, 16.0)), name, color);
                    let g = crate::widgets::elide(ui, *name, FontId::monospace(11.5), pal.text, cell.x - 34.0);
                    ui.painter().galley(rect.left_center() + vec2(28.0, -g.size().y / 2.0), g, pal.text);
                    let resp = resp.on_hover_text(t!("theme-tokens-icon-hover", name = (*name).to_owned(), lucide = (*lucide).to_owned()));
                    if resp.clicked() {
                        ui.ctx().copy_text(format!("{name} = \"icons/{name}.svg\""));
                    }
                }
            });
        }
    }
}

/// Строка ключа: образец цвета, ключ, значение и откуда оно; щелчок копирует строку для файла темы.
fn token_row(ui: &mut Ui, pal: &Palette, key: &str, doc: &str, color: Color32, set: bool, line: &str) {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(6), pal.list_hover);
    }
    let swatch = egui::Rect::from_center_size(rect.left_center() + vec2(14.0, 0.0), vec2(16.0, 16.0));
    ui.painter().rect(swatch, CornerRadius::same(4), color, Stroke::new(1.0, pal.border), egui::StrokeKind::Inside);
    let value = ui.painter().layout_no_wrap(hex(color), FontId::monospace(11.5), pal.text_muted);
    let origin = ui.painter().layout_no_wrap(if set { t!("theme-tokens-from-theme") } else { t!("theme-tokens-default") }, FontId::proportional(11.0), pal.text_faint);
    let right = rect.right() - 8.0;
    let origin_w = origin.size().x;
    ui.painter().galley(egui::pos2(right - origin_w, rect.center().y - origin.size().y / 2.0), origin, pal.text_faint);
    let value_x = right - origin_w - 12.0 - value.size().x;
    ui.painter().galley(egui::pos2(value_x, rect.center().y - value.size().y / 2.0), value, pal.text_muted);
    let g = crate::widgets::elide(ui, key, FontId::monospace(12.0), pal.text, value_x - 12.0 - (rect.left() + 30.0));
    ui.painter().galley(egui::pos2(rect.left() + 30.0, rect.center().y - g.size().y / 2.0), g, pal.text);
    let hint = if doc.is_empty() { t!("theme-tokens-copy") } else { format!("{doc}\n{}", t!("theme-tokens-copy")) };
    if resp.on_hover_text(hint).clicked() {
        ui.ctx().copy_text(line.to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeFile;

    #[test]
    fn template_is_a_valid_theme_that_extends_its_base() {
        let base = Theme::resolve(ThemeFile::parse("[theme]\nid = \"base\"\nname = \"Base\"\ndark = true").unwrap());
        let kit = Kit { groups: vec![("Forma".into(), Palette::TOKENS)], syntax: vec!["keyword", "string"] };
        let text = template("My theme", "my-theme", &base, &kit);
        let parsed = ThemeFile::parse(&text).expect("шаблон — правильная тема");
        assert_eq!(parsed.theme.id, "my-theme");
        assert_eq!(parsed.theme.extends.as_deref(), Some("base"));
        assert!(parsed.colors.is_empty(), "все ключи закомментированы");
        assert!(text.contains("\"accent\" = \"#"), "в шаблоне есть ключи с текущими значениями");
        assert!(text.contains("# run = \"icons/run.svg\""), "и значки");
        assert_eq!(hex(Color32::from_rgb(255, 105, 180)), "#ff69b4");
        assert_eq!(hex(Color32::from_rgba_unmultiplied(255, 0, 0, 128)), "#ff000080");
    }
}
