//! Выражения цвета в темах:
//!
//! - `#rrggbb`, `#rrggbbaa`, `#rgb`, `transparent`;
//! - `$ключ` — цвет из палитры темы или другой ключ темы (`$text.faint`);
//! - `alpha(цвет, 30%)` — прозрачность;
//! - `mix(цвет, цвет, 25%)` — смесь, 25% второго цвета;
//! - `lighten(цвет, 10%)`, `darken(цвет, 10%)` — светлее и темнее.

use egui::Color32;

/// Разбирает и вычисляет выражение. `lookup` отдаёт цвет по имени после `$`.
pub fn eval(expr: &str, lookup: &mut dyn FnMut(&str) -> Result<Color32, String>) -> Result<Color32, String> {
    let mut p = Parser { s: expr.trim(), pos: 0, lookup };
    let c = p.expr()?;
    p.skip_ws();
    if p.pos != p.s.len() {
        return Err(format!("лишнее в конце «{}»", &p.s[p.pos..]));
    }
    Ok(c)
}

/// Имена, на которые ссылается выражение (`$имя`) — для проверки шаблона темы.
pub fn references(expr: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = expr;
    while let Some(i) = rest.find('$') {
        let name: String = rest[i + 1..].chars().take_while(|c| is_name_char(*c)).collect();
        rest = &rest[i + 1 + name.len()..];
        out.push(name);
    }
    out
}

fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_')
}

struct Parser<'a, 'l> {
    s: &'a str,
    pos: usize,
    lookup: &'l mut dyn FnMut(&str) -> Result<Color32, String>,
}

impl Parser<'_, '_> {
    fn skip_ws(&mut self) {
        while self.s[self.pos..].starts_with(char::is_whitespace) {
            self.pos += self.s[self.pos..].chars().next().map_or(1, char::len_utf8);
        }
    }

    fn eat(&mut self, c: char) -> Result<(), String> {
        self.skip_ws();
        if self.s[self.pos..].starts_with(c) {
            self.pos += c.len_utf8();
            Ok(())
        } else {
            Err(format!("ожидался «{c}» в «{}»", self.s))
        }
    }

    fn word(&mut self) -> &str {
        self.skip_ws();
        let start = self.pos;
        while self.s[self.pos..].starts_with(|c: char| is_name_char(c) || c == '#') {
            self.pos += 1;
        }
        &self.s[start..self.pos]
    }

    fn expr(&mut self) -> Result<Color32, String> {
        self.skip_ws();
        if self.s[self.pos..].starts_with('$') {
            self.pos += 1;
            let name = self.word().to_owned();
            if name.is_empty() {
                return Err("после «$» нет имени".into());
            }
            return (self.lookup)(&name);
        }
        let word = self.word().to_owned();
        if let Some(hex) = word.strip_prefix('#') {
            return parse_hex(hex);
        }
        match word.as_str() {
            "transparent" => Ok(Color32::TRANSPARENT),
            "alpha" | "lighten" | "darken" => {
                self.eat('(')?;
                let c = self.expr()?;
                self.eat(',')?;
                let k = self.amount()?;
                self.eat(')')?;
                Ok(match word.as_str() {
                    "alpha" => with_alpha(c, k),
                    "lighten" => mix(c, Color32::WHITE, k),
                    _ => mix(c, Color32::BLACK, k),
                })
            }
            "mix" => {
                self.eat('(')?;
                let a = self.expr()?;
                self.eat(',')?;
                let b = self.expr()?;
                self.eat(',')?;
                let k = self.amount()?;
                self.eat(')')?;
                Ok(mix(a, b, k))
            }
            "" => Err(format!("пустое выражение цвета в «{}»", self.s)),
            other => Err(format!("неизвестная функция или цвет «{other}»")),
        }
    }

    /// Доля: `30%` или `0.3`.
    fn amount(&mut self) -> Result<f32, String> {
        self.skip_ws();
        let start = self.pos;
        while self.s[self.pos..].starts_with(|c: char| c.is_ascii_digit() || c == '.') {
            self.pos += 1;
        }
        let n: f32 = self.s[start..self.pos].parse().map_err(|_| format!("ожидалось число в «{}»", self.s))?;
        if self.s[self.pos..].starts_with('%') {
            self.pos += 1;
            Ok((n / 100.0).clamp(0.0, 1.0))
        } else {
            Ok(n.clamp(0.0, 1.0))
        }
    }
}

fn parse_hex(hex: &str) -> Result<Color32, String> {
    let digits: Vec<u8> = hex.chars().map(|c| c.to_digit(16).map(|d| d as u8)).collect::<Option<_>>().ok_or_else(|| format!("неверный цвет «#{hex}»"))?;
    let byte = |i: usize| digits[i] * 16 + digits[i + 1];
    match digits.len() {
        3 => Ok(Color32::from_rgb(digits[0] * 17, digits[1] * 17, digits[2] * 17)),
        6 => Ok(Color32::from_rgb(byte(0), byte(2), byte(4))),
        8 => Ok(Color32::from_rgba_unmultiplied(byte(0), byte(2), byte(4), byte(6))),
        _ => Err(format!("неверный цвет «#{hex}»: нужно 3, 6 или 8 цифр")),
    }
}

/// Цвет с другой непрозрачностью (0…1).
pub fn with_alpha(c: Color32, a: f32) -> Color32 {
    let [r, g, b, _] = c.to_srgba_unmultiplied();
    Color32::from_rgba_unmultiplied(r, g, b, (a * 255.0).round() as u8)
}

/// Смесь двух цветов: `k` — доля второго.
pub fn mix(a: Color32, b: Color32, k: f32) -> Color32 {
    let (a, b) = (a.to_srgba_unmultiplied(), b.to_srgba_unmultiplied());
    let ch = |i: usize| (a[i] as f32 + (b[i] as f32 - a[i] as f32) * k).round() as u8;
    Color32::from_rgba_unmultiplied(ch(0), ch(1), ch(2), ch(3))
}

/// Цвет как `#rrggbb` или `#rrggbbaa` — для шаблона темы.
pub fn to_hex(c: Color32) -> String {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    if a == 255 { format!("#{r:02x}{g:02x}{b:02x}") } else { format!("#{r:02x}{g:02x}{b:02x}{a:02x}") }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(expr: &str) -> Result<Color32, String> {
        eval(expr, &mut |name| match name {
            "accent" => Ok(Color32::from_rgb(150, 132, 255)),
            "bg" => Ok(Color32::from_rgb(28, 30, 36)),
            other => Err(format!("нет «{other}»")),
        })
    }

    #[test]
    fn literals_refs_and_functions() {
        assert_eq!(run("#1c1e24"), Ok(Color32::from_rgb(28, 30, 36)));
        assert_eq!(run(" #fff "), Ok(Color32::WHITE));
        assert_eq!(run("$bg"), Ok(Color32::from_rgb(28, 30, 36)));
        assert_eq!(to_hex(run("alpha($accent, 50%)").unwrap()), "#9583ff80");
        assert_eq!(run("mix(#000000, #ffffff, 0.5)"), Ok(Color32::from_rgb(128, 128, 128)));
        assert_eq!(run("lighten(#000000, 100%)"), Ok(Color32::WHITE));
        assert_eq!(run("darken(#ffffff, 100%)"), Ok(Color32::BLACK));
        assert_eq!(to_hex(run("alpha(mix($bg, $accent, 20%), 30%)").unwrap()), "#35324f4d");
        assert_eq!(run("transparent"), Ok(Color32::TRANSPARENT));
    }

    #[test]
    fn errors_are_readable() {
        assert!(run("#12345").unwrap_err().contains("3, 6 или 8"));
        assert!(run("$nope").unwrap_err().contains("nope"));
        assert!(run("glow($bg, 1)").unwrap_err().contains("glow"));
        assert!(run("alpha($bg 30%)").is_err());
        assert!(run("#fff junk").is_err());
        assert_eq!(references("mix($bg.panel, $accent, 30%)"), vec!["bg.panel".to_owned(), "accent".to_owned()]);
    }
}
