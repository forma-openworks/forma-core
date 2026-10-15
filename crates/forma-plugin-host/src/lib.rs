//! Code плагинов Forma: компоненты WebAssembly (`wasm32-wasip2`) в песочнице wasmtime, как
//! расширения редактора Zed. Плагин видит только API из `wit/forma-plugin.wit` и только то,
//! что разрешено в `[permissions]` его манифеста: файлы хранилища (`vault:read`,
//! `vault:write`), своё хранилище «ключ — значение» (`data`). Сети, переменных окружения и
//! чужих файлов у него нет.
//!
//! Как это работает:
//! - [`Runtime::sync`] берёт работающие плагины с `[code]` (итог проверки загрузчика) и
//!   компилирует их модули в фоне — первый вызов не ждёт компиляции;
//! - экземпляр плагина создаётся лениво, при первом вызове (подсказка, блок, команда), и
//!   сразу получает `activate`;
//! - каждый вызов ограничен по времени (эпохи wasmtime) и по памяти: зависший или упавший
//!   плагин отключается до перезапуска программы, а программа показывает почему
//!   ([`Runtime::take_notices`]).

use std::collections::BTreeMap;
use std::ops::Range;
use std::path::{Component as PathPart, Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime};

use forma_plugins::{Installed, Manifest, Permissions, Report};
use wasmtime::component::{Component, HasSelf, Linker, ResourceTable};
use wasmtime::{Config, Engine, Store, StoreLimits, StoreLimitsBuilder};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

wasmtime::component::bindgen!({ path: "wit", world: "plugin" });

use forma::plugin::types::{Entry, Level};

/// Шаг часов песочницы: каждые 50 мс счётчик эпох растёт на один.
const TICK: Duration = Duration::from_millis(50);
/// Сколько тиков дать одному вызову плагина (2 с): дольше — плагин завис.
const CALL_TICKS: u64 = 40;
/// Наибольшая память одного плагина.
const MAX_MEMORY: usize = 256 << 20;
/// Наибольший файл, который плагин может прочитать.
const MAX_FILE: u64 = 16 << 20;

/// Уведомление от плагина или о плагине: программа показывает его внизу окна.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    pub plugin: String,
    pub text: String,
    pub error: bool,
}

/// Подсказка при наведении: к каким символам текста она относится (в символах Unicode) и
/// что показать (markdown).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hover {
    pub plugin: String,
    pub range: Range<usize>,
    pub markdown: String,
}

/// Что видит code одного плагина.
struct State {
    wasi: WasiCtx,
    table: ResourceTable,
    limits: StoreLimits,
    plugin: String,
    permissions: Permissions,
    root: Option<PathBuf>,
    data_file: Option<PathBuf>,
    data: Option<BTreeMap<String, String>>,
    notices: Arc<Mutex<Vec<Notice>>>,
}

impl WasiView for State {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView { ctx: &mut self.wasi, table: &mut self.table }
    }
}

impl State {
    fn need(&self, permission: &str) -> Result<(), String> {
        if self.permissions.files.iter().any(|p| p == permission) {
            Ok(())
        } else {
            Err(format!("plugin {} has no permission {permission:?}", self.plugin))
        }
    }

    /// Путь внутри хранилища: только вниз от корня, без `..` и абсолютных путей.
    fn vault_path(&self, rel: &str) -> Result<PathBuf, String> {
        let root = self.root.as_ref().ok_or("no vault or project is open")?;
        let rel = rel.trim_start_matches(['/', '\\']);
        let safe = Path::new(rel).components().all(|c| matches!(c, PathPart::Normal(_) | PathPart::CurDir));
        if !safe {
            return Err(format!("path {rel:?} leaves the vault"));
        }
        Ok(root.join(rel))
    }

    fn data_map(&mut self) -> &mut BTreeMap<String, String> {
        if self.data.is_none() {
            let loaded = self.data_file.as_ref().and_then(|f| std::fs::read_to_string(f).ok()).and_then(|t| serde_json::from_str(&t).ok());
            self.data = Some(loaded.unwrap_or_default());
        }
        self.data.get_or_insert_default()
    }
}

impl forma::plugin::types::Host for State {}

impl forma::plugin::host::Host for State {
    fn log(&mut self, level: Level, message: String) {
        let level = match level {
            Level::Debug => log::Level::Debug,
            Level::Info => log::Level::Info,
            Level::Warn => log::Level::Warn,
            Level::Error => log::Level::Error,
        };
        log::log!(level, "plugin {}: {message}", self.plugin);
    }

    fn notify(&mut self, text: String, error: bool) {
        if let Ok(mut n) = self.notices.lock() {
            n.push(Notice { plugin: self.plugin.clone(), text, error });
        }
    }

    fn read_file(&mut self, path: String) -> Result<String, String> {
        self.need("vault:read").or_else(|_| self.need("vault:write"))?;
        let file = self.vault_path(&path)?;
        let meta = std::fs::metadata(&file).map_err(|e| format!("{path}: {e}"))?;
        if meta.len() > MAX_FILE {
            return Err(format!("{path}: the file is larger than {} MB", MAX_FILE >> 20));
        }
        std::fs::read_to_string(&file).map_err(|e| format!("{path}: {e}"))
    }

    fn list_dir(&mut self, path: String) -> Result<Vec<Entry>, String> {
        self.need("vault:read").or_else(|_| self.need("vault:write"))?;
        let dir = self.vault_path(&path)?;
        let mut out: Vec<Entry> = std::fs::read_dir(&dir)
            .map_err(|e| format!("{path}: {e}"))?
            .flatten()
            .map(|e| Entry { name: e.file_name().to_string_lossy().into_owned(), dir: e.file_type().is_ok_and(|t| t.is_dir()) })
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    fn write_file(&mut self, path: String, text: String) -> Result<(), String> {
        self.need("vault:write")?;
        let file = self.vault_path(&path)?;
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{path}: {e}"))?;
        }
        std::fs::write(&file, text).map_err(|e| format!("{path}: {e}"))
    }

    fn data_get(&mut self, key: String) -> Option<String> {
        self.need("data").ok()?;
        self.data_map().get(&key).cloned()
    }

    fn data_set(&mut self, key: String, value: String) -> Result<(), String> {
        self.need("data")?;
        self.data_map().insert(key, value);
        let file = self.data_file.clone().ok_or("no data folder")?;
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(self.data_map()).map_err(|e| e.to_string())?;
        std::fs::write(&file, json).map_err(|e| e.to_string())
    }
}

/// Скомпилированный модуль плагина (компиляция идёт в фоне).
enum Compiled {
    Pending,
    Ready(Component),
    Failed(String),
}

/// Работающий экземпляр плагина.
struct Live {
    store: Store<State>,
    bindings: Plugin,
}

/// Плагин с code.
struct Slot {
    manifest: Manifest,
    dir: PathBuf,
    /// Когда менялся модуль: пересобранный в папке разработчика плагин компилируется заново.
    stamp: Option<SystemTime>,
    compiled: Arc<Mutex<Compiled>>,
    live: Option<Live>,
    /// Почему плагин отключён (упал, завис, не загрузился).
    failed: Option<String>,
}

impl Slot {
    fn id(&self) -> &str {
        &self.manifest.plugin.id
    }

    fn wasm(&self) -> Option<PathBuf> {
        self.manifest.code.as_ref().map(|c| self.dir.join(&c.wasm))
    }
}

/// Code всех работающих плагинов программы.
pub struct Runtime {
    engine: Engine,
    linker: Linker<State>,
    app: String,
    language: String,
    root: Option<PathBuf>,
    /// Папка данных плагинов: `<папка>/<id>/data.json`.
    data_dir: Option<PathBuf>,
    slots: Vec<Slot>,
    notices: Arc<Mutex<Vec<Notice>>>,
}

/// Движок wasmtime — один на программу, с часами для ограничения времени вызовов.
fn engine() -> Result<Engine, String> {
    static ENGINE: OnceLock<Result<Engine, String>> = OnceLock::new();
    ENGINE
        .get_or_init(|| {
            let mut config = Config::new();
            config.wasm_component_model(true);
            config.epoch_interruption(true);
            let engine = Engine::new(&config).map_err(|e| e.to_string())?;
            let ticker = engine.clone();
            std::thread::Builder::new()
                .name("forma-plugin-clock".into())
                .spawn(move || {
                    loop {
                        std::thread::sleep(TICK);
                        ticker.increment_epoch();
                    }
                })
                .map_err(|e| e.to_string())?;
            Ok(engine)
        })
        .clone()
}

impl Runtime {
    /// `app` — программа (`holotype`), `data_dir` — папка данных плагинов.
    pub fn new(app: &str, data_dir: Option<PathBuf>) -> Result<Self, String> {
        let engine = engine()?;
        let mut linker = Linker::new(&engine);
        wasmtime_wasi::p2::add_to_linker_sync(&mut linker).map_err(|e| e.to_string())?;
        Plugin::add_to_linker::<State, HasSelf<State>>(&mut linker, |s| s).map_err(|e| e.to_string())?;
        Ok(Self { engine, linker, app: app.to_owned(), language: String::new(), root: None, data_dir, slots: Vec::new(), notices: Arc::default() })
    }

    /// Язык интерфейса — его получит `activate` следующих экземпляров.
    pub fn set_language(&mut self, language: &str) {
        self.language = language.to_owned();
    }

    /// Открытое хранилище или проект. Сменился — экземпляры создаются заново.
    pub fn set_root(&mut self, root: Option<&Path>) {
        if self.root.as_deref() != root {
            self.root = root.map(Path::to_path_buf);
            for s in &mut self.slots {
                s.live = None;
            }
        }
    }

    /// Плагины с code из работающих (в порядке подключения). Новые и изменённые модули
    /// компилируются в фоне; плагинов, которых больше нет, не становится. Вызывать можно
    /// каждый кадр: если плагины те же, ничего не происходит. `force` — ещё и проверить, не
    /// пересобрали ли модули (папки разработчика, кнопка «Обновить»).
    pub fn sync(&mut self, plugins: &[Installed], report: &Report, force: bool) {
        let wanted: Vec<&Installed> = report.order.iter().filter_map(|id| plugins.iter().find(|p| p.id() == id && !p.dev).or_else(|| plugins.iter().find(|p| p.id() == id))).filter(|p| p.manifest.code.is_some()).collect();
        let same = wanted.len() == self.slots.len() && wanted.iter().zip(&self.slots).all(|(p, s)| s.dir == p.dir && s.manifest == p.manifest);
        if same && !force {
            return;
        }
        let mut slots = Vec::with_capacity(wanted.len());
        for p in wanted {
            let wasm = p.manifest.code.as_ref().map(|c| p.dir.join(&c.wasm));
            let stamp = wasm.as_ref().and_then(|w| std::fs::metadata(w).ok()?.modified().ok());
            let keep = self.slots.iter().position(|s| s.id() == p.id() && s.dir == p.dir && s.manifest == p.manifest && s.stamp == stamp);
            match keep {
                Some(i) => slots.push(self.slots.swap_remove(i)),
                None => {
                    let slot = Slot { manifest: p.manifest.clone(), dir: p.dir.clone(), stamp, compiled: Arc::new(Mutex::new(Compiled::Pending)), live: None, failed: None };
                    self.compile(&slot);
                    slots.push(slot);
                }
            }
        }
        self.slots = slots;
    }

    /// Компилирует модуль плагина в фоне.
    fn compile(&self, slot: &Slot) {
        let (engine, cell, wasm, id) = (self.engine.clone(), slot.compiled.clone(), slot.wasm(), slot.id().to_owned());
        std::thread::spawn(move || {
            let result = match wasm {
                Some(w) => std::fs::read(&w).map_err(|e| format!("{}: {e}", w.display())).and_then(|bytes| Component::new(&engine, &bytes).map_err(|e| format!("{e:#}"))),
                None => Err("no [code]".into()),
            };
            if let Err(e) = &result {
                log::warn!("Plugin {id} failed to compile: {e}");
            }
            if let Ok(mut c) = cell.lock() {
                *c = match result {
                    Ok(c) => Compiled::Ready(c),
                    Err(e) => Compiled::Failed(e),
                };
            }
        });
    }

    /// Почему плагин отключён, если он отключён.
    pub fn failure(&self, id: &str) -> Option<String> {
        let slot = self.slots.iter().find(|s| s.id() == id)?;
        slot.failed.clone().or_else(|| match &*slot.compiled.lock().ok()? {
            Compiled::Failed(e) => Some(e.clone()),
            _ => None,
        })
    }

    /// Есть ли у работающих плагинов подсказки при наведении.
    pub fn has_hovers(&self) -> bool {
        self.slots.iter().any(|s| !s.manifest.contributes.hovers.is_empty() && s.failed.is_none())
    }

    /// Блоки каких языков рисуют плагины.
    pub fn block_langs(&self) -> Vec<String> {
        self.slots.iter().filter(|s| s.failed.is_none()).flat_map(|s| s.manifest.contributes.blocks.iter().map(|b| b.lang.clone())).collect()
    }

    /// Уведомления с прошлого раза.
    pub fn take_notices(&mut self) -> Vec<Notice> {
        self.notices.lock().map(|mut n| std::mem::take(&mut *n)).unwrap_or_default()
    }

    /// Готовит экземпляр плагина `i`: создаёт и активирует при первом вызове. `false` —
    /// модуль ещё компилируется или плагин отключён.
    fn ensure_live(&mut self, i: usize) -> bool {
        let slot = &self.slots[i];
        if slot.failed.is_some() {
            return false;
        }
        if slot.live.is_some() {
            return true;
        }
        let component = match &*slot.compiled.lock().expect("lock") {
            Compiled::Pending => return false,
            Compiled::Failed(_) => return false,
            Compiled::Ready(c) => c.clone(),
        };
        let id = slot.id().to_owned();
        let state = State {
            wasi: WasiCtxBuilder::new().build(),
            table: ResourceTable::new(),
            limits: StoreLimitsBuilder::new().memory_size(MAX_MEMORY).instances(16).build(),
            plugin: id.clone(),
            permissions: slot.manifest.permissions.clone(),
            root: self.root.clone(),
            data_file: self.data_dir.as_ref().map(|d| d.join(&id).join("data.json")),
            data: None,
            notices: self.notices.clone(),
        };
        let mut store = Store::new(&self.engine, state);
        store.limiter(|s| &mut s.limits);
        store.set_epoch_deadline(CALL_TICKS);
        let context = serde_json::json!({ "app": self.app, "language": self.language, "root": self.root.is_some() }).to_string();
        let started = Plugin::instantiate(&mut store, &component, &self.linker).map_err(|e| format!("{e:#}")).and_then(|bindings| {
            match bindings.call_activate(&mut store, &context) {
                Ok(Ok(())) => Ok(bindings),
                Ok(Err(e)) => Err(e),
                Err(trap) => Err(format!("{trap:#}")),
            }
        });
        match started {
            Ok(bindings) => {
                self.slots[i].live = Some(Live { store, bindings });
                true
            }
            Err(e) => {
                self.disable(i, e);
                false
            }
        }
    }

    /// Отключает плагин до перезапуска программы и сообщает почему.
    fn disable(&mut self, i: usize, reason: String) {
        let slot = &mut self.slots[i];
        log::warn!("Plugin {} is disabled: {reason}", slot.id());
        slot.live = None;
        if let Ok(mut n) = self.notices.lock() {
            n.push(Notice { plugin: slot.id().to_owned(), text: reason.clone(), error: true });
        }
        slot.failed = Some(reason);
    }

    /// Вызов в экземпляр плагина `i` с ограничением времени. Ловушка или зависание
    /// отключают плагин.
    fn call<R>(&mut self, i: usize, f: impl FnOnce(&Plugin, &mut Store<State>) -> wasmtime::Result<R>) -> Option<R> {
        if !self.ensure_live(i) {
            return None;
        }
        let live = self.slots[i].live.as_mut().expect("создан выше");
        live.store.set_epoch_deadline(CALL_TICKS);
        match f(&live.bindings, &mut live.store) {
            Ok(r) => Some(r),
            Err(trap) => {
                self.disable(i, format!("{trap:#}"));
                None
            }
        }
    }

    /// Подсказка при наведении на символ `offset` (в символах Unicode) текста `text` заметки
    /// `note`: первый плагин, у которого она есть.
    pub fn hover(&mut self, text: &str, offset: usize, note: &str) -> Option<Hover> {
        for i in 0..self.slots.len() {
            let hovers: Vec<String> = self.slots[i].manifest.contributes.hovers.iter().map(|h| h.id.clone()).collect();
            for hover in hovers {
                let request = HoverRequest { hover, text: text.to_owned(), offset: offset as u32, note: note.to_owned() };
                if let Some(Some(r)) = self.call(i, |b, s| b.call_hover(s, &request)) {
                    let len = text.chars().count();
                    let (start, end) = ((r.start as usize).min(len), (r.end as usize).min(len));
                    if start < end {
                        return Some(Hover { plugin: self.slots[i].id().to_owned(), range: start..end, markdown: r.markdown });
                    }
                }
            }
        }
        None
    }

    /// Блок заметки на языке `lang`: дерево элементов (JSON) от плагина, который его рисует.
    /// `None` — такого блока ни у кого нет (или плагин ещё готовится).
    pub fn render_block(&mut self, lang: &str, source: &str, context: &str) -> Option<Result<String, String>> {
        let i = self.slots.iter().position(|s| s.failed.is_none() && s.manifest.contributes.blocks.iter().any(|b| b.lang.eq_ignore_ascii_case(lang)))?;
        self.call(i, |b, s| b.call_render_block(s, lang, source, context))
    }

    /// Команда `id` (из манифеста какого-то плагина).
    pub fn command(&mut self, id: &str, args: &str) -> Option<Result<String, String>> {
        let i = self.slots.iter().position(|s| s.manifest.contributes.commands.iter().any(|c| c.id == id))?;
        self.call(i, |b, s| b.call_command(s, id, args))
    }

    /// В хранилище поменялись файлы: работающие экземпляры узнают об этом.
    pub fn changed(&mut self, paths: &[String]) {
        for i in 0..self.slots.len() {
            if self.slots[i].live.is_some() {
                let paths = paths.to_vec();
                self.call(i, |b, s| b.call_changed(s, &paths));
            }
        }
    }

    /// Ждёт, пока все модули скомпилируются (для проверок и тестов).
    pub fn wait_compiled(&self, timeout: Duration) -> bool {
        let start = std::time::Instant::now();
        loop {
            let pending = self.slots.iter().any(|s| matches!(&*s.compiled.lock().expect("lock"), Compiled::Pending));
            if !pending {
                return true;
            }
            if start.elapsed() > timeout {
                return false;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
