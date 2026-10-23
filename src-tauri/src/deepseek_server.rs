use tauri::Manager;

use std::io::Read;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::ollama::{deepseek_local_up_fast, DEEPSEEK_LOCAL_HEALTH_URL};

pub struct Control {
    gate: Mutex<()>,
    child: Mutex<Option<Child>>,
    owned: AtomicBool,
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
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../Deepseek-API")
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
    if child_running(control) {
        if wait_until(deepseek_local_up_fast, Duration::from_secs(20)) {
            return Ok(());
        }
        return Err(startup_failure(control));
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
    if wait_until(deepseek_local_up_fast, Duration::from_secs(20)) {
        return Ok(());
    }
    Err(startup_failure(control))
}

pub fn stop(control: &Control) -> Result<(), String> {
    let _gate = lock(&control.gate);
    kill_owned(control);
    for pid in listeners_on_8000() {
        kill_pid(pid);
    }
    let _ = wait_until(|| !deepseek_local_up_fast(), Duration::from_secs(5));
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
    let _gate = lock(&control.gate);
    if control.owned.load(Ordering::SeqCst) {
        kill_owned(control);
    }
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
}
