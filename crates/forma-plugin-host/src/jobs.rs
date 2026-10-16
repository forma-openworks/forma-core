//! Программы, которые запускают плагины (API 1.2, `spawn`): каждая — задача плагина.
//!
//! - Запустить можно только программу из `[permissions] process` манифеста: по имени (ищется
//!   в `PATH`) или полным путём к файлу с этим именем.
//! - Программа работает в фоне, её вывод читают отдельные потоки: строка за строкой, целиком
//!   (ответ Мьюри — одна строка JSON на десятки килобайт). Строка — UTF-8; не UTF-8 — на
//!   Windows это вывод старой консольной программы (`ping`, `cmd`) в кодировке консоли
//!   (CP866 у русской Windows), и он переводится из неё; иначе битые байты заменяются.
//!   Каждая строка будит кадр программы ([`Waker`]), а сами события плагин получает пачкой в
//!   начале кадра.
//! - На Windows задача живёт в Job Object с «убить всё при закрытии»: остановка задачи,
//!   выгрузка плагина, выход и даже падение программы закрывают его, и вместе с программой
//!   умирает всё, что она запустила (cargo, `claude -p`, программа Холо). Осиротевших
//!   процессов не остаётся.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::forma::plugin::types::{JobEvent, JobStream};

/// Будит кадр программы (у оболочки — `request_repaint`).
pub(crate) type Waker = Arc<Mutex<Option<Arc<dyn Fn() + Send + Sync>>>>;

/// Задачи всех плагинов и их события, которые ещё не отданы плагинам.
pub(crate) type Shared = Arc<Mutex<Jobs>>;

#[derive(Default)]
pub(crate) struct Jobs {
    next: u32,
    live: HashMap<u32, Entry>,
    events: Vec<(String, JobEvent)>,
}

struct Entry {
    plugin: String,
    child: Arc<Mutex<Child>>,
    stdin: Option<ChildStdin>,
    #[cfg(windows)]
    tree: Option<tree::Tree>,
}

impl Entry {
    /// Остановить программу и всё, что она запустила.
    fn kill(&self) {
        #[cfg(windows)]
        if let Some(t) = &self.tree {
            t.kill();
            return;
        }
        if let Ok(mut c) = self.child.lock() {
            let _ = c.kill();
        }
    }
}

fn wake(waker: &Waker) {
    let f = waker.lock().ok().and_then(|w| w.clone());
    if let Some(f) = f {
        f();
    }
}

fn push(jobs: &Shared, waker: &Waker, plugin: &str, event: JobEvent) {
    if let Ok(mut j) = jobs.lock() {
        j.events.push((plugin.to_owned(), event));
    }
    wake(waker);
}

/// Путь программы `program`, если плагину её можно запускать (`allowed` — `[permissions]
/// process`). Имя сравнивается без расширения и регистра: `blender` разрешает и
/// `blender.exe`, и `D:/…/blender.exe`.
pub(crate) fn resolve_program(program: &str, allowed: &[String]) -> Result<PathBuf, String> {
    let path = Path::new(program.trim());
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
    if stem.is_empty() || !allowed.iter().any(|a| a.trim().to_lowercase() == stem) {
        return Err(format!("the plugin may not run {program:?} (not in [permissions] process)"));
    }
    if path.components().count() > 1 {
        return if path.is_file() { Ok(path.to_path_buf()) } else { Err(format!("{program}: no such file")) };
    }
    let exts: &[&str] = if cfg!(windows) { &["", ".exe", ".cmd", ".bat", ".com"] } else { &[""] };
    let dirs = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect::<Vec<_>>()).unwrap_or_default();
    for dir in dirs {
        for ext in exts {
            let candidate = dir.join(format!("{program}{ext}"));
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }
    Err(format!("{program}: not found in PATH"))
}

/// Строки потока `r` — событиями `stream` задачи `job`.
fn read_lines(r: impl Read + Send + 'static, jobs: Shared, waker: Waker, plugin: String, job: u32, stream: JobStream) -> JoinHandle<()> {
    std::thread::spawn(move || {
        let mut r = BufReader::new(r);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match r.read_until(b'\n', &mut buf) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    while matches!(buf.last(), Some(b'\n' | b'\r')) {
                        buf.pop();
                    }
                    let text = decode(&buf);
                    push(&jobs, &waker, &plugin, JobEvent { job, kind: stream, text, code: None });
                }
            }
        }
    })
}

/// Строка вывода программы: UTF-8, а если нет — в кодировке консоли Windows.
fn decode(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_owned(),
        #[cfg(windows)]
        Err(_) => tree::oem(bytes).unwrap_or_else(|| String::from_utf8_lossy(bytes).into_owned()),
        #[cfg(not(windows))]
        Err(_) => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// Запускает программу; номер задачи. Проверки прав — у вызывающего.
pub(crate) fn spawn(jobs: &Shared, waker: &Waker, plugin: &str, program: &Path, args: &[String], cwd: &Path) -> Result<u32, String> {
    let mut cmd = Command::new(program);
    cmd.args(args).current_dir(cwd).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Без чёрного окна консоли поверх программы.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd.spawn().map_err(|e| format!("{}: {e}", program.display()))?;
    #[cfg(windows)]
    let tree = tree::Tree::new(&child);
    let (stdout, stderr, stdin) = (child.stdout.take(), child.stderr.take(), child.stdin.take());
    let child = Arc::new(Mutex::new(child));
    let id = {
        let mut j = jobs.lock().map_err(|_| "jobs are broken")?;
        j.next += 1;
        let id = j.next;
        j.live.insert(
            id,
            Entry {
                plugin: plugin.to_owned(),
                child: child.clone(),
                stdin,
                #[cfg(windows)]
                tree,
            },
        );
        id
    };
    let mut readers = Vec::new();
    if let Some(out) = stdout {
        readers.push(read_lines(out, jobs.clone(), waker.clone(), plugin.to_owned(), id, JobStream::Stdout));
    }
    if let Some(err) = stderr {
        readers.push(read_lines(err, jobs.clone(), waker.clone(), plugin.to_owned(), id, JobStream::Stderr));
    }
    let (jobs, waker, plugin) = (jobs.clone(), waker.clone(), plugin.to_owned());
    std::thread::spawn(move || {
        // Сначала дочитываем вывод: «завершилась» — последнее событие задачи.
        for r in readers {
            let _ = r.join();
        }
        let code = loop {
            match child.lock().map(|mut c| c.try_wait()) {
                Ok(Ok(Some(status))) => break status.code(),
                Ok(Ok(None)) => std::thread::sleep(Duration::from_millis(20)),
                _ => break None,
            }
        };
        // Запись задачи уходит (на Windows с ней закрывается Job Object — если что-то из
        // запущенного ещё живо, оно умирает вместе с задачей).
        if let Ok(mut j) = jobs.lock() {
            j.live.remove(&id);
        }
        push(&jobs, &waker, &plugin, JobEvent { job: id, kind: JobStream::Exit, text: String::new(), code });
    });
    Ok(id)
}

/// Остановить задачу `job` плагина `plugin` (чужие не трогаются).
pub(crate) fn cancel(jobs: &Shared, plugin: &str, job: u32) {
    if let Ok(j) = jobs.lock()
        && let Some(e) = j.live.get(&job).filter(|e| e.plugin == plugin)
    {
        e.kill();
    }
}

pub(crate) fn write_stdin(jobs: &Shared, plugin: &str, job: u32, text: &str) -> Result<(), String> {
    let mut j = jobs.lock().map_err(|_| "jobs are broken")?;
    let e = j.live.get_mut(&job).filter(|e| e.plugin == plugin).ok_or(format!("no job {job}"))?;
    let stdin = e.stdin.as_mut().ok_or("the job's input is closed")?;
    stdin.write_all(text.as_bytes()).and_then(|_| stdin.flush()).map_err(|e| e.to_string())
}

/// Остановить все задачи плагина (`None` — всех плагинов).
pub(crate) fn kill(jobs: &Shared, plugin: Option<&str>) {
    if let Ok(j) = jobs.lock() {
        for e in j.live.values().filter(|e| plugin.is_none_or(|p| e.plugin == p)) {
            e.kill();
        }
    }
}

/// События, которые ещё не отданы плагинам.
pub(crate) fn take_events(jobs: &Shared) -> Vec<(String, JobEvent)> {
    jobs.lock().map(|mut j| std::mem::take(&mut j.events)).unwrap_or_default()
}

/// Сколько задач плагина ещё работает.
pub(crate) fn running(jobs: &Shared, plugin: &str) -> usize {
    jobs.lock().map(|j| j.live.values().filter(|e| e.plugin == plugin).count()).unwrap_or(0)
}

/// Job Object Windows: программа и всё, что она запустит, умирают вместе с ним.
#[cfg(windows)]
mod tree {
    use std::os::windows::io::AsRawHandle;

    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::Globalization::{CP_OEMCP, MultiByteToWideChar};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation, SetInformationJobObject, TerminateJobObject,
    };

    pub(super) struct Tree(HANDLE);

    // Описатель Job Object можно закрыть и из другого потока.
    unsafe impl Send for Tree {}

    impl Tree {
        pub(super) fn new(child: &std::process::Child) -> Option<Tree> {
            // SAFETY: вызовы Win32 с нашими собственными описателями; ошибки проверяются.
            unsafe {
                let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
                if job.is_null() {
                    return None;
                }
                let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                let set = SetInformationJobObject(job, JobObjectExtendedLimitInformation, &info as *const _ as *const _, std::mem::size_of_val(&info) as u32);
                if set == 0 || AssignProcessToJobObject(job, child.as_raw_handle() as HANDLE) == 0 {
                    CloseHandle(job);
                    return None;
                }
                Some(Tree(job))
            }
        }

        pub(super) fn kill(&self) {
            // SAFETY: описатель живёт, пока жив `self`.
            unsafe {
                TerminateJobObject(self.0, 1);
            }
        }
    }

    /// Текст в кодировке консоли (OEM) → строка; `None` — не перевелось.
    pub(super) fn oem(bytes: &[u8]) -> Option<String> {
        let len = i32::try_from(bytes.len()).ok()?;
        // SAFETY: буферы наши, их длины переданы честно.
        unsafe {
            let n = MultiByteToWideChar(CP_OEMCP, 0, bytes.as_ptr(), len, std::ptr::null_mut(), 0);
            if n <= 0 {
                return None;
            }
            let mut wide = vec![0u16; n as usize];
            let n = MultiByteToWideChar(CP_OEMCP, 0, bytes.as_ptr(), len, wide.as_mut_ptr(), n);
            (n > 0).then(|| String::from_utf16_lossy(&wide[..n as usize]))
        }
    }

    impl Drop for Tree {
        fn drop(&mut self) {
            // SAFETY: закрываем свой описатель один раз.
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wait_exit(jobs: &Shared, job: u32) -> Vec<JobEvent> {
        let start = std::time::Instant::now();
        let mut got = Vec::new();
        while start.elapsed() < Duration::from_secs(20) {
            got.extend(take_events(jobs).into_iter().map(|(_, e)| e).filter(|e| e.job == job));
            if got.iter().any(|e| e.kind == JobStream::Exit) {
                return got;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("задача {job} не завершилась: {got:?}");
    }

    #[test]
    fn only_permitted_programs_run() {
        let allowed = vec!["cmd".to_owned()];
        assert!(resolve_program("learn", &allowed).unwrap_err().contains("[permissions] process"));
        assert!(resolve_program("C:/x/learn.exe", &allowed).is_err());
        #[cfg(windows)]
        assert!(resolve_program("cmd", &allowed).unwrap().to_string_lossy().to_lowercase().ends_with("cmd.exe"));
    }

    #[cfg(windows)]
    #[test]
    fn output_comes_line_by_line_whole_and_then_exit() {
        let jobs = Shared::default();
        let woke = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let w = woke.clone();
        let waker: Waker = Arc::new(Mutex::new(Some(Arc::new(move || {
            w.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }))));
        let cmd = resolve_program("cmd", &["cmd".to_owned()]).unwrap();
        // Строка длиннее командной строки Windows (32 767) — из файла.
        let dir = std::env::temp_dir().join(format!("forma-jobs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("long.txt"), format!("{}
", "x".repeat(70_000))).unwrap();
        let job = spawn(&jobs, &waker, "p", &cmd, &["/c".into(), "echo first & type long.txt & echo oops 1>&2 & exit 3".into()], &dir).unwrap();
        let ev = wait_exit(&jobs, job);
        let out: Vec<&str> = ev.iter().filter(|e| e.kind == JobStream::Stdout).map(|e| e.text.trim_end()).collect();
        assert_eq!(out.len(), 2);
        assert_eq!(out[1].len(), 70_000, "длинная строка не обрезана");
        assert!(ev.iter().any(|e| e.kind == JobStream::Stderr && e.text.contains("oops")));
        assert_eq!(ev.last().map(|e| (e.kind, e.code)), Some((JobStream::Exit, Some(3))));
        assert!(woke.load(std::sync::atomic::Ordering::SeqCst) >= 3, "каждое событие будит кадр");
        assert_eq!(running(&jobs, "p"), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn utf8_stays_and_console_text_is_decoded() {
        assert_eq!(decode("Мьюри ✓".as_bytes()), "Мьюри ✓");
        // «Привет» в CP866 — кодировке консоли русской Windows.
        #[cfg(windows)]
        if unsafe { windows_sys::Win32::Globalization::GetOEMCP() } == 866 {
            assert_eq!(decode(&[0x8F, 0xE0, 0xA8, 0xA2, 0xA5, 0xE2]), "Привет");
        }
    }

    #[cfg(windows)]
    #[test]
    fn cancel_kills_the_whole_tree() {
        let jobs = Shared::default();
        let waker: Waker = Arc::default();
        let cmd = resolve_program("cmd", &["cmd".to_owned()]).unwrap();
        // cmd запускает ещё один cmd, а тот ждёт ~30 секунд.
        let job = spawn(&jobs, &waker, "p", &cmd, &["/c".into(), "cmd /c ping -n 30 127.0.0.1 > nul".into()], &std::env::temp_dir()).unwrap();
        std::thread::sleep(Duration::from_millis(300));
        cancel(&jobs, "other", job);
        assert_eq!(running(&jobs, "p"), 1, "чужой плагин не останавливает");
        let start = std::time::Instant::now();
        cancel(&jobs, "p", job);
        let ev = wait_exit(&jobs, job);
        assert!(start.elapsed() < Duration::from_secs(5), "дерево остановлено сразу, не через 30 секунд");
        assert_eq!(ev.last().map(|e| e.kind), Some(JobStream::Exit));
    }
}
