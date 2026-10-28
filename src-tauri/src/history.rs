use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::job::{read_json, write_json};
use crate::render::RenderSettings;

static SEQUENCE: AtomicU64 = AtomicU64::new(0);
pub(crate) fn new_id() -> String {
    format!(
        "v-{}-{}-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )
}
fn valid_id(id: &str) -> bool {
    id.starts_with("v-")
        && id.len() > 2
        && id.len() < 100
        && id.as_bytes()[2].is_ascii_digit()
        && id[2..].bytes().all(|b| b.is_ascii_digit() || b == b'-')
}
fn safe_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', ':'])
}
fn media(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("mp4") || e.eq_ignore_ascii_case("m4a"))
        && !path
            .file_name()
            .is_some_and(|n| n.to_string_lossy().contains(".partial."))
}
pub(crate) fn display(path: &Path) -> String {
    let value = path.to_string_lossy();
    value.strip_prefix(r"\\?\").unwrap_or(&value).to_string()
}
fn stamp(path: &Path) -> Result<(u64, u128), String> {
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if !meta.is_file() || meta.len() == 0 {
        return Err("Không thấy file kết quả đã xong.".into());
    }
    Ok((
        meta.len(),
        meta.modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|t| t.as_nanos())
            .unwrap_or(0),
    ))
}

type HashCache = HashMap<PathBuf, (u64, u128, String)>;
static HASHES: OnceLock<Mutex<HashCache>> = OnceLock::new();
fn digest(path: &Path, cancel: &AtomicBool) -> Result<String, String> {
    let (len, time) = stamp(path)?;
    let hashes = HASHES.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some((l, t, hash)) = hashes.lock().unwrap_or_else(|e| e.into_inner()).get(path) {
        if *l == len && *t == time {
            return Ok(hash.clone());
        }
    }
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut bytes = vec![0; 1024 * 1024];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("Đã dừng.".into());
        }
        let count = file.read(&mut bytes).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        hash.update(&bytes[..count]);
    }
    if stamp(path)? != (len, time) {
        return Err("File kết quả vừa thay đổi; chưa lưu lịch sử.".into());
    }
    let value = format!("{:x}", hash.finalize());
    let mut saved = hashes.lock().unwrap_or_else(|e| e.into_inner());
    if saved.len() >= 512 {
        saved.clear();
    }
    saved.insert(path.to_path_buf(), (len, time, value.clone()));
    Ok(value)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Provider {
    pub provider: String,
    pub model: String,
}

// Explicit allowlist: never serialize TranslationChoice or app preferences/keys.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Config {
    pub target: String,
    pub voice: String,
    pub use_terms: bool,
    pub skip_translate: bool,
    pub text_only: bool,
    pub audio_only: bool,
    pub burn_captions: bool,
    pub render: RenderSettings,
    pub translation: Option<Provider>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct Content {
    words: String,
    timing: String,
    structure: String,
    lines: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Output {
    name: String,
    bytes: u64,
    modified: u128,
    sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Snapshot {
    version: u32,
    id: String,
    created_ms: u64,
    primary: String,
    reasons: Vec<String>,
    config: Option<Config>,
    content: Option<Content>,
    files: Vec<Output>,
}

#[derive(Serialize, Deserialize)]
struct Current {
    id: String,
    primary: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VersionFile {
    pub path: String,
    pub kind: &'static str,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Details {
    pub id: Option<String>,
    pub created_ms: u64,
    pub reasons: Vec<String>,
    pub config: Option<Config>,
    pub lines: Option<usize>,
    pub files: Vec<VersionFile>,
    pub note: String,
}

fn directory(root: &Path, id: &str) -> Result<PathBuf, String> {
    if !valid_id(id) {
        return Err("Mã bản xuất không hợp lệ.".into());
    }
    Ok(root.join(crate::job::KEPT_DIR).join(format!("ban-{id}")))
}
fn read_snapshot(folder: &Path) -> Result<Snapshot, String> {
    let record: Snapshot = read_json(&folder.join("version.json"))?;
    if record.version != 1
        || !valid_id(&record.id)
        || !safe_name(&record.primary)
        || record.files.is_empty()
        || record.files.len() > 3
        || record.files.iter().any(|f| {
            !safe_name(&f.name)
                || !media(Path::new(&f.name))
                || f.bytes == 0
                || f.sha256.len() != 64
                || !f.sha256.bytes().all(|b| b.is_ascii_hexdigit())
        })
        || !record.files.iter().any(|f| f.name == record.primary)
    {
        return Err("Thông tin bản xuất không hợp lệ.".into());
    }
    if let Some(config) = &record.config {
        config.render.validate()?;
    }
    Ok(record)
}
pub(crate) fn pending(root: &Path, id: &str) -> bool {
    let undo = root.join("lines/_undo");
    if undo.join("done").is_file() {
        return false;
    }
    read_json::<serde_json::Value>(&undo.join("manifest.json"))
        .ok()
        .and_then(|v| {
            v.get("history_new")
                .and_then(|s| s.as_str())
                .map(|s| s == id)
        })
        .unwrap_or(false)
}
fn present(folder: &Path, file: &Output) -> bool {
    stamp(&folder.join(&file.name)).is_ok_and(|s| s == (file.bytes, file.modified))
}
fn snapshots(root: &Path) -> Vec<(PathBuf, Snapshot)> {
    std::fs::read_dir(root.join(crate::job::KEPT_DIR))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let folder = entry.path();
            if !entry.file_type().ok()?.is_dir() {
                return None;
            }
            let record = read_snapshot(&folder).ok()?;
            if directory(root, &record.id).ok()? != folder || pending(root, &record.id) {
                return None;
            }
            Some((folder, record))
        })
        .collect()
}

fn content(root: &Path) -> Option<Content> {
    let lines = crate::timing::read(root).ok()?;
    let words: Vec<_> = lines.iter().map(|l| &l.translation).collect();
    let times: Vec<_> = lines.iter().map(|l| (l.start, l.end)).collect();
    let ids: Vec<_> = match crate::editing::Editing::load(root).ok()? {
        Some(doc) => doc.cues.iter().map(|c| c.id.clone()).collect(),
        None => (0..lines.len()).map(crate::editing::legacy_id).collect(),
    };
    let hash = |v: Vec<u8>| format!("{:x}", Sha256::digest(v));
    Some(Content {
        words: hash(serde_json::to_vec(&words).ok()?),
        timing: hash(serde_json::to_vec(&times).ok()?),
        structure: hash(serde_json::to_vec(&ids).ok()?),
        lines: lines.len(),
    })
}
fn configuration(root: &Path, result: &Path, provider: Option<Provider>) -> Option<Config> {
    let saved: crate::pipeline::RunSettings = read_json(&root.join("settings.json")).ok()?;
    if saved.target != crate::job::media_language(result) {
        return None;
    }
    Some(Config {
        target: saved.target,
        voice: saved.voice,
        use_terms: saved.use_terms,
        skip_translate: saved.skip_translate,
        text_only: saved.text_only,
        audio_only: result
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("m4a")),
        burn_captions: result
            .file_name()
            .is_some_and(|n| n.to_string_lossy().ends_with(".chu.mp4")),
        render: RenderSettings::read(root).ok()?,
        translation: provider,
    })
}

fn changes(
    previous: Option<&Snapshot>,
    config: Option<&Config>,
    text: Option<&Content>,
    hints: &[String],
) -> Vec<String> {
    let mut notes = hints.to_vec();
    if let (Some(old), Some(new)) = (previous.and_then(|p| p.config.as_ref()), config) {
        let a = &old.render;
        let b = &new.render;
        let mut add = |yes: bool, label: &str| {
            if yes {
                notes.push(label.into());
            }
        };
        add(old.target != new.target, "Đổi ngôn ngữ");
        add(old.voice != new.voice, "Đổi giọng");
        add(
            (old.text_only, old.audio_only, old.burn_captions)
                != (new.text_only, new.audio_only, new.burn_captions),
            "Đổi loại kết quả",
        );
        add(
            (old.skip_translate, old.use_terms) != (new.skip_translate, new.use_terms)
                || old.translation != new.translation,
            "Đổi lựa chọn dịch",
        );
        add(
            (a.aspect.as_str(), a.framing.as_str(), a.focus_x, a.focus_y)
                != (b.aspect.as_str(), b.framing.as_str(), b.focus_x, b.focus_y),
            "Đổi khung hình",
        );
        add(a.logo != b.logo, "Đổi logo");
        add(a.mask != b.mask, "Đổi vùng che");
        add(a.audio != b.audio, "Đổi âm lượng");
        add(a.pronunciations != b.pronunciations, "Đổi cách đọc");
        add(
            (
                &a.font,
                a.size,
                &a.color,
                &a.outline_color,
                a.outline,
                a.background,
                a.caption_x,
                a.caption_y,
            ) != (
                &b.font,
                b.size,
                &b.color,
                &b.outline_color,
                b.outline,
                b.background,
                b.caption_x,
                b.caption_y,
            ),
            "Đổi kiểu chữ",
        );
    }
    if let (Some(old), Some(new)) = (previous.and_then(|p| p.content.as_ref()), text) {
        if old.structure != new.structure
            && !hints
                .iter()
                .any(|h| matches!(h.as_str(), "Tách câu" | "Gộp câu" | "Hoàn tác tách/gộp"))
        {
            notes.push("Tách/gộp câu".into());
        } else if old.words != new.words {
            notes.push("Sửa lời".into());
        }
        if old.timing != new.timing {
            notes.push("Đổi mốc câu".into());
        }
    }
    let mut unique = Vec::new();
    for note in notes {
        if !unique.contains(&note) {
            unique.push(note);
        }
    }
    if unique.is_empty() {
        unique.push(
            if previous.is_none() {
                "Xuất lần đầu"
            } else {
                "Xuất lại"
            }
            .into(),
        );
    }
    unique
}

fn previous(root: &Path) -> Option<Snapshot> {
    for marker in [
        root.join("history-current.json"),
        root.join("lines/_undo/park/history-current.json"),
    ] {
        if let Ok(current) = read_json::<Current>(&marker) {
            if let Ok(folder) = directory(root, &current.id) {
                if let Ok(record) = read_snapshot(&folder) {
                    if record.id == current.id {
                        return Some(record);
                    }
                }
            }
        }
    }
    None
}

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn preserve(from: &Path, to: &Path, cancel: &AtomicBool) -> Result<(), String> {
    if std::fs::hard_link(from, to).is_ok() {
        return Ok(());
    }
    let mut input = std::fs::File::open(from).map_err(|e| e.to_string())?;
    let mut out = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(to)
        .map_err(|e| e.to_string())?;
    let mut bytes = vec![0; 1024 * 1024];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("Đã dừng.".into());
        }
        let n = input.read(&mut bytes).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        out.write_all(&bytes[..n]).map_err(|e| e.to_string())?;
    }
    out.sync_all().map_err(|e| e.to_string())
}

fn publish_inner(
    root: &Path,
    result: &Path,
    hints: &[String],
    provider: Option<Provider>,
    transaction: Option<&str>,
    legacy: bool,
    cancel: &AtomicBool,
) -> Result<String, String> {
    if result.parent() != Some(root) || !media(result) {
        return Err("Chỉ ghi lịch sử kết quả trong folder việc.".into());
    }
    crate::pipeline::media_duration(result, cancel)?;
    let old = previous(root);
    let provider = provider.or_else(|| {
        old.as_ref()
            .and_then(|p| p.config.as_ref())
            .and_then(|c| c.translation.clone())
    });
    let config = if legacy {
        None
    } else {
        configuration(root, result, provider)
    };
    let text = if legacy { None } else { content(root) };
    let mut sources = vec![result.to_path_buf()];
    if let Some(stem) = result
        .file_name()
        .and_then(|n| n.to_str())
        .and_then(|n| n.strip_suffix(".chu.mp4"))
    {
        let plain = root.join(format!("{stem}.mp4"));
        if plain.is_file() && crate::pipeline::media_duration(&plain, cancel).is_ok() {
            sources.push(plain);
        }
    }
    let mut files = Vec::new();
    for source in &sources {
        let (bytes, _) = stamp(source)?;
        files.push(Output {
            name: source.file_name().unwrap().to_string_lossy().into_owned(),
            bytes,
            modified: 0,
            sha256: digest(source, cancel)?,
        });
    }
    if !legacy && transaction.is_none() {
        if let Some(old) = &old {
            if old.config == config
                && old.content == text
                && old.files.len() == files.len()
                && files.iter().all(|f| {
                    old.files
                        .iter()
                        .any(|o| o.name == f.name && o.sha256 == f.sha256)
                })
                && directory(root, &old.id)
                    .is_ok_and(|folder| old.files.iter().all(|f| present(&folder, f)))
            {
                return Ok(old.id.clone());
            }
        }
    }
    let id = transaction.map(str::to_string).unwrap_or_else(new_id);
    let target = directory(root, &id)?;
    let kept = root.join(crate::job::KEPT_DIR);
    std::fs::create_dir_all(&kept).map_err(|e| e.to_string())?;
    let scratch = Scratch(kept.join(format!(".pending-{id}")));
    std::fs::create_dir(&scratch.0).map_err(|e| e.to_string())?;
    for (source, file) in sources.iter().zip(&mut files) {
        let dest = scratch.0.join(&file.name);
        preserve(source, &dest, cancel)?;
        if digest(&dest, cancel)? != file.sha256 {
            return Err("File thay đổi khi lưu bản xuất; chưa chốt lịch sử.".into());
        }
        file.modified = stamp(&dest)?.1;
    }
    let plain = result
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .replace(".chu.mp4", ".mp4");
    let srt = Path::new(&plain).with_extension("srt");
    if root.join(&srt).is_file() {
        preserve(&root.join(&srt), &scratch.0.join(&srt), cancel)?;
    }
    let reasons = if legacy {
        vec!["Bản cũ · không rõ thay đổi".into()]
    } else {
        changes(old.as_ref(), config.as_ref(), text.as_ref(), hints)
    };
    let record = Snapshot {
        version: 1,
        id: id.clone(),
        created_ms: crate::now_ms(),
        primary: result.file_name().unwrap().to_string_lossy().into_owned(),
        reasons,
        config,
        content: text,
        files,
    };
    write_json(&scratch.0.join("version.json"), &record)?;
    if cancel.load(Ordering::Relaxed) {
        return Err("Đã dừng.".into());
    }
    std::fs::rename(&scratch.0, &target).map_err(|e| e.to_string())?;
    if let Err(error) = write_json(
        &root.join("history-current.json"),
        &Current {
            id: id.clone(),
            primary: record.primary,
        },
    ) {
        remove(root, &id)?;
        return Err(error);
    }
    Ok(id)
}

pub(crate) fn publish(
    root: &Path,
    result: &Path,
    hints: &[String],
    provider: Option<Provider>,
    transaction: Option<&str>,
    cancel: &AtomicBool,
) -> Result<String, String> {
    publish_inner(root, result, hints, provider, transaction, false, cancel)
}

pub(crate) fn ensure_previous(root: &Path) -> Result<(), String> {
    let cancel = AtomicBool::new(false);
    if let Some(record) = previous(root) {
        if record
            .files
            .iter()
            .filter(|file| file.name == record.primary)
            .any(|file| {
                stamp(&root.join(&file.name)).is_ok_and(|s| s.0 == file.bytes)
                    && digest(&root.join(&file.name), &cancel).is_ok_and(|d| d == file.sha256)
            })
        {
            return Ok(());
        }
    }
    let mut candidates: Vec<_> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| media(p) && !crate::job::media_language(p).is_empty())
        .collect();
    candidates.sort_by_key(|p| {
        !p.file_name()
            .is_some_and(|n| n.to_string_lossy().ends_with(".chu.mp4"))
    });
    for file in candidates {
        if crate::pipeline::media_duration(&file, &cancel).is_ok() {
            publish_inner(root, &file, &[], None, None, true, &cancel)?;
            break;
        }
    }
    Ok(())
}

pub(crate) fn remove(root: &Path, id: &str) -> Result<(), String> {
    let folder = directory(root, id)?;
    if !folder.exists() {
        return Ok(());
    }
    if std::fs::symlink_metadata(&folder)
        .map_err(|e| e.to_string())?
        .file_type()
        .is_symlink()
    {
        return Err("Không xóa folder lịch sử liên kết ngoài.".into());
    }
    let kept = root
        .join(crate::job::KEPT_DIR)
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let resolved = folder.canonicalize().map_err(|e| e.to_string())?;
    if resolved.parent() != Some(kept.as_path()) {
        return Err("Folder lịch sử nằm ngoài việc.".into());
    }
    std::fs::remove_dir_all(resolved).map_err(|e| e.to_string())
}

pub(crate) fn enrich(root: &Path, items: &mut Vec<crate::job::KeptVersion>) {
    let records = snapshots(root);
    let current = read_json::<Current>(&root.join("history-current.json"))
        .ok()
        .filter(|c| {
            records.iter().any(|(_, r)| {
                r.id == c.id
                    && r.files.iter().any(|f| {
                        f.name == c.primary
                            && digest(&root.join(&f.name), &AtomicBool::new(false))
                                .is_ok_and(|d| d == f.sha256)
                    })
            })
        });
    items.retain_mut(|item| {
        let path = Path::new(&item.path);
        let folder = path.parent().unwrap_or(root);
        if folder
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|n| n.starts_with(".pending-"))
        {
            return false;
        }
        if let Some(id) = folder
            .file_name()
            .and_then(|s| s.to_str())
            .and_then(|n| n.strip_prefix("ban-"))
        {
            if pending(root, id) {
                return false;
            }
        }
        if let Some((_, record)) = records.iter().find(|(f, _)| f == folder) {
            if current.as_ref().is_some_and(|c| c.id == record.id) {
                return false;
            }
            let Some(primary) = record
                .files
                .iter()
                .find(|f| f.name == record.primary && present(folder, f))
                .or_else(|| record.files.iter().find(|f| present(folder, f)))
            else {
                return false;
            };
            item.path = folder.join(&primary.name).to_string_lossy().into_owned();
            item.language = crate::job::media_language(Path::new(&primary.name));
            item.audio = primary.name.ends_with(".m4a");
            item.modified_ms = record.created_ms;
            item.version_id = Some(record.id.clone());
            item.summary = Some(record.reasons.join(" · "));
            item.known_config = record.config.is_some();
            return true;
        }
        // Existing archive folders remain untouched. Suppress only exact media
        // duplicates already preserved as a typed snapshot, never infer config.
        let duplicate = records.iter().any(|(f, r)| {
            r.files.iter().any(|saved| {
                present(f, saved)
                    && stamp(path).is_ok_and(|s| s.0 == saved.bytes)
                    && digest(path, &AtomicBool::new(false)).is_ok_and(|hash| hash == saved.sha256)
            })
        });
        !duplicate
    });
    // Some valid exports (notably continuous-listening audio) are deliberately
    // excluded by the legacy archive scanner. Typed records supply those rows.
    for (folder, record) in records {
        if current.as_ref().is_some_and(|c| c.id == record.id)
            || items
                .iter()
                .any(|i| Path::new(&i.path).parent() == Some(folder.as_path()))
        {
            continue;
        }
        if let Some(file) = record
            .files
            .iter()
            .find(|f| f.name == record.primary && present(&folder, f))
            .or_else(|| record.files.iter().find(|f| present(&folder, f)))
        {
            items.push(crate::job::KeptVersion {
                path: folder.join(&file.name).to_string_lossy().into_owned(),
                language: crate::job::media_language(Path::new(&file.name)),
                modified_ms: record.created_ms,
                audio: file.name.ends_with(".m4a"),
                version_id: Some(record.id),
                summary: Some(record.reasons.join(" · ")),
                known_config: record.config.is_some(),
            });
        }
    }
}

pub(crate) fn root_for(result: &Path) -> Result<PathBuf, String> {
    let root = if result.is_dir() {
        result
    } else {
        result.parent().ok_or("Chưa thấy folder việc.")?
    };
    crate::job::Job::reopen(root)?;
    root.canonicalize().map_err(|e| e.to_string())
}
pub(crate) fn member(root: &Path, selected: &Path) -> Result<PathBuf, String> {
    let file = selected
        .canonicalize()
        .map_err(|_| "File bản xuất không còn tồn tại.".to_string())?;
    if !media(&file) || !file.is_file() {
        return Err("Chỉ mở bản video hoặc audio đã xuất.".into());
    }
    let parent = file.parent().ok_or("Không thấy folder bản xuất.")?;
    if parent
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with(".pending-"))
    {
        return Err("Bản xuất chưa được chốt.".into());
    }
    if parent != root && parent.parent() != Some(root.join(crate::job::KEPT_DIR).as_path()) {
        return Err("Bản xuất nằm ngoài việc đang xem.".into());
    }
    if let Some(id) = parent
        .file_name()
        .and_then(|s| s.to_str())
        .and_then(|n| n.strip_prefix("ban-"))
    {
        if pending(root, id) {
            return Err("Bản xuất chưa được chốt.".into());
        }
    }
    Ok(file)
}

pub(crate) fn details(root: &Path, selected: &Path) -> Result<Details, String> {
    let file = member(root, selected)?;
    let folder = file.parent().unwrap();
    let mut records = snapshots(root);
    records.sort_by_key(|(_, r)| std::cmp::Reverse(r.created_ms));
    let own = if folder != root {
        records.iter().find(|(f, _)| f == folder)
    } else {
        records.iter().find(|(_, r)| {
            r.files.iter().any(|o| {
                o.name == file.file_name().unwrap().to_string_lossy()
                    && digest(&file, &AtomicBool::new(false)).is_ok_and(|d| d == o.sha256)
            })
        })
    };
    if let Some((location, record)) = own {
        let saved = record
            .files
            .iter()
            .find(|f| f.name == file.file_name().unwrap().to_string_lossy());
        if saved
            .is_some_and(|s| digest(&file, &AtomicBool::new(false)).is_ok_and(|d| d == s.sha256))
        {
            let files = record
                .files
                .iter()
                .filter(|f| present(location, f))
                .map(|f| VersionFile {
                    path: display(&location.join(&f.name)),
                    kind: if f.name.ends_with(".m4a") {
                        "audio"
                    } else if f.name.ends_with(".chu.mp4") {
                        "captioned"
                    } else {
                        "video"
                    },
                })
                .collect();
            return Ok(Details { id: Some(record.id.clone()), created_ms: record.created_ms, reasons: record.reasons.clone(), config: record.config.clone(), lines: record.content.as_ref().map(|c| c.lines), files,
                note: "Nạp cấu hình dùng các lựa chọn hình/tiếng này với lời và mốc hiện tại. Dịch lại dùng dịch vụ đang chọn.".into() });
        }
    }
    Ok(Details {
        id: None,
        created_ms: 0,
        reasons: vec!["Bản cũ · không rõ thay đổi".into()],
        config: None,
        lines: None,
        files: vec![VersionFile {
            path: display(&file),
            kind: if file
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("m4a"))
            {
                "audio"
            } else {
                "video"
            },
        }],
        note: "Không có metadata khớp file để nạp cấu hình; vẫn mở và lưu được bản này.".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture(PathBuf);
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!("export-history-{}", new_id()));
            std::fs::create_dir_all(root.join("lines")).unwrap();
            write_json(&root.join("settings.json"), &serde_json::json!({"target":"vi","voice":"voice-a","apiKey":"SECRET-MUST-NOT-LEAK"})).unwrap();
            write_json(
                &root.join("translation.json"),
                &vec![crate::job::TranslatedLine {
                    start: 0.,
                    end: 1.,
                    source: "hello".into(),
                    translation: "xin chào".into(),
                }],
            )
            .unwrap();
            std::fs::write(root.join("lines/001.wav"), b"untouched voice cache").unwrap();
            let fixture = Self(root);
            fixture.video("red");
            fixture
        }
        fn result(&self) -> PathBuf {
            self.0.join("clip.vi.chu.mp4")
        }
        fn video(&self, color: &str) {
            let temp = self.0.join("fixture.mp4");
            let status = std::process::Command::new("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-y",
                    "-f",
                    "lavfi",
                    "-i",
                    &format!("color=c={color}:s=64x64:d=0.2"),
                    "-c:v",
                    "libx264",
                    "-threads",
                    "1",
                ])
                .arg(&temp)
                .status()
                .unwrap();
            assert!(status.success());
            // App exports replace a file atomically; never overwrite a hard link in place.
            let _ = std::fs::remove_file(self.result());
            std::fs::rename(temp, self.result()).unwrap();
        }
        fn publish(&self) -> String {
            publish(
                &self.0,
                &self.result(),
                &[],
                Some(Provider {
                    provider: "gemini".into(),
                    model: "test-model".into(),
                }),
                None,
                &AtomicBool::new(false),
            )
            .unwrap()
        }
    }

    #[test]
    fn versions_preserve_media_config_and_words_without_keys_or_voice_copies() {
        let f = Fixture::new();
        let first = f.publish();
        assert_eq!(f.publish(), first); // cached run is not a new version
        let old = directory(&f.0, &first).unwrap().join("clip.vi.chu.mp4");
        let original = std::fs::read(&old).unwrap();
        let mut render = RenderSettings::default();
        render.color = "#ffcc00".into();
        write_json(&f.0.join("render.json"), &render).unwrap();
        f.video("blue");
        let second = f.publish();
        assert_ne!(first, second);
        let current = read_snapshot(&directory(&f.0, &second).unwrap()).unwrap();
        assert!(current.reasons.contains(&"Đổi kiểu chữ".into()));
        let root = f.0.canonicalize().unwrap();
        assert_eq!(
            details(&root, &old).unwrap().config.unwrap().render.color,
            "#ffffff"
        );
        assert_eq!(
            details(&root, &f.result())
                .unwrap()
                .config
                .unwrap()
                .render
                .color,
            "#ffcc00"
        );
        assert_eq!(std::fs::read(&old).unwrap(), original);
        let mut lines: Vec<crate::job::TranslatedLine> =
            read_json(&f.0.join("translation.json")).unwrap();
        lines[0].translation = "chào bạn".into();
        write_json(&f.0.join("translation.json"), &lines).unwrap();
        let third = f.publish();
        let record = read_snapshot(&directory(&f.0, &third).unwrap()).unwrap();
        assert!(record.reasons.contains(&"Sửa lời".into()));
        assert_eq!(crate::job::list_kept(&f.0).unwrap().len(), 2);
        for (folder, _) in snapshots(&f.0) {
            let json = std::fs::read_to_string(folder.join("version.json")).unwrap();
            assert!(!json.contains("SECRET") && !json.contains("apiKey"));
            assert!(std::fs::read_dir(folder).unwrap().all(|e| e
                .unwrap()
                .path()
                .extension()
                .is_none_or(|ext| ext != "wav")));
        }
        assert_eq!(
            std::fs::read(f.0.join("lines/001.wav")).unwrap(),
            b"untouched voice cache"
        );
    }

    #[test]
    fn failed_or_cancelled_publications_keep_last_success_and_clean_staging() {
        let f = Fixture::new();
        let first = f.publish();
        let marker = std::fs::read(f.0.join("history-current.json")).unwrap();
        f.video("blue");
        assert!(publish(&f.0, &f.result(), &[], None, None, &AtomicBool::new(true)).is_err());
        let _ = std::fs::remove_file(f.result());
        std::fs::write(f.result(), b"broken mp4").unwrap();
        assert!(publish(&f.0, &f.result(), &[], None, None, &AtomicBool::new(false)).is_err());
        assert_eq!(
            std::fs::read(f.0.join("history-current.json")).unwrap(),
            marker
        );
        assert_eq!(snapshots(&f.0).len(), 1);
        assert_eq!(previous(&f.0).unwrap().id, first);
        assert!(std::fs::read_dir(f.0.join("cu")).unwrap().all(|e| !e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".pending-")));
    }

    #[test]
    fn transaction_version_is_hidden_until_done_and_can_be_rolled_back() {
        let f = Fixture::new();
        f.publish();
        let marker = std::fs::read(f.0.join("history-current.json")).unwrap();
        let id = new_id();
        let undo = f.0.join("lines/_undo");
        std::fs::create_dir(&undo).unwrap();
        write_json(
            &undo.join("manifest.json"),
            &serde_json::json!({"history_new":id}),
        )
        .unwrap();
        f.video("blue");
        publish(
            &f.0,
            &f.result(),
            &["Sửa lời".into()],
            None,
            Some(&id),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(snapshots(&f.0).len(), 1);
        let file = directory(&f.0, &id).unwrap().join("clip.vi.chu.mp4");
        assert!(details(&f.0.canonicalize().unwrap(), &file).is_err());
        assert!(crate::job::list_kept(&f.0)
            .unwrap()
            .iter()
            .all(|v| v.version_id.as_deref() != Some(&id)));
        std::fs::write(undo.join("done"), b"ok").unwrap();
        assert_eq!(snapshots(&f.0).len(), 2);
        std::fs::remove_file(undo.join("done")).unwrap();
        remove(&f.0, &id).unwrap();
        std::fs::write(f.0.join("history-current.json"), marker).unwrap();
        assert_eq!(snapshots(&f.0).len(), 1);
        assert!(!file.exists());
    }

    #[test]
    fn legacy_media_has_no_guessed_config_and_paths_cannot_cross_jobs() {
        let f = Fixture::new();
        ensure_previous(&f.0).unwrap();
        let root = f.0.canonicalize().unwrap();
        let legacy = details(&root, &f.result()).unwrap();
        assert!(legacy.config.is_none());
        let other = Fixture::new();
        assert!(member(&root, &other.result()).is_err());
        let partial = f.0.join("clip.partial.mp4");
        std::fs::copy(f.result(), &partial).unwrap();
        assert!(member(&root, &partial).is_err());
        assert!(remove(&f.0, "v-../outside").is_err());
        let old = f.0.join("cu/legacy");
        std::fs::create_dir(&old).unwrap();
        std::fs::copy(f.result(), old.join("clip.vi.mp4")).unwrap();
        write_json(
            &old.join("settings.json"),
            &serde_json::json!({"target":"vi","voice":"guessed"}),
        )
        .unwrap();
        assert!(details(&root, &old.join("clip.vi.mp4"))
            .unwrap()
            .config
            .is_none());
    }

    #[test]
    fn missing_preserved_media_does_not_reuse_an_incomplete_record() {
        let f = Fixture::new();
        let first = f.publish();
        std::fs::remove_file(directory(&f.0, &first).unwrap().join("clip.vi.chu.mp4")).unwrap();
        let second = f.publish();
        assert_ne!(first, second);
        assert!(directory(&f.0, &second)
            .unwrap()
            .join("clip.vi.chu.mp4")
            .is_file());
    }

    #[test]
    fn continuous_audio_history_survives_a_later_video_and_saves_exact_old_bytes() {
        let f = Fixture::new();
        f.publish();
        let audio = f.0.join("clip.vi.nghe.m4a");
        assert!(std::process::Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=0.2",
                "-c:a",
                "aac"
            ])
            .arg(&audio)
            .status()
            .unwrap()
            .success());
        let id = publish(&f.0, &audio, &[], None, None, &AtomicBool::new(false)).unwrap();
        f.video("blue");
        f.publish();
        let kept = crate::job::list_kept(&f.0).unwrap();
        let item = kept
            .iter()
            .find(|v| v.version_id.as_deref() == Some(id.as_str()))
            .unwrap();
        assert!(item.audio && item.known_config);
        assert_eq!(item.language, "vi");
        let info = details(&f.0.canonicalize().unwrap(), Path::new(&item.path)).unwrap();
        assert!(info.config.unwrap().audio_only);
        let dest = f.0.join("saved.m4a");
        crate::pipeline::save_spoken(Path::new(&item.path), &dest).unwrap();
        assert_eq!(
            std::fs::read(&dest).unwrap(),
            std::fs::read(&audio).unwrap()
        );
    }
}
