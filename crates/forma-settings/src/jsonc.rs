//! JSON с комментариями (JSONC), как `settings.json` у VS Code: `//` и `/* */`, запятая
//! после последнего элемента. Корень — объект с плоскими ключами
//! (`"rust.inlayHints.types": false`).
//!
//! Правка ([`set`]) меняет только свой ключ: остальной текст, комментарии, порядок и отступы
//! остаются, как их оставил человек.

use std::collections::BTreeMap;
use std::ops::Range;

use serde_json::{Map, Value};

/// Ошибка разбора: строка и столбец (с единицы, в символах) и что не так.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    pub line: usize,
    pub col: usize,
    pub kind: ErrorKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    /// Текст кончился посреди значения или комментария.
    End,
    /// Ждали этот знак: `:`, `,`, `}`, `]`, `"`.
    Expected(char),
    /// Строка с ошибкой (перевод строки внутри, плохой `\`).
    BadString,
    BadNumber,
    /// Здесь не может начинаться значение.
    BadValue,
    /// Корень — не объект `{ … }`.
    NotObject,
    /// После корневого объекта что-то есть.
    Trailing,
}

/// Ключ корневого объекта: где он в тексте (байты).
#[derive(Clone, Debug, PartialEq)]
pub struct Member {
    pub key: String,
    pub value: Value,
    /// Начало ключа (открывающая кавычка).
    pub key_start: usize,
    /// Значение целиком.
    pub value_span: Range<usize>,
}

/// Разобранный текст.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Doc {
    pub members: Vec<Member>,
    /// Фигурные скобки корня (байты); `None` — в тексте нет ничего, кроме пробелов и
    /// комментариев.
    pub braces: Option<(usize, usize)>,
}

impl Doc {
    /// Значения по ключам; у повторённого ключа действует последний, как в VS Code.
    pub fn values(&self) -> BTreeMap<String, Value> {
        self.members.iter().map(|m| (m.key.clone(), m.value.clone())).collect()
    }
}

/// Разбирает текст. Пустой текст (или только комментарии) — пустой объект.
pub fn parse(text: &str) -> Result<Doc, ParseError> {
    let mut p = Parser { text, s: text.as_bytes(), i: 0 };
    if text.starts_with('\u{feff}') {
        p.i = 3;
    }
    p.skip()?;
    if p.i >= p.s.len() {
        return Ok(Doc::default());
    }
    if p.s[p.i] != b'{' {
        return Err(p.error(p.i, ErrorKind::NotObject));
    }
    let open = p.i;
    p.i += 1;
    let mut members = Vec::new();
    loop {
        p.skip()?;
        match p.peek() {
            Some(b'}') => break,
            Some(b'"') => {}
            Some(_) => return Err(p.error(p.i, ErrorKind::Expected('"'))),
            None => return Err(p.error(p.i, ErrorKind::End)),
        }
        let key_start = p.i;
        let key = p.string()?;
        p.skip()?;
        p.expect(b':')?;
        p.skip()?;
        let start = p.i;
        let value = p.value()?;
        members.push(Member { key, value, key_start, value_span: start..p.i });
        p.skip()?;
        match p.peek() {
            Some(b',') => p.i += 1,
            Some(b'}') => break,
            Some(_) => return Err(p.error(p.i, ErrorKind::Expected(','))),
            None => return Err(p.error(p.i, ErrorKind::End)),
        }
    }
    let close = p.i;
    p.i += 1;
    p.skip()?;
    if p.i < p.s.len() {
        return Err(p.error(p.i, ErrorKind::Trailing));
    }
    Ok(Doc { members, braces: Some((open, close)) })
}

struct Parser<'a> {
    text: &'a str,
    s: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn error(&self, at: usize, kind: ErrorKind) -> ParseError {
        let before = &self.text[..at.min(self.text.len())];
        let line = before.matches('\n').count() + 1;
        let col = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
        ParseError { line, col, kind }
    }

    fn expect(&mut self, c: u8) -> Result<(), ParseError> {
        match self.peek() {
            Some(x) if x == c => {
                self.i += 1;
                Ok(())
            }
            None => Err(self.error(self.i, ErrorKind::End)),
            Some(_) => Err(self.error(self.i, ErrorKind::Expected(c as char))),
        }
    }

    /// Пробелы и комментарии.
    fn skip(&mut self) -> Result<(), ParseError> {
        loop {
            match self.peek() {
                Some(b' ' | b'\t' | b'\r' | b'\n') => self.i += 1,
                Some(b'/') if self.s.get(self.i + 1) == Some(&b'/') => {
                    while self.peek().is_some_and(|c| c != b'\n') {
                        self.i += 1;
                    }
                }
                Some(b'/') if self.s.get(self.i + 1) == Some(&b'*') => {
                    let start = self.i;
                    match self.text[self.i + 2..].find("*/") {
                        Some(end) => self.i += 2 + end + 2,
                        None => return Err(self.error(start, ErrorKind::End)),
                    }
                }
                _ => return Ok(()),
            }
        }
    }

    fn value(&mut self) -> Result<Value, ParseError> {
        match self.peek() {
            None => Err(self.error(self.i, ErrorKind::End)),
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => self.string().map(Value::String),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(_) => {
                for (word, value) in [("true", Value::Bool(true)), ("false", Value::Bool(false)), ("null", Value::Null)] {
                    if self.text[self.i..].starts_with(word) {
                        self.i += word.len();
                        return Ok(value);
                    }
                }
                Err(self.error(self.i, ErrorKind::BadValue))
            }
        }
    }

    fn object(&mut self) -> Result<Value, ParseError> {
        self.i += 1;
        let mut map = Map::new();
        loop {
            self.skip()?;
            match self.peek() {
                Some(b'}') => {
                    self.i += 1;
                    return Ok(Value::Object(map));
                }
                Some(b'"') => {}
                Some(_) => return Err(self.error(self.i, ErrorKind::Expected('"'))),
                None => return Err(self.error(self.i, ErrorKind::End)),
            }
            let key = self.string()?;
            self.skip()?;
            self.expect(b':')?;
            self.skip()?;
            let value = self.value()?;
            map.insert(key, value);
            self.skip()?;
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(b'}') => {}
                Some(_) => return Err(self.error(self.i, ErrorKind::Expected(','))),
                None => return Err(self.error(self.i, ErrorKind::End)),
            }
        }
    }

    fn array(&mut self) -> Result<Value, ParseError> {
        self.i += 1;
        let mut items = Vec::new();
        loop {
            self.skip()?;
            if self.peek() == Some(b']') {
                self.i += 1;
                return Ok(Value::Array(items));
            }
            items.push(self.value()?);
            self.skip()?;
            match self.peek() {
                Some(b',') => self.i += 1,
                Some(b']') => {}
                Some(_) => return Err(self.error(self.i, ErrorKind::Expected(']'))),
                None => return Err(self.error(self.i, ErrorKind::End)),
            }
        }
    }

    /// Строка в кавычках; разбирает её serde_json (все `\`-последовательности JSON).
    fn string(&mut self) -> Result<String, ParseError> {
        let start = self.i;
        self.i += 1;
        loop {
            match self.peek() {
                None => return Err(self.error(start, ErrorKind::End)),
                Some(b'\n') => return Err(self.error(start, ErrorKind::BadString)),
                Some(b'\\') => self.i += 2,
                Some(b'"') => {
                    self.i += 1;
                    break;
                }
                Some(_) => self.i += 1,
            }
        }
        let raw = self.text.get(start..self.i).ok_or_else(|| self.error(start, ErrorKind::BadString))?;
        serde_json::from_str(raw).map_err(|_| self.error(start, ErrorKind::BadString))
    }

    fn number(&mut self) -> Result<Value, ParseError> {
        let start = self.i;
        while self.peek().is_some_and(|c| c.is_ascii_digit() || matches!(c, b'-' | b'+' | b'.' | b'e' | b'E')) {
            self.i += 1;
        }
        serde_json::from_str::<serde_json::Number>(&self.text[start..self.i]).map(Value::Number).map_err(|_| self.error(start, ErrorKind::BadNumber))
    }
}

/// Текст, где у ключа `key` значение `value`; `None` — ключа больше нет. Остальное не
/// меняется. Новый ключ встаёт последним, с отступом соседей.
pub fn set(text: &str, key: &str, value: Option<&Value>) -> Result<String, ParseError> {
    let doc = parse(text)?;
    let nl = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let found: Vec<usize> = doc.members.iter().enumerate().filter(|(_, m)| m.key == key).map(|(i, _)| i).collect();
    Ok(match (value, found.last()) {
        // Есть — меняется значение последнего (действующего) повтора.
        (Some(v), Some(&i)) => {
            let m = &doc.members[i];
            let indent = line_indent(text, m.key_start);
            format!("{}{}{}", &text[..m.value_span.start], render(v, &indent, nl), &text[m.value_span.end..])
        }
        (Some(v), None) => insert(text, &doc, key, v, nl),
        (None, Some(_)) => {
            // Все повторы — с конца, чтобы места остальных не сдвинулись.
            let mut out = text.to_owned();
            for &i in found.iter().rev() {
                out = remove(&out, &parse(&out)?, i);
            }
            out
        }
        (None, None) => text.to_owned(),
    })
}

/// Отступ строки, где стоит `at`.
fn line_indent(text: &str, at: usize) -> String {
    let start = text[..at].rfind('\n').map_or(0, |i| i + 1);
    text[start..at].chars().take_while(|c| *c == ' ' || *c == '\t').collect()
}

/// Значение как в `settings.json` у VS Code: отступ в четыре пробела, вложенные строки — с
/// отступом ключа.
fn render(v: &Value, indent: &str, nl: &str) -> String {
    let mut buf = Vec::new();
    let mut ser = serde_json::Serializer::with_formatter(&mut buf, serde_json::ser::PrettyFormatter::with_indent(b"    "));
    let text = match serde::Serialize::serialize(v, &mut ser) {
        Ok(()) => String::from_utf8(buf).unwrap_or_default(),
        Err(_) => "null".into(),
    };
    text.lines().enumerate().map(|(i, l)| if i == 0 { l.to_owned() } else { format!("{indent}{l}") }).collect::<Vec<_>>().join(nl)
}

/// Где после `from` первый знак, не пробел и не комментарий.
fn next_token(text: &str, from: usize) -> usize {
    let mut p = Parser { text, s: text.as_bytes(), i: from };
    let _ = p.skip();
    p.i
}

fn insert(text: &str, doc: &Doc, key: &str, v: &Value, nl: &str) -> String {
    let key_json = serde_json::to_string(key).unwrap_or_default();
    let Some((open, close)) = doc.braces else {
        // Файла ещё нет (или в нём только комментарии) — новый объект после них.
        let head = text.trim_end();
        let sep = if head.is_empty() { "" } else { nl };
        return format!("{head}{sep}{{{nl}    {key_json}: {}{nl}}}{nl}", render(v, "    ", nl));
    };
    let Some(last) = doc.members.last() else {
        // Пустой объект: ключ — внутри скобок, на своей строке.
        let inner = &text[open + 1..close];
        let tail = if inner.trim().is_empty() { nl.to_owned() } else { inner.to_owned() };
        return format!("{}{nl}    {key_json}: {}{tail}{}", &text[..=open], render(v, "    ", nl), &text[close..]);
    };
    let indent = line_indent(text, last.key_start);
    let member = format!("{key_json}: {}", render(v, &indent, nl));
    let after = next_token(text, last.value_span.end);
    if text.as_bytes().get(after) == Some(&b',') {
        // Запятая после последнего уже стоит — так пишет человек: новый ключ тоже с ней.
        return format!("{}{nl}{indent}{member},{}", &text[..=after], &text[after + 1..]);
    }
    let end = last.value_span.end;
    // Комментарий в конце строки последнего ключа остаётся при нём.
    let line_end = text[end..].find('\n').map_or(text.len(), |i| end + i);
    let line_end = if text[..line_end].ends_with('\r') { line_end - 1 } else { line_end };
    if text[end..line_end].trim_start().starts_with("//") {
        return format!("{},{}{nl}{indent}{member}{}", &text[..end], &text[end..line_end], &text[line_end..]);
    }
    format!("{},{nl}{indent}{member}{}", &text[..end], &text[end..])
}

/// Убирает ключ номер `i` вместе с его запятой, а если он стоял на своей строке — и строку.
fn remove(text: &str, doc: &Doc, i: usize) -> String {
    let m = &doc.members[i];
    let mut start = m.key_start;
    let mut end = m.value_span.end;
    let after = next_token(text, end);
    let mut comma_before = None;
    if text.as_bytes().get(after) == Some(&b',') {
        end = after + 1;
    } else if i > 0 {
        // Последний ключ — уходит запятая перед ним.
        let prev = &doc.members[i - 1];
        let c = next_token(text, prev.value_span.end);
        if text.as_bytes().get(c) == Some(&b',') {
            comma_before = Some(c);
        }
    }
    // Ключ один на строке — строка уходит целиком (с комментарием о нём в её конце).
    let line_start = text[..start].rfind('\n').map_or(0, |p| p + 1);
    let rest = &text[end..];
    let line_rest = rest.find('\n').map_or(rest, |p| &rest[..p]).trim();
    if text[line_start..start].trim().is_empty() && (line_rest.is_empty() || line_rest.starts_with("//")) {
        start = line_start;
        end += rest.find('\n').map_or(rest.len(), |p| p + 1);
    }
    let mut out = format!("{}{}", &text[..start], &text[end..]);
    if let Some(c) = comma_before {
        out.remove(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const FILE: &str = "{\n    // Подсказки в коде\n    \"rust.inlayHints.types\": false, // не нужны\n    /* длинные */\n    \"rust.inlayHints.maxLength\": 40,\n}\n";

    #[test]
    fn comments_and_trailing_commas_are_fine() {
        let doc = parse(FILE).unwrap();
        let v = doc.values();
        assert_eq!(v["rust.inlayHints.types"], json!(false));
        assert_eq!(v["rust.inlayHints.maxLength"], json!(40));
        assert_eq!(parse("").unwrap(), Doc::default());
        assert_eq!(parse("// только комментарий\n").unwrap().members.len(), 0);
        assert_eq!(parse("\u{feff}{\"a\": [1, 2,], \"b\": {\"c\": null}}").unwrap().values()["a"], json!([1, 2]));
    }

    #[test]
    fn errors_say_where() {
        let e = parse("{\n  \"a\": tru\n}").unwrap_err();
        assert_eq!((e.line, e.col, e.kind), (2, 8, ErrorKind::BadValue));
        let e = parse("{\n  \"a\": 1\n  \"b\": 2\n}").unwrap_err();
        assert_eq!((e.line, e.kind), (3, ErrorKind::Expected(',')));
        assert_eq!(parse("[1]").unwrap_err().kind, ErrorKind::NotObject);
        assert_eq!(parse("{} x").unwrap_err().kind, ErrorKind::Trailing);
        assert_eq!(parse("{\"a\": 1 /* ").unwrap_err().kind, ErrorKind::End);
    }

    #[test]
    fn edits_keep_everything_else() {
        // Замена значения: комментарии на месте.
        let out = set(FILE, "rust.inlayHints.maxLength", Some(&json!(25))).unwrap();
        assert_eq!(out, FILE.replace("40", "25"));
        // Новый ключ — последним, с запятой, как у соседей.
        let out = set(FILE, "rust.check.command", Some(&json!("check"))).unwrap();
        assert!(out.ends_with("    \"rust.inlayHints.maxLength\": 40,\n    \"rust.check.command\": \"check\",\n}\n"), "{out}");
        assert_eq!(parse(&out).unwrap().values()["rust.check.command"], json!("check"));
        // Удаление: строка уходит, комментарий над ней остаётся.
        let out = set(FILE, "rust.inlayHints.maxLength", None).unwrap();
        assert_eq!(out, "{\n    // Подсказки в коде\n    \"rust.inlayHints.types\": false, // не нужны\n    /* длинные */\n}\n");
        assert!(parse(&out).is_ok());
    }

    #[test]
    fn edits_without_trailing_commas() {
        let text = "{\n    \"a.x\": 1,\n    \"a.y\": 2 // два\n}";
        let added = set(text, "a.z", Some(&json!(true))).unwrap();
        assert_eq!(added, "{\n    \"a.x\": 1,\n    \"a.y\": 2, // два\n    \"a.z\": true\n}");
        let removed = set(text, "a.y", None).unwrap();
        assert_eq!(parse(&removed).unwrap().values().len(), 1);
        assert!(!removed.contains("1,"), "запятая перед последним ушла: {removed}");
        let first = set(text, "a.x", None).unwrap();
        assert_eq!(first, "{\n    \"a.y\": 2 // два\n}");
    }

    #[test]
    fn new_files_and_empty_objects() {
        assert_eq!(set("", "a.b", Some(&json!(1))).unwrap(), "{\n    \"a.b\": 1\n}\n");
        assert_eq!(set("{}", "a.b", Some(&json!(1))).unwrap(), "{\n    \"a.b\": 1\n}");
        assert_eq!(set("{\r\n}\r\n", "a.b", Some(&json!([1, 2]))).unwrap(), "{\r\n    \"a.b\": [\r\n        1,\r\n        2\r\n    ]\r\n}\r\n");
        // Сломанный файл не трогаем.
        assert!(set("{\"a\": }", "a.b", Some(&json!(1))).is_err());
        // Повторы ключа убираются все.
        assert_eq!(parse(&set("{\"a.b\": 1, \"a.b\": 2}", "a.b", None).unwrap()).unwrap().members.len(), 0);
    }
}
