//! Список тем: встроенные (их передаёт программа), свои — файлы `*.toml` в папке
//! `themes` настроек программы — и темы из плагинов. Свои темы перечитываются на лету:
//! сохранила файл в любом редакторе — тема обновилась.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::{Accent, Theme, ThemeFile};

/// Откуда тема.
#[derive(Clone, Debug, PartialEq)]
pub enum Origin {
    Builtin,
    File(PathBuf),
}

#[derive(Clone, Debug)]
pub struct ThemeEntry {
    pub id: String,
    pub name: String,
    pub dark: bool,
    pub origin: Origin,
    file: ThemeFile,
}

pub struct Themes {
    builtin: Vec<ThemeEntry>,
    user: Vec<ThemeEntry>,
    /// Ошибки разбора своих тем: путь → текст.
    pub broken: Vec<(PathBuf, String)>,
    dir: Option<PathBuf>,
    /// Папки тем из плагинов.
    plugin_dirs: Vec<PathBuf>,
    stamps: HashMap<PathBuf, SystemTime>,
    last_scan: f64,
}

/// Сколько секунд между проверками папки своих тем.
const SCAN_EVERY: f64 = 1.0;

impl Themes {
    /// `builtin` — тексты встроенных тем, `dir` — папка своих тем.
    pub fn new(builtin: &[&str], dir: Option<PathBuf>) -> Self {
        let builtin = builtin
            .iter()
            .filter_map(|src| match ThemeFile::parse(src) {
                Ok(f) => Some(entry(f, Origin::Builtin)),
                Err(e) => {
                    log::error!("Built-in theme is broken: {e}");
                    None
                }
            })
            .collect();
        let mut t = Self { builtin, user: Vec::new(), broken: Vec::new(), dir, plugin_dirs: Vec::new(), stamps: HashMap::new(), last_scan: f64::NEG_INFINITY };
        t.scan();
        t
    }

    pub fn dir(&self) -> Option<&Path> {
        self.dir.as_deref()
    }

    /// Папки тем из плагинов. `true` — список сменился и темы перечитаны.
    pub fn set_plugin_dirs(&mut self, dirs: Vec<PathBuf>) -> bool {
        if self.plugin_dirs == dirs {
            return false;
        }
        self.plugin_dirs = dirs;
        self.scan();
        true
    }

    /// Все темы: сначала встроенные, потом свои. Своя тема с тем же id заменяет встроенную.
    pub fn list(&self) -> Vec<&ThemeEntry> {
        let mut out: Vec<&ThemeEntry> = self.builtin.iter().filter(|b| !self.user.iter().any(|u| u.id == b.id)).collect();
        out.extend(self.user.iter());
        out
    }

    pub fn get(&self, id: &str) -> Option<&ThemeEntry> {
        self.user.iter().find(|t| t.id == id).or_else(|| self.builtin.iter().find(|t| t.id == id))
    }

    /// Вычисленная тема. Неизвестная — первая встроенная (её `fallback`).
    pub fn resolve(&self, id: &str, fallback: &str, accent: Option<Accent>) -> Theme {
        let entry = self.get(id).or_else(|| self.get(fallback)).or_else(|| self.builtin.first());
        let file = match entry {
            Some(e) => self.merged(e, 0),
            None => ThemeFile::parse("[theme]\nid = \"fallback\"\ndark = true").unwrap_or_default(),
        };
        Theme::resolve(file, accent)
    }

    /// Тема вместе с родителями (`extends`).
    fn merged(&self, e: &ThemeEntry, depth: usize) -> ThemeFile {
        let parent = e.file.theme.extends.as_deref().filter(|p| *p != e.id && depth < 8).and_then(|p| self.get(p));
        match parent {
            Some(p) => e.file.over(&self.merged(p, depth + 1)),
            None => e.file.clone(),
        }
    }

    /// Проверяет папку своих тем не чаще раза в секунду. `true` — что-то изменилось.
    pub fn poll(&mut self, now: f64) -> bool {
        if now - self.last_scan < SCAN_EVERY {
            return false;
        }
        self.last_scan = now;
        let stamps = self.stamps();
        if stamps == self.stamps {
            return false;
        }
        self.scan();
        true
    }

    fn stamps(&self) -> HashMap<PathBuf, SystemTime> {
        // Сначала темы плагинов, потом свои: своя тема с тем же id главнее.
        self.plugin_dirs
            .iter()
            .chain(self.dir.iter())
            .filter_map(|dir| std::fs::read_dir(dir).ok())
            .flat_map(|entries| entries.flatten().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("toml")))
            .filter_map(|p| Some((p.clone(), std::fs::metadata(&p).ok()?.modified().ok()?)))
            .collect()
    }

    /// Перечитывает свои темы.
    pub fn scan(&mut self) {
        self.stamps = self.stamps();
        self.user.clear();
        self.broken.clear();
        // Порядок: темы плагинов, потом свои — так своя тема заменяет тему плагина с тем же id.
        let user = |p: &&PathBuf| self.dir.as_ref().is_some_and(|d| p.starts_with(d));
        let mut paths: Vec<&PathBuf> = self.stamps.keys().collect();
        paths.sort_by_key(|p| (user(p), (*p).clone()));
        for path in paths {
            let parsed = std::fs::read_to_string(path).map_err(|e| e.to_string()).and_then(|t| ThemeFile::parse(&t));
            match parsed {
                Ok(mut f) => {
                    f.dir = path.parent().map(Path::to_path_buf);
                    f.resolve_icon_paths();
                    self.user.retain(|u| u.id != f.theme.id);
                    self.user.push(entry(f, Origin::File(path.clone())));
                }
                Err(e) => {
                    log::warn!("Theme {} is broken: {e}", path.display());
                    self.broken.push((path.clone(), e));
                }
            }
        }
    }
}

fn entry(file: ThemeFile, origin: Origin) -> ThemeEntry {
    ThemeEntry { id: file.theme.id.clone(), name: file.theme.name.clone(), dark: file.theme.dark.unwrap_or(true), origin, file }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "[theme]\nid = \"base-dark\"\nname = \"Base Dark\"\ndark = true\n[colors]\naccent = \"#112233\"";

    #[test]
    fn user_themes_extend_and_reload() {
        let dir = std::env::temp_dir().join(format!("forma-themes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut themes = Themes::new(&[BASE], Some(dir.clone()));
        assert_eq!(themes.list().len(), 1);

        std::fs::write(dir.join("pink.toml"), "[theme]\nname = \"Pink\"\nextends = \"base-dark\"\n[colors]\n\"menubar.bg\" = \"#ff69b4\"").unwrap();
        std::fs::write(dir.join("broken.toml"), "[theme\nname=").unwrap();
        assert!(themes.poll(10.0));
        assert_eq!(themes.list().iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), vec!["base-dark", "pink"]);
        assert_eq!(themes.broken.len(), 1);

        let pink = themes.resolve("pink", "base-dark", None);
        assert!(pink.pal.dark, "тёмность унаследована от основы");
        assert_eq!(pink.pal.accent, egui::Color32::from_rgb(0x11, 0x22, 0x33), "акцент унаследован от основы");
        assert_eq!(pink.pal.menubar_bg, egui::Color32::from_rgb(255, 105, 180));

        // Без изменений — без перечитывания; неизвестная тема — запасная.
        assert!(!themes.poll(20.0));
        assert_eq!(themes.resolve("nope", "base-dark", None).id, "base-dark");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
