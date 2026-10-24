use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::Value;
use tauri::AppHandle;

use crate::job::{
    file_ready, is_extra_video, is_listen_audio, is_partial_audio, read_json, write_json, Cue, Job,
    TranslatedLine,
};
use crate::ollama::{divide_time, split_line};

use super::retake::restore_retake;
use super::{report, report_progress, stopped};

// Chép lời đã nằm trong transcript.json thì audio.wav không còn việc. Gọn file đó lại thành câu và xóa wav.
pub(crate) fn settle_transcript(job: &Job) -> Option<Vec<Cue>> {
    let cues = read_cues(&job.transcript()).ok()?;
    if cues.is_empty() {
        return None;
    }
    let _ = write_json(&job.transcript(), &cues);
    let _ = std::fs::remove_file(job.audio());
    Some(cues)
}

pub(super) fn drop_fit_wavs(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir.join("lines")) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        if name.ends_with(".fit.wav") || name == "_fit.wav" {
            let _ = std::fs::remove_file(path);
        }
    }
}

// Các bản cũ từng ghi thẳng vào tên kết quả. Giữ file hỏng ở thư mục riêng,
// để lần chạy tiếp có thể ghép lại mà không mất các câu đã dịch/đọc.
pub(crate) fn recover_incomplete_exports(job: &Job, cancel: &AtomicBool) -> Result<(), String> {
    for path in [job.result(), job.captioned(), job.spoken()] {
        if !file_ready(&path) {
            continue;
        }
        super::require_tool("ffprobe")?;
        stopped(cancel)?;
        if media_duration(&path, cancel).is_ok() {
            continue;
        }
        stopped(cancel)?;
        let saved = job.root.join("incomplete");
        std::fs::create_dir_all(&saved).map_err(|error| error.to_string())?;
        let name = path
            .file_name()
            .ok_or("Không đọc được tên file kết quả.")?
            .to_string_lossy();
        let mut index = 1u64;
        let dest = loop {
            let dest = saved.join(format!("{index}-{name}"));
            if !dest.exists() {
                break dest;
            }
            index += 1;
        };
        std::fs::rename(&path, &dest)
            .map_err(|error| format!("Không cất được file kết quả ghi dở: {error}"))?;
    }
    Ok(())
}

fn has_deliverable(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let path = entry.path();
        let ext = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if ext == "mp4" {
            return !is_extra_video(&path) && file_ready(&path);
        }
        ext == "m4a" && !is_partial_audio(&path) && !is_listen_audio(&path) && file_ready(&path)
    })
}

// dub.wav là cả timeline PCM. Khi đã có video hoặc .m4a thì tiếng đó nằm trong file nén.
pub(super) fn release_mix(dir: &Path) {
    drop_fit_wavs(dir);
    if has_deliverable(dir) {
        let _ = std::fs::remove_file(dir.join("dub.wav"));
    }
}

// Dọn folder kết quả đang nằm im: wav tách tiếng, wav co từng câu, và bản trộn.
pub(crate) fn reclaim_output(output_dir: &Path) {
    let Ok(entries) = std::fs::read_dir(output_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir()
            || entry
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case("tai-ve")
        {
            continue;
        }
        let _ = restore_retake(&dir);
        let transcript = dir.join("transcript.json");
        if let Ok(cues) = read_cues(&transcript) {
            if !cues.is_empty() {
                let _ = write_json(&transcript, &cues);
                let _ = std::fs::remove_file(dir.join("audio.wav"));
            }
        }
        release_mix(&dir);
    }
}

pub(crate) fn extract(app: &AppHandle, job: &Job, cancel: &AtomicBool) -> Result<(), String> {
    if settle_transcript(job).is_some() {
        report(
            app,
            "extract",
            "skipped",
            "Đã có chép lời, bỏ qua tách tiếng.",
        );
        return Ok(());
    }
    if file_ready(&job.audio()) {
        report(app, "extract", "skipped", "Đã có audio.wav, bỏ qua.");
        return Ok(());
    }
    if !file_ready(&job.video) {
        return Err("Chưa có file video. Bấm Tải video trước.".into());
    }
    stopped(cancel)?;
    report(app, "extract", "running", "Đang tách tiếng.");
    let total = media_duration(&job.video, cancel).ok();
    let mut command = Command::new("ffmpeg");
    command
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-progress",
            "pipe:1",
            "-nostats",
            "-y",
            "-i",
        ])
        .arg(&job.video)
        .args(["-vn", "-ac", "1", "-ar", "16000", "-c:a", "pcm_s16le"])
        .arg(job.audio());
    run_ffmpeg_progress(
        command,
        total,
        Duration::from_secs(30 * 60),
        cancel,
        &mut |fraction| {
            report_progress(app, "extract", "Đang tách tiếng.", fraction, 1.0);
        },
    )?;
    if !file_ready(&job.audio()) {
        return Err("FFmpeg không ghi được audio.wav.".into());
    }
    report(app, "extract", "done", "Đã tách tiếng.");
    Ok(())
}

pub(super) fn transcribe(
    app: &AppHandle,
    job: &Job,
    language: &str,
    cancel: &AtomicBool,
) -> Result<Vec<Cue>, String> {
    if let Some(cues) = settle_transcript(job) {
        report(
            app,
            "transcribe",
            "skipped",
            format!("Đã có {} câu chép lời, bỏ qua.", cues.len()),
        );
        return Ok(cues);
    }
    if file_ready(&job.transcript()) {
        return Err("Không nghe được lời thoại.".into());
    }
    stopped(cancel)?;
    report(app, "transcribe", "running", "Đang chép lời.");
    let _ = language;
    let duration = media_duration(&job.audio(), cancel).ok();
    let launch = launch_for(&job.root, &job.audio(), &job.transcript(), duration)?;
    let mut command = Command::new(&launch.program);
    command.args(&launch.args);
    if let Some(dir) = &launch.cwd {
        command.current_dir(dir);
    }
    let mut lines = LineSplitter::default();
    run_tool_with(
        command,
        Duration::from_secs(4 * 60 * 60),
        cancel,
        &mut |is_err, text| {
            let _ = is_err;
            for line in lines.push(text) {
                if let Some(fraction) = whisper_fraction(&line) {
                    report_progress(app, "transcribe", "Đang chép lời.", fraction, 1.0);
                }
            }
        },
    )?;
    if !file_ready(&job.transcript()) {
        return Err("Whisper không ghi transcript.json.".into());
    }
    let cues = read_cues(&job.transcript())?;
    if cues.is_empty() {
        return Err("Không nghe được lời thoại.".into());
    }
    write_json(&job.transcript(), &cues)?;
    let _ = std::fs::remove_file(job.audio());
    report(
        app,
        "transcribe",
        "done",
        format!("Đã chép {} câu.", cues.len()),
    );
    Ok(cues)
}

// Two threads, on purpose. whisper-cli's own default is 4, and scaling toward the core count
// leaves the machine unable to do anything else. Slower transcription is the trade.
pub(crate) fn whisper_threads() -> usize {
    2
}

pub(super) fn split(
    app: &AppHandle,
    job: &Job,
    cues: &[Cue],
    cancel: &AtomicBool,
) -> Result<Vec<Cue>, String> {
    if file_ready(&job.lines_path()) {
        let lines: Vec<Cue> = read_json(&job.lines_path())?;
        if !lines.is_empty() {
            report(
                app,
                "split",
                "skipped",
                format!("Đã có {} câu, bỏ qua.", lines.len()),
            );
            return Ok(lines);
        }
    }
    stopped(cancel)?;
    report(app, "split", "running", "Đang ngắt câu.");
    let mut lines = Vec::new();
    for (index, cue) in cues.iter().enumerate() {
        stopped(cancel)?;
        report_progress(
            app,
            "split",
            format!("Câu {}/{}.", index + 1, cues.len()),
            index as f64,
            cues.len() as f64,
        );
        let parts = split_line(&cue.text);
        lines.extend(divide_time(cue.start, cue.end, &parts));
    }
    if lines.is_empty() {
        return Err("Không có câu để dịch.".into());
    }
    write_json(&job.lines_path(), &lines)?;
    report(app, "split", "done", format!("{} câu.", lines.len()));
    Ok(lines)
}

#[derive(Debug)]
pub(crate) struct FastPath {
    pub python: PathBuf,
    pub python_args: Vec<String>,
    pub model: PathBuf,
}

#[derive(Debug)]
pub(crate) enum AsrKind {
    Faster(FastPath),
    WhisperCpp { exe: PathBuf, model: PathBuf },
}

pub(crate) struct AsrLaunch {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
}

pub(crate) fn faster_model_dir(whisper_dir: &Path) -> Option<PathBuf> {
    let dir = whisper_dir.join("large-v3-turbo");
    dir.join("model.bin").is_file().then_some(dir)
}

pub(crate) fn assemble_fast_path(
    script_ok: bool,
    model: Option<PathBuf>,
    python: Option<(PathBuf, Vec<String>)>,
) -> Option<FastPath> {
    if !script_ok {
        return None;
    }
    let model = model?;
    let (python, python_args) = python?;
    Some(FastPath {
        python,
        python_args,
        model,
    })
}

pub(crate) fn choose_asr(
    fast: Option<FastPath>,
    exe: Option<PathBuf>,
    ggml: Option<PathBuf>,
) -> Result<AsrKind, String> {
    if let Some(fast) = fast {
        return Ok(AsrKind::Faster(fast));
    }
    let Some(exe) = exe else {
        return Err("Chưa thấy whisper-cli.exe. Xem mục Whisper ở màn chuẩn bị.".into());
    };
    let Some(model) = ggml else {
        return Err(
            "Chưa thấy file model ggml-small.bin trong folder whisper. App không tự tải model."
                .into(),
        );
    };
    Ok(AsrKind::WhisperCpp { exe, model })
}

pub(crate) fn bundled_asr_script() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../scripts/faster_whisper.py")
}

fn project_whisper_dir() -> Option<PathBuf> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../whisper");
    path.is_dir().then_some(path)
}

const CUDA_PROBE: &str = r#"
import os, pathlib, importlib.util, ctypes
bins = []
for name in ("nvidia.cublas", "nvidia.cudnn", "nvidia.cuda_nvrtc"):
    spec = importlib.util.find_spec(name)
    if spec is None or not spec.submodule_search_locations:
        raise SystemExit(1)
    folder = pathlib.Path(list(spec.submodule_search_locations)[0]) / "bin"
    if not folder.is_dir():
        raise SystemExit(1)
    os.add_dll_directory(str(folder))
    bins.append(str(folder))
os.environ["PATH"] = os.pathsep.join(bins) + os.pathsep + os.environ.get("PATH", "")
ctypes.WinDLL("cublas64_12.dll")
import faster_whisper, ctranslate2
raise SystemExit(0 if ctranslate2.get_cuda_device_count() > 0 else 1)
"#;

fn command_ok(command: &mut Command, timeout: Duration) -> bool {
    let mut child = match command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(_) => return false,
    };
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) if started.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
            Ok(None) => thread::sleep(Duration::from_millis(50)),
            Err(_) => return false,
        }
    }
}

fn python_for_cuda() -> Option<(PathBuf, Vec<String>)> {
    let attempts = [
        (PathBuf::from("py"), vec!["-3".to_string()]),
        (PathBuf::from("python"), Vec::new()),
    ];
    for (program, args) in attempts {
        let mut command = Command::new(&program);
        command.args(&args).arg("-c").arg(CUDA_PROBE);
        if command_ok(&mut command, Duration::from_secs(20)) {
            return Some((program, args));
        }
    }
    None
}

pub(crate) fn detect_fast_path() -> Option<FastPath> {
    let script_ok = bundled_asr_script().is_file();
    let model = project_whisper_dir().and_then(|dir| faster_model_dir(&dir));
    assemble_fast_path(script_ok, model, python_for_cuda())
}

pub(crate) fn launch_for(
    root: &Path,
    audio: &Path,
    transcript: &Path,
    duration: Option<f64>,
) -> Result<AsrLaunch, String> {
    let exe = crate::whisper_exe();
    let ggml = exe.as_ref().and_then(|path| crate::whisper_model(path));
    let kind = choose_asr(detect_fast_path(), exe, ggml)?;
    Ok(asr_argv(
        &kind,
        root,
        audio,
        transcript,
        &bundled_asr_script(),
        duration,
    ))
}

pub(crate) fn asr_argv(
    kind: &AsrKind,
    root: &Path,
    audio: &Path,
    transcript: &Path,
    script: &Path,
    duration: Option<f64>,
) -> AsrLaunch {
    match kind {
        AsrKind::Faster(fast) => {
            let mut args = fast.python_args.clone();
            args.push("-u".into());
            args.push(script.display().to_string());
            args.push("--model".into());
            args.push(fast.model.display().to_string());
            args.push("--audio".into());
            args.push(audio.display().to_string());
            args.push("--out".into());
            args.push(transcript.display().to_string());
            if let Some(seconds) = duration {
                if seconds.is_finite() && seconds > 0.0 {
                    args.push("--duration".into());
                    args.push(seconds.to_string());
                }
            }
            AsrLaunch {
                program: fast.python.clone(),
                args,
                cwd: None,
            }
        }
        AsrKind::WhisperCpp { exe, model } => AsrLaunch {
            program: exe.clone(),
            args: vec![
                "-m".into(),
                model.display().to_string(),
                "-f".into(),
                "audio.wav".into(),
                "-t".into(),
                whisper_threads().to_string(),
                "-l".into(),
                "auto".into(),
                "-oj".into(),
                "-pp".into(),
                "-of".into(),
                "transcript".into(),
            ],
            cwd: Some(root.to_path_buf()),
        },
    }
}

pub(crate) fn write_silence(path: &Path, cancel: &AtomicBool) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let mut command = Command::new("ffmpeg");
    command
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-t",
            "0.1",
            "-i",
            "anullsrc=r=22050:cl=mono",
            "-c:a",
            "pcm_s16le",
        ])
        .arg(path);
    run_tool(command, Duration::from_secs(30), cancel)?;
    Ok(())
}

pub(crate) fn media_duration(path: &Path, cancel: &AtomicBool) -> Result<f64, String> {
    let mut command = Command::new("ffprobe");
    command
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "csv=p=0",
        ])
        .arg(path);
    let text = run_tool(command, Duration::from_secs(30), cancel)?;
    parse_duration(&text).ok_or_else(|| "Không đọc được độ dài.".to_string())
}

pub(crate) fn read_cues(path: &Path) -> Result<Vec<Cue>, String> {
    let value: Value = read_json(path)?;
    cues_from_value(&value).ok_or_else(|| format!("Không đọc được {}.", path.display()))
}

fn cues_from_value(value: &Value) -> Option<Vec<Cue>> {
    if let Some(items) = value.as_array() {
        return Some(items.iter().filter_map(cue_from_saved).collect());
    }
    if let Some(items) = value.get("transcription").and_then(Value::as_array) {
        return Some(items.iter().filter_map(cue_from_whisper_cpp).collect());
    }
    if let Some(items) = value.get("segments").and_then(Value::as_array) {
        return Some(items.iter().filter_map(cue_from_segments).collect());
    }
    None
}

fn cue_from_saved(value: &Value) -> Option<Cue> {
    let text = value.get("text")?.as_str()?.trim().to_string();
    if text.is_empty() {
        return None;
    }
    let start = value.get("start")?.as_f64()?;
    let end = value.get("end")?.as_f64()?;
    Some(Cue {
        start,
        end: end.max(start + 0.05),
        text,
    })
}

fn cue_from_whisper_cpp(value: &Value) -> Option<Cue> {
    let text = value.get("text")?.as_str()?.trim().to_string();
    if text.is_empty() {
        return None;
    }
    let (start, end) = if let Some(stamps) = value.get("timestamps") {
        (
            parse_clock(stamps.get("from")?.as_str()?)?,
            parse_clock(stamps.get("to")?.as_str()?)?,
        )
    } else {
        let offsets = value.get("offsets")?;
        (
            offsets.get("from")?.as_f64()? / 1000.0,
            offsets.get("to")?.as_f64()? / 1000.0,
        )
    };
    Some(Cue {
        start,
        end: end.max(start + 0.05),
        text,
    })
}

fn cue_from_segments(value: &Value) -> Option<Cue> {
    let text = value.get("text")?.as_str()?.trim().to_string();
    if text.is_empty() {
        return None;
    }
    let start = value.get("start")?.as_f64()?;
    let end = value.get("end")?.as_f64()?;
    Some(Cue {
        start,
        end: end.max(start + 0.05),
        text,
    })
}

pub(crate) fn parse_clock(text: &str) -> Option<f64> {
    let text = text.trim();
    let (hms, fraction) = text
        .split_once([',', '.'])
        .map(|(left, right)| (left, right))
        .unwrap_or((text, ""));
    let parts: Vec<&str> = hms.split(':').collect();
    let (hours, minutes, seconds) = match parts.as_slice() {
        [hours, minutes, seconds] => (
            hours.parse::<f64>().ok()?,
            minutes.parse::<f64>().ok()?,
            seconds.parse::<f64>().ok()?,
        ),
        [minutes, seconds] => (
            0.0,
            minutes.parse::<f64>().ok()?,
            seconds.parse::<f64>().ok()?,
        ),
        [seconds] => (0.0, 0.0, seconds.parse::<f64>().ok()?),
        _ => return None,
    };
    let digits: String = fraction.chars().take(3).collect();
    let millis = if digits.is_empty() {
        0.0
    } else {
        digits.parse::<f64>().ok()? * 10f64.powi(3 - digits.len() as i32)
    };
    Some(hours * 3600.0 + minutes * 60.0 + seconds + millis / 1000.0)
}

fn parse_duration(text: &str) -> Option<f64> {
    let token = text.split_whitespace().next()?.trim();
    let value = token.parse::<f64>().ok()?;
    (value.is_finite() && value > 0.0).then_some(value)
}

pub(super) fn write_srt(path: &Path, lines: &[TranslatedLine]) -> Result<(), String> {
    let mut text = String::from("\u{feff}");
    let mut number = 1;
    for line in lines {
        let caption = line.translation.replace('\n', " ").trim().to_string();
        if caption.is_empty() {
            continue;
        }
        text.push_str(&format!(
            "{number}\n{} --> {}\n{caption}\n\n",
            srt_time(line.start),
            srt_time(line.end)
        ));
        number += 1;
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(path, text).map_err(|error| error.to_string())
}

fn srt_time(secs: f64) -> String {
    let total_ms = (secs.max(0.0) * 1000.0).round() as u64;
    let hours = total_ms / 3_600_000;
    let minutes = (total_ms / 60_000) % 60;
    let seconds = (total_ms / 1000) % 60;
    let millis = total_ms % 1000;
    format!("{hours:02}:{minutes:02}:{seconds:02},{millis:03}")
}

pub(super) fn run_tool(
    command: Command,
    timeout: Duration,
    cancel: &AtomicBool,
) -> Result<String, String> {
    run_tool_with(command, timeout, cancel, &mut |_, _| {})
}

// Same as run_tool, but hands each chunk of stdout/stderr text to `on_text` while the tool runs.
pub(crate) fn run_tool_with(
    mut command: Command,
    timeout: Duration,
    cancel: &AtomicBool,
    on_text: &mut dyn FnMut(bool, &str),
) -> Result<String, String> {
    let mut child = crate::hidden(&mut command)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| error.to_string())?;
    crate::yield_cpu(&child);
    let (sender, receiver) = mpsc::channel::<(bool, Vec<u8>)>();
    spawn_reader(child.stdout.take(), false, sender.clone());
    spawn_reader(child.stderr.take(), true, sender);
    let mut stdout_bytes = Vec::new();
    let mut stderr_bytes = Vec::new();
    let mut take = |is_err: bool, bytes: Vec<u8>| {
        on_text(is_err, &String::from_utf8_lossy(&bytes));
        let target = if is_err {
            &mut stderr_bytes
        } else {
            &mut stdout_bytes
        };
        target.extend(bytes);
    };
    let started = Instant::now();
    let status = loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Đã dừng.".into());
        }
        match receiver.recv_timeout(Duration::from_millis(80)) {
            Ok((is_err, bytes)) => take(is_err, bytes),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => thread::sleep(Duration::from_millis(80)),
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("Lệnh chạy quá lâu.".into());
            }
            Ok(None) => {}
            Err(error) => return Err(error.to_string()),
        }
    };
    while let Ok((is_err, bytes)) = receiver.recv() {
        take(is_err, bytes);
    }
    let stdout = String::from_utf8_lossy(&stdout_bytes).into_owned();
    let stderr = String::from_utf8_lossy(&stderr_bytes).into_owned();
    if status.success() {
        return Ok(stdout);
    }
    let error = if stderr.trim().is_empty() {
        stdout
    } else {
        stderr
    };
    Err(if error.trim().is_empty() {
        "Lệnh thất bại.".into()
    } else {
        brief_error(&error)
    })
}

fn spawn_reader(
    pipe: Option<impl Read + Send + 'static>,
    is_err: bool,
    sender: mpsc::Sender<(bool, Vec<u8>)>,
) {
    let Some(mut pipe) = pipe else { return };
    thread::spawn(move || {
        let mut buffer = [0u8; 4096];
        while let Ok(count) = pipe.read(&mut buffer) {
            if count == 0 || sender.send((is_err, buffer[..count].to_vec())).is_err() {
                break;
            }
        }
    });
}

// Splits streamed text into complete lines; tools end progress lines with \n or \r.
#[derive(Default)]
pub(super) struct LineSplitter {
    pending: String,
}

impl LineSplitter {
    pub(super) fn push(&mut self, text: &str) -> Vec<String> {
        self.pending.push_str(text);
        let mut lines: Vec<String> = self
            .pending
            .split(['\n', '\r'])
            .map(str::to_string)
            .collect();
        self.pending = lines.pop().unwrap_or_default();
        lines.retain(|line| !line.trim().is_empty());
        lines
    }
}

// "whisper_print_progress_callback: progress =  45%" -> 0.45
fn whisper_fraction(line: &str) -> Option<f64> {
    let rest = line.split("progress =").nth(1)?;
    let percent: f64 = rest.trim().trim_end_matches('%').trim().parse().ok()?;
    Some((percent / 100.0).clamp(0.0, 1.0))
}

// "out_time_us=12345678" from ffmpeg -progress -> seconds
fn ffmpeg_seconds(line: &str) -> Option<f64> {
    let micros: f64 = line
        .trim()
        .strip_prefix("out_time_us=")?
        .trim()
        .parse()
        .ok()?;
    (micros >= 0.0).then_some(micros / 1_000_000.0)
}

// Runs ffmpeg started with `-progress pipe:1` and reports `seconds / total_secs`.
pub(super) fn run_ffmpeg_progress(
    command: Command,
    total_secs: Option<f64>,
    timeout: Duration,
    cancel: &AtomicBool,
    on_fraction: &mut dyn FnMut(f64),
) -> Result<(), String> {
    let mut lines = LineSplitter::default();
    run_tool_with(command, timeout, cancel, &mut |_, text| {
        let Some(total) = total_secs else { return };
        if total <= 0.0 {
            return;
        }
        for line in lines.push(text) {
            if let Some(secs) = ffmpeg_seconds(&line) {
                on_fraction((secs / total).clamp(0.0, 1.0));
            }
        }
    })?;
    Ok(())
}

fn brief_error(text: &str) -> String {
    let trimmed = text.trim();
    let count = trimmed.chars().count();
    if count <= 500 {
        trimmed.to_string()
    } else {
        trimmed.chars().skip(count - 500).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::super::assemble::srt_has_cues;
    use super::*;

    #[test]
    fn legacy_incomplete_export_is_preserved_and_no_longer_blocks_resume() {
        if crate::program_path("ffprobe").is_none() {
            return;
        }
        let root = std::env::temp_dir().join(format!("incomplete-export-{}", std::process::id()));
        std::fs::create_dir_all(root.join("lines")).unwrap();
        let job = Job {
            video: root.join("clip.mp4"),
            picture: root.join("clip.mp4"),
            root: root.clone(),
            target: "vi".into(),
        };
        std::fs::write(job.result(), b"incomplete mp4").unwrap();
        std::fs::write(job.line_wav(1), b"spoken line").unwrap();
        std::fs::write(job.transcript(), b"transcript").unwrap();
        recover_incomplete_exports(&job, &AtomicBool::new(false)).unwrap();
        assert!(!job.result().exists());
        let saved = std::fs::read_dir(root.join("incomplete"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        assert_eq!(std::fs::read(saved).unwrap(), b"incomplete mp4");
        assert_eq!(std::fs::read(job.line_wav(1)).unwrap(), b"spoken line");
        assert_eq!(std::fs::read(job.transcript()).unwrap(), b"transcript");
        assert!(crate::job::finished_result(&job.root, &root, false).is_none());
        std::fs::remove_dir_all(root).unwrap();
    }
    use serde_json::json;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn whisper_json_and_saved_cues_keep_times() {
        let cpp = json!({
            "transcription": [{
                "timestamps": {"from": "00:00:02,500", "to": "00:00:04,000"},
                "text": " Hello"
            }]
        });
        let cues = cues_from_value(&cpp).unwrap();
        assert_eq!(cues[0].text, "Hello");
        assert!((cues[0].start - 2.5).abs() < 0.001);
        assert!((cues[0].end - 4.0).abs() < 0.001);

        let openai = json!({"segments": [{"start": 1.0, "end": 2.0, "text": " Xin chào"}]});
        let cues = cues_from_value(&openai).unwrap();
        assert_eq!(cues[0].text, "Xin chào");
        assert!((cues[0].start - 1.0).abs() < 0.001);
    }

    #[test]
    fn whisper_stays_on_two_threads() {
        assert_eq!(whisper_threads(), 2);
    }

    #[test]
    fn tool_progress_lines_are_parsed_across_chunks() {
        assert_eq!(
            whisper_fraction("whisper_print_progress_callback: progress =  45%"),
            Some(0.45)
        );
        assert_eq!(whisper_fraction("some other line"), None);
        assert_eq!(ffmpeg_seconds("out_time_us=2500000"), Some(2.5));
        assert_eq!(ffmpeg_seconds("progress=continue"), None);
        let mut lines = LineSplitter::default();
        assert!(lines.push("out_time_us=12").is_empty());
        assert_eq!(
            lines.push("34\r\nprogress=end\n"),
            vec!["out_time_us=1234", "progress=end"]
        );
    }

    #[test]
    fn srt_and_duration_parse() {
        assert_eq!(srt_time(2.5), "00:00:02,500");
        let dir = std::env::temp_dir().join(format!("srt-bom-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.vi.srt");
        write_srt(
            &path,
            &[TranslatedLine {
                start: 1.0,
                end: 2.0,
                source: "Hi".into(),
                translation: "Chào".into(),
            }],
        )
        .unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.starts_with('\u{feff}'));
        assert!(text.contains("Chào"));
        assert!(text.contains("00:00:01,000 --> 00:00:02,000"));
        write_srt(
            &path,
            &[TranslatedLine {
                start: 0.0,
                end: 1.0,
                source: "x".into(),
                translation: "  ".into(),
            }],
        )
        .unwrap();
        assert!(!srt_has_cues(&path));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(parse_duration("12.5\n").unwrap(), 12.5);
        assert!(parse_duration("N/A").is_none());
        let cancel = AtomicBool::new(true);
        let err = stopped(&cancel).unwrap_err();
        assert_eq!(err, "Đã dừng.");
    }

    #[test]
    fn reclaim_drops_wav_copies_kept_only_for_the_next_step() {
        let dir = std::env::temp_dir().join(format!("reclaim-wav-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let out = dir.join("out");
        let job = out.join("tap");
        let lines = job.join("lines");
        std::fs::create_dir_all(&lines).unwrap();
        let cues = vec![Cue {
            start: 0.0,
            end: 1.0,
            text: "Xin chào".into(),
        }];
        write_json(&job.join("transcript.json"), &cues).unwrap();
        std::fs::write(job.join("audio.wav"), vec![0u8; 64]).unwrap();
        std::fs::write(lines.join("001.wav"), b"line").unwrap();
        std::fs::write(lines.join("001.fit.wav"), b"fit").unwrap();
        std::fs::write(lines.join("_fit.wav"), b"scratch").unwrap();
        std::fs::write(job.join("dub.wav"), b"dub").unwrap();
        std::fs::write(job.join("tap.vi.mp4"), b"video").unwrap();
        std::fs::write(job.join("tap.vi.chu.mp4"), b"text").unwrap();
        let downloaded = out.join("tai-ve");
        std::fs::create_dir_all(&downloaded).unwrap();
        std::fs::write(downloaded.join("audio.wav"), b"keep").unwrap();
        reclaim_output(&out);
        assert!(!job.join("audio.wav").exists());
        assert!(!lines.join("001.fit.wav").exists());
        assert!(!lines.join("_fit.wav").exists());
        assert!(!job.join("dub.wav").exists());
        assert_eq!(std::fs::read(lines.join("001.wav")).unwrap(), b"line");
        assert!(file_ready(&job.join("transcript.json")));
        assert_eq!(
            std::fs::read(downloaded.join("audio.wav")).unwrap(),
            b"keep"
        );

        let open = out.join("dang-do");
        std::fs::create_dir_all(open.join("lines")).unwrap();
        write_json(&open.join("transcript.json"), &cues).unwrap();
        std::fs::write(open.join("audio.wav"), b"pcm").unwrap();
        std::fs::write(open.join("dub.wav"), b"dub").unwrap();
        std::fs::write(open.join("lines").join("002.fit.wav"), b"fit").unwrap();
        std::fs::write(open.join("lines").join("002.wav"), b"keep").unwrap();
        let captions = out.join("chi-chu");
        std::fs::create_dir_all(&captions).unwrap();
        std::fs::write(captions.join("tap.vi.chu.mp4"), b"text").unwrap();
        std::fs::write(captions.join("dub.wav"), b"dub").unwrap();
        reclaim_output(&out);
        assert!(!open.join("audio.wav").exists());
        assert!(!open.join("lines").join("002.fit.wav").exists());
        assert!(open.join("dub.wav").is_file());
        assert_eq!(
            std::fs::read(open.join("lines").join("002.wav")).unwrap(),
            b"keep"
        );
        assert!(captions.join("dub.wav").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn vietnamese_job_uses_absolute_audio_on_the_fast_path() {
        let fast = FastPath {
            python: PathBuf::from("py"),
            python_args: vec!["-3".into()],
            model: PathBuf::from(r"D:\whisper\large-v3-turbo"),
        };
        let root = PathBuf::from(r"D:\Việc\Chép Chính Tả");
        let audio = root.join("audio.wav");
        let transcript = root.join("transcript.json");
        let script = PathBuf::from(r"D:\app\scripts\faster_whisper.py");
        let launch = asr_argv(
            &AsrKind::Faster(fast),
            &root,
            &audio,
            &transcript,
            &script,
            Some(12.5),
        );
        assert_eq!(launch.program, PathBuf::from("py"));
        assert_eq!(launch.cwd, None);
        let expect = [
            "-3",
            "-u",
            r"D:\app\scripts\faster_whisper.py",
            "--model",
            r"D:\whisper\large-v3-turbo",
            "--audio",
            &audio.display().to_string(),
            "--out",
            &transcript.display().to_string(),
            "--duration",
            "12.5",
        ];
        assert_eq!(
            launch.args,
            expect.into_iter().map(str::to_string).collect::<Vec<_>>()
        );
        assert!(!launch.args.iter().any(|arg| arg == "audio.wav"));
        assert!(!launch.args.iter().any(|arg| arg == "large-v3-turbo"));
    }

    #[test]
    fn missing_fast_path_keeps_the_two_thread_whisper_cli() {
        let exe = PathBuf::from(r"D:\whisper\whisper-cli.exe");
        let ggml = PathBuf::from(r"D:\whisper\ggml-small.bin");
        let kind = choose_asr(None, Some(exe.clone()), Some(ggml.clone())).unwrap();
        let root = PathBuf::from(r"D:\job");
        let launch = asr_argv(
            &kind,
            &root,
            &root.join("audio.wav"),
            &root.join("transcript.json"),
            &PathBuf::from("unused.py"),
            Some(99.0),
        );
        assert_eq!(launch.program, exe);
        assert_eq!(launch.cwd.as_deref(), Some(root.as_path()));
        let model = ggml.display().to_string();
        let expect = [
            "-m",
            model.as_str(),
            "-f",
            "audio.wav",
            "-t",
            "2",
            "-l",
            "auto",
            "-oj",
            "-pp",
            "-of",
            "transcript",
        ];
        assert_eq!(
            launch.args,
            expect.into_iter().map(str::to_string).collect::<Vec<_>>()
        );
    }

    #[test]
    fn neither_engine_keeps_the_whisper_cli_error() {
        let error = choose_asr(None, None, None).unwrap_err();
        assert_eq!(
            error,
            "Chưa thấy whisper-cli.exe. Xem mục Whisper ở màn chuẩn bị."
        );
    }

    #[test]
    fn exe_without_ggml_keeps_the_model_error() {
        let error = choose_asr(None, Some(PathBuf::from("whisper-cli.exe")), None).unwrap_err();
        assert_eq!(
            error,
            "Chưa thấy file model ggml-small.bin trong folder whisper. App không tự tải model."
        );
    }

    #[test]
    fn fast_path_needs_script_model_bin_and_python() {
        let dir = std::env::temp_dir().join(format!("asr-model-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(faster_model_dir(&dir).is_none());
        let model = dir.join("large-v3-turbo");
        std::fs::create_dir_all(&model).unwrap();
        assert!(faster_model_dir(&dir).is_none());
        std::fs::write(model.join("model.bin"), b"x").unwrap();
        assert_eq!(faster_model_dir(&dir).as_deref(), Some(model.as_path()));
        let python = Some((PathBuf::from("py"), vec!["-3".into()]));
        assert!(assemble_fast_path(false, faster_model_dir(&dir), python.clone()).is_none());
        assert!(assemble_fast_path(true, None, python.clone()).is_none());
        assert!(assemble_fast_path(true, faster_model_dir(&dir), None).is_none());
        let fast = assemble_fast_path(true, faster_model_dir(&dir), python).unwrap();
        assert_eq!(fast.python_args, vec!["-3".to_string()]);
        assert!(matches!(
            choose_asr(Some(fast), None, None).unwrap(),
            AsrKind::Faster(_)
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bundled_script_path_ends_at_the_python_file() {
        let path = bundled_asr_script();
        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some("faster_whisper.py")
        );
    }
}
