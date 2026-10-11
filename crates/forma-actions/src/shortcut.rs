//! Сочетание клавиш: модификаторы и одна клавиша. В файлах хранится строкой
//! вида `Ctrl+Shift+F12`, чтобы раскладку было удобно читать и править руками.

use std::fmt;

use egui::{Event, InputState, Key, Modifiers};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Shortcut {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub key: Key,
}

/// Символы, которые на обычной раскладке набираются с Shift (`+`, `:`, `?`…).
/// Для них лишний Shift в нажатии не мешает совпадению: `Ctrl++` — это Ctrl+Shift+=.
fn shifted_symbol(key: Key) -> bool {
    matches!(
        key,
        Key::Plus | Key::Colon | Key::Pipe | Key::Questionmark | Key::Exclamationmark | Key::OpenCurlyBracket | Key::CloseCurlyBracket
    )
}

impl Shortcut {
    pub const fn new(ctrl: bool, alt: bool, shift: bool, key: Key) -> Self {
        Self { ctrl, alt, shift, key }
    }

    /// `Ctrl+Shift+F12`, `Alt+1`, `Shift+F10`. Регистр не важен.
    pub fn parse(text: &str) -> Option<Self> {
        let mut s = Self::new(false, false, false, Key::Escape);
        let mut key = None;
        // `Ctrl++`: последний «+» — это клавиша, а не разделитель.
        let (mods, last) = match text.strip_suffix("++") {
            Some(rest) => (rest, "+"),
            None => text.rsplit_once('+').unwrap_or(("", text)),
        };
        for part in mods.split('+').filter(|p| !p.is_empty()) {
            match part.trim().to_ascii_lowercase().as_str() {
                "ctrl" | "control" | "cmd" | "command" => s.ctrl = true,
                "alt" | "option" => s.alt = true,
                "shift" => s.shift = true,
                _ => return None,
            }
        }
        let last = last.trim();
        if !last.is_empty() {
            key = Key::from_name(last).or_else(|| Key::from_name(&last.to_ascii_uppercase()));
        }
        s.key = key?;
        Some(s)
    }

    /// Совпадает ли нажатие с сочетанием. Модификаторы сравниваются точно,
    /// чтобы `Ctrl+S` не срабатывал на `Ctrl+Shift+S`.
    pub fn matches(&self, key: Key, m: Modifiers) -> bool {
        let shift_ok = m.shift == self.shift || (!self.shift && shifted_symbol(key));
        key == self.key && (m.ctrl || m.command) == self.ctrl && m.alt == self.alt && shift_ok
    }

    /// Съедает нажатия этого сочетания в кадре. Возвращает, было ли хоть одно.
    ///
    /// Вместе с нажатием убирается и символ, который система присылает за ним
    /// (Windows на `Alt+6` присылает ещё и «6»), иначе он впечатался бы в редактор.
    pub fn consume(&self, input: &mut InputState) -> bool {
        let mut hit = false;
        input.events.retain(|e| {
            let is = matches!(e, Event::Key { key, pressed: true, modifiers, .. } if self.matches(*key, *modifiers));
            hit |= is;
            !is
        });
        if hit {
            let typed = if self.key == Key::Space { " " } else { key_text(self.key) };
            input.events.retain(|e| !matches!(e, Event::Text(t) if t.eq_ignore_ascii_case(typed)));
        }
        hit
    }

    /// Сочетание из события нажатия (при записи нового сочетания в настройках).
    /// Нажатия одних модификаторов не считаются.
    pub fn from_event(event: &Event) -> Option<Self> {
        match event {
            Event::Key { key, pressed: true, modifiers, .. } => {
                Some(Self::new(modifiers.ctrl || modifiers.command, modifiers.alt, modifiers.shift, *key))
            }
            _ => None,
        }
    }

    /// То же сочетание в виде egui — для кода, который ловит клавиши сам (раскладка областей).
    pub fn to_egui(&self) -> egui::KeyboardShortcut {
        let modifiers = Modifiers { alt: self.alt, shift: self.shift, command: self.ctrl, ..Modifiers::NONE };
        egui::KeyboardShortcut::new(modifiers, self.key)
    }

    /// Сколько модификаторов: более точные сочетания проверяются первыми.
    pub fn weight(&self) -> u8 {
        self.ctrl as u8 + self.alt as u8 + self.shift as u8
    }
}

impl fmt::Display for Shortcut {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.ctrl {
            f.write_str("Ctrl+")?;
        }
        if self.alt {
            f.write_str("Alt+")?;
        }
        if self.shift {
            f.write_str("Shift+")?;
        }
        f.write_str(key_text(self.key))
    }
}

/// Подпись клавиши. Её же понимает [`Key::from_name`], поэтому строка годится и для файлов.
/// Минус пишем обычным дефисом (у egui это типографский «−»), стрелки — словами.
fn key_text(key: Key) -> &'static str {
    match key {
        Key::Minus => "-",
        Key::ArrowUp | Key::ArrowDown | Key::ArrowLeft | Key::ArrowRight => key.name(),
        _ => key.symbol_or_name(),
    }
}

impl Serialize for Shortcut {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Shortcut {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        Shortcut::parse(&text).ok_or_else(|| serde::de::Error::custom(format!("неизвестное сочетание «{text}»")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_prints() {
        for text in ["Ctrl+Shift+F12", "Shift+F10", "Ctrl+Alt+S", "Alt+1", "F12", "Ctrl+Space", "Ctrl+=", "Ctrl+-"] {
            let s = Shortcut::parse(text).unwrap_or_else(|| panic!("{text}"));
            assert_eq!(s.to_string(), text);
        }
        assert_eq!(Shortcut::parse("ctrl+b"), Some(Shortcut::new(true, false, false, Key::B)));
        assert_eq!(Shortcut::parse("Ctrl++"), Some(Shortcut::new(true, false, false, Key::Plus)));
        assert_eq!(Shortcut::parse("Hyper+X"), None);
        assert_eq!(Shortcut::parse("Ctrl+"), None);
    }

    #[test]
    fn modifiers_match_exactly() {
        let save = Shortcut::parse("Ctrl+S").unwrap();
        let ctrl = Modifiers { ctrl: true, command: true, ..Default::default() };
        let ctrl_shift = Modifiers { shift: true, ..ctrl };
        assert!(save.matches(Key::S, ctrl));
        assert!(!save.matches(Key::S, ctrl_shift), "Ctrl+Shift+S — другое сочетание");
        assert!(!save.matches(Key::S, Modifiers::NONE));
        // `+` набирается с Shift, но сочетание пишут как Ctrl++.
        let zoom = Shortcut::parse("Ctrl++").unwrap();
        assert!(zoom.matches(Key::Plus, ctrl_shift));
    }

    #[test]
    fn consuming_also_eats_the_typed_character() {
        let mut input = InputState::default();
        let alt = Modifiers { alt: true, ..Modifiers::NONE };
        input.events = vec![
            Event::Key { key: Key::Num6, physical_key: None, pressed: true, repeat: false, modifiers: alt },
            Event::Text("6".into()),
            Event::Text("x".into()),
        ];
        assert!(Shortcut::parse("Alt+6").unwrap().consume(&mut input));
        assert_eq!(input.events, vec![Event::Text("x".into())], "«6» не должна попасть в редактор, остальной ввод остаётся");
        // Без совпадения ничего не трогаем.
        input.events = vec![Event::Text("6".into())];
        assert!(!Shortcut::parse("Alt+6").unwrap().consume(&mut input));
        assert_eq!(input.events.len(), 1);
    }

    #[test]
    fn serde_uses_text() {
        let s = Shortcut::parse("Ctrl+Shift+A").unwrap();
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(json, "\"Ctrl+Shift+A\"");
        assert_eq!(serde_json::from_str::<Shortcut>(&json).unwrap(), s);
    }
}
