//! Плагины платформы Forma: всё большое и необязательное — языковые серверы, среды
//! выполнения, наборы тем — ставится и убирается одной кнопкой.
//!
//! Плагин — папка `<plugins>/<id>` с `plugin.toml` (см. [`manifest`]) и содержимым.
//! Что можно поставить, программа описывает каталогом ([`CatalogEntry`]): откуда взять
//! архив и как проверить, что он годный, или какие файлы плагин везёт в самой программе
//! (так устроены небольшие наборы тем). Наборы тем и другие плагины не из каталога
//! ставятся из `.zip` с `plugin.toml` внутри.
//!
//! Архивы скачиваются и распаковываются системными `curl` и `tar` (в Windows 10+ они есть
//! в System32), так платформе не нужны свои HTTP-, TLS- и zip-библиотеки. Установка идёт
//! в отдельной папке и только в конце подменяет прежнюю версию: прерванная установка
//! ничего не ломает.

pub mod manifest;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

pub use manifest::{Contributes, Kind, Manifest, Meta};

/// Откуда берётся плагин из каталога.
#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    /// Архив (`.tar.gz` или `.zip`) по ссылке.
    Download { url: &'static str },
    /// Файлы внутри программы: путь в папке плагина и содержимое. Ставится без сети.
    Bundled { files: &'static [(&'static str, &'static str)] },
    /// Появится в следующих версиях программы — в каталоге, но поставить пока нельзя.
    Later,
}

/// Проверка папки плагина: `Err` — чего в ней не хватает.
pub type Validate = fn(&Path) -> Result<(), String>;

/// Плагин, который программа предлагает поставить.
#[derive(Clone, Debug)]
pub struct CatalogEntry {
    pub id: &'static str,
    /// Название и описание — на языке интерфейса.
    pub name: String,
    pub description: String,
    pub kind: Kind,
    /// Примерный размер загрузки в байтах (для подписи «≈ 50 МБ»).
    pub size: Option<u64>,
    pub author: &'static str,
    pub license: &'static str,
    pub homepage: &'static str,
    /// Значок (имя из набора значков платформы).
    pub icon: &'static str,
    pub source: Source,
    /// Показывать в «Рекомендуемом» и на экране приветствия.
    pub recommended: bool,
    /// Проверка распакованного архива: есть ли в нём то, что нужно программе.
    pub validate: Option<Validate>,
    /// Папка с темами внутри плагина, если он их приносит.
    pub themes: Option<&'static str>,
}

/// Установленный плагин.
#[derive(Clone, Debug, PartialEq)]
pub struct Installed {
    pub manifest: Manifest,
    pub dir: PathBuf,
}

impl Installed {
    pub fn id(&self) -> &str {
        &self.manifest.plugin.id
    }

    /// Папка с темами плагина, если он их приносит.
    pub fn themes_dir(&self) -> Option<PathBuf> {
        self.manifest.contributes.themes.as_ref().map(|t| self.dir.join(t))
    }
}

/// Шаг идущей установки.
#[derive(Clone, Debug, PartialEq)]
pub enum Progress {
    Downloading { done: u64, total: Option<u64> },
    Extracting,
}

/// Что случилось с плагинами с прошлого опроса.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Installed(String),
    Failed { id: String, error: String },
    Removed(String),
}

#[derive(Clone, Debug, PartialEq)]
enum JobState {
    Running(Progress),
    Done,
    Failed(String),
}

/// Настройки плагинов — компонент настроек: что выключено и что стоит (для переноса
/// на другой компьютер: при импорте недостающие плагины предлагаются к установке).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PluginSettings {
    pub disabled: Vec<String>,
    pub installed: Vec<String>,
}

impl forma_settings::Component for PluginSettings {
    const ID: &'static str = "plugins";
}

impl PluginSettings {
    pub fn enabled(&self, id: &str) -> bool {
        !self.disabled.iter().any(|d| d == id)
    }

    pub fn set_enabled(&mut self, id: &str, on: bool) {
        self.disabled.retain(|d| d != id);
        if !on {
            self.disabled.push(id.to_owned());
            self.disabled.sort();
        }
    }
}

/// Все плагины программы: установленные и идущие установки.
pub struct Plugins {
    root: Option<PathBuf>,
    installed: Vec<Installed>,
    jobs: HashMap<String, Arc<Mutex<JobState>>>,
    /// Последняя ошибка установки по id — показывается, пока не начнётся новая.
    errors: HashMap<String, String>,
    events: Vec<Event>,
}

impl Plugins {
    /// `root` — папка плагинов: `%LOCALAPPDATA%\Forma\<программа>\plugins`.
    pub fn new(root: Option<PathBuf>) -> Self {
        let mut p = Self { root, installed: Vec::new(), jobs: HashMap::new(), errors: HashMap::new(), events: Vec::new() };
        p.scan();
        p
    }

    pub fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    /// Перечитывает папку плагинов.
    pub fn scan(&mut self) {
        self.installed.clear();
        let Some(root) = &self.root else { return };
        let Ok(entries) = std::fs::read_dir(root) else { return };
        for e in entries.flatten() {
            let dir = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            if !dir.is_dir() || name.starts_with('.') {
                continue;
            }
            match std::fs::read_to_string(dir.join("plugin.toml")).map_err(|e| e.to_string()).and_then(|t| Manifest::parse(&t)) {
                Ok(manifest) if manifest.plugin.id == name => self.installed.push(Installed { manifest, dir }),
                Ok(manifest) => log::warn!("Plugin folder {name} holds plugin {}; skipped", manifest.plugin.id),
                Err(err) => log::warn!("Plugin {name} is broken: {err}"),
            }
        }
        self.installed.sort_by_key(|p| p.manifest.title().to_lowercase());
    }

    pub fn installed(&self) -> &[Installed] {
        &self.installed
    }

    pub fn get(&self, id: &str) -> Option<&Installed> {
        self.installed.iter().find(|p| p.id() == id)
    }

    /// Шаг установки, если плагин сейчас ставится.
    pub fn progress(&self, id: &str) -> Option<Progress> {
        match &*self.jobs.get(id)?.lock().ok()? {
            JobState::Running(p) => Some(p.clone()),
            _ => None,
        }
    }

    pub fn is_installing(&self, id: &str) -> bool {
        self.jobs.contains_key(id)
    }

    pub fn busy(&self) -> bool {
        !self.jobs.is_empty()
    }

    /// Чем кончилась последняя неудачная установка.
    pub fn error(&self, id: &str) -> Option<&str> {
        self.errors.get(id).map(String::as_str)
    }

    /// События с прошлого вызова.
    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    /// Начинает установку плагина из каталога в фоне. Уже идущая установка не повторяется.
    /// Плагин из файлов программы ставится сразу.
    pub fn install(&mut self, entry: &CatalogEntry) {
        let Some(root) = self.root.clone() else { return };
        let url = match &entry.source {
            Source::Download { url } => *url,
            Source::Bundled { files } => {
                self.errors.remove(entry.id);
                match install_bundled(&root, entry, files) {
                    Ok(()) => {
                        self.scan();
                        self.events.push(Event::Installed(entry.id.to_owned()));
                    }
                    Err(error) => {
                        log::warn!("Plugin {} failed to install: {error}", entry.id);
                        self.errors.insert(entry.id.to_owned(), error.clone());
                        self.events.push(Event::Failed { id: entry.id.to_owned(), error });
                    }
                }
                return;
            }
            Source::Later => return,
        };
        if self.jobs.contains_key(entry.id) {
            return;
        }
        self.errors.remove(entry.id);
        let state = Arc::new(Mutex::new(JobState::Running(Progress::Downloading { done: 0, total: None })));
        self.jobs.insert(entry.id.to_owned(), state.clone());
        let entry = entry.clone();
        let url = url.to_string();
        std::thread::spawn(move || {
            let result = download_and_install(&root, &entry, &url, &state);
            if let Ok(mut s) = state.lock() {
                *s = match result {
                    Ok(()) => JobState::Done,
                    Err(e) => JobState::Failed(e),
                };
            }
        });
    }

    /// Ставит плагин из `.zip` с `plugin.toml` (в корне архива или в его единственной папке).
    /// Плагин с тем же id заменяется. Возвращает id.
    pub fn install_file(&mut self, archive: &Path) -> Result<String, String> {
        let root = self.root.clone().ok_or("no plugins folder")?;
        let staging = root.join(".staging").join("from-file");
        let _ = std::fs::remove_dir_all(&staging);
        std::fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
        extract(archive, &staging)?;
        let content = single_child(&staging);
        let text = std::fs::read_to_string(content.join("plugin.toml")).map_err(|_| "the archive has no plugin.toml".to_owned())?;
        let manifest = Manifest::parse(&text)?;
        if manifest.plugin.kind == Kind::Tool {
            return Err("tools are installed from the catalog only".into());
        }
        let id = manifest.plugin.id.clone();
        replace_dir(&content, &root.join(&id))?;
        let _ = std::fs::remove_dir_all(&staging);
        self.scan();
        self.events.push(Event::Installed(id.clone()));
        Ok(id)
    }

    /// Удаляет плагин с диска. Программа сначала отпускает его файлы (останавливает сервер).
    pub fn uninstall(&mut self, id: &str) -> Result<(), String> {
        let dir = self.get(id).map(|p| p.dir.clone()).ok_or_else(|| format!("plugin {id} is not installed"))?;
        std::fs::remove_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        self.scan();
        self.events.push(Event::Removed(id.to_owned()));
        Ok(())
    }

    /// Каждый кадр: завершившиеся установки. `true` — что-то изменилось (стоит перерисовать).
    pub fn poll(&mut self) -> bool {
        if self.jobs.is_empty() {
            return false;
        }
        let mut finished = Vec::new();
        for (id, state) in &self.jobs {
            let s = state.lock().map(|s| s.clone()).unwrap_or(JobState::Failed("lost".into()));
            match s {
                JobState::Running(_) => {}
                JobState::Done => finished.push((id.clone(), None)),
                JobState::Failed(e) => finished.push((id.clone(), Some(e))),
            }
        }
        let changed = !finished.is_empty();
        for (id, error) in finished {
            self.jobs.remove(&id);
            match error {
                None => self.events.push(Event::Installed(id)),
                Some(error) => {
                    log::warn!("Plugin {id} failed to install: {error}");
                    self.errors.insert(id.clone(), error.clone());
                    self.events.push(Event::Failed { id, error });
                }
            }
        }
        if changed {
            self.scan();
        }
        true
    }

    /// Переносит уже установленную папку (например, из прежнего места программы) в плагины
    /// и пишет ей `plugin.toml`. Ничего не делает, если плагин уже стоит или папки нет.
    pub fn adopt(&mut self, entry: &CatalogEntry, old: &Path) -> Result<bool, String> {
        let Some(root) = self.root.clone() else { return Ok(false) };
        if self.get(entry.id).is_some() || !old.is_dir() {
            return Ok(false);
        }
        if let Some(check) = entry.validate {
            check(old)?;
        }
        std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        let target = root.join(entry.id);
        std::fs::rename(old, &target).map_err(|e| format!("{} → {}: {e}", old.display(), target.display()))?;
        write_manifest(&target, entry)?;
        self.scan();
        Ok(true)
    }
}

/// Манифест для плагина из каталога.
fn write_manifest(dir: &Path, entry: &CatalogEntry) -> Result<(), String> {
    let manifest = Manifest {
        plugin: Meta {
            id: entry.id.to_owned(),
            name: entry.name.clone(),
            version: "latest".into(),
            kind: entry.kind,
            description: entry.description.clone(),
            author: entry.author.into(),
            license: entry.license.into(),
            homepage: entry.homepage.into(),
            installed: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs()),
        },
        contributes: Contributes { themes: entry.themes.map(Into::into) },
    };
    std::fs::write(dir.join("plugin.toml"), manifest.to_toml()).map_err(|e| e.to_string())
}

/// Пишет файлы плагина из программы и ставит его на место прежней версии.
fn install_bundled(root: &Path, entry: &CatalogEntry, files: &[(&str, &str)]) -> Result<(), String> {
    let staging = root.join(".staging").join(entry.id);
    let _ = std::fs::remove_dir_all(&staging);
    for (path, text) in files {
        let target = staging.join(path);
        if let Some(dir) = target.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        std::fs::write(&target, text).map_err(|e| format!("{}: {e}", target.display()))?;
    }
    std::fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
    write_manifest(&staging, entry)?;
    replace_dir(&staging, &root.join(entry.id))
}

/// Скачивает архив, распаковывает, проверяет и ставит на место прежней версии.
fn download_and_install(root: &Path, entry: &CatalogEntry, url: &str, state: &Mutex<JobState>) -> Result<(), String> {
    let set = |p: Progress| {
        if let Ok(mut s) = state.lock() {
            *s = JobState::Running(p);
        }
    };
    let downloads = root.join(".downloads");
    std::fs::create_dir_all(&downloads).map_err(|e| e.to_string())?;
    let ext = if url.ends_with(".zip") { "zip" } else { "tar.gz" };
    let archive = downloads.join(format!("{}.{ext}", entry.id));
    let _ = std::fs::remove_file(&archive);

    // Размер — из заголовков, чтобы показывать проценты.
    let total = system_tool("curl").args(["-sIL", url]).output().ok().and_then(|o| {
        String::from_utf8_lossy(&o.stdout)
            .lines()
            .filter_map(|l| l.split_once(':').filter(|(k, _)| k.eq_ignore_ascii_case("content-length")).and_then(|(_, v)| v.trim().parse::<u64>().ok()))
            .next_back()
    });
    let mut child = system_tool("curl")
        .args(["-L", "-f", "-s", "-S", "-o"])
        .arg(&archive)
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("curl: {e}"))?;
    let status = loop {
        let done = std::fs::metadata(&archive).map(|m| m.len()).unwrap_or(0);
        set(Progress::Downloading { done, total });
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(150)),
            Err(e) => return Err(e.to_string()),
        }
    };
    if !status.success() {
        let mut err = String::new();
        if let Some(mut s) = child.stderr.take() {
            use std::io::Read;
            let _ = s.read_to_string(&mut err);
        }
        let _ = std::fs::remove_file(&archive);
        return Err(format!("curl: {}", err.trim()));
    }

    set(Progress::Extracting);
    let staging = root.join(".staging").join(entry.id);
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
    let extracted = extract(&archive, &staging);
    let _ = std::fs::remove_file(&archive);
    extracted?;
    let content = single_child(&staging);
    if let Some(check) = entry.validate {
        check(&content)?;
    }
    write_manifest(&content, entry)?;
    replace_dir(&content, &root.join(entry.id))?;
    let _ = std::fs::remove_dir_all(&staging);
    Ok(())
}

/// Распаковывает `.zip` или `.tar.gz` системным `tar` (он понимает оба формата).
fn extract(archive: &Path, into: &Path) -> Result<(), String> {
    let out = system_tool("tar").arg("-xf").arg(archive).arg("-C").arg(into).output().map_err(|e| format!("tar: {e}"))?;
    if out.status.success() { Ok(()) } else { Err(format!("tar: {}", String::from_utf8_lossy(&out.stderr).trim())) }
}

/// Архив с единственной папкой внутри — её содержимое и есть плагин.
fn single_child(dir: &Path) -> PathBuf {
    let entries: Vec<PathBuf> = std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).collect();
    match entries.as_slice() {
        [only] if only.is_dir() => only.clone(),
        _ => dir.to_path_buf(),
    }
}

/// Ставит `from` на место `to`: прежняя версия убирается только когда новая готова.
fn replace_dir(from: &Path, to: &Path) -> Result<(), String> {
    if to.exists() {
        std::fs::remove_dir_all(to).map_err(|e| format!("{}: {e}", to.display()))?;
    }
    std::fs::rename(from, to).map_err(|e| format!("{}: {e}", to.display()))
}

/// Системная программа: в Windows — из System32 (у Git свои `tar` и `curl`, и GNU tar
/// принимает `C:` в пути за имя сервера и не умеет zip).
pub fn system_tool(name: &str) -> Command {
    #[cfg(windows)]
    {
        let root = std::env::var_os("SystemRoot").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        let exe = root.join("System32").join(format!("{name}.exe"));
        let mut cmd = Command::new(if exe.is_file() { exe } else { PathBuf::from(name) });
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
        cmd
    }
    #[cfg(not(windows))]
    {
        Command::new(name)
    }
}

/// Размер для людей: «52 МБ», «900 КБ».
pub fn human_size(bytes: u64, mb: &str, kb: &str) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.0} {mb}", bytes as f64 / 1_048_576.0)
    } else {
        format!("{} {kb}", (bytes / 1024).max(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("forma-plugins-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn entry() -> CatalogEntry {
        CatalogEntry {
            id: "tool",
            name: "A tool".into(),
            description: String::new(),
            kind: Kind::Tool,
            size: None,
            author: "",
            license: "MIT",
            homepage: "",
            icon: "plugin",
            source: Source::Later,
            recommended: true,
            validate: Some(|dir| if dir.join("bin").is_dir() { Ok(()) } else { Err("no bin".into()) }),
            themes: None,
        }
    }

    #[test]
    fn bundled_theme_pack_installs_without_network() {
        let base = temp("bundled");
        let pack = CatalogEntry {
            id: "pink-pack",
            name: "Pink".into(),
            kind: Kind::Themes,
            icon: "palette",
            source: Source::Bundled { files: &[("themes/pink.toml", "[theme]
id = \"pink\"")] },
            validate: None,
            themes: Some("themes"),
            ..entry()
        };
        let mut plugins = Plugins::new(Some(base.join("plugins")));
        plugins.install(&pack);
        let p = plugins.get("pink-pack").expect("установлен сразу");
        assert_eq!(p.manifest.plugin.kind, Kind::Themes);
        assert!(p.themes_dir().unwrap().join("pink.toml").is_file());
        assert_eq!(plugins.take_events(), vec![Event::Installed("pink-pack".into())]);
        assert!(!base.join("plugins").join(".staging").join("pink-pack").exists(), "черновая папка убрана");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn scan_finds_plugins_and_skips_broken() {
        let root = temp("scan");
        std::fs::create_dir_all(root.join("pink")).unwrap();
        std::fs::write(root.join("pink").join("plugin.toml"), "[plugin]\nid = \"pink\"\nname = \"Pink\"\nkind = \"themes\"\n[contributes]\nthemes = \"themes\"").unwrap();
        std::fs::create_dir_all(root.join("broken")).unwrap();
        std::fs::write(root.join("broken").join("plugin.toml"), "[plugin\n").unwrap();
        std::fs::create_dir_all(root.join(".staging").join("x")).unwrap();
        let plugins = Plugins::new(Some(root.clone()));
        assert_eq!(plugins.installed().iter().map(|p| p.id()).collect::<Vec<_>>(), vec!["pink"]);
        assert_eq!(plugins.get("pink").unwrap().themes_dir(), Some(root.join("pink").join("themes")));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn adopt_moves_an_old_install_and_uninstall_removes_it() {
        let base = temp("adopt");
        let old = base.join("servers").join("tool");
        std::fs::create_dir_all(old.join("bin")).unwrap();
        let mut plugins = Plugins::new(Some(base.join("plugins")));
        assert!(plugins.adopt(&entry(), &old).unwrap());
        assert!(!old.exists(), "старая папка перенесена");
        let p = plugins.get("tool").unwrap();
        assert!(p.dir.join("bin").is_dir());
        assert_eq!(p.manifest.plugin.name, "A tool");
        assert!(!plugins.adopt(&entry(), &old).unwrap(), "второй раз ничего не делает");
        plugins.uninstall("tool").unwrap();
        assert!(plugins.get("tool").is_none());
        assert_eq!(plugins.take_events(), vec![Event::Removed("tool".into())]);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn adopt_refuses_an_invalid_folder() {
        let base = temp("invalid");
        let old = base.join("servers").join("tool");
        std::fs::create_dir_all(&old).unwrap();
        let mut plugins = Plugins::new(Some(base.join("plugins")));
        assert!(plugins.adopt(&entry(), &old).is_err());
        assert!(old.exists(), "непроверенная папка остаётся на месте");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn theme_pack_installs_from_zip() {
        let base = temp("zip");
        let src = base.join("src").join("pink-pack");
        std::fs::create_dir_all(src.join("themes")).unwrap();
        std::fs::write(src.join("plugin.toml"), "[plugin]\nid = \"pink-pack\"\nname = \"Pink pack\"\nkind = \"themes\"\n[contributes]\nthemes = \"themes\"").unwrap();
        std::fs::write(src.join("themes").join("pink.toml"), "[theme]\nname = \"Pink\"").unwrap();
        let zip = base.join("pink.zip");
        let out = system_tool("tar").arg("-a").arg("-cf").arg(&zip).arg("-C").arg(base.join("src")).arg("pink-pack").output().unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        let mut plugins = Plugins::new(Some(base.join("plugins")));
        assert_eq!(plugins.install_file(&zip).unwrap(), "pink-pack");
        let p = plugins.get("pink-pack").unwrap();
        assert!(p.themes_dir().unwrap().join("pink.toml").is_file());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn settings_toggle_plugins() {
        let mut s = PluginSettings::default();
        assert!(s.enabled("jdtls"));
        s.set_enabled("jdtls", false);
        s.set_enabled("jdtls", false);
        assert_eq!(s.disabled, vec!["jdtls".to_owned()]);
        s.set_enabled("jdtls", true);
        assert!(s.enabled("jdtls"));
        assert_eq!(human_size(52 * 1024 * 1024, "MB", "KB"), "52 MB");
        assert_eq!(human_size(900 * 1024, "MB", "KB"), "900 KB");
    }
}
