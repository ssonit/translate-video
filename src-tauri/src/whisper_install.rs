// One button in the setup screen: check the newest whisper.cpp release and the model, then
// download only what is missing or older, verify the SHA-256 the publisher lists, and unpack
// into the project's whisper folder. Nothing else is ever downloaded.
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager};

const RELEASES_URL: &str = "https://api.github.com/repos/ggml-org/whisper.cpp/releases?per_page=30";
const PROGRAM_ASSET: &str = "whisper-bin-x64.zip";
const MODEL_FILE: &str = "ggml-small.bin";
const MODEL_TREE_URL: &str = "https://huggingface.co/api/models/ggerganov/whisper.cpp/tree/main";
const MODEL_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin";
const USER_AGENT: &str = "translate-video";
const INSTALLED_FILE: &str = "installed.json";
const STOPPED: &str = "Đã dừng.";

#[derive(Debug, PartialEq)]
struct Release {
    tag: String,
    url: String,
    size: u64,
    sha256: Option<String>,
}

#[derive(Debug, PartialEq)]
struct RemoteFile {
    size: u64,
    sha256: String,
}

#[derive(Default)]
pub(crate) struct Control {
    running: AtomicBool,
    cancel: AtomicBool,
}

struct RunGuard<'a>(&'a AtomicBool);

impl Drop for RunGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

#[derive(Clone, Serialize)]
struct Event {
    stage: &'static str,
    detail: String,
    done: u64,
    total: u64,
}

// The newest release that carries the Windows build. The "bNNNN" builds are what hold the files,
// and GitHub marks them pre-release, so only drafts are skipped. The top "v1.x" release has no files.
fn parse_release(list: &Value) -> Option<Release> {
    for release in list.as_array()? {
        if release
            .get("draft")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            continue;
        }
        let Some(tag) = release.get("tag_name").and_then(Value::as_str) else {
            continue;
        };
        let Some(assets) = release.get("assets").and_then(Value::as_array) else {
            continue;
        };
        for asset in assets {
            if asset.get("name").and_then(Value::as_str) != Some(PROGRAM_ASSET) {
                continue;
            }
            let Some(url) = asset.get("browser_download_url").and_then(Value::as_str) else {
                continue;
            };
            return Some(Release {
                tag: tag.to_string(),
                url: url.to_string(),
                size: asset.get("size").and_then(Value::as_u64).unwrap_or(0),
                sha256: asset
                    .get("digest")
                    .and_then(Value::as_str)
                    .and_then(|digest| digest.strip_prefix("sha256:"))
                    .map(str::to_string),
            });
        }
    }
    None
}

fn parse_model(tree: &Value, name: &str) -> Option<RemoteFile> {
    let entry = tree
        .as_array()?
        .iter()
        .find(|item| item.get("path").and_then(Value::as_str) == Some(name))?;
    let lfs = entry.get("lfs")?;
    Some(RemoteFile {
        size: lfs
            .get("size")
            .or_else(|| entry.get("size"))
            .and_then(Value::as_u64)?,
        sha256: lfs.get("oid").and_then(Value::as_str)?.to_string(),
    })
}

fn tag_number(tag: &str) -> Option<u64> {
    tag.trim().trim_start_matches('b').parse().ok()
}

// Up to date only when the program is really there and the recorded build is not older.
fn program_is_current(installed: Option<&str>, remote: &str, present: bool) -> bool {
    if !present {
        return false;
    }
    if installed == Some(remote) {
        return true;
    }
    match (installed.and_then(tag_number), tag_number(remote)) {
        (Some(have), Some(want)) => have >= want,
        _ => false,
    }
}

fn read_installed_tag(dir: &Path) -> Option<String> {
    let text = fs::read_to_string(dir.join(INSTALLED_FILE)).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    value.get("program_tag")?.as_str().map(str::to_string)
}

fn write_installed_tag(dir: &Path, tag: &str) -> Result<(), String> {
    let text = serde_json::json!({ "program_tag": tag }).to_string();
    fs::write(dir.join(INSTALLED_FILE), text).map_err(|error| error.to_string())
}

fn sha256_file(path: &Path, cancel: &AtomicBool) -> Result<String, String> {
    let mut file = File::open(path).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(STOPPED.into());
        }
        let read = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn file_matches(path: &Path, size: u64, sha256: &str, cancel: &AtomicBool) -> Result<bool, String> {
    match fs::metadata(path) {
        Ok(meta) if meta.is_file() && meta.len() == size => {
            Ok(sha256_file(path, cancel)?.eq_ignore_ascii_case(sha256))
        }
        _ => Ok(false),
    }
}

// Moves what the zip unpacked into the whisper folder, replacing same-named entries.
// Model files (.bin) are never touched.
fn move_entries(from: &Path, to: &Path) -> Result<(), String> {
    for entry in fs::read_dir(from)
        .map_err(|error| error.to_string())?
        .flatten()
    {
        let name = entry.file_name();
        if name
            .to_string_lossy()
            .to_ascii_lowercase()
            .ends_with(".bin")
        {
            continue;
        }
        let target = to.join(&name);
        if target.is_dir() {
            fs::remove_dir_all(&target).map_err(|error| error.to_string())?;
        } else if target.exists() {
            fs::remove_file(&target).map_err(|error| error.to_string())?;
        }
        fs::rename(entry.path(), &target).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn mb(bytes: u64) -> String {
    format!("{:.0} MB", bytes as f64 / 1_048_576.0)
}

fn describe(error: ureq::Error) -> String {
    match error {
        ureq::Error::Status(code, _) => format!("Máy chủ trả lỗi {code}."),
        ureq::Error::Transport(_) => {
            "Không kết nối được mạng. Kiểm tra internet rồi thử lại.".into()
        }
    }
}

fn fetch_json(agent: &ureq::Agent, url: &str) -> Result<Value, String> {
    let text = agent
        .get(url)
        .set("User-Agent", USER_AGENT)
        .set("Accept", "application/json")
        .call()
        .map_err(describe)?
        .into_string()
        .map_err(|error| error.to_string())?;
    serde_json::from_str(&text).map_err(|_| "Phản hồi của máy chủ không đọc được.".to_string())
}

fn download(
    agent: &ureq::Agent,
    url: &str,
    part: &Path,
    expected: u64,
    cancel: &AtomicBool,
    progress: &dyn Fn(u64, u64),
) -> Result<(), String> {
    let response = agent
        .get(url)
        .set("User-Agent", USER_AGENT)
        .call()
        .map_err(describe)?;
    let total = response
        .header("Content-Length")
        .and_then(|value| value.parse().ok())
        .unwrap_or(expected);
    let mut reader = response.into_reader();
    let mut file = File::create(part).map_err(|error| error.to_string())?;
    let mut buffer = vec![0u8; 64 * 1024];
    let mut done = 0u64;
    let mut last = Instant::now();
    loop {
        if cancel.load(Ordering::Relaxed) {
            drop(file);
            let _ = fs::remove_file(part);
            return Err(STOPPED.into());
        }
        let read = reader
            .read(&mut buffer)
            .map_err(|_| "Mạng bị ngắt giữa chừng. Bấm tải lại để tải tiếp.".to_string())?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read])
            .map_err(|error| error.to_string())?;
        done += read as u64;
        if last.elapsed() > Duration::from_millis(250) {
            progress(done, total);
            last = Instant::now();
        }
    }
    file.flush().map_err(|error| error.to_string())?;
    progress(done, total);
    if expected > 0 && done != expected {
        let _ = fs::remove_file(part);
        return Err("File tải về bị thiếu dữ liệu. Bấm tải lại.".into());
    }
    Ok(())
}

fn extract_zip(zip: &Path, dest: &Path) -> Result<(), String> {
    fs::create_dir_all(dest).map_err(|error| error.to_string())?;
    // The Windows tar in System32 reads zip; a tar from Git's PATH would not.
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
    let mut command = Command::new(PathBuf::from(root).join("System32").join("tar.exe"));
    command.arg("-xf").arg(zip).arg("-C").arg(dest);
    let output = crate::hidden(&mut command)
        .output()
        .map_err(|error| format!("Không giải nén được: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "Không giải nén được file zip. {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

fn remove_quietly(path: &Path) {
    if path.is_dir() {
        let _ = fs::remove_dir_all(path);
    } else {
        let _ = fs::remove_file(path);
    }
}

fn install_program(
    agent: &ureq::Agent,
    release: &Release,
    dir: &Path,
    cancel: &AtomicBool,
    emit: &dyn Fn(&'static str, String, u64, u64),
) -> Result<(), String> {
    let part = dir.join("whisper-program.zip.part");
    let staging = dir.join(".new-program");
    remove_quietly(&staging);
    let label = format!(
        "Đang tải chương trình Whisper {} ({})",
        release.tag,
        mb(release.size)
    );
    emit("program", label.clone(), 0, release.size);
    let outcome = (|| {
        download(
            agent,
            &release.url,
            &part,
            release.size,
            cancel,
            &|done, total| emit("program", label.clone(), done, total),
        )?;
        emit("program", "Đang kiểm tra file vừa tải".into(), 0, 0);
        if let Some(expected) = &release.sha256 {
            if !sha256_file(&part, cancel)?.eq_ignore_ascii_case(expected) {
                return Err("File chương trình tải về không khớp mã băm của nơi phát hành. Đã bỏ file đó, thử lại.".to_string());
            }
        }
        emit("program", "Đang giải nén".into(), 0, 0);
        extract_zip(&part, &staging)?;
        if crate::whisper_exe_in(&staging).is_none() {
            return Err("File zip không có whisper-cli.exe.".to_string());
        }
        move_entries(&staging, dir)?;
        write_installed_tag(dir, &release.tag)
    })();
    remove_quietly(&part);
    remove_quietly(&staging);
    outcome
}

fn install_model(
    agent: &ureq::Agent,
    remote: &RemoteFile,
    dir: &Path,
    cancel: &AtomicBool,
    emit: &dyn Fn(&'static str, String, u64, u64),
) -> Result<(), String> {
    let part = dir.join(format!("{MODEL_FILE}.part"));
    let label = format!("Đang tải model {MODEL_FILE} ({})", mb(remote.size));
    emit("model", label.clone(), 0, remote.size);
    let outcome = (|| {
        download(
            agent,
            MODEL_URL,
            &part,
            remote.size,
            cancel,
            &|done, total| emit("model", label.clone(), done, total),
        )?;
        emit("model", "Đang kiểm tra file model vừa tải".into(), 0, 0);
        if !sha256_file(&part, cancel)?.eq_ignore_ascii_case(&remote.sha256) {
            return Err(
                "File model tải về không khớp mã băm của nơi phát hành. Đã bỏ file đó, thử lại."
                    .to_string(),
            );
        }
        let target = dir.join(MODEL_FILE);
        remove_quietly(&target);
        fs::rename(&part, &target).map_err(|error| error.to_string())
    })();
    if outcome.is_err() {
        remove_quietly(&part);
    }
    outcome
}

fn whisper_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../whisper")
}

fn run(app: &AppHandle, cancel: &AtomicBool) -> Result<String, String> {
    let emit = |stage: &'static str, detail: String, done: u64, total: u64| {
        let _ = app.emit(
            "whisper-install",
            Event {
                stage,
                detail,
                done,
                total,
            },
        );
    };
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(60))
        .build();
    emit("check", "Đang kiểm tra phiên bản mới nhất...".into(), 0, 0);
    let release = parse_release(&fetch_json(&agent, RELEASES_URL)?)
        .ok_or("Không thấy bản whisper.cpp có file Windows trên GitHub.")?;
    let remote_model = parse_model(&fetch_json(&agent, MODEL_TREE_URL)?, MODEL_FILE)
        .ok_or("Không thấy model ggml-small.bin trên Hugging Face.")?;
    let dir = whisper_dir();
    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;

    let mut notes: Vec<String> = Vec::new();
    let in_folder = crate::whisper_exe_in(&dir).is_some();
    if program_is_current(read_installed_tag(&dir).as_deref(), &release.tag, in_folder) {
        notes.push(format!(
            "Chương trình Whisper đã là bản mới nhất ({}).",
            release.tag
        ));
    } else if !in_folder && crate::program_path("whisper-cli").is_some() {
        notes.push("Máy đã có whisper-cli trong Path, giữ nguyên.".into());
    } else {
        install_program(&agent, &release, &dir, cancel, &emit)?;
        notes.push(format!("Đã cài chương trình Whisper {}.", release.tag));
    }

    emit("check", "Đang đối chiếu file model...".into(), 0, 0);
    if file_matches(
        &dir.join(MODEL_FILE),
        remote_model.size,
        &remote_model.sha256,
        cancel,
    )? {
        notes.push(format!("Model {MODEL_FILE} đã đúng bản, không tải lại."));
    } else {
        install_model(&agent, &remote_model, &dir, cancel, &emit)?;
        notes.push(format!("Đã tải model {MODEL_FILE}."));
    }
    Ok(notes.join(" "))
}

#[tauri::command]
pub(crate) async fn install_whisper(app: AppHandle) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let control = app.state::<Control>();
        if control.running.swap(true, Ordering::SeqCst) {
            return Err("Đang tải Whisper.".to_string());
        }
        let _guard = RunGuard(&control.running);
        control.cancel.store(false, Ordering::SeqCst);
        let result = run(&app, &control.cancel);
        let (stage, detail) = match &result {
            Ok(text) => ("done", text.clone()),
            Err(error) => ("error", error.clone()),
        };
        let _ = app.emit(
            "whisper-install",
            Event {
                stage,
                detail,
                done: 0,
                total: 0,
            },
        );
        result
    })
    .await
    .unwrap_or_else(|_| Err("Không tải được Whisper.".into()))
}

#[tauri::command]
pub(crate) fn cancel_whisper_install(app: AppHandle) {
    app.state::<Control>().cancel.store(true, Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("whisper-install-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn release_is_the_newest_one_that_has_the_windows_zip_even_if_pre_release() {
        let list = json!([
            {"tag_name": "v1.9.5", "assets": []},
            {"tag_name": "b9999", "draft": true, "assets": [
                {"name": PROGRAM_ASSET, "browser_download_url": "https://x/draft.zip", "size": 1}]},
            {"tag_name": "b5454", "prerelease": true, "assets": [
                {"name": "other.zip", "browser_download_url": "https://x/other.zip", "size": 2},
                {"name": PROGRAM_ASSET, "browser_download_url": "https://x/win.zip", "size": 8928640,
                 "digest": "sha256:abc123"}]},
            {"tag_name": "b5130", "assets": [
                {"name": PROGRAM_ASSET, "browser_download_url": "https://x/old.zip", "size": 3}]}
        ]);
        let release = parse_release(&list).unwrap();
        assert_eq!(release.tag, "b5454");
        assert_eq!(release.url, "https://x/win.zip");
        assert_eq!(release.size, 8928640);
        assert_eq!(release.sha256.as_deref(), Some("abc123"));
        assert!(parse_release(&json!([{"tag_name": "v1", "assets": []}])).is_none());
        assert!(parse_release(&json!({"message": "rate limit"})).is_none());
    }

    #[test]
    fn model_size_and_hash_come_from_the_lfs_entry() {
        let tree = json!([
            {"path": "ggml-base.bin", "size": 1, "lfs": {"oid": "aaa", "size": 147951465}},
            {"path": "ggml-small.bin", "size": 134, "lfs": {"oid": "bbb", "size": 487601967}}
        ]);
        assert_eq!(
            parse_model(&tree, "ggml-small.bin"),
            Some(RemoteFile {
                size: 487601967,
                sha256: "bbb".into()
            })
        );
        assert_eq!(parse_model(&tree, "ggml-tiny.bin"), None);
        assert_eq!(
            parse_model(
                &json!([{"path": "ggml-small.bin", "size": 5}]),
                "ggml-small.bin"
            ),
            None
        );
    }

    #[test]
    fn version_check_skips_only_when_not_older_and_present() {
        assert!(program_is_current(Some("b5454"), "b5454", true));
        assert!(program_is_current(Some("b6000"), "b5454", true));
        assert!(!program_is_current(Some("b5130"), "b5454", true));
        assert!(!program_is_current(None, "b5454", true));
        assert!(!program_is_current(Some("b5454"), "b5454", false));
        assert!(!program_is_current(Some("v1.9"), "b5454", true));
        assert!(program_is_current(Some("v1.9.6"), "v1.9.6", true));
    }

    #[test]
    fn sha256_matches_the_known_digest_of_abc() {
        let dir = scratch("sha");
        let path = dir.join("abc.txt");
        fs::write(&path, b"abc").unwrap();
        let cancel = AtomicBool::new(false);
        let digest = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert_eq!(sha256_file(&path, &cancel).unwrap(), digest);
        assert!(file_matches(&path, 3, digest, &cancel).unwrap());
        assert!(!file_matches(&path, 4, digest, &cancel).unwrap());
        assert!(!file_matches(&path, 3, "00", &cancel).unwrap());
        assert!(!file_matches(&dir.join("missing"), 3, digest, &cancel).unwrap());
        assert_eq!(
            sha256_file(&path, &AtomicBool::new(true)),
            Err(STOPPED.to_string())
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn move_entries_replaces_the_program_and_never_touches_models() {
        let root = scratch("move");
        let (from, to) = (root.join("new"), root.join("whisper"));
        fs::create_dir_all(from.join("Release")).unwrap();
        fs::write(from.join("Release").join("whisper-cli.exe"), b"new").unwrap();
        fs::write(from.join("ggml-small.bin"), b"from zip").unwrap();
        fs::create_dir_all(to.join("Release")).unwrap();
        fs::write(to.join("Release").join("old.dll"), b"old").unwrap();
        fs::write(to.join("ggml-small.bin"), b"keep me").unwrap();
        move_entries(&from, &to).unwrap();
        assert!(to.join("Release").join("whisper-cli.exe").is_file());
        assert!(!to.join("Release").join("old.dll").exists());
        assert_eq!(fs::read(to.join("ggml-small.bin")).unwrap(), b"keep me");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn installed_tag_round_trips() {
        let dir = scratch("tag");
        assert_eq!(read_installed_tag(&dir), None);
        write_installed_tag(&dir, "b5454").unwrap();
        assert_eq!(read_installed_tag(&dir).as_deref(), Some("b5454"));
        let _ = fs::remove_dir_all(&dir);
    }
}
