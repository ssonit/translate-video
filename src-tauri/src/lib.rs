mod deepseek_server;
mod job;
mod listen;
mod names;
mod ollama;
mod parallel;
mod pipeline;
mod piper;
mod speech;
mod timeline;
mod tts;
mod watch;
mod wav;
mod whisper_install;

use serde::{Deserialize, Serialize};
use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager};
use tauri_plugin_opener::OpenerExt;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

fn hidden(command: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

// Whisper, FFmpeg and piper.exe sit below the window so the machine stays usable while they run.
pub(crate) fn yield_cpu(child: &std::process::Child) {
    #[cfg(windows)]
    {
        const PROCESS_SET_INFORMATION: u32 = 0x0200;
        const BELOW_NORMAL_PRIORITY_CLASS: u32 = 0x00004000;
        #[link(name = "kernel32")]
        extern "system" {
            fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut std::ffi::c_void;
            fn SetPriorityClass(handle: *mut std::ffi::c_void, class: u32) -> i32;
            fn CloseHandle(handle: *mut std::ffi::c_void) -> i32;
        }
        unsafe {
            let handle = OpenProcess(PROCESS_SET_INFORMATION, 0, child.id());
            if !handle.is_null() {
                let _ = SetPriorityClass(handle, BELOW_NORMAL_PRIORITY_CLASS);
                let _ = CloseHandle(handle);
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = child;
    }
}

#[derive(Serialize)]
struct ToolCheck {
    id: String,
    ready: bool,
    detail: String,
}

fn command_on_path(name: &str) -> bool {
    hidden(&mut Command::new("where.exe").arg(name))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn ollama_program() -> Option<PathBuf> {
    if command_on_path("ollama") {
        return Some(PathBuf::from("ollama"));
    }
    let local = std::env::var("LOCALAPPDATA").ok()?;
    let path = PathBuf::from(local)
        .join("Programs")
        .join("Ollama")
        .join("ollama.exe");
    path.is_file().then_some(path)
}

pub(crate) fn capture(mut command: Command, timeout: Duration) -> Result<String, String> {
    let mut child = hidden(&mut command)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| error.to_string())?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut stdout = String::new();
                let mut stderr = String::new();
                if let Some(mut pipe) = child.stdout.take() {
                    let _ = pipe.read_to_string(&mut stdout);
                }
                if let Some(mut pipe) = child.stderr.take() {
                    let _ = pipe.read_to_string(&mut stderr);
                }
                if status.success() {
                    return Ok(stdout);
                }
                let error = if stderr.trim().is_empty() {
                    stdout
                } else {
                    stderr
                };
                return Err(error);
            }
            Ok(None) if started.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(String::from("timeout"));
            }
            Ok(None) => thread::sleep(Duration::from_millis(60)),
            Err(error) if error.kind() == ErrorKind::TimedOut => {
                return Err(String::from("timeout"));
            }
            Err(error) => return Err(error.to_string()),
        }
    }
}

fn version_line(program: &str) -> Option<String> {
    let mut command = Command::new(program);
    command.arg("-version");
    let text = capture(command, Duration::from_secs(4)).ok()?;
    text.lines().next().map(|line| line.trim().to_string())
}

fn ffmpeg_check() -> ToolCheck {
    let ffmpeg = command_on_path("ffmpeg");
    let ffprobe = command_on_path("ffprobe");
    let (ready, detail) = match (ffmpeg, ffprobe) {
        (true, true) => (
            true,
            version_line("ffmpeg").unwrap_or_else(|| "Đã thấy ffmpeg và ffprobe.".into()),
        ),
        (true, false) => (
            false,
            "Có ffmpeg, chưa thấy ffprobe. Tải bản essentials đầy đủ.".into(),
        ),
        _ => (false, "Chưa thấy ffmpeg trong PATH.".into()),
    };
    ToolCheck {
        id: "ffmpeg".into(),
        ready,
        detail,
    }
}

fn ollama_check(choice: &TranslationChoice, skip_translate: bool) -> (ToolCheck, Option<PathBuf>) {
    let program = ollama_program();
    let cloud = choice.provider != "local";
    let check = ToolCheck {
        id: "ollama".into(),
        ready: skip_translate || cloud || program.is_some(),
        detail: if skip_translate {
            "Không dịch, chỉ đổi giọng. Không cần Ollama.".into()
        } else if choice.provider == "deepseek-local" {
            "Không cần Ollama. DeepSeek local dùng server trên máy.".into()
        } else if cloud {
            "Không cần Ollama khi dịch bằng API key. Chỉ cài nếu muốn dịch trên máy.".into()
        } else if program.is_some() {
            "Đã thấy Ollama trên máy.".into()
        } else {
            "Chưa thấy Ollama.".into()
        },
    };
    (check, program)
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TranslationChoice {
    provider: String,
    api_key: String,
    // Trống = model mặc định của dịch vụ. Chọn ở màn lồng tiếng.
    #[serde(default)]
    model: String,
    // Key và model đã lưu cho từng dịch vụ qua mạng, để đổi dịch vụ không phải nhập lại key.
    #[serde(default)]
    saved: std::collections::BTreeMap<String, SavedProvider>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SavedProvider {
    api_key: String,
    #[serde(default)]
    model: String,
}

impl TranslationChoice {
    // Ghi key và model đang dùng vào danh sách đã lưu của dịch vụ hiện tại.
    fn stashed(mut self) -> Self {
        if self.provider != "local" {
            let entry = SavedProvider {
                api_key: self.api_key.clone(),
                model: self.model.clone(),
            };
            self.saved.insert(self.provider.clone(), entry);
        }
        self
    }

    // Đổi sang dịch vụ khác bằng key đã lưu. "local" và "deepseek-local" không cần key.
    fn switched(self, next: &str) -> Result<Self, String> {
        let mut current = self.stashed();
        let saved = std::mem::take(&mut current.saved);
        if next == "local" {
            return Ok(Self {
                provider: "local".into(),
                api_key: String::new(),
                model: String::new(),
                saved,
            });
        }
        if next == "deepseek-local" {
            let model = saved
                .get(next)
                .map(|entry| entry.model.clone())
                .unwrap_or_default();
            return Ok(Self {
                provider: "deepseek-local".into(),
                api_key: String::new(),
                model,
                saved,
            });
        }
        let entry = saved
            .get(next)
            .filter(|entry| !entry.api_key.trim().is_empty());
        let Some(entry) = entry else {
            return Err("Chưa có API key.".into());
        };
        Ok(Self {
            provider: next.into(),
            api_key: entry.api_key.clone(),
            model: entry.model.clone(),
            saved,
        })
    }
}

impl Default for TranslationChoice {
    fn default() -> Self {
        Self {
            provider: "local".into(),
            api_key: String::new(),
            model: String::new(),
            saved: Default::default(),
        }
    }
}

fn normalize_choice(provider: &str, api_key: &str) -> Result<TranslationChoice, String> {
    match provider {
        "local" => Ok(TranslationChoice {
            provider: "local".into(),
            api_key: String::new(),
            model: String::new(),
            saved: Default::default(),
        }),
        // Server Deepseek-API trên máy dùng tài khoản chat, không có API key.
        "deepseek-local" => Ok(TranslationChoice {
            provider: "deepseek-local".into(),
            api_key: String::new(),
            model: String::new(),
            saved: Default::default(),
        }),
        "gemini" | "openai" | "claude" | "deepseek" => {
            let key = api_key.trim();
            if key.is_empty() {
                return Err("Chưa có API key.".into());
            }
            Ok(TranslationChoice {
                provider: provider.into(),
                api_key: key.into(),
                model: String::new(),
                saved: Default::default(),
            })
        }
        _ => Err("Không rõ dịch vụ.".into()),
    }
}

// Tên model đi vào URL hoặc JSON gửi đi, nên chỉ nhận ký tự an toàn. Dấu "/" chỉ cho Ollama (hf.co/...).
fn clean_model(provider: &str, model: &str) -> Result<String, String> {
    let model = model.trim();
    let safe = |c: char| {
        c.is_ascii_alphanumeric()
            || matches!(c, '.' | '_' | '-' | ':')
            || (c == '/' && provider == "local")
    };
    if model.len() > 80 || !model.chars().all(safe) {
        return Err("Tên model chỉ gồm chữ, số và các dấu . _ - :".into());
    }
    Ok(model.to_string())
}

// Bản chỉ có DeepSeek trên máy lưu nó là "deepseek" và không có key. Nay đó là "deepseek-local",
// còn "deepseek" là API key. Chuyển lựa chọn cũ sang để khỏi rơi về Ollama và mất các key đã lưu.
fn migrated(mut choice: TranslationChoice) -> TranslationChoice {
    if choice.provider == "deepseek" && choice.api_key.trim().is_empty() {
        choice.provider = "deepseek-local".into();
    }
    let keyless = choice
        .saved
        .get("deepseek")
        .is_some_and(|entry| entry.api_key.trim().is_empty());
    if keyless {
        if let Some(old) = choice.saved.remove("deepseek") {
            choice.saved.entry("deepseek-local".into()).or_insert(old);
        }
    }
    choice
}

fn read_choice(path: &Path) -> TranslationChoice {
    let Ok(text) = std::fs::read_to_string(path) else {
        return TranslationChoice::default();
    };
    serde_json::from_str(&text)
        .ok()
        .and_then(|choice: TranslationChoice| {
            let choice = migrated(choice);
            let mut fixed = normalize_choice(&choice.provider, &choice.api_key).ok()?;
            fixed.model = clean_model(&fixed.provider, &choice.model).unwrap_or_default();
            for (name, entry) in &choice.saved {
                let Ok(clean) = normalize_choice(name, &entry.api_key) else {
                    continue;
                };
                if clean.provider == "local" {
                    continue;
                }
                let model = clean_model(name, &entry.model).unwrap_or_default();
                fixed.saved.insert(
                    name.clone(),
                    SavedProvider {
                        api_key: clean.api_key,
                        model,
                    },
                );
            }
            Some(fixed)
        })
        .unwrap_or_default()
}

fn write_choice(path: &Path, choice: &TranslationChoice) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    }
    let text =
        serde_json::to_string(&choice.clone().stashed()).map_err(|error| error.to_string())?;
    std::fs::write(path, text).map_err(|error| error.to_string())
}

fn choice_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    Ok(dir.join("translation.json"))
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppPrefs {
    #[serde(default)]
    output_dir: String,
    #[serde(default)]
    use_terms: bool,
    // false: video kết quả không đốt chữ. File phụ đề .srt vẫn có.
    #[serde(default)]
    burn_captions: bool,
    // true: video đã đúng ngôn ngữ muốn nghe. Chép lời rồi đọc lại, không gọi model dịch.
    #[serde(default)]
    skip_translate: bool,
}

fn prefs_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    Ok(dir.join("prefs.json"))
}

fn read_prefs(path: &Path) -> AppPrefs {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or(AppPrefs {
            output_dir: String::new(),
            use_terms: false,
            burn_captions: false,
            skip_translate: false,
        })
}

fn write_prefs(path: &Path, prefs: &AppPrefs) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    }
    let text = serde_json::to_string(prefs).map_err(|error| error.to_string())?;
    std::fs::write(path, text).map_err(|error| error.to_string())
}

const SUGGESTED_LOCAL_MODELS: &[&str] = &["translategemma:4b", "qwen3.5:9b-q4_K_M", "gemma3:12b"];

/// Chỉ ba model gợi ý. Model local khác vẫn chạy được, không đưa vào câu kiểm tra.
pub(crate) fn suggested_models_present(installed: &[String]) -> Vec<&'static str> {
    SUGGESTED_LOCAL_MODELS
        .iter()
        .copied()
        .filter(|name| installed.iter().any(|have| have == name))
        .collect()
}

/// Tên model trong JSON của `GET /api/tags`. Không gọi `ollama list`: lệnh đó mở ứng dụng Ollama nếu chưa chạy.
pub(crate) fn tags_model_names(body: &serde_json::Value) -> Vec<String> {
    body.get("models")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|model| {
            let name = model.get("name")?.as_str()?.trim();
            if name.is_empty() {
                None
            } else {
                Some(name.to_string())
            }
        })
        .collect()
}

/// Hỏi Ollama khi nó đã mở. Cổng đóng thì trả lỗi, không khởi động chương trình.
fn running_model_names() -> Result<Vec<String>, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_millis(800))
        .timeout(Duration::from_secs(2))
        .build();
    let response = agent
        .get("http://127.0.0.1:11434/api/tags")
        .call()
        .map_err(|_| String::from("closed"))?;
    let text = response
        .into_string()
        .map_err(|_| String::from("Ollama không trả danh sách model."))?;
    let body: serde_json::Value = serde_json::from_str(&text)
        .map_err(|_| String::from("Ollama không trả danh sách model."))?;
    Ok(tags_model_names(&body))
}

// Hỏi Ollama mất gần một giây khi nó chưa mở. Lệnh không async chạy trên luồng chính của cửa sổ
// và làm nó đơ, nên việc này chạy ở luồng nền như check_setup.
#[tauri::command]
async fn list_local_models() -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        if ollama_program().is_none() {
            return Err(String::from("Chưa thấy Ollama."));
        }
        running_model_names().map_err(|error| {
            if error == "closed" {
                String::from("Mở Ollama rồi kiểm tra lại.")
            } else {
                error
            }
        })
    })
    .await
    .unwrap_or_else(|_| Err(String::from("Chưa đọc được danh sách model.")))
}

fn usable_output_dir(saved: &str) -> Result<String, String> {
    let saved = saved.trim();
    if !saved.is_empty() {
        let path = PathBuf::from(saved);
        if path.is_dir() {
            return Ok(saved.to_string());
        }
    }
    default_output_dir()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PrefsView {
    output_dir: String,
    use_terms: bool,
    burn_captions: bool,
    skip_translate: bool,
}

fn prefs_view(prefs: &AppPrefs) -> Result<PrefsView, String> {
    Ok(PrefsView {
        output_dir: usable_output_dir(&prefs.output_dir)?,
        use_terms: prefs.use_terms,
        burn_captions: prefs.burn_captions,
        skip_translate: prefs.skip_translate,
    })
}

#[tauri::command]
fn get_prefs(app: tauri::AppHandle) -> Result<PrefsView, String> {
    let path = prefs_path(&app)?;
    prefs_view(&read_prefs(&path))
}

#[tauri::command]
fn save_prefs(
    app: tauri::AppHandle,
    output_dir: Option<String>,
    use_terms: Option<bool>,
    burn_captions: Option<bool>,
    skip_translate: Option<bool>,
) -> Result<PrefsView, String> {
    let path = prefs_path(&app)?;
    let mut prefs = read_prefs(&path);
    if let Some(dir) = output_dir {
        let dir = dir.trim();
        if !Path::new(dir).is_dir() {
            return Err("Folder kết quả chưa có.".into());
        }
        prefs.output_dir = dir.to_string();
    }
    if let Some(use_terms) = use_terms {
        prefs.use_terms = use_terms;
    }
    if let Some(burn_captions) = burn_captions {
        prefs.burn_captions = burn_captions;
    }
    if let Some(skip_translate) = skip_translate {
        prefs.skip_translate = skip_translate;
    }
    write_prefs(&path, &prefs)?;
    prefs_view(&prefs)
}

fn model_check(
    program: Option<&PathBuf>,
    choice: &TranslationChoice,
    skip_translate: bool,
) -> ToolCheck {
    if skip_translate {
        return ToolCheck {
            id: "model".into(),
            ready: true,
            detail: "Không dịch, chỉ đổi giọng. Không cần model dịch.".into(),
        };
    }
    if choice.provider == "deepseek-local" {
        return deepseek_local_status(ollama::deepseek_local_up());
    }
    if choice.provider != "local" {
        let name = match choice.provider.as_str() {
            "gemini" => "Gemini",
            "openai" => "ChatGPT",
            "claude" => "Claude",
            "deepseek" => "DeepSeek",
            _ => "dịch vụ qua mạng",
        };
        return ToolCheck {
            id: "model".into(),
            ready: !choice.api_key.trim().is_empty(),
            detail: if choice.api_key.trim().is_empty() {
                format!("Chưa có API key cho {name}.")
            } else {
                format!("Dịch bằng {name}. Chỉ chữ lời thoại đi qua mạng.")
            },
        };
    }
    if program.is_none() {
        return ToolCheck {
            id: "model".into(),
            ready: false,
            detail: "Cài Ollama trước, rồi mới kéo model dịch.".into(),
        };
    }
    match running_model_names() {
        Ok(names) => {
            if names.is_empty() {
                ToolCheck {
                    id: "model".into(),
                    ready: false,
                    detail: "Ollama đang chạy, chưa có model dịch.".into(),
                }
            } else {
                let suggested = suggested_models_present(&names);
                ToolCheck {
                    id: "model".into(),
                    ready: true,
                    detail: if suggested.is_empty() {
                        "Ba model gợi ý chưa tải. Model local khác vẫn gõ tên được.".into()
                    } else {
                        "Đã có model dịch trên máy.".into()
                    },
                }
            }
        }
        Err(_) => ToolCheck {
            id: "model".into(),
            ready: false,
            detail: "Ollama có trên máy. Hãy mở ứng dụng Ollama, rồi kiểm tra lại.".into(),
        },
    }
}

fn deepseek_local_status(up: bool) -> ToolCheck {
    ToolCheck {
        id: "model".into(),
        ready: up,
        detail: if up {
            "Dịch bằng DeepSeek local. Server đang chạy tại http://127.0.0.1:8000.".into()
        } else {
            "Chưa bật DeepSeek local. Bấm Bật DeepSeek bên cạnh.".into()
        },
    }
}

pub(crate) fn program_path(name: &str) -> Option<PathBuf> {
    let mut command = Command::new("where.exe");
    command.arg(name);
    let text = capture(command, Duration::from_secs(4)).ok()?;
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(PathBuf::from)
        .filter(|path| path.is_file())
}

// Voices of one target language live in voices/<code>. The code is checked against the known targets.
fn bundled_voice_dir(lang: &str) -> Option<PathBuf> {
    let code = pipeline::clean_target(lang).0;
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../voices")
        .join(code);
    path.is_dir().then_some(path)
}

fn voice_names(dir: &std::path::Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_str()?.to_string();
            if !name.ends_with(".onnx") || name.ends_with(".onnx.json") {
                return None;
            }
            if name.to_ascii_lowercase().contains("vais1000") {
                return None;
            }
            let json = dir.join(format!("{name}.json"));
            json.is_file()
                .then(|| name.trim_end_matches(".onnx").to_string())
        })
        .collect();
    names.sort();
    names
}

fn voice_check() -> ToolCheck {
    let count = bundled_voice_dir("vi")
        .map(|dir| voice_names(&dir).len())
        .unwrap_or(0);
    if count > 0 {
        ToolCheck {
            id: "voice".into(),
            ready: true,
            detail: format!("Đã có {count} giọng trong voices/vi. App tự đọc, không cần cài thêm."),
        }
    } else {
        ToolCheck {
            id: "voice".into(),
            ready: false,
            detail: "Chưa thấy cặp file giọng (.onnx và .onnx.json) trong folder voices\\vi."
                .into(),
        }
    }
}

fn voice_choice_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    Ok(dir.join("voice.json"))
}

fn resolve_voice(saved: Option<String>, names: &[String]) -> Option<String> {
    saved
        .filter(|name| names.contains(name))
        .or_else(|| names.first().cloned())
}

// voice.json keeps one chosen voice per target language: {"vi": "...", "en": "..."}.
// The old file only had {"name": "..."} for Vietnamese.
fn saved_voices(app: &tauri::AppHandle) -> serde_json::Map<String, serde_json::Value> {
    voice_choice_path(app)
        .ok()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default()
}

fn no_voice_dir(code: &str) -> String {
    format!("Chưa thấy folder voices/{code}.")
}

pub(crate) fn selected_voice(app: &tauri::AppHandle, lang: &str) -> Result<String, String> {
    let code = pipeline::clean_target(lang).0;
    let dir = bundled_voice_dir(code).ok_or_else(|| no_voice_dir(code))?;
    let saved = saved_voices(app);
    let name = saved
        .get(code)
        .or_else(|| {
            if code == "vi" {
                saved.get("name")
            } else {
                None
            }
        })
        .and_then(|name| name.as_str())
        .map(str::to_string);
    resolve_voice(name, &voice_names(&dir))
        .ok_or_else(|| format!("Folder voices/{code} chưa có cặp file giọng."))
}

#[tauri::command]
fn list_voices(lang: String) -> Vec<String> {
    bundled_voice_dir(&lang)
        .map(|dir| voice_names(&dir))
        .unwrap_or_default()
}

#[tauri::command]
fn get_voice(app: tauri::AppHandle, lang: String) -> Result<String, String> {
    selected_voice(&app, &lang)
}

#[tauri::command]
fn save_voice(app: tauri::AppHandle, lang: String, name: String) -> Result<(), String> {
    let code = pipeline::clean_target(&lang).0;
    let dir = bundled_voice_dir(code).ok_or_else(|| no_voice_dir(code))?;
    if !voice_names(&dir).contains(&name) {
        return Err("Không có giọng này.".into());
    }
    let path = voice_choice_path(&app)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let mut saved = saved_voices(&app);
    saved.insert(code.to_string(), serde_json::Value::String(name));
    let text = serde_json::Value::Object(saved).to_string();
    std::fs::write(path, text).map_err(|error| error.to_string())
}

const WHISPER_MODELS: [&str; 5] = [
    "ggml-small.bin",
    "ggml-base.bin",
    "ggml-medium.bin",
    "ggml-large-v3.bin",
    "ggml-tiny.bin",
];

fn bundled_whisper_dir() -> Option<PathBuf> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../whisper");
    path.is_dir().then_some(path)
}

// whisper-cli.exe directly in the folder, or one folder down (the zip may unpack into Release\).
pub(crate) fn whisper_exe_in(dir: &Path) -> Option<PathBuf> {
    let direct = dir.join("whisper-cli.exe");
    if direct.is_file() {
        return Some(direct);
    }
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|entry| entry.path().join("whisper-cli.exe"))
        .find(|path| path.is_file())
}

pub(crate) fn whisper_exe() -> Option<PathBuf> {
    bundled_whisper_dir()
        .and_then(|dir| whisper_exe_in(&dir))
        .or_else(|| program_path("whisper-cli"))
        .or_else(|| program_path("whisper"))
}

fn whisper_model_in(dir: &Path) -> Option<PathBuf> {
    WHISPER_MODELS
        .iter()
        .map(|name| dir.join(name))
        .find(|path| path.is_file())
}

// The model sits next to whisper-cli.exe, or in the project's whisper folder.
pub(crate) fn whisper_model(exe: &Path) -> Option<PathBuf> {
    exe.parent()
        .and_then(whisper_model_in)
        .or_else(|| bundled_whisper_dir().and_then(|dir| whisper_model_in(&dir)))
}

fn whisper_check() -> ToolCheck {
    let legacy = match whisper_exe() {
        None => LegacyWhisper::MissingExe,
        Some(exe) => match whisper_model(&exe) {
            Some(model) => LegacyWhisper::Ready {
                model_name: model
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("ggml")
                    .to_string(),
            },
            None => LegacyWhisper::MissingModel,
        },
    };
    let (ready, detail) = whisper_ready_detail(pipeline::detect_fast_path().is_some(), legacy);
    ToolCheck {
        id: "whisper".into(),
        ready,
        detail,
    }
}

pub(crate) enum LegacyWhisper {
    MissingExe,
    MissingModel,
    Ready { model_name: String },
}

pub(crate) fn whisper_ready_detail(fast: bool, legacy: LegacyWhisper) -> (bool, String) {
    if fast {
        return (
            true,
            "Đã thấy faster-whisper trên GPU và model large-v3-turbo.".into(),
        );
    }
    match legacy {
        LegacyWhisper::MissingExe => (
            false,
            "Chưa thấy whisper-cli.exe. Làm theo các bước bên cạnh.".into(),
        ),
        LegacyWhisper::MissingModel => (
            false,
            "Đã thấy whisper-cli.exe. Còn thiếu file model ggml-small.bin: chép nó vào folder whisper."
                .into(),
        ),
        LegacyWhisper::Ready { model_name } => (
            true,
            format!("Đã thấy whisper-cli và file model {model_name}."),
        ),
    }
}

// Only needed when the user pastes a YouTube link; picking a local file never uses it.
fn ytdlp_check() -> ToolCheck {
    let mut command = Command::new("yt-dlp");
    command.arg("--version");
    match capture(command, Duration::from_secs(6)) {
        Ok(text) => ToolCheck {
            id: "ytdlp".into(),
            ready: true,
            detail: format!("Đã thấy yt-dlp {}.", text.trim()),
        },
        Err(_) => ToolCheck {
            id: "ytdlp".into(),
            ready: false,
            detail: "Chưa thấy yt-dlp. Chỉ cần khi dán link YouTube hoặc TikTok; Douyin dùng Edge, chọn file từ máy thì không cần.".into(),
        },
    }
}

fn piper_check() -> ToolCheck {
    let exe = crate::piper::locate();
    let caps = exe.as_ref().and_then(|path| crate::piper::read_caps(path));
    if crate::piper::choose_engine(exe.clone(), caps) == crate::piper::SpeakEngine::Native {
        return ToolCheck {
            id: "piper".into(),
            ready: true,
            detail: "Đã thấy piper.exe. App đọc cả lô bằng chương trình này.".into(),
        };
    }
    let detail = if exe.is_none() {
        "Chưa có piper.exe. App vẫn đọc giọng trong cửa sổ."
    } else {
        "Bản piper này không đọc được cả lô. App vẫn đọc trong cửa sổ."
    };
    ToolCheck {
        id: "piper".into(),
        ready: false,
        detail: detail.into(),
    }
}

const CHECK_IDS: [&str; 7] = [
    "ffmpeg", "ollama", "model", "voice", "piper", "whisper", "ytdlp",
];

// Mỗi mục có thể mở một chương trình ngoài (ffmpeg, python, yt-dlp, piper). Chạy cùng lúc thì
// tổng thời gian chỉ bằng mục chậm nhất, không cộng dồn. Thứ tự kết quả theo CHECK_IDS.
fn run_checks(choice: &TranslationChoice, skip_translate: bool) -> Vec<ToolCheck> {
    let jobs: Vec<parallel::Job<ToolCheck>> = vec![
        Box::new(ffmpeg_check),
        Box::new(move || ollama_check(choice, skip_translate).0),
        Box::new(move || {
            // Chỉ khi dịch trên máy mới cần biết Ollama nằm ở đâu.
            let program = if choice.provider == "local" && !skip_translate {
                ollama_program()
            } else {
                None
            };
            model_check(program.as_ref(), choice, skip_translate)
        }),
        Box::new(voice_check),
        Box::new(piper_check),
        Box::new(whisper_check),
        Box::new(ytdlp_check),
    ];
    parallel::in_order(jobs, |index| ToolCheck {
        id: CHECK_IDS[index].into(),
        ready: false,
        detail: "Chưa kiểm tra được mục này.".into(),
    })
}

#[tauri::command]
async fn check_setup(app: tauri::AppHandle) -> Vec<ToolCheck> {
    tauri::async_runtime::spawn_blocking(move || {
        let choice = choice_path(&app)
            .ok()
            .map(|path| read_choice(&path))
            .unwrap_or_default();
        let skip_translate = prefs_path(&app)
            .ok()
            .map(|path| read_prefs(&path).skip_translate)
            .unwrap_or(false);
        run_checks(&choice, skip_translate)
    })
    .await
    .unwrap_or_default()
}

#[tauri::command]
fn get_translation_choice(app: tauri::AppHandle) -> TranslationChoice {
    choice_path(&app)
        .ok()
        .map(|path| read_choice(&path))
        .unwrap_or_default()
}

#[tauri::command]
fn save_translation_choice(
    app: tauri::AppHandle,
    provider: String,
    api_key: String,
) -> Result<(), String> {
    let mut choice = normalize_choice(&provider, &api_key)?;
    let path = choice_path(&app)?;
    // Cùng dịch vụ thì giữ model đã chọn; đổi dịch vụ thì quay về model mặc định.
    let before = read_choice(&path).stashed();
    choice.saved = before.saved;
    if let Some(entry) = choice.saved.get(&choice.provider) {
        choice.model = entry.model.clone();
    }
    write_choice(&path, &choice)
}

#[tauri::command]
fn switch_translation_provider(app: tauri::AppHandle, provider: String) -> Result<(), String> {
    let path = choice_path(&app)?;
    let next = read_choice(&path).switched(&provider)?;
    write_choice(&path, &next)
}

#[tauri::command]
fn save_translation_model(app: tauri::AppHandle, model: String) -> Result<(), String> {
    let path = choice_path(&app)?;
    let mut choice = read_choice(&path);
    choice.model = clean_model(&choice.provider, &model)?;
    write_choice(&path, &choice)
}

struct DubControl {
    cancel: AtomicBool,
    running: AtomicBool,
    captioning: AtomicBool,
    snapshot: std::sync::Mutex<DubSnapshot>,
    // File đang đốt chữ, để lệnh xóa không gỡ đúng folder đó.
    caption_path: std::sync::Mutex<String>,
    // Cửa sổ gửi danh sách tên vào đây khi pipeline đang chờ.
    names_reply: std::sync::Mutex<Option<std::sync::mpsc::Sender<names::NameDecision>>>,
}

// Where the current (or last) run stands, so a reloaded window can draw it again.
#[derive(Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DubSnapshot {
    running: bool,
    downloading: bool,
    started_ms: u64,
    video: String,
    output_dir: String,
    download: Option<pipeline::DownloadEvent>,
    steps: Vec<pipeline::StepEvent>,
    result: Option<String>,
    error: Option<String>,
    audio_only: bool,
    names: Option<names::NamesPending>,
    text_only: bool,
}

impl DubControl {
    fn update(&self, change: impl FnOnce(&mut DubSnapshot)) {
        change(
            &mut self
                .snapshot
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
        );
    }

    fn read(&self) -> DubSnapshot {
        self.snapshot
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    fn set_caption_path(&self, path: &str) {
        *self
            .caption_path
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = path.to_string();
    }

    pub(crate) fn park_name_reply(&self, sender: std::sync::mpsc::Sender<names::NameDecision>) {
        *self
            .names_reply
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(sender);
    }

    pub(crate) fn take_name_reply(&self) -> Option<std::sync::mpsc::Sender<names::NameDecision>> {
        self.names_reply
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
    }
}

// Ghi lại folder hoặc file đang xử lý, để lúc xóa danh sách biết việc nào đang chạy.
pub(crate) fn note_live_video(app: &tauri::AppHandle, video: &Path) {
    let control = app.state::<DubControl>();
    if !control.running.load(Ordering::SeqCst) {
        return;
    }
    control.update(|snap| {
        snap.video = video.to_string_lossy().into_owned();
    });
}

fn live_job_name(video: &Path) -> String {
    if video.is_dir() {
        video
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
    } else {
        job::folder_name(video)
    }
}

fn same_dir(left: &Path, right: &Path) -> bool {
    match (std::fs::canonicalize(left), std::fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

// Việc đang chạy thì không xóa. So tên folder, vì folder có thể vừa được tạo.
fn is_live_job(snap: &DubSnapshot, output_dir: &Path, root: &Path) -> bool {
    let video = Path::new(&snap.video);
    if video.as_os_str().is_empty() || snap.output_dir.is_empty() {
        return false;
    }
    if !same_dir(Path::new(&snap.output_dir), output_dir) {
        return false;
    }
    let live = live_job_name(video);
    let name = root
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_default();
    !live.is_empty() && live.eq_ignore_ascii_case(&name)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

// Records how a run ended and tells the window, which may have been reloaded and lost the pending call.
fn finish_dub(app: &tauri::AppHandle, control: &DubControl, outcome: &Result<String, String>) {
    let _ = control.take_name_reply();
    control.update(|snap| {
        snap.running = false;
        snap.downloading = false;
        snap.names = None;
        match outcome {
            Ok(path) => snap.result = Some(path.clone()),
            Err(message) => snap.error = Some(message.clone()),
        }
    });
    let _ = app.emit("dub-finished", control.read());
}

#[tauri::command]
fn dub_status(app: tauri::AppHandle) -> DubSnapshot {
    app.state::<DubControl>().read()
}

#[tauri::command]
async fn start_dub(
    app: tauri::AppHandle,
    video: String,
    output_dir: String,
    target: Option<String>,
    redo: Option<bool>,
    burn: Option<bool>,
    audio_only: Option<bool>,
    skip_translate: Option<bool>,
    text_only: Option<bool>,
) -> Result<String, String> {
    let target = target.unwrap_or_else(|| "vi".into());
    let redo = redo.unwrap_or(false);
    let audio_only = audio_only.unwrap_or(false);
    let text_only = text_only.unwrap_or(false);
    tauri::async_runtime::spawn_blocking(move || {
        let control = app.state::<DubControl>();
        if control.captioning.load(Ordering::SeqCst) {
            return Err("Đang ghi chữ vào video.".into());
        }
        if control.running.swap(true, Ordering::SeqCst) {
            return Err("Đang lồng một video.".into());
        }
        if control.captioning.load(Ordering::SeqCst) {
            control.running.store(false, Ordering::SeqCst);
            return Err("Đang ghi chữ vào video.".into());
        }
        let _ = control.take_name_reply();
        control.cancel.store(false, Ordering::SeqCst);
        control.update(|snap| {
            *snap = DubSnapshot {
                running: true,
                started_ms: now_ms(),
                video: video.clone(),
                output_dir: output_dir.clone(),
                audio_only,
                text_only,
                ..DubSnapshot::default()
            };
        });
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let prefs = prefs_path(&app).ok().map(|path| read_prefs(&path));
            let use_terms = prefs.as_ref().map(|item| item.use_terms).unwrap_or(false);
            let skip_translate = skip_translate.unwrap_or_else(|| {
                prefs
                    .as_ref()
                    .map(|item| item.skip_translate)
                    .unwrap_or(false)
            });
            let burn = burn.unwrap_or_else(|| {
                prefs
                    .as_ref()
                    .map(|item| item.burn_captions)
                    .unwrap_or(false)
            });
            pipeline::run(
                &app,
                Path::new(&video),
                Path::new(&output_dir),
                &target,
                redo,
                use_terms,
                burn && !audio_only,
                audio_only,
                text_only,
                skip_translate,
                &control.cancel,
            )
        }));
        control.running.store(false, Ordering::SeqCst);
        let outcome = match result {
            Ok(value) => value,
            Err(_) => Err("Không chạy được pipeline.".into()),
        };
        finish_dub(&app, &control, &outcome);
        outcome
    })
    .await
    .unwrap_or_else(|_| Err("Không chạy được pipeline.".into()))
}

#[tauri::command]
async fn start_again(
    app: tauri::AppHandle,
    result: String,
    target: String,
    voice: String,
    names: Vec<names::NameFix>,
    retranscribe: Option<bool>,
    burn: Option<bool>,
    audio_only: Option<bool>,
    text_only: Option<bool>,
) -> Result<String, String> {
    let retranscribe = retranscribe.unwrap_or(false);
    let audio_only = audio_only.unwrap_or(false);
    let text_only = text_only.unwrap_or(false);
    tauri::async_runtime::spawn_blocking(move || {
        let control = app.state::<DubControl>();
        if control.captioning.load(Ordering::SeqCst) {
            return Err("Đang ghi chữ vào video.".into());
        }
        if control.running.swap(true, Ordering::SeqCst) {
            return Err("Đang lồng một video.".into());
        }
        if control.captioning.load(Ordering::SeqCst) {
            control.running.store(false, Ordering::SeqCst);
            return Err("Đang ghi chữ vào video.".into());
        }
        let _ = control.take_name_reply();
        control.cancel.store(false, Ordering::SeqCst);
        control.update(|snap| {
            *snap = DubSnapshot {
                running: true,
                started_ms: now_ms(),
                video: result.clone(),
                output_dir: String::new(),
                audio_only,
                text_only,
                ..DubSnapshot::default()
            };
        });
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let prefs = prefs_path(&app).ok().map(|path| read_prefs(&path));
            let use_terms = prefs.as_ref().map(|item| item.use_terms).unwrap_or(false);
            let skip_translate = prefs.as_ref().map(|item| item.skip_translate).unwrap_or(false);
            let burn = burn.unwrap_or_else(|| {
                prefs.as_ref().map(|item| item.burn_captions).unwrap_or(false)
            });
            let file = result_path(&result)?;
            let root = file
                .parent()
                .ok_or("Chưa thấy folder việc.")?
                .to_path_buf();
            let output = root
                .parent()
                .ok_or("Chưa thấy folder kết quả.")?
                .to_path_buf();
            if !voice.trim().is_empty() {
                save_voice(app.clone(), target.clone(), voice)?;
            }
            let (target_code, target_name) = pipeline::clean_target(&target);
            let voice_name = if text_only {
                selected_voice(&app, target_code).unwrap_or_default()
            } else {
                selected_voice(&app, target_code).map_err(|_| {
                    format!(
                        "Chưa có giọng đọc {target_name}. Bỏ cặp file giọng Piper (.onnx và .onnx.json) vào folder voices\\{target_code}."
                    )
                })?
            };
            let mut job = job::Job::reopen(&root)?;
            job.target = target_code.into();
            pipeline::prepare_again(
                &job,
                target_code,
                &voice_name,
                &names,
                retranscribe,
                use_terms,
                skip_translate,
                text_only,
            )?;
            pipeline::run(
                &app,
                &root,
                &output,
                target_code,
                false,
                use_terms,
                burn && !audio_only,
                audio_only,
                text_only,
                skip_translate,
                &control.cancel,
            )
        }));
        control.running.store(false, Ordering::SeqCst);
        let outcome = match result {
            Ok(value) => value,
            Err(_) => Err("Không chạy được pipeline.".into()),
        };
        finish_dub(&app, &control, &outcome);
        outcome
    })
    .await
    .unwrap_or_else(|_| Err("Không chạy được pipeline.".into()))
}

#[tauri::command]
fn list_kept(result: String) -> Result<Vec<job::KeptVersion>, String> {
    let file = result_path(&result)?;
    let root = file.parent().ok_or("Chưa thấy folder việc.")?;
    job::list_kept(root)
}

#[tauri::command]
fn delete_kept_version(output_dir: String, path: String) -> Result<(), String> {
    job::delete_kept(Path::new(&output_dir), Path::new(&path))
}

#[tauri::command]
async fn finished_result(
    app: tauri::AppHandle,
    video: String,
    output_dir: String,
    text_only: Option<bool>,
    target: Option<String>,
    voice: Option<String>,
    audio_only: Option<bool>,
    burn: Option<bool>,
    skip_translate: Option<bool>,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let target = pipeline::clean_target(target.as_deref().unwrap_or("vi")).0;
        let prefs = read_prefs(&prefs_path(&app)?);
        let settings = pipeline::RunSettings {
            target: target.into(),
            voice: voice
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| selected_voice(&app, target).unwrap_or_default()),
            use_terms: prefs.use_terms,
            skip_translate: skip_translate.unwrap_or(prefs.skip_translate),
            text_only: text_only.unwrap_or(false),
        };
        let result = pipeline::matching_result(
            Path::new(&video),
            Path::new(&output_dir),
            &settings,
            audio_only.unwrap_or(false),
            burn.unwrap_or(prefs.burn_captions),
        );
        Ok(result
            .filter(|path| {
                pipeline::media_duration(Path::new(path), &AtomicBool::new(false)).is_ok()
            })
            .unwrap_or_default())
    })
    .await
    .unwrap_or_else(|_| Err("Không kiểm tra được video đã lưu.".into()))
}

#[tauri::command]
async fn start_watch(
    app: tauri::AppHandle,
    video: String,
    output_dir: String,
    target: Option<String>,
    skip_translate: Option<bool>,
    text_only: Option<bool>,
) -> Result<(), String> {
    let target = target.unwrap_or_else(|| "vi".into());
    let text_only = text_only.unwrap_or(false);
    tauri::async_runtime::spawn_blocking(move || {
        let control = app.state::<DubControl>();
        if control.captioning.load(Ordering::SeqCst) {
            return Err("Đang ghi chữ vào video.".into());
        }
        if control.running.swap(true, Ordering::SeqCst) {
            return Err("Đang lồng một video.".into());
        }
        if control.captioning.load(Ordering::SeqCst) {
            control.running.store(false, Ordering::SeqCst);
            return Err("Đang ghi chữ vào video.".into());
        }
        control.cancel.store(false, Ordering::SeqCst);
        control.update(|snap| {
            *snap = DubSnapshot {
                running: true,
                started_ms: now_ms(),
                video: video.clone(),
                output_dir: output_dir.clone(),
                text_only,
                ..DubSnapshot::default()
            };
        });
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let prefs = prefs_path(&app).ok().map(|path| read_prefs(&path));
            let skip_translate = skip_translate.unwrap_or_else(|| {
                prefs
                    .as_ref()
                    .map(|item| item.skip_translate)
                    .unwrap_or(false)
            });
            watch::run_watch(
                &app,
                Path::new(&video),
                Path::new(&output_dir),
                &target,
                skip_translate,
                text_only,
                &control.cancel,
            )
        }));
        control.running.store(false, Ordering::SeqCst);
        let outcome = match result {
            Ok(Ok(())) => Ok(String::new()),
            Ok(Err(message)) => Err(message),
            Err(_) => Err("Không chạy được pipeline.".into()),
        };
        finish_dub(&app, &control, &outcome);
        outcome.map(|_| ())
    })
    .await
    .unwrap_or_else(|_| Err("Không chạy được pipeline.".into()))
}

// Default folder for results: `output` at the project root, made on first ask.
#[tauri::command]
fn default_output_dir() -> Result<String, String> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../output");
    std::fs::create_dir_all(&path).map_err(|error| error.to_string())?;
    let path = path.canonicalize().map_err(|error| error.to_string())?;
    Ok(path
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_string())
}

#[tauri::command]
async fn link_title(url: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || pipeline::link_title(&url))
        .await
        .unwrap_or_else(|_| Err("Không lấy được tên video.".into()))
}

#[tauri::command]
async fn download_video(
    app: tauri::AppHandle,
    url: String,
    output_dir: String,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let control = app.state::<DubControl>();
        if control.captioning.load(Ordering::SeqCst) {
            return Err("Đang ghi chữ vào video.".into());
        }
        if control.running.swap(true, Ordering::SeqCst) {
            return Err("Đang lồng một video.".into());
        }
        if control.captioning.load(Ordering::SeqCst) {
            control.running.store(false, Ordering::SeqCst);
            return Err("Đang ghi chữ vào video.".into());
        }
        control.cancel.store(false, Ordering::SeqCst);
        control.update(|snap| {
            *snap = DubSnapshot {
                running: true,
                downloading: true,
                started_ms: now_ms(),
                output_dir: output_dir.clone(),
                ..DubSnapshot::default()
            };
        });
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pipeline::download(&app, &url, Path::new(&output_dir), &control.cancel)
        }));
        control.running.store(false, Ordering::SeqCst);
        let outcome = match result {
            Ok(value) => value,
            Err(_) => Err("Không tải được video.".into()),
        };
        // A finished download hands over to start_dub, so only a failure ends the run for the window.
        match &outcome {
            Ok(path) => control.update(|snap| {
                snap.running = false;
                snap.downloading = false;
                snap.video = path.clone();
            }),
            Err(_) => finish_dub(&app, &control, &outcome),
        }
        outcome
    })
    .await
    .unwrap_or_else(|_| Err("Không tải được video.".into()))
}

// Chỉ audio: lưu link, lấy phụ đề hoặc tiếng. Trả folder việc, chưa có file hình.
#[tauri::command]
async fn prepare_listen(
    app: tauri::AppHandle,
    url: String,
    output_dir: String,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let control = app.state::<DubControl>();
        if control.captioning.load(Ordering::SeqCst) {
            return Err("Đang ghi chữ vào video.".into());
        }
        if control.running.swap(true, Ordering::SeqCst) {
            return Err("Đang lồng một video.".into());
        }
        if control.captioning.load(Ordering::SeqCst) {
            control.running.store(false, Ordering::SeqCst);
            return Err("Đang ghi chữ vào video.".into());
        }
        control.cancel.store(false, Ordering::SeqCst);
        control.update(|snap| {
            *snap = DubSnapshot {
                running: true,
                downloading: true,
                started_ms: now_ms(),
                output_dir: output_dir.clone(),
                audio_only: true,
                ..DubSnapshot::default()
            };
        });
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pipeline::prepare_listen(&app, &url, Path::new(&output_dir), &control.cancel)
        }));
        control.running.store(false, Ordering::SeqCst);
        let outcome = match result {
            Ok(value) => value,
            Err(_) => Err("Không lấy được phụ đề hoặc tiếng.".into()),
        };
        match &outcome {
            Ok(path) => control.update(|snap| {
                snap.running = false;
                snap.downloading = false;
                snap.video = path.clone();
                snap.audio_only = true;
            }),
            Err(_) => finish_dub(&app, &control, &outcome),
        }
        outcome
    })
    .await
    .unwrap_or_else(|_| Err("Không lấy được phụ đề hoặc tiếng.".into()))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ListenPartPayload {
    index: usize,
    count: usize,
    words: usize,
    source: String,
    text: String,
}

fn preview_dir(root: &Path, target: &str) -> PathBuf {
    // Cache cũ không ghi ngôn ngữ nên không thể dùng lại một cách chắc chắn.
    root.join("preview-parts")
        .join(pipeline::clean_target(target).0)
}

fn listen_root(root: &str) -> Result<PathBuf, String> {
    let root = PathBuf::from(root.trim());
    if !root.is_dir() || !root.join("source.json").is_file() {
        return Err("Chưa thấy việc đã lưu.".into());
    }
    Ok(root)
}

fn listen_outline(root: &Path, part: usize, target: &str) -> Result<ListenPartPayload, String> {
    let cues: Vec<job::Cue> = job::read_json(&root.join("transcript.json")).unwrap_or_default();
    let parts = listen::pack_listen_parts(&cues);
    if parts.is_empty() {
        return Ok(ListenPartPayload {
            index: 1,
            count: 0,
            words: 0,
            source: String::new(),
            text: String::new(),
        });
    }
    let index = part.clamp(1, parts.len());
    let source = parts[index - 1].clone();
    let text = listen::cached_part(&preview_dir(root, target), index, &source).unwrap_or_default();
    Ok(ListenPartPayload {
        index,
        count: parts.len(),
        words: source.split_whitespace().count(),
        source,
        text,
    })
}

// Phụ đề đã có: trả một phần khoảng 1.000 từ để dịch và nghe, không tải video.
#[tauri::command]
fn listen_part(
    root: String,
    part: Option<usize>,
    target: Option<String>,
) -> Result<ListenPartPayload, String> {
    listen_outline(
        &listen_root(&root)?,
        part.unwrap_or(1),
        target.as_deref().unwrap_or("vi"),
    )
}

// Dịch đúng phần đang chọn. Bản đã dịch thì trả lại, không gọi lại.
#[tauri::command]
async fn translate_listen_part(
    app: tauri::AppHandle,
    root: String,
    part: usize,
    target: String,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = listen_root(&root)?;
        let outline = listen_outline(&root, part, &target)?;
        if outline.count == 0 {
            return Err("Chưa có phụ đề để dịch.".into());
        }
        if !outline.text.is_empty() {
            return Ok(outline.text);
        }
        let choice = choice_path(&app)
            .ok()
            .map(|path| read_choice(&path))
            .unwrap_or_default();
        ollama::require_deepseek_local(&choice, false)?;
        let (_, target_name) = pipeline::clean_target(&target);
        let prev = if outline.index > 1 {
            listen_outline(&root, outline.index - 1, &target)?.text
        } else {
            String::new()
        };
        let prompt = ollama::prose_prompt(&outline.source, target_name, &prev);
        let cancel = AtomicBool::new(false);
        let text = ollama::complete_plain(&choice, &prompt, &cancel)?;
        listen::store_part(
            &preview_dir(&root, &target),
            outline.index,
            &outline.source,
            &text,
        )?;
        Ok(text)
    })
    .await
    .unwrap_or_else(|_| Err("Không dịch được phần này.".into()))
}

// Tải file hình của link đã lưu trong việc chỉ audio. Trả folder việc để ghép tiếp.
#[tauri::command]
async fn download_saved_video(
    app: tauri::AppHandle,
    result: String,
    output_dir: String,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let control = app.state::<DubControl>();
        if control.captioning.load(Ordering::SeqCst) {
            return Err("Đang ghi chữ vào video.".into());
        }
        if control.running.swap(true, Ordering::SeqCst) {
            return Err("Đang lồng một video.".into());
        }
        if control.captioning.load(Ordering::SeqCst) {
            control.running.store(false, Ordering::SeqCst);
            return Err("Đang ghi chữ vào video.".into());
        }
        control.cancel.store(false, Ordering::SeqCst);
        control.update(|snap| {
            *snap = DubSnapshot {
                running: true,
                downloading: true,
                started_ms: now_ms(),
                output_dir: output_dir.clone(),
                ..DubSnapshot::default()
            };
        });
        let result_path = PathBuf::from(&result);
        let fetched = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pipeline::attach_video(&app, &result_path, Path::new(&output_dir), &control.cancel)
        }));
        control.running.store(false, Ordering::SeqCst);
        let outcome = match fetched {
            Ok(value) => value,
            Err(_) => Err("Không tải được video.".into()),
        };
        match &outcome {
            Ok(path) => control.update(|snap| {
                snap.running = false;
                snap.downloading = false;
                snap.video = path.clone();
            }),
            Err(_) => finish_dub(&app, &control, &outcome),
        }
        outcome
    })
    .await
    .unwrap_or_else(|_| Err("Không tải được video.".into()))
}

// Chỉ nhận file kết quả (mp4, srt) hoặc folder đã có, để lệnh này không mở được thứ khác.
fn result_path(path: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(path);
    let ext = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let allowed =
        path.is_dir() || (path.is_file() && matches!(ext.as_str(), "mp4" | "m4a" | "srt"));
    if !allowed {
        return Err("Chưa thấy file kết quả.".into());
    }
    Ok(path)
}

// Cho cửa sổ app phát video kết quả ngay trong giao diện (giao thức asset chỉ mở đúng file này).
#[tauri::command]
fn allow_result(app: tauri::AppHandle, path: String) -> Result<(), String> {
    let path = result_path(&path)?;
    app.asset_protocol_scope()
        .allow_file(&path)
        .map_err(|error| error.to_string())
}

// Video đã ghép: nối các câu đã đọc thành một mạch, không đụng file hình.
#[tauri::command]
async fn join_listen(app: tauri::AppHandle, path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let control = app.state::<DubControl>();
        if control.captioning.load(Ordering::SeqCst) {
            return Err("Đang ghi chữ vào video.".into());
        }
        if control.running.swap(true, Ordering::SeqCst) {
            return Err("Đang lồng một video.".into());
        }
        if control.captioning.load(Ordering::SeqCst) {
            control.running.store(false, Ordering::SeqCst);
            return Err("Đang ghi chữ vào video.".into());
        }
        control.cancel.store(false, Ordering::SeqCst);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let file = result_path(&path)?;
            pipeline::join_listen(&app, &file, &control.cancel)
        }));
        control.running.store(false, Ordering::SeqCst);
        match result {
            Ok(value) => value,
            Err(_) => Err("Không nối được các câu.".into()),
        }
    })
    .await
    .unwrap_or_else(|_| Err("Không nối được các câu.".into()))
}

// Ghi audio bản dịch ra đường dẫn người dùng vừa chọn trong hộp thoại lưu.
#[tauri::command]
async fn save_dub_audio(path: String, dest: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let source = result_path(&path)?;
        if !source.is_file() {
            return Err("Chưa thấy file kết quả.".into());
        }
        let mut dest = PathBuf::from(dest.trim());
        if dest.as_os_str().is_empty() {
            return Err("Chưa chọn chỗ lưu audio.".into());
        }
        if !dest
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .eq_ignore_ascii_case("m4a")
        {
            dest.set_extension("m4a");
        }
        pipeline::save_spoken(&source, &dest)
    })
    .await
    .unwrap_or_else(|_| Err("Không lưu được audio.".into()))
}

// Ghi video kết quả ra đường dẫn người dùng vừa chọn trong hộp thoại lưu.
#[tauri::command]
async fn save_dub_video(path: String, dest: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        pipeline::save_video(Path::new(&path), Path::new(dest.trim()))
    })
    .await
    .unwrap_or_else(|_| Err("Không lưu được video.".into()))
}

// Mở file kết quả bằng chương trình mặc định của Windows, hoặc mở folder.
#[tauri::command]
fn open_result(app: tauri::AppHandle, path: String) -> Result<(), String> {
    let path = result_path(&path)?;
    app.opener()
        .open_path(path.to_string_lossy().into_owned(), None::<&str>)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn delete_dub(app: tauri::AppHandle, output_dir: String, path: String) -> Result<(), String> {
    let control = app.state::<DubControl>();
    let output = PathBuf::from(&output_dir);
    let item = PathBuf::from(&path);
    let root = job::job_folder(&output, &item)?;
    if control.running.load(Ordering::SeqCst) && is_live_job(&control.read(), &output, &root) {
        return Err("Việc này đang chạy.".into());
    }
    if control.captioning.load(Ordering::SeqCst) {
        let caption = control
            .caption_path
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        if !caption.is_empty() {
            if let Ok(caption_root) = job::job_folder(&output, Path::new(&caption)) {
                if caption_root == root {
                    return Err("Đang ghi chữ vào video này.".into());
                }
            }
        }
    }
    job::delete_job(&output, &item)
}

#[tauri::command]
fn list_dubs(app: tauri::AppHandle, output_dir: String) -> Result<Vec<job::FinishedDub>, String> {
    let dir = Path::new(&output_dir);
    let found = job::list_finished(dir)?;
    if !app.state::<DubControl>().running.load(Ordering::SeqCst) {
        pipeline::reclaim_output(dir);
    }
    Ok(found)
}

// Sửa vài câu của video hoặc audio đã xong. Câu không chọn giữ nguyên tiếng.
#[tauri::command]
async fn revise_lines(
    app: tauri::AppHandle,
    path: String,
    edits: Vec<pipeline::LinePatch>,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let control = app.state::<DubControl>();
        if control.captioning.load(Ordering::SeqCst) {
            return Err("Đang ghi chữ vào video.".into());
        }
        if control.running.swap(true, Ordering::SeqCst) {
            return Err("Đang lồng một video.".into());
        }
        if control.captioning.load(Ordering::SeqCst) {
            control.running.store(false, Ordering::SeqCst);
            return Err("Đang ghi chữ vào video.".into());
        }
        let _ = control.take_name_reply();
        control.cancel.store(false, Ordering::SeqCst);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let file = result_path(&path)?;
            let root = file.parent().ok_or("Chưa thấy folder việc.")?.to_path_buf();
            let output = root
                .parent()
                .map(|dir| dir.to_string_lossy().into_owned())
                .unwrap_or_default();
            let audio_only = file
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or("")
                .eq_ignore_ascii_case("m4a");
            control.update(|snap| {
                *snap = DubSnapshot {
                    running: true,
                    started_ms: now_ms(),
                    video: root.to_string_lossy().into_owned(),
                    output_dir: output,
                    audio_only,
                    ..DubSnapshot::default()
                };
            });
            pipeline::revise(&app, &file, &edits, &control.cancel)
        }));
        control.running.store(false, Ordering::SeqCst);
        let outcome = match result {
            Ok(value) => value,
            Err(_) => Err("Không sửa được bản này.".into()),
        };
        finish_dub(&app, &control, &outcome);
        outcome
    })
    .await
    .unwrap_or_else(|_| Err("Không sửa được bản này.".into()))
}

#[tauri::command]
fn name_retake_count(path: String) -> Result<usize, String> {
    let path = result_path(&path)?;
    Ok(pipeline::saved_name_edits(&path)?.len())
}

// Đọc lại những câu mà danh sách tên đổi chữ. Câu không đổi giữ tiếng cũ.
#[tauri::command]
async fn retake_saved_names(app: tauri::AppHandle, path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let control = app.state::<DubControl>();
        if control.captioning.load(Ordering::SeqCst) {
            return Err("Đang ghi chữ vào video.".into());
        }
        if control.running.swap(true, Ordering::SeqCst) {
            return Err("Đang lồng một video.".into());
        }
        if control.captioning.load(Ordering::SeqCst) {
            control.running.store(false, Ordering::SeqCst);
            return Err("Đang ghi chữ vào video.".into());
        }
        let _ = control.take_name_reply();
        control.cancel.store(false, Ordering::SeqCst);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let file = result_path(&path)?;
            let root = file.parent().ok_or("Chưa thấy folder việc.")?.to_path_buf();
            let output = root
                .parent()
                .map(|dir| dir.to_string_lossy().into_owned())
                .unwrap_or_default();
            let audio_only = file
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or("")
                .eq_ignore_ascii_case("m4a");
            control.update(|snap| {
                *snap = DubSnapshot {
                    running: true,
                    started_ms: now_ms(),
                    video: root.to_string_lossy().into_owned(),
                    output_dir: output,
                    audio_only,
                    ..DubSnapshot::default()
                };
            });
            pipeline::retake_saved_names(&app, &file, &control.cancel)
        }));
        control.running.store(false, Ordering::SeqCst);
        let outcome = match result {
            Ok(value) => value,
            Err(_) => Err("Không sửa được bản này.".into()),
        };
        finish_dub(&app, &control, &outcome);
        outcome
    })
    .await
    .unwrap_or_else(|_| Err("Không sửa được bản này.".into()))
}

#[tauri::command]
fn dub_lines(path: String) -> Result<Vec<job::TranslatedLine>, String> {
    let path = result_path(&path)?;
    if !path.is_file() {
        return Err("Chưa thấy file kết quả.".into());
    }
    pipeline::script_for(&path)
}

#[tauri::command]
fn cancel_dub(app: tauri::AppHandle) {
    app.state::<DubControl>()
        .cancel
        .store(true, Ordering::SeqCst);
    let _ = app.emit("tts-cancel", ());
}

#[tauri::command]
fn confirm_names(
    app: tauri::AppHandle,
    keep: bool,
    names: Vec<names::NameFix>,
) -> Result<(), String> {
    let Some(sender) = app.state::<DubControl>().take_name_reply() else {
        return Err("Không đang chờ tên.".into());
    };
    sender
        .send(names::NameDecision { keep, names })
        .map_err(|_| "Lượt này đã dừng.".to_string())
}

// Vài con số và câu đầu của một việc dở, để thẻ việc dở cho người dùng nhớ đó là video nào.
#[tauri::command]
fn draft_brief(path: String) -> Result<job::DraftBrief, String> {
    job::draft_brief(Path::new(&path))
}

#[tauri::command]
fn read_names(root: String) -> Result<names::NameList, String> {
    let root = PathBuf::from(&root);
    if !root.join("source.json").is_file() && !root.join("lines.json").is_file() {
        return Err("Chưa thấy việc này.".into());
    }
    Ok(job::read_json(&root.join("names.json")).unwrap_or_default())
}

#[tauri::command]
fn save_names(root: String, names: Vec<names::NameFix>) -> Result<bool, String> {
    let job = job::Job::reopen(Path::new(&root))?;
    let before: names::NameList = job::read_json(&job.names_path()).unwrap_or_default();
    let cleaned = names::usable_names(&names);
    let changed = names::name_signature(&before.names) != names::name_signature(&cleaned);
    job::write_json(
        &job.names_path(),
        &names::NameList {
            confirmed: true,
            names: cleaned,
        },
    )?;
    Ok(changed)
}

// Đốt chữ lên video đã lồng. Không dịch lại, không đọc lại giọng.
#[tauri::command]
async fn burn_captions(app: tauri::AppHandle, path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let control = app.state::<DubControl>();
        if control.running.load(Ordering::SeqCst) {
            return Err("Đang lồng một video.".into());
        }
        if control.captioning.swap(true, Ordering::SeqCst) {
            return Err("Đang ghi chữ vào video.".into());
        }
        if control.running.load(Ordering::SeqCst) {
            control.captioning.store(false, Ordering::SeqCst);
            return Err("Đang lồng một video.".into());
        }
        control.cancel.store(false, Ordering::SeqCst);
        control.set_caption_path(&path);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pipeline::burn_saved(&app, Path::new(&path), &control.cancel)
        }));
        control.set_caption_path("");
        control.captioning.store(false, Ordering::SeqCst);
        match result {
            Ok(value) => value,
            Err(_) => Err("Không ghi được chữ vào video.".into()),
        }
    })
    .await
    .unwrap_or_else(|_| Err("Không ghi được chữ vào video.".into()))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|_app| Ok(()))
        .manage(DubControl {
            cancel: AtomicBool::new(false),
            running: AtomicBool::new(false),
            captioning: AtomicBool::new(false),
            snapshot: Default::default(),
            caption_path: std::sync::Mutex::new(String::new()),
            names_reply: std::sync::Mutex::new(None),
        })
        .manage(tts::Bridge::default())
        .manage(whisper_install::Control::default())
        .invoke_handler(tauri::generate_handler![
            check_setup,
            list_local_models,
            get_translation_choice,
            save_translation_choice,
            save_translation_model,
            switch_translation_provider,
            start_dub,
            start_again,
            start_watch,
            confirm_names,
            read_names,
            save_names,
            link_title,
            download_video,
            prepare_listen,
            listen_part,
            translate_listen_part,
            download_saved_video,
            cancel_dub,
            burn_captions,
            dub_status,
            allow_result,
            open_result,
            join_listen,
            save_dub_audio,
            save_dub_video,
            list_dubs,
            list_kept,
            draft_brief,
            delete_kept_version,
            finished_result,
            delete_dub,
            dub_lines,
            name_retake_count,
            retake_saved_names,
            revise_lines,
            default_output_dir,
            get_prefs,
            save_prefs,
            list_voices,
            get_voice,
            save_voice,
            tts::preview_voice,
            tts::read_voice,
            tts::save_line_wav,
            tts::tts_finish,
            tts::tts_prepared,
            whisper_install::install_whisper,
            whisper_install::cancel_whisper_install,
            deepseek_server::deepseek_server_running,
            deepseek_server::start_deepseek_server,
            deepseek_server::stop_deepseek_server
        ])
        .manage(deepseek_server::Control::default())
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                deepseek_server::stop_owned(&app.state::<deepseek_server::Control>());
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn listen_cache_never_reuses_another_language_or_an_unlabelled_legacy_part() {
        let root = scratch("listen-language");
        job::write_json(
            &root.join("transcript.json"),
            &vec![job::Cue {
                start: 0.0,
                end: 1.0,
                text: "Hello.".into(),
            }],
        )
        .unwrap();
        listen::store_part(&root.join("preview-parts"), 1, "Hello.", "legacy").unwrap();
        assert!(listen_outline(&root, 1, "vi").unwrap().text.is_empty());
        listen::store_part(&preview_dir(&root, "vi"), 1, "Hello.", "Xin chào.").unwrap();
        assert_eq!(listen_outline(&root, 1, "vi").unwrap().text, "Xin chào.");
        assert!(listen_outline(&root, 1, "en").unwrap().text.is_empty());
        listen::store_part(&preview_dir(&root, "en"), 1, "Hello.", "Hello!").unwrap();
        assert_eq!(listen_outline(&root, 1, "en").unwrap().text, "Hello!");
        assert_eq!(listen_outline(&root, 1, "vi").unwrap().text, "Xin chào.");
        fs::remove_dir_all(root).unwrap();
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("voice-pair-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn switching_provider_keeps_each_key_and_model() {
        let gemini = normalize_choice("gemini", "g-key").unwrap();
        let gemini = TranslationChoice {
            model: "gemini-2.5-flash".into(),
            ..gemini
        };
        let claude = gemini.clone().switched("claude");
        assert!(claude.is_err(), "claude has no key yet");

        let mut start = gemini.clone().stashed();
        start.saved.insert(
            "claude".into(),
            SavedProvider {
                api_key: "c-key".into(),
                model: String::new(),
            },
        );
        let on_claude = start.switched("claude").unwrap();
        assert_eq!(
            (on_claude.provider.as_str(), on_claude.api_key.as_str()),
            ("claude", "c-key")
        );

        let back = on_claude.switched("gemini").unwrap();
        assert_eq!(
            (back.api_key.as_str(), back.model.as_str()),
            ("g-key", "gemini-2.5-flash")
        );

        let local = back.switched("local").unwrap();
        assert_eq!(local.provider, "local");
        assert_eq!(local.saved.len(), 2);

        // DeepSeek API cần key như Gemini hay Claude. DeepSeek local thì không.
        assert!(local.clone().switched("deepseek").is_err());
        let on_deepseek_local = local.switched("deepseek-local").unwrap();
        assert_eq!(on_deepseek_local.provider, "deepseek-local");
        assert!(on_deepseek_local.api_key.is_empty());
        let back_local = on_deepseek_local.switched("local").unwrap();
        assert_eq!(back_local.provider, "local");
        assert!(back_local.saved.contains_key("deepseek-local"));
        assert!(!back_local.saved.contains_key("deepseek"));
    }

    #[test]
    fn deepseek_api_and_local_keep_separate_keys_and_models() {
        let api = TranslationChoice {
            model: "deepseek-v4-pro".into(),
            ..normalize_choice("deepseek", "sk-ds").unwrap()
        };
        let on_local = api.switched("deepseek-local").unwrap();
        assert_eq!(on_local.provider, "deepseek-local");
        assert!(on_local.api_key.is_empty());
        assert!(on_local.model.is_empty());

        let on_local = TranslationChoice {
            model: "deepseek-expert".into(),
            ..on_local
        };
        let back_api = on_local.switched("deepseek").unwrap();
        assert_eq!(back_api.provider, "deepseek");
        assert_eq!(
            (back_api.api_key.as_str(), back_api.model.as_str()),
            ("sk-ds", "deepseek-v4-pro")
        );

        let back_local = back_api.switched("deepseek-local").unwrap();
        assert_eq!(back_local.model, "deepseek-expert");
        assert!(back_local.api_key.is_empty());
    }

    #[test]
    fn choice_saved_by_the_local_only_build_moves_to_deepseek_local() {
        let dir = scratch("choice-migrate");
        let path = dir.join("translation.json");
        // Bản chỉ có DeepSeek trên máy lưu "deepseek" không key, kèm key Gemini đã lưu.
        fs::write(
            &path,
            br#"{"provider":"deepseek","apiKey":"","model":"deepseek-expert","saved":{"gemini":{"apiKey":"g-key","model":""},"deepseek":{"apiKey":"","model":"deepseek-expert"}}}"#,
        )
        .unwrap();
        let choice = read_choice(&path);
        assert_eq!(choice.provider, "deepseek-local");
        assert_eq!(choice.model, "deepseek-expert");
        assert_eq!(choice.saved["gemini"].api_key, "g-key");
        assert!(!choice.saved.contains_key("deepseek"));
        assert_eq!(choice.saved["deepseek-local"].model, "deepseek-expert");

        // Có key thì vẫn là DeepSeek API.
        fs::write(
            &path,
            br#"{"provider":"deepseek","apiKey":"sk-ds","model":"deepseek-flash","saved":{"deepseek-local":{"apiKey":"","model":"deepseek-chat"}}}"#,
        )
        .unwrap();
        let choice = read_choice(&path);
        assert_eq!(
            (choice.provider.as_str(), choice.api_key.as_str()),
            ("deepseek", "sk-ds")
        );
        assert_eq!(choice.model, "deepseek-flash");
        assert_eq!(choice.saved["deepseek-local"].model, "deepseek-chat");
    }

    #[test]
    fn tags_list_keeps_model_names() {
        let body = serde_json::json!({
            "models": [
                {"name": "gemma3:12b"},
                {"name": "  "},
                {"name": "qwen3.5:9b-q4_K_M"}
            ]
        });
        assert_eq!(
            tags_model_names(&body),
            vec!["gemma3:12b".to_string(), "qwen3.5:9b-q4_K_M".to_string()]
        );
        assert!(tags_model_names(&serde_json::json!({"models": []})).is_empty());
    }

    #[test]
    fn setup_check_names_only_the_three_suggested_models() {
        let installed = vec![
            "llama3.1:8b".to_string(),
            "qwen3.5:9b-q4_K_M".to_string(),
            "mistral:7b".to_string(),
        ];
        assert_eq!(
            suggested_models_present(&installed),
            vec!["qwen3.5:9b-q4_K_M"]
        );
        assert!(suggested_models_present(&["phi4:14b".to_string()]).is_empty());
    }

    #[test]
    fn model_name_is_checked_before_it_is_saved() {
        assert_eq!(
            clean_model("gemini", " gemini-2.5-flash ").unwrap(),
            "gemini-2.5-flash"
        );
        assert_eq!(
            clean_model("local", "qwen3.5:9b-q4_K_M").unwrap(),
            "qwen3.5:9b-q4_K_M"
        );
        assert!(clean_model("local", "hf.co/a/b").is_ok());
        assert!(clean_model("gemini", "a/b").is_err());
        assert!(clean_model("openai", "gpt\"x").is_err());
        assert_eq!(clean_model("claude", "").unwrap(), "");
        // Mỗi bên DeepSeek giữ đúng tên model của mình, không đổi qua lại.
        assert_eq!(
            clean_model("deepseek", "deepseek-flash").unwrap(),
            "deepseek-flash"
        );
        assert_eq!(
            clean_model("deepseek-local", "deepseek-expert").unwrap(),
            "deepseek-expert"
        );
        assert!(clean_model("deepseek", "a/b").is_err());
        assert!(clean_model("deepseek-local", "a/b").is_err());
    }

    #[test]
    fn voice_uses_local_pair_not_vais() {
        let dir = scratch("voice");
        fs::write(dir.join("vi_VN-vais1000-medium.onnx"), b"x").unwrap();
        fs::write(dir.join("vi_VN-vais1000-medium.onnx.json"), b"{}").unwrap();
        fs::write(dir.join("lonely.onnx"), b"x").unwrap();
        fs::write(dir.join("calmwoman3688.onnx"), b"x").unwrap();
        fs::write(dir.join("calmwoman3688.onnx.json"), b"{}").unwrap();
        assert_eq!(voice_names(&dir), vec!["calmwoman3688".to_string()]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn whisper_is_found_directly_or_one_folder_down() {
        let dir = scratch("whisper");
        assert!(whisper_exe_in(&dir).is_none());
        fs::create_dir_all(dir.join("Release")).unwrap();
        fs::write(dir.join("Release").join("whisper-cli.exe"), b"x").unwrap();
        assert_eq!(
            whisper_exe_in(&dir),
            Some(dir.join("Release").join("whisper-cli.exe"))
        );
        fs::write(dir.join("whisper-cli.exe"), b"x").unwrap();
        assert_eq!(whisper_exe_in(&dir), Some(dir.join("whisper-cli.exe")));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn whisper_model_prefers_small_over_tiny() {
        let dir = scratch("whisper-model");
        assert!(whisper_model_in(&dir).is_none());
        fs::write(dir.join("ggml-tiny.bin"), b"x").unwrap();
        assert_eq!(whisper_model_in(&dir), Some(dir.join("ggml-tiny.bin")));
        fs::write(dir.join("ggml-small.bin"), b"x").unwrap();
        assert_eq!(whisper_model_in(&dir), Some(dir.join("ggml-small.bin")));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn gpu_path_counts_as_ready_without_whisper_cli() {
        let (ready, detail) = whisper_ready_detail(true, LegacyWhisper::MissingExe);
        assert!(ready);
        assert_eq!(
            detail,
            "Đã thấy faster-whisper trên GPU và model large-v3-turbo."
        );
    }

    #[test]
    fn old_whisper_messages_stay_when_the_gpu_path_is_absent() {
        let (ready, detail) = whisper_ready_detail(false, LegacyWhisper::MissingExe);
        assert!(!ready);
        assert_eq!(
            detail,
            "Chưa thấy whisper-cli.exe. Làm theo các bước bên cạnh."
        );
        let (ready, detail) = whisper_ready_detail(
            false,
            LegacyWhisper::Ready {
                model_name: "ggml-small.bin".into(),
            },
        );
        assert!(ready);
        assert_eq!(detail, "Đã thấy whisper-cli và file model ggml-small.bin.");
    }

    #[test]
    fn saved_voice_wins_only_if_it_still_exists() {
        let names = vec!["a".to_string(), "b".to_string()];
        assert_eq!(
            resolve_voice(Some("b".into()), &names).as_deref(),
            Some("b")
        );
        assert_eq!(
            resolve_voice(Some("gone".into()), &names).as_deref(),
            Some("a")
        );
        assert_eq!(resolve_voice(None, &names).as_deref(), Some("a"));
        assert_eq!(resolve_voice(None, &[]), None);
    }

    #[test]
    fn cloud_choice_needs_a_key() {
        assert!(normalize_choice("gemini", "  ").is_err());
        let choice = normalize_choice("openai", " sk-test ").unwrap();
        assert_eq!(choice.provider, "openai");
        assert_eq!(choice.api_key, "sk-test");
        assert!(normalize_choice("local", "secret")
            .unwrap()
            .api_key
            .is_empty());
        let ready = model_check(None, &normalize_choice("claude", "key").unwrap(), false);
        assert!(ready.ready);
        // DeepSeek API cần key như các dịch vụ qua mạng khác.
        assert!(normalize_choice("deepseek", "  ").is_err());
        let deepseek = normalize_choice("deepseek", " sk-ds ").unwrap();
        assert_eq!(deepseek.provider, "deepseek");
        assert_eq!(deepseek.api_key, "sk-ds");
        let api = model_check(None, &deepseek, false);
        assert!(api.ready);
        assert!(api.detail.contains("DeepSeek"));
        // DeepSeek local không có key, chỉ cần server đang chạy.
        let local = normalize_choice("deepseek-local", "secret").unwrap();
        assert_eq!(local.provider, "deepseek-local");
        assert!(local.api_key.is_empty());
        let up = deepseek_local_status(true);
        assert!(up.ready);
        assert!(up.detail.contains("127.0.0.1:8000"));
        let down = deepseek_local_status(false);
        assert!(!down.ready);
        assert!(down.detail.contains("Bật DeepSeek"));
        assert!(!model_check(None, &TranslationChoice::default(), false).ready);
        assert!(model_check(None, &TranslationChoice::default(), true).ready);
    }

    // Mở chương trình thật trên máy (where.exe, ffmpeg, python, yt-dlp), nên không chạy mặc định:
    // cargo test --lib -- --ignored run_checks_on_this_machine --nocapture
    #[test]
    #[ignore]
    fn run_checks_on_this_machine() {
        let choice = TranslationChoice::default();
        let started = Instant::now();
        let one_by_one = vec![
            ffmpeg_check(),
            ollama_check(&choice, false).0,
            model_check(ollama_program().as_ref(), &choice, false),
            voice_check(),
            piper_check(),
            whisper_check(),
            ytdlp_check(),
        ];
        let sequential = started.elapsed();
        let started = Instant::now();
        let together = run_checks(&choice, false);
        eprintln!("lần lượt {sequential:?}, song song {:?}", started.elapsed());
        let ids: Vec<&str> = together.iter().map(|check| check.id.as_str()).collect();
        assert_eq!(ids, CHECK_IDS);
        let same: Vec<(&str, bool)> = one_by_one
            .iter()
            .map(|c| (c.id.as_str(), c.ready))
            .collect();
        let now: Vec<(&str, bool)> = together.iter().map(|c| (c.id.as_str(), c.ready)).collect();
        assert_eq!(same, now, "song song phải cho cùng kết quả với lần lượt");
    }

    #[test]
    fn save_names_keeps_the_translation() {
        let dir = scratch("save-names");
        let root = dir.join("tap");
        fs::create_dir_all(root.join("lines")).unwrap();
        fs::write(root.join("source.json"), br#"{"video":"x"}"#).unwrap();
        fs::write(
            root.join("translation.json"),
            br#"[{"start":0,"end":1,"source":"Ann","translation":"An"}]"#,
        )
        .unwrap();
        let changed = save_names(
            root.to_string_lossy().into_owned(),
            vec![names::NameFix {
                from: "Ann".into(),
                to: "Anne".into(),
            }],
        )
        .unwrap();
        assert!(changed);
        let text = fs::read_to_string(root.join("translation.json")).unwrap();
        assert!(
            text.contains("\"translation\": \"An\"") || text.contains("\"translation\":\"An\"")
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
