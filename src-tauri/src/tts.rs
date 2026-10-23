// The voice runs in the app window (src/tts): Rust asks for lines to be read, the window
// speaks them with the Piper voice in voices/<language> and sends every wav back to be written here.
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::ipc::{InvokeBody, Request, Response};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::wav::{write_wav, Pcm};

const FIRST_LINE_WAIT: Duration = Duration::from_secs(180);
const NEXT_LINE_WAIT: Duration = Duration::from_secs(120);

#[derive(Clone, Serialize)]
struct Item {
    index: usize,
    text: String,
}

#[derive(Clone, Serialize)]
struct Ask {
    voice: String,
    lang: String,
    whole: bool,
    items: Vec<Item>,
}

#[derive(Default)]
struct Pending {
    done: Option<mpsc::Sender<Result<(), String>>>,
    prepared: Option<mpsc::Sender<Result<Vec<(usize, Vec<String>)>, String>>>,
    paths: HashMap<usize, PathBuf>,
    saved: usize,
    touched: Option<Instant>,
}

#[derive(Default)]
pub(crate) struct Bridge(Mutex<Pending>);

impl Bridge {
    fn lock(&self) -> MutexGuard<'_, Pending> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn clear(&self) {
        *self.lock() = Pending::default();
    }
}

pub(crate) fn voice_file(dir: &Path, name: &str, kind: &str) -> Result<PathBuf, String> {
    if !crate::voice_names(dir).iter().any(|known| known == name) {
        return Err("Không có giọng này.".into());
    }
    match kind {
        "model" => Ok(dir.join(format!("{name}.onnx"))),
        "config" => Ok(dir.join(format!("{name}.onnx.json"))),
        _ => Err("Không rõ loại file giọng.".into()),
    }
}

fn samples_from_bytes(bytes: &[u8]) -> Vec<i16> {
    bytes
        .chunks_exact(2)
        .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
        .collect()
}

// Items are (line index, text to speak, wav to write). Blocks until the window has written them all.
pub(crate) fn run(
    app: &AppHandle,
    voice: &str,
    lang: &str,
    items: &[(usize, String, PathBuf)],
    whole: bool,
    cancel: &AtomicBool,
    progress: &dyn Fn(usize, usize),
) -> Result<(), String> {
    let bridge = app.state::<Bridge>();
    let (sender, receiver) = mpsc::channel();
    {
        let mut pending = bridge.lock();
        if pending.done.is_some() {
            return Err("Đang đọc một yêu cầu khác.".into());
        }
        pending.done = Some(sender);
        pending.paths = items
            .iter()
            .map(|(index, _, path)| (*index, path.clone()))
            .collect();
        pending.saved = 0;
        pending.touched = Some(Instant::now());
    }
    let ask = Ask {
        voice: voice.to_string(),
        lang: lang.to_string(),
        whole,
        items: items
            .iter()
            .map(|(index, text, _)| Item {
                index: *index,
                text: text.clone(),
            })
            .collect(),
    };
    if let Err(error) = app.emit("tts-request", ask) {
        bridge.clear();
        return Err(error.to_string());
    }
    let total = items.len();
    let mut reported = 0;
    let outcome = loop {
        if cancel.load(Ordering::Relaxed) {
            break Err("Đã dừng.".to_string());
        }
        match receiver.recv_timeout(Duration::from_millis(200)) {
            Ok(result) => break result,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let (saved, touched) = {
                    let pending = bridge.lock();
                    (pending.saved, pending.touched)
                };
                if saved != reported {
                    reported = saved;
                    progress(saved, total);
                }
                let limit = if saved == 0 {
                    FIRST_LINE_WAIT
                } else {
                    NEXT_LINE_WAIT
                };
                if touched.is_some_and(|at| at.elapsed() > limit) {
                    let _ = app.emit("tts-cancel", ());
                    break Err(
                        "Giao diện không trả lời khi đọc giọng. Hãy giữ cửa sổ app mở.".to_string(),
                    );
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                break Err("Mất kết nối với bộ đọc giọng.".to_string())
            }
        }
    };
    bridge.clear();
    outcome
}

#[derive(Clone, Serialize)]
struct CleanItem {
    index: usize,
    text: String,
}

#[derive(Clone, Serialize)]
struct CleanAsk {
    lang: String,
    items: Vec<CleanItem>,
}

// Asks the window to normalize text (numbers, dates, abbreviations) and waits for the chunks.
// Does not write wav files. The first reply has the same 180s budget as the first spoken line.
pub(crate) fn prepare(
    app: &AppHandle,
    lang: &str,
    items: &[(usize, String)],
    cancel: &AtomicBool,
) -> Result<Vec<(usize, Vec<String>)>, String> {
    let bridge = app.state::<Bridge>();
    let (sender, receiver) = mpsc::channel();
    {
        let mut pending = bridge.lock();
        if pending.prepared.is_some() {
            return Err("Đang chuẩn hóa một yêu cầu khác.".into());
        }
        pending.prepared = Some(sender);
    }
    let ask = CleanAsk {
        lang: lang.to_string(),
        items: items
            .iter()
            .map(|(index, text)| CleanItem {
                index: *index,
                text: text.clone(),
            })
            .collect(),
    };
    if let Err(error) = app.emit("tts-clean", ask) {
        bridge.lock().prepared = None;
        return Err(error.to_string());
    }
    let started = Instant::now();
    let outcome = loop {
        if cancel.load(Ordering::Relaxed) {
            break Err("Đã dừng.".to_string());
        }
        match receiver.recv_timeout(Duration::from_millis(200)) {
            Ok(result) => break result,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if started.elapsed() > FIRST_LINE_WAIT {
                    break Err(
                        "Giao diện không trả lời khi chuẩn hóa chữ. Hãy giữ cửa sổ app mở."
                            .to_string(),
                    );
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                break Err("Mất kết nối với bộ đọc giọng.".to_string());
            }
        }
    };
    bridge.lock().prepared = None;
    let lines = outcome?;
    let got: std::collections::HashSet<usize> = lines.iter().map(|(index, _)| *index).collect();
    if items.iter().any(|(index, _)| !got.contains(index)) {
        return Err("Thiếu câu đã chuẩn hóa.".into());
    }
    Ok(lines)
}

#[tauri::command]
pub(crate) async fn read_voice(
    lang: String,
    name: String,
    kind: String,
) -> Result<Response, String> {
    let read = tauri::async_runtime::spawn_blocking(move || {
        let dir =
            crate::bundled_voice_dir(&lang).ok_or("Chưa thấy folder giọng của ngôn ngữ này.")?;
        let path = voice_file(&dir, &name, &kind)?;
        std::fs::read(&path).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?;
    read.map(Response::new)
}

fn preview_text(code: &str) -> &'static str {
    match code {
        "en" => "Hello, this is my voice. Today is the twenty-fifth of December.",
        "zh" => "你好，这是我的声音。今天是十二月二十五日。",
        "ja" => "こんにちは、これは私の声です。今日は十二月二十五日です。",
        "ko" => "안녕하세요, 제 목소리입니다. 오늘은 십이월 이십오일입니다.",
        "fr" => "Bonjour, voici ma voix. Aujourd'hui, nous sommes le vingt-cinq décembre.",
        "de" => "Hallo, das ist meine Stimme. Heute ist der fünfundzwanzigste Dezember.",
        "es" => "Hola, esta es mi voz. Hoy es veinticinco de diciembre.",
        "ru" => "Здравствуйте, это мой голос. Сегодня двадцать пятое декабря.",
        "th" => "สวัสดี นี่คือเสียงของฉัน วันนี้คือวันที่ยี่สิบห้าธันวาคม",
        "id" => "Halo, ini suara saya. Hari ini tanggal dua puluh lima Desember.",
        "pt" => "Olá, esta é a minha voz. Hoje é vinte e cinco de dezembro.",
        _ => "Xin chào, đây là giọng đọc của tôi. Hôm nay là ngày 25 tháng 12 năm 2024.",
    }
}

// Reads one sentence through the same path as a dub and returns the wav so the window can play it.
#[tauri::command]
pub(crate) async fn preview_voice(
    app: AppHandle,
    lang: String,
    name: String,
) -> Result<Response, String> {
    let done = tauri::async_runtime::spawn_blocking(move || {
        let code = crate::pipeline::clean_target(&lang).0;
        let dir =
            crate::bundled_voice_dir(code).ok_or("Chưa thấy folder giọng của ngôn ngữ này.")?;
        voice_file(&dir, &name, "model")?;
        let path = std::env::temp_dir().join("voice-preview.wav");
        let _ = std::fs::remove_file(&path);
        let cancel = AtomicBool::new(false);
        let exe = crate::piper::locate();
        let caps = exe.as_ref().and_then(|path| crate::piper::read_caps(path));
        if crate::piper::choose_engine(exe.clone(), caps) == crate::piper::SpeakEngine::Native {
            let exe = exe.expect("choose_engine Native chỉ khi có exe");
            let caps = caps.expect("choose_engine Native chỉ khi có cờ");
            let prepared = prepare(
                &app,
                code,
                &[(0usize, preview_text(code).to_string())],
                &cancel,
            )?;
            let chunks = prepared
                .iter()
                .find(|(index, _)| *index == 0)
                .map(|(_, chunks)| chunks.clone())
                .unwrap_or_default();
            match crate::piper::plan_line(&chunks) {
                crate::piper::LinePlan::Silence => {
                    write_wav(
                        &path,
                        &Pcm {
                            rate: 22050,
                            samples: vec![0; 2205],
                        },
                    )?;
                    return std::fs::read(&path).map_err(|error| error.to_string());
                }
                crate::piper::LinePlan::Speak(text) => {
                    let model = crate::piper::model_path(&dir, &name);
                    let scales = crate::piper::scales_from_config(
                        &std::fs::read_to_string(dir.join(format!("{name}.onnx.json")))
                            .unwrap_or_default(),
                    );
                    let items = [crate::piper::SpeechLine {
                        text,
                        output: path.clone(),
                    }];
                    match crate::piper::speak_native(
                        &exe,
                        &model,
                        &caps,
                        scales,
                        &items,
                        &cancel,
                        &|_, _| {},
                    ) {
                        Ok(()) => return std::fs::read(&path).map_err(|error| error.to_string()),
                        Err(error)
                            if crate::piper::after_native(&error)
                                == crate::piper::AfterNative::Stop =>
                        {
                            return Err(error);
                        }
                        Err(_) => {
                            let valid = std::fs::read(&path)
                                .ok()
                                .is_some_and(|bytes| crate::wav::parse_wav(&bytes).is_ok());
                            if valid {
                                return std::fs::read(&path).map_err(|error| error.to_string());
                            }
                            let _ = std::fs::remove_file(&path);
                        }
                    }
                }
            }
        }
        let items = vec![(0usize, preview_text(code).to_string(), path.clone())];
        run(&app, &name, code, &items, false, &cancel, &|_, _| {})?;
        std::fs::read(&path).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?;
    done.map(Response::new)
}

#[tauri::command]
pub(crate) fn save_line_wav(bridge: State<'_, Bridge>, request: Request<'_>) -> Result<(), String> {
    let number = |name: &str| {
        request
            .headers()
            .get(name)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())
    };
    let index = number("index").ok_or("Thiếu số câu.")? as usize;
    let rate = number("rate").ok_or("Thiếu tần số mẫu.")? as u32;
    let InvokeBody::Raw(bytes) = request.body() else {
        return Err("Thiếu dữ liệu âm thanh.".into());
    };
    let path = bridge
        .lock()
        .paths
        .get(&index)
        .cloned()
        .ok_or("Câu này không nằm trong yêu cầu đang đọc.")?;
    write_wav(
        &path,
        &Pcm {
            rate,
            samples: samples_from_bytes(bytes),
        },
    )?;
    let mut pending = bridge.lock();
    pending.saved += 1;
    pending.touched = Some(Instant::now());
    Ok(())
}

#[derive(serde::Deserialize)]
pub(crate) struct PreparedLine {
    index: usize,
    chunks: Vec<String>,
}

#[tauri::command]
pub(crate) fn tts_prepared(
    bridge: State<'_, Bridge>,
    lines: Vec<PreparedLine>,
    error: Option<String>,
) {
    let sender = bridge.lock().prepared.clone();
    if let Some(sender) = sender {
        let _ = sender.send(match error {
            Some(message) => Err(message),
            None => Ok(lines
                .into_iter()
                .map(|line| (line.index, line.chunks))
                .collect()),
        });
    }
}

#[tauri::command]
pub(crate) fn tts_finish(bridge: State<'_, Bridge>, error: Option<String>) {
    let sender = bridge.lock().done.clone();
    if let Some(sender) = sender {
        let _ = sender.send(match error {
            Some(message) => Err(message),
            None => Ok(()),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("tts-voice-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn voice_file_only_serves_known_voices() {
        let dir = scratch("files");
        fs::write(dir.join("a.onnx"), b"x").unwrap();
        fs::write(dir.join("a.onnx.json"), b"{}").unwrap();
        assert_eq!(voice_file(&dir, "a", "model").unwrap(), dir.join("a.onnx"));
        assert_eq!(
            voice_file(&dir, "a", "config").unwrap(),
            dir.join("a.onnx.json")
        );
        assert!(voice_file(&dir, "a", "other").is_err());
        assert!(voice_file(&dir, "..\\secret", "model").is_err());
        assert!(voice_file(&dir, "b", "model").is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn samples_are_little_endian_pairs() {
        assert_eq!(
            samples_from_bytes(&[1, 0, 0xff, 0xff, 0x00, 0x80, 7]),
            vec![1, -1, i16::MIN]
        );
    }
}
