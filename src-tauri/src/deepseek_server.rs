use tauri::Manager;

use std::io::Read;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::ollama::{deepseek_local_up_fast, DEEPSEEK_LOCAL_HEALTH_URL};

// The first start after boot has taken about two minutes (Python loading Playwright and the
// rest of the venv). Cutting it at 20 s killed the process, so the button could never succeed.
const START_LIMIT: Duration = Duration::from_secs(180);
// A process of ours that is alive but not answering: give it a moment, then report.
const RESTART_LIMIT: Duration = Duration::from_secs(20);
const POLL: Duration = Duration::from_millis(200);

pub struct Control {
    gate: Mutex<()>,
    child: Mutex<Option<Child>>,
    owned: AtomicBool,
    // Set by stop and by closing the app, so a start that is still waiting lets go of `gate`.
    abort: AtomicBool,
    output: Arc<OutputTap>,
}

struct OutputTap {
    text: Mutex<String>,
    readers: AtomicUsize,
}

impl Default for Control {
    fn default() -> Self {
        Self {
            gate: Mutex::new(()),
            child: Mutex::new(None),
            owned: AtomicBool::new(false),
            abort: AtomicBool::new(false),
            output: Arc::new(OutputTap {
                text: Mutex::new(String::new()),
                readers: AtomicUsize::new(0),
            }),
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn deepseek_dir() -> PathBuf {
    crate::runtime_paths::tool_dir("Deepseek-API")
}

fn python_exe() -> PathBuf {
    deepseek_dir()
        .join("venv")
        .join("Scripts")
        .join("python.exe")
}

fn missing_python() -> String {
    "Chưa có Deepseek-API\\venv\\Scripts\\python.exe. Trong thư mục Deepseek-API chạy: python -m venv venv, pip install -r requirements.txt, playwright install chromium, rồi python -m deepseek.auth.".into()
}

pub fn start(control: &Control) -> Result<(), String> {
    let _gate = lock(&control.gate);
    if deepseek_local_up_fast() {
        return Ok(());
    }
    let python = python_exe();
    if !python.is_file() {
        return Err(missing_python());
    }
    if control.abort.load(Ordering::SeqCst) {
        return Err("Đã dừng.".into());
    }
    if child_running(control) {
        return finish_start(control, RESTART_LIMIT);
    }
    wait_readers(&control.output);
    *lock(&control.output.text) = String::new();
    let mut command = Command::new(&python);
    command
        .arg("app.py")
        .current_dir(deepseek_dir())
        .env("PYTHONUTF8", "1")
        .env("PYTHONIOENCODING", "utf-8")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = crate::hidden(&mut command)
        .spawn()
        .map_err(|error| format!("Không bật được DeepSeek. {error}"))?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    *lock(&control.child) = Some(child);
    control.owned.store(true, Ordering::SeqCst);
    if let Some(pipe) = stdout {
        watch_pipe(pipe, Arc::clone(&control.output));
    }
    if let Some(pipe) = stderr {
        watch_pipe(pipe, Arc::clone(&control.output));
    }
    finish_start(control, START_LIMIT)
}

fn finish_start(control: &Control, limit: Duration) -> Result<(), String> {
    match wait_for_server(
        deepseek_local_up_fast,
        || child_running(control),
        &control.abort,
        limit,
        POLL,
    ) {
        Wait::Ready => Ok(()),
        Wait::Stopped => Err("Đã dừng.".into()),
        Wait::Exited | Wait::TimedOut => Err(startup_failure(control)),
    }
}

#[derive(Debug, PartialEq)]
enum Wait {
    Ready,
    // The process ended before the server answered, so waiting longer cannot help.
    Exited,
    TimedOut,
    Stopped,
}

fn wait_for_server(
    mut ready: impl FnMut() -> bool,
    mut alive: impl FnMut() -> bool,
    stop: &AtomicBool,
    limit: Duration,
    step: Duration,
) -> Wait {
    let started = Instant::now();
    loop {
        if ready() {
            return Wait::Ready;
        }
        if stop.load(Ordering::SeqCst) {
            return Wait::Stopped;
        }
        if !alive() {
            return Wait::Exited;
        }
        if started.elapsed() >= limit {
            return Wait::TimedOut;
        }
        thread::sleep(step);
    }
}

// A start can hold `gate` for minutes. Raise the flag first so it lets go within a poll.
fn take_gate_after_aborting(control: &Control) -> std::sync::MutexGuard<'_, ()> {
    control.abort.store(true, Ordering::SeqCst);
    lock(&control.gate)
}

pub fn stop(control: &Control) -> Result<(), String> {
    let _gate = take_gate_after_aborting(control);
    kill_owned(control);
    for pid in listeners_on_8000() {
        kill_pid(pid);
    }
    let _ = wait_until(|| !deepseek_local_up_fast(), Duration::from_secs(5));
    // Any wait that was running has ended. Let the next start run normally.
    control.abort.store(false, Ordering::SeqCst);
    if deepseek_local_up_fast() {
        Err("Chưa tắt được server tại cổng 8000.".into())
    } else {
        Ok(())
    }
}

/// Đóng app thì chỉ tắt process do nút Bật đã mở.
pub fn stop_owned(control: &Control) {
    if !control.owned.load(Ordering::SeqCst) {
        return;
    }
    let _gate = take_gate_after_aborting(control);
    if control.owned.load(Ordering::SeqCst) {
        kill_owned(control);
    }
    control.abort.store(false, Ordering::SeqCst);
}

fn child_running(control: &Control) -> bool {
    let mut slot = lock(&control.child);
    let running = match slot.as_mut() {
        Some(child) => matches!(child.try_wait(), Ok(None)),
        None => false,
    };
    if !running {
        *slot = None;
    }
    running
}

fn kill_owned(control: &Control) {
    let child = lock(&control.child).take();
    if let Some(mut child) = child {
        kill_child(&mut child);
    }
    control.owned.store(false, Ordering::SeqCst);
}

fn kill_child(child: &mut Child) {
    kill_pid(child.id());
    let _ = child.kill();
    let _ = child.wait();
}

fn startup_failure(control: &Control) -> String {
    kill_owned(control);
    wait_readers(&control.output);
    startup_message(&lock(&control.output.text))
}

fn startup_message(log: &str) -> String {
    let head = format!("Đã mở DeepSeek nhưng chưa thấy {DEEPSEEK_LOCAL_HEALTH_URL}.");
    let detail = log_tail(log);
    if detail.is_empty() {
        head
    } else {
        format!("{head}\n{detail}")
    }
}

fn log_tail(log: &str) -> String {
    let lines: Vec<&str> = log
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    if lines.is_empty() {
        return String::new();
    }
    let start = lines.len().saturating_sub(6);
    let mut text = lines[start..].join("\n");
    const MAX_CHARS: usize = 360;
    let count = text.chars().count();
    if count > MAX_CHARS {
        text = text.chars().skip(count - MAX_CHARS).collect();
    }
    text
}

fn remember_output(slot: &Mutex<String>, chunk: &str) {
    const LIMIT: usize = 4000;
    let mut text = lock(slot);
    text.push_str(chunk);
    if text.len() <= LIMIT {
        return;
    }
    let mut cut = text.len() - LIMIT;
    while cut < text.len() && !text.is_char_boundary(cut) {
        cut += 1;
    }
    text.replace_range(..cut, "");
}

fn watch_pipe(mut pipe: impl Read + Send + 'static, tap: Arc<OutputTap>) {
    tap.readers.fetch_add(1, Ordering::SeqCst);
    thread::spawn(move || {
        let mut buf = [0u8; 1024];
        loop {
            match pipe.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => remember_output(&tap.text, &String::from_utf8_lossy(&buf[..n])),
            }
        }
        tap.readers.fetch_sub(1, Ordering::SeqCst);
    });
}

fn wait_readers(tap: &OutputTap) {
    let started = Instant::now();
    while tap.readers.load(Ordering::SeqCst) > 0 && started.elapsed() < Duration::from_millis(400)
    {
        thread::sleep(Duration::from_millis(20));
    }
}

fn kill_pid(pid: u32) {
    if pid < 10 || pid == std::process::id() {
        return;
    }
    let mut command = Command::new("taskkill");
    command.args(["/PID", &pid.to_string(), "/T", "/F"]);
    let _ = crate::hidden(&mut command)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

fn listeners_on_8000() -> Vec<u32> {
    let mut command = Command::new("netstat");
    command.args(["-ano", "-p", "tcp"]);
    let Ok(text) = crate::capture(command, Duration::from_secs(4)) else {
        return Vec::new();
    };
    listening_pids(&text)
}

fn wait_until(mut ready: impl FnMut() -> bool, limit: Duration) -> bool {
    let started = Instant::now();
    loop {
        if ready() {
            return true;
        }
        if started.elapsed() >= limit {
            return false;
        }
        thread::sleep(Duration::from_millis(200));
    }
}

fn listening_pids(text: &str) -> Vec<u32> {
    let mut pids = Vec::new();
    for line in text.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 5 || !parts[3].eq_ignore_ascii_case("LISTENING") {
            continue;
        }
        let Some((host, port)) = split_host_port(parts[1]) else {
            continue;
        };
        if port != 8000 || !is_local_bind(host) {
            continue;
        }
        if let Ok(pid) = parts[4].parse::<u32>() {
            if pid >= 10 && !pids.contains(&pid) {
                pids.push(pid);
            }
        }
    }
    pids
}

fn split_host_port(addr: &str) -> Option<(&str, u16)> {
    let (host, port) = addr.rsplit_once(':')?;
    Some((host, port.parse().ok()?))
}

fn is_local_bind(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "0.0.0.0" | "[::]" | "[::1]" | "*")
}

#[tauri::command]
pub async fn deepseek_server_running() -> bool {
    tauri::async_runtime::spawn_blocking(deepseek_local_up_fast)
        .await
        .unwrap_or(false)
}

#[tauri::command]
pub async fn start_deepseek_server(app: tauri::AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || start(&app.state::<Control>()))
        .await
        .unwrap_or_else(|_| Err("Không bật được DeepSeek.".into()))
}

#[tauri::command]
pub async fn stop_deepseek_server(app: tauri::AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || stop(&app.state::<Control>()))
        .await
        .unwrap_or_else(|_| Err("Không tắt được DeepSeek.".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn port_scan_keeps_only_the_local_deepseek_listener() {
        let text = "\
TCP    127.0.0.1:8000         0.0.0.0:0              LISTENING       4321
TCP    127.0.0.1:18000        0.0.0.0:0              LISTENING       9999
TCP    0.0.0.0:11434          0.0.0.0:0              LISTENING       1111
TCP    192.168.1.5:8000       0.0.0.0:0              LISTENING       2222
TCP    [::1]:8000             [::]:0                 LISTENING       4321
TCP    127.0.0.1:8000         0.0.0.0:0              ESTABLISHED     5555
";
        assert_eq!(listening_pids(text), vec![4321]);
    }

    #[test]
    fn startup_failure_shows_the_python_error() {
        let log = "\
INFO a
INFO b
INFO c
INFO d
INFO e
INFO f
OSError: [Errno 10048] port
";
        let message = startup_message(log);
        assert!(message.contains("127.0.0.1:8000/healthz"));
        assert!(message.contains("OSError: [Errno 10048] port"));
        assert!(!message.contains("INFO a"));
        assert_eq!(
            startup_message(" \n "),
            format!("Đã mở DeepSeek nhưng chưa thấy {DEEPSEEK_LOCAL_HEALTH_URL}.")
        );
    }

    const FAST: Duration = Duration::from_millis(1);

    #[test]
    fn a_slow_cold_start_is_waited_out_not_cut_at_twenty_seconds() {
        // The first start after boot took about two minutes. Killing it at 20 s meant it
        // could never come up through the button.
        assert!(START_LIMIT >= Duration::from_secs(120));
        let stop = AtomicBool::new(false);
        let mut polls = 0;
        let result = wait_for_server(
            || {
                polls += 1;
                polls > 50
            },
            || true,
            &stop,
            Duration::from_secs(30),
            FAST,
        );
        assert_eq!(result, Wait::Ready);
    }

    #[test]
    fn a_server_that_dies_while_starting_fails_at_once_not_after_the_limit() {
        let stop = AtomicBool::new(false);
        let begun = Instant::now();
        let mut checks = 0;
        let result = wait_for_server(
            || false,
            || {
                checks += 1;
                checks < 4
            },
            &stop,
            Duration::from_secs(60),
            FAST,
        );
        assert_eq!(result, Wait::Exited);
        assert!(begun.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn a_server_that_never_answers_gives_up_at_the_limit() {
        let stop = AtomicBool::new(false);
        let result = wait_for_server(|| false, || true, &stop, Duration::from_millis(40), FAST);
        assert_eq!(result, Wait::TimedOut);
    }

    #[test]
    fn a_stop_request_ends_the_wait_but_an_answering_server_still_counts() {
        let stop = AtomicBool::new(true);
        assert_eq!(
            wait_for_server(|| false, || true, &stop, Duration::from_secs(60), FAST),
            Wait::Stopped
        );
        assert_eq!(
            wait_for_server(|| true, || true, &stop, Duration::from_secs(60), FAST),
            Wait::Ready
        );
    }

    #[test]
    fn stopping_does_not_sit_behind_a_start_that_is_still_waiting() {
        // Closing the app calls stop_owned. It must not wait out a two-minute start.
        let control = Control::default();
        let (entered, started) = std::sync::mpsc::channel();
        thread::scope(|scope| {
            scope.spawn(|| {
                let _gate = lock(&control.gate);
                entered.send(()).unwrap();
                let result = wait_for_server(
                    || false,
                    || true,
                    &control.abort,
                    Duration::from_secs(20),
                    Duration::from_millis(5),
                );
                assert_eq!(result, Wait::Stopped);
            });
            started.recv().unwrap();
            let begun = Instant::now();
            let _gate = take_gate_after_aborting(&control);
            assert!(begun.elapsed() < Duration::from_secs(10));
        });
    }
}
