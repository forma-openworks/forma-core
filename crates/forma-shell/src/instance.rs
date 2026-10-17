//! Один экземпляр программы на папку и «открыть в другой программе Forma».
//!
//! Как у IDE от JetBrains и VS Code: если проект уже открыт, второй запуск с ним не
//! открывает второе окно, а передаёт открытому, что открыть (`--open`), и тот выходит
//! вперёд. Программа слушает свою папку (хранилище, проект) — [`Listener`]; второй запуск
//! сначала пробует [`forward`]. Канал — именованный канал Windows (`\\.\pipe\forma-…`),
//! по одному на программу и папку.
//!
//! Каждая программа Forma при запуске записывает, где лежит её `.exe` ([`register`]), — так
//! плагин одной программы может открыть файл в другой ([`open_app`]): кнопка «Открыть в
//! Instar» в главе курса в Holotype.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};

/// Что передаётся открытой программе: что открыть в её папке.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Message {
    pub open: String,
}

/// Папка с записями «программа → её `.exe`»: `%LOCALAPPDATA%\Forma\apps`, с
/// `FORMA_CONFIG_HOME` — `$FORMA_CONFIG_HOME\apps` (проверки не трогают настоящие записи).
fn registry_dir() -> Option<PathBuf> {
    match std::env::var_os(forma_settings::CONFIG_HOME_VAR).filter(|v| !v.is_empty()) {
        Some(home) => Some(PathBuf::from(home).join("apps")),
        None => dirs::data_local_dir().map(|d| d.join("Forma").join("apps")),
    }
}

/// Запомнить, где лежит `.exe` этой программы (`app` — как в манифестах плагинов: `instar`).
pub fn register(app: &str) {
    let (Some(dir), Ok(exe)) = (registry_dir(), std::env::current_exe()) else { return };
    let file = dir.join(format!("{app}.json"));
    let text = serde_json::json!({ "exe": exe }).to_string();
    if std::fs::read_to_string(&file).ok().as_deref() == Some(text.as_str()) {
        return;
    }
    if let Err(e) = std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&file, text)) {
        log::warn!("Could not register {app} in {}: {e}", dir.display());
    }
}

/// Где лежит `.exe` программы `app`: из записи [`register`], а нет её — `forma-<app>` в PATH.
pub fn exe_of(app: &str) -> Option<PathBuf> {
    let recorded = registry_dir()
        .and_then(|d| std::fs::read_to_string(d.join(format!("{app}.json"))).ok())
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|v| v.get("exe")?.as_str().map(PathBuf::from))
        .filter(|p| p.is_file());
    recorded.or_else(|| {
        let name = format!("forma-{app}{}", std::env::consts::EXE_SUFFIX);
        std::env::split_paths(&std::env::var_os("PATH")?).map(|d| d.join(&name)).find(|p| p.is_file())
    })
}

/// Папка как ключ: полный путь без `\\?\`, без регистра (Windows), без разделителя на конце.
fn root_key(root: &Path) -> String {
    let full = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let s = full.to_string_lossy();
    let s = s.strip_prefix(r"\\?\").unwrap_or(&s);
    let s = s.trim_end_matches(['\\', '/']);
    if cfg!(windows) { s.to_lowercase().replace('/', "\\") } else { s.to_owned() }
}

/// Имя канала программы `app` с папкой `root`. В него входит и `FORMA_CONFIG_HOME`: проверка в
/// песочнице не достучится до настоящей программы.
pub fn channel_name(app: &str, root: &Path) -> String {
    let home = std::env::var(forma_settings::CONFIG_HOME_VAR).unwrap_or_default();
    // FNV-1a: стабильный между запусками (в отличие от `DefaultHasher`).
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in root_key(root).bytes().chain(*b"|").chain(home.bytes()) {
        h ^= b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!(r"\\.\pipe\forma-{app}-{h:016x}")
}

/// Передать `open` программе `app`, у которой открыта папка `root`. `true` — она получила;
/// `false` — такой программы нет (или канал недоступен).
pub fn forward(app: &str, root: &Path, open: &str) -> bool {
    let text = serde_json::to_string(&Message { open: open.to_owned() }).unwrap_or_default();
    sys::send(&channel_name(app, root), &text).is_ok()
}

/// Открыть в программе Forma `app` папку `root` и в ней `open` (файл или заметку; пусто —
/// только папку): уже открытая с этой папкой программа получает `open`, иначе программа
/// запускается (`<exe> <root> --open <open>`). Ошибка — словами.
pub fn open_app(app: &str, root: &Path, open: &str) -> Result<(), String> {
    if forward(app, root, open) {
        return Ok(());
    }
    let exe = exe_of(app).ok_or_else(|| format!("forma-{app} not found: start it once so Forma knows where it is"))?;
    let mut cmd = std::process::Command::new(&exe);
    cmd.arg(root).current_dir(root);
    if !open.is_empty() {
        cmd.arg("--open").arg(open);
    }
    sys::detach(&mut cmd);
    cmd.spawn().map(|_| ()).map_err(|e| format!("{}: {e}", exe.display()))
}

/// Что открыть из аргументов запуска: значение `--open`.
pub fn open_arg(args: &[std::ffi::OsString]) -> Option<String> {
    let i = args.iter().position(|a| a == "--open")?;
    args.get(i + 1).map(|a| a.to_string_lossy().into_owned())
}

/// Папка из аргументов запуска: первый аргумент, который не `--open` и не его значение.
pub fn root_arg(args: &[std::ffi::OsString]) -> Option<PathBuf> {
    let mut skip = false;
    for a in args.iter().skip(1) {
        if skip {
            skip = false;
            continue;
        }
        if a == "--open" {
            skip = true;
            continue;
        }
        if !a.to_string_lossy().starts_with("--") {
            return Some(PathBuf::from(a));
        }
    }
    None
}

/// Слушает канал своей папки: что просят открыть другие запуски и программы. Пока живёт —
/// второй запуск с этой папкой передаёт ему, а не открывает окно.
pub struct Listener {
    name: String,
    rx: Receiver<Message>,
    stop: Arc<AtomicBool>,
}

impl Listener {
    /// Начать слушать папку `root` программы `app`. `wake` будит кадр программы, когда пришла
    /// просьба. `None` — канал уже занят другим экземпляром (или его не создать).
    pub fn start(app: &str, root: &Path, wake: impl Fn() + Send + 'static) -> Option<Self> {
        let name = channel_name(app, root);
        let (tx, rx) = channel();
        let stop = Arc::new(AtomicBool::new(false));
        sys::serve(&name, tx, stop.clone(), Box::new(wake))?;
        Some(Self { name, rx, stop })
    }

    /// Просьбы с прошлого раза.
    pub fn take(&self) -> Vec<Message> {
        self.rx.try_iter().collect()
    }
}

impl Drop for Listener {
    /// Канал закрывается: поток слушателя ждёт в `ConnectNamedPipe` — его будит пустое
    /// сообщение.
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = sys::send(&self.name, "");
    }
}

/// Разбор сообщения канала.
fn parse(text: &str) -> Option<Message> {
    serde_json::from_str(text.trim()).ok()
}

#[cfg(windows)]
mod sys {
    use std::io::Write;
    use std::os::windows::io::AsRawHandle;
    use std::os::windows::process::CommandExt;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::Sender;

    use windows_sys::Win32::Foundation::{CloseHandle, ERROR_PIPE_CONNECTED, GetLastError, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_INBOUND, ReadFile};
    use windows_sys::Win32::System::Pipes::{ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, GetNamedPipeServerProcessId, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT};
    use windows_sys::Win32::UI::WindowsAndMessaging::{ASFW_ANY, AllowSetForegroundWindow};

    use super::Message;

    /// Занят: все экземпляры канала обслуживают других — подождать и повторить.
    const ERROR_PIPE_BUSY: i32 = 231;

    /// Запуск без консоли и отдельно от группы процессов вызвавшего.
    pub fn detach(cmd: &mut std::process::Command) {
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
        // Новая программа выйдет вперёд: щелчок был у нас, мы и разрешаем.
        unsafe { AllowSetForegroundWindow(ASFW_ANY) };
    }

    /// Записать `text` в канал `name`. Программе на том конце разрешается выйти вперёд.
    pub fn send(name: &str, text: &str) -> std::io::Result<()> {
        let mut tries = 0;
        loop {
            match std::fs::OpenOptions::new().write(true).open(name) {
                Ok(mut f) => {
                    let mut pid = 0u32;
                    if unsafe { GetNamedPipeServerProcessId(f.as_raw_handle() as HANDLE, &mut pid) } != 0 {
                        unsafe { AllowSetForegroundWindow(pid) };
                    }
                    return f.write_all(text.as_bytes());
                }
                Err(e) if e.raw_os_error() == Some(ERROR_PIPE_BUSY) && tries < 50 => {
                    tries += 1;
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
                Err(e) => return Err(e),
            }
        }
    }

    fn create(wide: &[u16], first: bool) -> Option<HANDLE> {
        let flags = PIPE_ACCESS_INBOUND | if first { FILE_FLAG_FIRST_PIPE_INSTANCE } else { 0 };
        let h = unsafe { CreateNamedPipeW(wide.as_ptr(), flags, PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS, PIPE_UNLIMITED_INSTANCES, 0, 64 * 1024, 0, std::ptr::null()) };
        (h != INVALID_HANDLE_VALUE).then_some(h)
    }

    fn read_all(h: HANDLE) -> String {
        let mut out = Vec::new();
        let mut buf = [0u8; 4096];
        loop {
            let mut n = 0u32;
            let ok = unsafe { ReadFile(h, buf.as_mut_ptr(), buf.len() as u32, &mut n, std::ptr::null_mut()) } != 0;
            if !ok || n == 0 {
                break;
            }
            out.extend_from_slice(&buf[..n as usize]);
            if out.len() > 1 << 20 {
                break;
            }
        }
        String::from_utf8_lossy(&out).into_owned()
    }

    /// Поток-слушатель. Новый экземпляр канала создаётся сразу, как только кто-то подключился,
    /// — так у канала всегда есть хотя бы один экземпляр, и второй запуск видит, что папка
    /// занята (а не запускает второе окно в щель между экземплярами).
    pub fn serve(name: &str, tx: Sender<Message>, stop: Arc<AtomicBool>, wake: Box<dyn Fn() + Send>) -> Option<()> {
        let wide: Vec<u16> = name.encode_utf16().chain([0]).collect();
        let first = create(&wide, true)?;
        // Сырой HANDLE — указатель; в поток он уходит числом.
        let first = first as usize;
        std::thread::Builder::new()
            .name("forma-instance".into())
            .spawn(move || {
                let mut next = first as HANDLE;
                loop {
                    let h = next;
                    let connected = unsafe { ConnectNamedPipe(h, std::ptr::null_mut()) } != 0 || unsafe { GetLastError() } == ERROR_PIPE_CONNECTED;
                    if stop.load(Ordering::SeqCst) {
                        unsafe { CloseHandle(h) };
                        break;
                    }
                    let Some(n) = create(&wide, false) else {
                        unsafe { CloseHandle(h) };
                        break;
                    };
                    next = n;
                    let text = if connected { read_all(h) } else { String::new() };
                    unsafe {
                        DisconnectNamedPipe(h);
                        CloseHandle(h);
                    }
                    if let Some(m) = super::parse(&text) {
                        if tx.send(m).is_err() {
                            break;
                        }
                        wake();
                    }
                }
                unsafe { CloseHandle(next) };
            })
            .ok()?;
        Some(())
    }
}

#[cfg(not(windows))]
mod sys {
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use std::sync::mpsc::Sender;

    use super::Message;

    pub fn detach(_cmd: &mut std::process::Command) {}

    /// Пока только Windows: на других системах второй запуск открывает своё окно.
    pub fn send(_name: &str, _text: &str) -> std::io::Result<()> {
        Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "single instance is Windows-only for now"))
    }

    pub fn serve(_name: &str, _tx: Sender<Message>, _stop: Arc<AtomicBool>, _wake: Box<dyn Fn() + Send>) -> Option<()> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_args_give_the_folder_and_what_to_open() {
        let args: Vec<std::ffi::OsString> = ["forma-instar", "--open", r"C:\P\0.1\a\src\lib.rs:3", r"C:\P"].iter().map(Into::into).collect();
        assert_eq!(root_arg(&args), Some(PathBuf::from(r"C:\P")));
        assert_eq!(open_arg(&args).as_deref(), Some(r"C:\P\0.1\a\src\lib.rs:3"));
        let plain: Vec<std::ffi::OsString> = ["forma-holotype", "D:/Vault"].iter().map(Into::into).collect();
        assert_eq!((root_arg(&plain), open_arg(&plain)), (Some(PathBuf::from("D:/Vault")), None));
    }

    #[test]
    fn channel_names_ignore_case_and_trailing_slashes() {
        let a = channel_name("instar", Path::new(r"C:\NoSuchDir\Practice"));
        assert_eq!(a, channel_name("instar", Path::new(r"c:\nosuchdir\practice\")));
        assert_ne!(a, channel_name("holotype", Path::new(r"C:\NoSuchDir\Practice")));
        assert!(a.starts_with(r"\\.\pipe\forma-instar-"));
    }

    /// Второй запуск передаёт `open` первому, а не открывает своё окно; после закрытия
    /// слушателя передавать некому.
    #[cfg(windows)]
    #[test]
    fn second_launch_forwards_to_the_listener() {
        let root = std::env::temp_dir().join(format!("forma-instance-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let woke = Arc::new(AtomicBool::new(false));
        let w = woke.clone();
        let l = Listener::start("test", &root, move || w.store(true, Ordering::SeqCst)).expect("канал создаётся");
        assert!(Listener::start("test", &root, || {}).is_none(), "второй слушатель той же папки не создаётся");
        assert!(forward("test", &root, "src/lib.rs:7"));
        assert!(forward("test", &root, "Глава#Раздел"));
        let mut got = Vec::new();
        for _ in 0..100 {
            got.extend(l.take());
            if got.len() == 2 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(got, vec![Message { open: "src/lib.rs:7".into() }, Message { open: "Глава#Раздел".into() }]);
        assert!(woke.load(Ordering::SeqCst));
        drop(l);
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(!forward("test", &root, "x"), "слушателя нет — передавать некому");
        let _ = std::fs::remove_dir_all(&root);
    }
}
