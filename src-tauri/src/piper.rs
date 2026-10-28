use serde_json::Value;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PiperCaps {
    pub json_input: bool,
    pub noise_scale: Option<&'static str>,
    pub length_scale: Option<&'static str>,
    pub noise_w: Option<&'static str>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SpeakEngine {
    Native,
    Window,
}

fn flag<'a>(
    help: &'a str,
    dashed: &'static str,
    underscored: &'static str,
) -> Option<&'static str> {
    if help.contains(dashed) {
        Some(dashed)
    } else if help.contains(underscored) {
        Some(underscored)
    } else {
        None
    }
}

pub(crate) fn caps_from_help(help: &str) -> Option<PiperCaps> {
    if !help.contains("--json-input") {
        return None;
    }
    Some(PiperCaps {
        json_input: true,
        noise_scale: flag(help, "--noise-scale", "--noise_scale"),
        length_scale: flag(help, "--length-scale", "--length_scale"),
        noise_w: flag(help, "--noise-w", "--noise_w"),
    })
}

pub(crate) fn piper_exe_in(dir: &Path) -> Option<PathBuf> {
    let exe = dir.join("piper.exe");
    let data = dir.join("espeak-ng-data");
    (exe.is_file() && data.is_dir()).then_some(exe)
}

pub(crate) const ALLOW_NATIVE: bool = true;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Scales {
    pub noise: f64,
    pub length: f64,
    pub noise_w: f64,
}

pub(crate) fn join_chunks(chunks: &[String]) -> String {
    chunks
        .iter()
        .map(|chunk| chunk.trim())
        .filter(|chunk| !chunk.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn scales_from_config(text: &str) -> Scales {
    let value: Value = serde_json::from_str(text).unwrap_or(Value::Null);
    let inference = value.get("inference");
    let number = |name: &str, default: f64| {
        inference
            .and_then(|block| block.get(name))
            .and_then(Value::as_f64)
            .unwrap_or(default)
    };
    Scales {
        noise: number("noise_scale", 0.667),
        length: number("length_scale", 1.0),
        noise_w: number("noise_w", 0.8),
    }
}

pub(crate) struct SpeechLine {
    pub text: String,
    pub output: PathBuf,
}

pub(crate) fn json_batch(items: &[SpeechLine]) -> String {
    let mut out = String::new();
    for item in items {
        let text = item.text.trim();
        if text.is_empty() {
            continue;
        }
        let row = serde_json::json!({
            "text": text,
            "output_file": item.output.to_string_lossy(),
        });
        out.push_str(&row.to_string());
        out.push('\n');
    }
    out
}

pub(crate) fn choose_engine(exe: Option<PathBuf>, caps: Option<PiperCaps>) -> SpeakEngine {
    if !ALLOW_NATIVE {
        return SpeakEngine::Window;
    }
    match (exe, caps) {
        (Some(_), Some(caps)) if caps.json_input => SpeakEngine::Native,
        _ => SpeakEngine::Window,
    }
}

pub(crate) fn scale_args(caps: &PiperCaps, scales: Scales) -> Vec<String> {
    let mut args = Vec::new();
    for (flag, value) in [
        (caps.noise_scale, scales.noise),
        (caps.length_scale, scales.length),
        (caps.noise_w, scales.noise_w),
    ] {
        let Some(flag) = flag else { continue };
        args.push(flag.to_string());
        args.push(format!("{value:.3}"));
    }
    args
}

pub(crate) fn keep_valid_wavs(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut kept = Vec::new();
    for path in paths {
        if !path.exists() {
            continue;
        }
        if wav_ok(path) {
            kept.push(path.clone());
        } else {
            let _ = std::fs::remove_file(path);
        }
    }
    kept
}

fn path_is_ascii(path: &Path) -> bool {
    path.to_string_lossy().chars().all(|ch| ch.is_ascii())
}

fn stage_plan(
    model: &Path,
    items: &[SpeechLine],
    stage: &Path,
) -> (PathBuf, Vec<SpeechLine>, Vec<(PathBuf, PathBuf)>) {
    let use_model = if path_is_ascii(model) {
        model.to_path_buf()
    } else {
        stage.join("voice.onnx")
    };
    let mut staged = Vec::with_capacity(items.len());
    let mut relays = Vec::new();
    for (index, item) in items.iter().enumerate() {
        if path_is_ascii(&item.output) {
            staged.push(SpeechLine {
                text: item.text.clone(),
                output: item.output.clone(),
            });
        } else {
            let relay = stage.join(format!("{index:03}.wav"));
            relays.push((relay.clone(), item.output.clone()));
            staged.push(SpeechLine {
                text: item.text.clone(),
                output: relay,
            });
        }
    }
    (use_model, staged, relays)
}

fn deliver_staged(relays: &[(PathBuf, PathBuf)]) {
    for (staged, real) in relays {
        if !wav_ok(staged) {
            let _ = std::fs::remove_file(staged);
            continue;
        }
        if let Some(parent) = real.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::remove_file(real);
        if std::fs::rename(staged, real).is_err() && std::fs::copy(staged, real).is_ok() {
            let _ = std::fs::remove_file(staged);
        }
    }
}

fn mark_ready(items: &[SpeechLine], ready: &mut [bool]) -> usize {
    for (index, item) in items.iter().enumerate() {
        if ready.get(index) == Some(&true) {
            continue;
        }
        if wav_ok(&item.output) {
            if let Some(slot) = ready.get_mut(index) {
                *slot = true;
            }
        }
    }
    ready.iter().filter(|flag| **flag).count()
}

fn ascii_stage_dir() -> PathBuf {
    let base = std::env::temp_dir();
    let root = if path_is_ascii(&base) {
        base
    } else {
        PathBuf::from(r"C:\Windows\Temp")
    };
    let tick = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|dur| dur.as_millis())
        .unwrap_or(0);
    root.join(format!("piper-stage-{}-{tick}", std::process::id()))
}

fn settle(stage: &Path, relays: &[(PathBuf, PathBuf)], outputs: &[PathBuf], failed: bool) {
    deliver_staged(relays);
    if failed {
        let _ = keep_valid_wavs(outputs);
    }
    let _ = std::fs::remove_dir_all(stage);
}

pub(crate) fn speak_native(
    exe: &Path,
    model: &Path,
    caps: &PiperCaps,
    scales: Scales,
    items: &[SpeechLine],
    cancel: &AtomicBool,
    progress: &dyn Fn(usize, usize),
) -> Result<(), String> {
    let home = exe.parent().ok_or("Không thấy thư mục của piper.exe.")?;
    let stage = ascii_stage_dir();
    let (use_model, staged, relays) = stage_plan(model, items, &stage);
    if use_model != model || !relays.is_empty() {
        std::fs::create_dir_all(&stage).map_err(|error| error.to_string())?;
        if use_model != model {
            if let Err(error) = std::fs::copy(model, &use_model) {
                let _ = std::fs::remove_dir_all(&stage);
                return Err(error.to_string());
            }
        }
    }
    let mut command = Command::new(exe);
    command.current_dir(home).arg("--model").arg(&use_model);
    command.args(scale_args(caps, scales));
    command.arg("--json-input");
    let mut child = match crate::hidden(&mut command)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => {
            crate::yield_cpu(&child);
            child
        }
        Err(error) => {
            settle(&stage, &relays, &[], false);
            return Err(error.to_string());
        }
    };
    let batch = json_batch(&staged);
    if let Some(mut stdin) = child.stdin.take() {
        thread::spawn(move || {
            let _ = stdin.write_all(batch.as_bytes());
            let _ = stdin.flush();
        });
    }
    let (sender, receiver) = mpsc::channel::<(bool, Vec<u8>)>();
    spawn_reader(child.stdout.take(), false, sender.clone());
    spawn_reader(child.stderr.take(), true, sender);
    let mut stderr_bytes = Vec::new();
    let started = Instant::now();
    let timeout = Duration::from_secs(60 + 2 * items.len() as u64);
    let outputs: Vec<PathBuf> = items.iter().map(|item| item.output.clone()).collect();
    let mut ready = vec![false; staged.len()];
    let status = loop {
        progress(mark_ready(&staged, &mut ready), items.len());
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            settle(&stage, &relays, &outputs, true);
            return Err("Đã dừng.".into());
        }
        match receiver.recv_timeout(Duration::from_millis(80)) {
            Ok((is_err, bytes)) => {
                if is_err {
                    stderr_bytes.extend(bytes);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => thread::sleep(Duration::from_millis(80)),
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                settle(&stage, &relays, &outputs, true);
                return Err("Lệnh chạy quá lâu.".into());
            }
            Ok(None) => {}
            Err(error) => {
                settle(&stage, &relays, &outputs, true);
                return Err(error.to_string());
            }
        }
    };
    while let Ok((is_err, bytes)) = receiver.recv() {
        if is_err {
            stderr_bytes.extend(bytes);
        }
    }
    settle(&stage, &relays, &outputs, false);
    progress(
        mark_ready(items, &mut vec![false; items.len()]),
        items.len(),
    );
    let missing = items
        .iter()
        .any(|item| !item.text.trim().is_empty() && !wav_ok(&item.output));
    if !status.success() || missing {
        let _ = keep_valid_wavs(&outputs);
        return Err(one_line(&String::from_utf8_lossy(&stderr_bytes)));
    }
    Ok(())
}

fn wav_ok(path: &Path) -> bool {
    std::fs::read(path).is_ok_and(|bytes| crate::wav::parse_wav(&bytes).is_ok())
}

fn one_line(text: &str) -> String {
    let line = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("");
    let brief: String = line.chars().take(400).collect();
    if brief.is_empty() {
        "piper.exe thất bại.".into()
    } else {
        brief
    }
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

pub(crate) fn locate() -> Option<PathBuf> {
    let bundled = crate::runtime_paths::tool_dir("piper");
    if let Some(exe) = piper_exe_in(&bundled) {
        return Some(exe);
    }
    let on_path = crate::program_path("piper")?;
    piper_exe_in(on_path.parent()?)
}

pub(crate) fn read_caps(exe: &Path) -> Option<PiperCaps> {
    let home = exe.parent()?;
    let mut command = Command::new(exe);
    command.current_dir(home).arg("--help");
    let mut child = crate::hidden(&mut command)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    let (sender, receiver) = mpsc::channel::<(bool, Vec<u8>)>();
    spawn_reader(child.stdout.take(), false, sender.clone());
    spawn_reader(child.stderr.take(), true, sender);
    let mut help = Vec::new();
    let started = Instant::now();
    loop {
        match receiver.recv_timeout(Duration::from_millis(80)) {
            Ok((_, bytes)) => help.extend(bytes),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => thread::sleep(Duration::from_millis(80)),
        }
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() > Duration::from_secs(8) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) => {}
            Err(_) => return None,
        }
    }
    while let Ok((_, bytes)) = receiver.recv() {
        help.extend(bytes);
    }
    caps_from_help(&String::from_utf8_lossy(&help))
}

pub(crate) fn model_path(voice_dir: &Path, name: &str) -> PathBuf {
    voice_dir.join(format!("{name}.onnx"))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LinePlan {
    Silence,
    Speak(String),
}

pub(crate) fn plan_line(chunks: &[String]) -> LinePlan {
    let spoken = join_chunks(chunks);
    if spoken.is_empty() {
        LinePlan::Silence
    } else {
        LinePlan::Speak(spoken)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AfterNative {
    Stop,
    KeepGoing,
}

pub(crate) fn after_native(error: &str) -> AfterNative {
    if error == "Đã dừng." {
        AfterNative::Stop
    } else {
        AfterNative::KeepGoing
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn help_without_json_input_is_not_native() {
        assert!(caps_from_help("usage: piper --model FILE").is_none());
        assert_eq!(
            choose_engine(Some(PathBuf::from("piper.exe")), None),
            SpeakEngine::Window
        );
    }

    #[test]
    fn help_with_json_input_keeps_scale_flags_it_actually_prints() {
        let caps =
            caps_from_help("usage:\n  --json-input\n  --noise-scale\n  --length_scale\n").unwrap();
        assert!(caps.json_input);
        assert_eq!(caps.noise_scale, Some("--noise-scale"));
        assert_eq!(caps.length_scale, Some("--length_scale"));
        assert_eq!(caps.noise_w, None);
        assert_eq!(
            choose_engine(Some(PathBuf::from("piper.exe")), Some(caps)),
            SpeakEngine::Native
        );
    }

    #[test]
    fn exe_without_espeak_data_is_ignored() {
        let dir = std::env::temp_dir().join(format!("piper-home-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("piper.exe"), b"nope").unwrap();
        assert!(piper_exe_in(&dir).is_none());
        fs::create_dir_all(dir.join("espeak-ng-data")).unwrap();
        assert_eq!(piper_exe_in(&dir), Some(dir.join("piper.exe")));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn chunks_join_with_one_space_and_blank_chunks_drop() {
        assert_eq!(
            join_chunks(&["Xin chào.".into(), "  ".into(), "Hôm nay.".into()]),
            "Xin chào. Hôm nay."
        );
        assert_eq!(join_chunks(&["  ".into()]), "");
    }

    #[test]
    fn scales_fall_back_when_the_config_has_none() {
        assert_eq!(scales_from_config("{}").noise, 0.667);
        assert_eq!(scales_from_config("nope").length, 1.0);
        let scales = scales_from_config(
            r#"{"inference":{"noise_scale":0.5,"length_scale":1.2,"noise_w":0.4}}"#,
        );
        assert_eq!(
            (scales.noise, scales.length, scales.noise_w),
            (0.5, 1.2, 0.4)
        );
    }

    #[test]
    fn json_batch_is_one_object_per_line_and_skips_empty_text() {
        let items = vec![
            SpeechLine {
                text: "Xin chào".into(),
                output: PathBuf::from(r"D:\job\lines\001.wav"),
            },
            SpeechLine {
                text: "  ".into(),
                output: PathBuf::from(r"D:\job\lines\002.wav"),
            },
        ];
        let batch = json_batch(&items);
        let lines: Vec<&str> = batch.lines().collect();
        assert_eq!(lines.len(), 1);
        let row: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(row["text"], "Xin chào");
        assert_eq!(row["output_file"], r"D:\job\lines\001.wav");
    }

    #[test]
    fn scale_args_skip_flags_the_help_did_not_list() {
        let caps = caps_from_help("--json-input\n--length-scale\n").unwrap();
        let scales = Scales {
            noise: 0.5,
            length: 1.25,
            noise_w: 0.4,
        };
        assert_eq!(
            scale_args(&caps, scales),
            vec!["--length-scale".to_string(), "1.250".to_string()]
        );
    }

    #[test]
    fn keep_valid_wavs_deletes_a_file_that_is_not_pcm() {
        let dir = std::env::temp_dir().join(format!("piper-wav-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let good = dir.join("good.wav");
        let bad = dir.join("bad.wav");
        let pcm = crate::wav::Pcm {
            rate: 22050,
            samples: vec![0, 100, -100],
        };
        fs::write(&good, crate::wav::encode_wav(&pcm)).unwrap();
        fs::write(&bad, b"not a wav").unwrap();
        let kept = keep_valid_wavs(&[good.clone(), bad.clone()]);
        assert_eq!(kept, vec![good]);
        assert!(!bad.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn locate_on_an_empty_temp_tree_is_none() {
        let missing = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../piper-does-not-exist");
        assert!(piper_exe_in(&missing).is_none());
    }

    #[test]
    fn a_blank_line_is_silence_and_cancel_does_not_continue() {
        assert_eq!(plan_line(&["  ".into()]), LinePlan::Silence);
        assert_eq!(
            plan_line(&["Xin chào.".into()]),
            LinePlan::Speak("Xin chào.".into())
        );
        assert_eq!(after_native("Đã dừng."), AfterNative::Stop);
        assert_eq!(after_native("Lệnh chạy quá lâu."), AfterNative::KeepGoing);
    }

    #[test]
    fn native_stays_off_when_the_measure_gate_is_closed() {
        let caps = caps_from_help("--json-input").unwrap();
        let before = choose_engine(Some(PathBuf::from("piper.exe")), Some(caps));
        assert_eq!(
            before,
            if ALLOW_NATIVE {
                SpeakEngine::Native
            } else {
                SpeakEngine::Window
            }
        );
    }

    #[test]
    fn unicode_job_paths_are_relayed_to_an_ascii_stage() {
        let stage = PathBuf::from(r"C:\Temp\piper-stage");
        let items = vec![SpeechLine {
            text: "Xin chào".into(),
            output: PathBuf::from(r"D:\Việc\lines\001.wav"),
        }];
        let model = PathBuf::from(r"D:\Project\voices\vi\a.onnx");
        let (use_model, staged, relays) = stage_plan(&model, &items, &stage);
        assert_eq!(use_model, model);
        assert_eq!(relays.len(), 1);
        assert!(staged[0]
            .output
            .to_string_lossy()
            .chars()
            .all(|ch| ch.is_ascii()));
        assert_eq!(relays[0].1, items[0].output);
        assert_eq!(staged[0].output, relays[0].0);
    }

    #[test]
    fn ascii_paths_stay_where_the_caller_asked() {
        let stage = PathBuf::from(r"C:\Temp\piper-stage");
        let items = vec![SpeechLine {
            text: "Hi".into(),
            output: PathBuf::from(r"D:\job\lines\001.wav"),
        }];
        let model = PathBuf::from(r"D:\voices\a.onnx");
        let (use_model, staged, relays) = stage_plan(&model, &items, &stage);
        assert!(relays.is_empty());
        assert_eq!(staged[0].output, items[0].output);
        assert_eq!(use_model, model);
    }

    #[test]
    fn unicode_model_is_staged_under_an_ascii_name() {
        let model = PathBuf::from(r"D:\Giọng\a.onnx");
        let stage = PathBuf::from(r"C:\Temp\piper-stage");
        let (use_model, _, _) = stage_plan(&model, &[], &stage);
        assert_eq!(use_model, stage.join("voice.onnx"));
        assert!(use_model.to_string_lossy().chars().all(|ch| ch.is_ascii()));
    }

    #[test]
    fn a_staged_wav_is_moved_onto_the_real_path() {
        let dir = std::env::temp_dir().join(format!("piper-relay-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("stage")).unwrap();
        let staged = dir.join("stage").join("000.wav");
        let real = dir.join("Việc").join("001.wav");
        let pcm = crate::wav::Pcm {
            rate: 22050,
            samples: vec![0, 100, -100],
        };
        fs::write(&staged, crate::wav::encode_wav(&pcm)).unwrap();
        deliver_staged(&[(staged.clone(), real.clone())]);
        assert!(crate::wav::parse_wav(&fs::read(&real).unwrap()).is_ok());
        assert!(!staged.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn progress_remembers_a_wav_it_already_accepted() {
        let dir = std::env::temp_dir().join(format!("piper-prog-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let good = dir.join("good.wav");
        let pcm = crate::wav::Pcm {
            rate: 22050,
            samples: vec![1, 2, 3],
        };
        fs::write(&good, crate::wav::encode_wav(&pcm)).unwrap();
        let items = [SpeechLine {
            text: "Xin chào".into(),
            output: good.clone(),
        }];
        let mut ready = vec![false];
        assert_eq!(mark_ready(&items, &mut ready), 1);
        fs::remove_file(&good).unwrap();
        assert_eq!(mark_ready(&items, &mut ready), 1);
        let _ = fs::remove_dir_all(&dir);
    }
}
