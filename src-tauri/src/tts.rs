// The voice runs in the app window (src/tts): Rust asks for lines to be read, the window
// speaks them with the Piper voice in voices/<language> and sends every wav back to be written here.
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::ipc::{InvokeBody, Request, Response};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::wav::{write_wav, Pcm};

const FIRST_LINE_WAIT: Duration = Duration::from_secs(180);
const NEXT_LINE_WAIT: Duration = Duration::from_secs(120);

fn next_session() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed).to_string()
}

#[derive(Clone, Serialize)]
struct Item {
    index: usize,
    text: String,
}

#[derive(Clone, Serialize)]
struct Ask {
    session: String,
    voice: String,
    lang: String,
    whole: bool,
    items: Vec<Item>,
}

#[derive(Default)]
struct Pending {
    session: String,
    done: Option<mpsc::Sender<Result<(), String>>>,
    prepared: Option<mpsc::Sender<Result<Vec<(usize, Vec<String>)>, String>>>,
    paths: HashMap<usize, PathBuf>,
    tickets: HashMap<usize, crate::draft::SpeechTicket>,
    saved: usize,
    touched: Option<Instant>,
}

impl Pending {
    // Keep ownership checked through the file commit. A cancelled request must not
    // look up the same index in the next job's paths or acknowledge its progress.
    fn save(&mut self, session: &str, index: usize, pcm: &Pcm) -> Result<(), String> {
        if self.session != session || self.done.is_none() {
            return Err("Yêu cầu đọc giọng đã hết hiệu lực.".into());
        }
        let path = self.paths.get(&index).ok_or("Câu này không còn chờ ghi tiếng.")?;
        if let Some(ticket) = self.tickets.get(&index) {
            crate::draft::commit_speech(ticket, path, pcm)?;
        } else {
            write_wav(path, pcm)?;
        }
        self.paths.remove(&index);
        self.tickets.remove(&index);
        self.saved += 1;
        self.touched = Some(Instant::now());
        Ok(())
    }

    fn finish(&mut self, session: &str, error: Option<String>) {
        if self.session != session { return; }
        if let Some(sender) = self.done.take() {
            let result = match error {
                Some(message) => Err(message),
                None if !self.paths.is_empty() => Err("Bộ đọc giọng chưa gửi đủ câu.".into()),
                None => Ok(()),
            };
            let _ = sender.send(result);
        }
    }

    fn prepared(&mut self, session: &str, lines: Vec<PreparedLine>, error: Option<String>) {
        if self.session != session { return; }
        if let Some(sender) = self.prepared.take() {
            let _ = sender.send(match error {
                Some(message) => Err(message),
                None => Ok(lines.into_iter().map(|line| (line.index, line.chunks)).collect()),
            });
        }
    }
}

#[derive(Default)]
pub(crate) struct Bridge(Mutex<Pending>);

impl Bridge {
    fn lock(&self) -> MutexGuard<'_, Pending> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn clear(&self, session: &str) {
        let mut pending = self.lock();
        if pending.session == session {
            *pending = Pending::default();
        }
    }

    pub(crate) fn cancel(&self) {
        let mut pending = self.lock();
        if let Some(sender) = pending.done.take() {
            let _ = sender.send(Err("Đã dừng.".into()));
        }
        if let Some(sender) = pending.prepared.take() {
            let _ = sender.send(Err("Đã dừng.".into()));
        }
        *pending = Pending::default();
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
    let session = next_session();
    let mut tickets = HashMap::new();
    for (index, text, path) in items {
        if let Some(ticket) = crate::draft::speech_ticket(path, text, lang, voice)? {
            tickets.insert(*index, ticket);
        }
    }
    {
        let mut pending = bridge.lock();
        if cancel.load(Ordering::SeqCst) { return Err("Đã dừng.".into()); }
        if !pending.session.is_empty() {
            return Err("Đang đọc một yêu cầu khác.".into());
        }
        pending.done = Some(sender);
        pending.tickets = tickets;
        pending.session = session.clone();
        pending.paths = items
            .iter()
            .map(|(index, _, path)| (*index, path.clone()))
            .collect();
        pending.saved = 0;
        pending.touched = Some(Instant::now());
    }
    let ask = Ask {
        session: session.clone(),
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
        bridge.clear(&session);
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
    bridge.clear(&session);
    if outcome.is_err() {
        let _ = app.emit("tts-cancel", &session);
    }
    outcome
}

#[derive(Clone, Serialize)]
struct CleanItem {
    index: usize,
    text: String,
}

#[derive(Clone, Serialize)]
struct CleanAsk {
    session: String,
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
    let session = next_session();
    {
        let mut pending = bridge.lock();
        if cancel.load(Ordering::SeqCst) { return Err("Đã dừng.".into()); }
        if !pending.session.is_empty() {
            return Err("Đang chuẩn hóa một yêu cầu khác.".into());
        }
        pending.prepared = Some(sender);
        pending.session = session.clone();
    }
    let ask = CleanAsk {
        session: session.clone(),
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
        bridge.clear(&session);
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
    bridge.clear(&session);
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
    text: Option<String>,
) -> Result<Response, String> {
    let done = tauri::async_runtime::spawn_blocking(move || {
        let code = crate::pipeline::clean_target(&lang).0;
        let dir =
            crate::bundled_voice_dir(code).ok_or("Chưa thấy folder giọng của ngôn ngữ này.")?;
        voice_file(&dir, &name, "model")?;
        let text=text.filter(|s|!s.trim().is_empty()).unwrap_or_else(||preview_text(code).into());
        if text.len()>3000{return Err("Câu thử giọng quá dài.".into());}
        let control=app.state::<crate::DubControl>();
        let _lease=control.tasks.acquire(crate::coordinator::Kind::Auxiliary)?;
        static NEXT:std::sync::atomic::AtomicU64=std::sync::atomic::AtomicU64::new(0);
        let path=std::env::temp_dir().join(format!("voice-preview-{}-{}.wav",std::process::id(),NEXT.fetch_add(1,Ordering::Relaxed)));
        let _file=std::fs::OpenOptions::new().write(true).create_new(true).open(&path).map_err(|e|e.to_string())?;
        struct Temp(std::path::PathBuf);
        impl Drop for Temp{fn drop(&mut self){let _=std::fs::remove_file(&self.0);}}
        let _temp=Temp(path.clone());
        drop(_file);
        let cancel = &control.cancel;
        let exe = crate::piper::locate();
        let caps = exe.as_ref().and_then(|path| crate::piper::read_caps(path));
        if crate::piper::choose_engine(exe.clone(), caps) == crate::piper::SpeakEngine::Native {
            let exe = exe.expect("choose_engine Native chỉ khi có exe");
            let caps = caps.expect("choose_engine Native chỉ khi có cờ");
            let prepared = prepare(
                &app,
                code,
                &[(0usize, text.clone())],
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
        let items = vec![(0usize, text, path.clone())];
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
    let rate = u32::try_from(number("rate").ok_or("Thiếu tần số mẫu.")?)
        .map_err(|_| "Tần số mẫu không hợp lệ.")?;
    let session = request.headers().get("session").and_then(|v| v.to_str().ok())
        .ok_or("Thiếu mã yêu cầu đọc giọng.")?;
    let InvokeBody::Raw(bytes) = request.body() else {
        return Err("Thiếu dữ liệu âm thanh.".into());
    };
    if bytes.len() % 2 != 0 { return Err("Dữ liệu âm thanh bị cụt.".into()); }
    bridge.lock().save(
        session, index,
        &Pcm {
            rate,
            samples: samples_from_bytes(bytes),
        },
    )
}

#[derive(serde::Deserialize)]
pub(crate) struct PreparedLine {
    index: usize,
    chunks: Vec<String>,
}

#[tauri::command]
pub(crate) fn tts_prepared(
    bridge: State<'_, Bridge>,
    session: String,
    lines: Vec<PreparedLine>,
    error: Option<String>,
) {
    bridge.lock().prepared(&session, lines, error);
}

#[tauri::command]
pub(crate) fn tts_finish(bridge: State<'_, Bridge>, session: String, error: Option<String>) {
    bridge.lock().finish(&session, error);
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

    fn pending_line(path: PathBuf) -> (Pending, mpsc::Receiver<Result<(), String>>) {
        let (sender, receiver) = mpsc::channel();
        (Pending {
            session: "new-job".into(),
            done: Some(sender),
            paths: [(1, path)].into_iter().collect(),
            ..Pending::default()
        }, receiver)
    }

    #[test]
    fn late_pcm_and_finish_cannot_overwrite_or_finish_the_next_job() {
        let root = scratch("stale-session");
        let path = root.join("001.wav");
        fs::write(&path, b"previous checkpoint").unwrap();
        let (mut pending, receiver) = pending_line(path.clone());
        let pcm = Pcm { rate: 22050, samples: vec![1, 2, 3] };
        assert!(pending.save("cancelled-job", 1, &pcm).is_err());
        pending.finish("cancelled-job", None);
        assert!(matches!(receiver.try_recv(), Err(mpsc::TryRecvError::Empty)));
        assert_eq!(fs::read(&path).unwrap(), b"previous checkpoint");
        assert_eq!(pending.saved, 0);
        pending.save("new-job", 1, &pcm).unwrap();
        assert_eq!(crate::wav::read_wav(&path).unwrap().samples, pcm.samples);
        // A repeated IPC response does not count another sentence as complete.
        assert!(pending.save("new-job", 1, &pcm).is_err());
        assert_eq!(pending.saved, 1);
        pending.finish("new-job", None);
        assert!(receiver.recv().unwrap().is_ok());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn incomplete_or_invalid_audio_is_not_acknowledged_as_ready() {
        let root = scratch("invalid-pcm");
        let path = root.join("001.wav");
        fs::write(&path, b"keep").unwrap();
        let (mut pending, receiver) = pending_line(path.clone());
        for pcm in [Pcm { rate: 0, samples: vec![1] }, Pcm { rate: 22050, samples: vec![] }] {
            assert!(pending.save("new-job", 1, &pcm).is_err());
        }
        assert_eq!(fs::read(&path).unwrap(), b"keep");
        assert_eq!(pending.saved, 0);
        pending.finish("new-job", None);
        assert!(receiver.recv().unwrap().is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cancelling_revokes_paths_and_wakes_the_waiter() {
        let root = scratch("cancel-session");
        let path = root.join("001.wav");
        let (pending, receiver) = pending_line(path.clone());
        let bridge = Bridge(Mutex::new(pending));
        bridge.cancel();
        assert!(receiver.recv().unwrap().is_err());
        let pcm = Pcm { rate: 22050, samples: vec![1] };
        assert!(bridge.lock().save("new-job", 1, &pcm).is_err());
        assert!(!path.exists());
        // Cleanup from the previous task must not revoke a newer owner.
        bridge.lock().session = "another-job".into();
        bridge.clear("new-job");
        assert_eq!(bridge.lock().session, "another-job");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn late_normalization_cannot_complete_a_new_native_request() {
        let (sender, receiver) = mpsc::channel();
        let mut pending = Pending {
            session: "new-clean".into(), prepared: Some(sender), ..Pending::default()
        };
        pending.prepared("old-clean", vec![], Some("old error".into()));
        assert!(matches!(receiver.try_recv(), Err(mpsc::TryRecvError::Empty)));
        pending.prepared("new-clean", vec![PreparedLine { index: 7, chunks: vec!["đúng câu".into()] }], None);
        assert_eq!(receiver.recv().unwrap().unwrap(), vec![(7, vec!["đúng câu".into()])]);
    }
}
