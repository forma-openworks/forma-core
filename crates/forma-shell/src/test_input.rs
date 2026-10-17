//! Тестовый ввод для проверки программы без настоящей мыши. Если задана переменная окружения
//! `FORMA_TEST_INPUT=<файл>`, оболочка читает строки, дописанные в этот файл, и подаёт их в
//! главное окно как ввод мыши и клавиатуры — по одной команде за кадр. Так проверяется,
//! например, перетаскивание в окне, лежащем под другими окнами.
//!
//! Координаты — в пикселях окна (как на снимке окна), команды:
//! `move X Y` — указатель в точку; `glide X Y N` — плавно за N кадров;
//! `down` / `up` — левая кнопка (`down right` — правая); `click X Y`;
//! `mods ctrl+shift` — держать клавиши-модификаторы (`mods` — отпустить);
//! `key Escape` / `key ctrl+S` — нажать клавишу; `text абв` — ввести текст;
//! `wait N` — пропустить N кадров; `leave` — указатель ушёл из окна; `drop <путь>` — бросить
//! файл в окно; `wheel DX DY` — колесо мыши (в точках, с `mods ctrl` — масштаб).

use std::collections::VecDeque;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::time::Duration;

use eframe::egui::{self, Event, Key, Modifiers, PointerButton, Pos2, RawInput, pos2};

pub(crate) struct TestInput<A> {
    app: A,
    script: Option<Script>,
}

struct Script {
    path: PathBuf,
    /// Сколько байт файла уже прочитано.
    read: u64,
    queue: VecDeque<String>,
    pointer: Pos2,
    mods: Modifiers,
    wait: u32,
    /// Плавное движение: откуда, куда, сделано шагов, всего шагов.
    glide: Option<(Pos2, Pos2, u32, u32)>,
}

impl<A> TestInput<A> {
    pub(crate) fn wrap(app: A) -> Self {
        let script = std::env::var_os("FORMA_TEST_INPUT").map(|p| Script {
            path: PathBuf::from(p),
            read: 0,
            queue: VecDeque::new(),
            pointer: pos2(0.0, 0.0),
            mods: Modifiers::NONE,
            wait: 0,
            glide: None,
        });
        Self { app, script }
    }
}

impl Script {
    /// Новые строки из файла.
    fn poll(&mut self) {
        let Ok(mut file) = std::fs::File::open(&self.path) else { return };
        let len = file.metadata().map_or(0, |m| m.len());
        if len < self.read {
            // Файл начали заново.
            self.read = 0;
        }
        if len == self.read || file.seek(SeekFrom::Start(self.read)).is_err() {
            return;
        }
        let mut text = String::new();
        if file.read_to_string(&mut text).is_err() {
            return;
        }
        // Недописанную последнюю строку оставляем до следующего раза.
        let complete = text.rfind('\n').map_or(0, |i| i + 1);
        self.read += complete as u64;
        // BOM в начале строки (PowerShell пишет его, создавая файл в UTF-8) — не часть команды.
        self.queue.extend(text[..complete].lines().map(|l| l.trim().trim_start_matches('\u{feff}')).filter(|l| !l.is_empty() && !l.starts_with('#')).map(str::to_owned));
    }

    fn busy(&self) -> bool {
        self.wait > 0 || self.glide.is_some() || !self.queue.is_empty()
    }

    fn step(&mut self, input: &mut RawInput, ppp: f32) {
        if self.wait > 0 {
            self.wait -= 1;
            return;
        }
        if let Some((from, to, done, total)) = &mut self.glide {
            *done += 1;
            let t = *done as f32 / (*total).max(1) as f32;
            self.pointer = from.lerp(*to, t);
            input.events.push(Event::PointerMoved(self.pointer));
            if *done >= *total {
                self.glide = None;
            }
            return;
        }
        let Some(line) = self.queue.pop_front() else { return };
        let mut parts = line.split_whitespace();
        let cmd = parts.next().unwrap_or_default();
        let args: Vec<&str> = parts.collect();
        let num = |i: usize| args.get(i).and_then(|s| s.parse::<f32>().ok());
        let at = |i: usize| Some(pos2(num(i)? / ppp, num(i + 1)? / ppp));
        let button = |events: &mut Vec<Event>, pos: Pos2, which: PointerButton, pressed: bool, mods: Modifiers| {
            events.push(Event::PointerButton { pos, button: which, pressed, modifiers: mods });
        };
        let which = if args.first() == Some(&"right") { PointerButton::Secondary } else { PointerButton::Primary };
        match cmd {
            "move" => {
                if let Some(p) = at(0) {
                    self.pointer = p;
                    input.events.push(Event::PointerMoved(p));
                }
            }
            "glide" => {
                if let (Some(p), Some(n)) = (at(0), num(2)) {
                    self.glide = Some((self.pointer, p, 0, n.max(1.0) as u32));
                }
            }
            "down" => button(&mut input.events, self.pointer, which, true, self.mods),
            "up" => button(&mut input.events, self.pointer, which, false, self.mods),
            "click" => {
                if let Some(p) = at(0) {
                    self.pointer = p;
                    input.events.push(Event::PointerMoved(p));
                }
                // Нажатие и отпускание — в следующих кадрах: egui должен сначала увидеть
                // указатель над виджетом.
                self.queue.push_front("up".into());
                self.queue.push_front("down".into());
            }
            "mods" => {
                self.mods = args.first().map_or(Modifiers::NONE, |s| parse_mods(s));
                input.events.push(Event::ModifiersChanged(self.mods));
            }
            "key" => {
                let Some(spec) = args.first() else { return };
                let (mods, name) = match spec.rsplit_once('+') {
                    Some((m, k)) => (parse_mods(m), k),
                    None => (Modifiers::NONE, *spec),
                };
                if let Some(key) = Key::from_name(name) {
                    let modifiers = self.mods | mods;
                    input.events.push(Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers });
                    input.events.push(Event::Key { key, physical_key: None, pressed: false, repeat: false, modifiers });
                }
            }
            "text" => input.events.push(Event::Text(args.join(" "))),
            // Колесо мыши: `wheel 0 -300` — прокрутка вниз на 300 точек (с `mods ctrl` — масштаб).
            "wheel" => input.events.push(Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(num(0).unwrap_or(0.0), num(1).unwrap_or(0.0)),
                phase: egui::TouchPhase::Move,
                modifiers: self.mods,
            }),
            "wait" => self.wait = num(0).unwrap_or(1.0) as u32,
            "leave" => input.events.push(Event::PointerGone),
            "drop" => input.dropped_files.push(std::sync::Arc::new(TestFile(PathBuf::from(args.join(" "))))),
            other => log::warn!("FORMA_TEST_INPUT: неизвестная команда «{other}»"),
        }
    }
}

/// Файл, брошенный в окно тестовым вводом.
#[derive(Debug)]
struct TestFile(PathBuf);

impl egui::DroppedFile for TestFile {
    fn path(&self) -> &std::path::Path {
        &self.0
    }

    fn bytes(&self) -> Result<Vec<u8>, String> {
        std::fs::read(&self.0).map_err(|e| e.to_string())
    }
}

fn parse_mods(s: &str) -> Modifiers {
    let mut m = Modifiers::NONE;
    for part in s.split('+') {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" => m |= Modifiers::CTRL | Modifiers::COMMAND,
            "shift" => m |= Modifiers::SHIFT,
            "alt" => m |= Modifiers::ALT,
            _ => {}
        }
    }
    m
}

impl<A: eframe::App> eframe::App for TestInput<A> {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.app.logic(ctx, frame);
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.app.ui(ui, frame);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.app.save(storage);
    }

    fn on_exit(&mut self) {
        self.app.on_exit();
    }

    fn auto_save_interval(&self) -> Duration {
        self.app.auto_save_interval()
    }

    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        self.app.clear_color(visuals)
    }

    fn persist_egui_memory(&self) -> bool {
        self.app.persist_egui_memory()
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut RawInput) {
        self.app.raw_input_hook(ctx, raw_input);
        let Some(script) = &mut self.script else { return };
        script.poll();
        script.step(raw_input, ctx.pixels_per_point());
        if script.busy() {
            ctx.request_repaint();
        } else {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }
}
