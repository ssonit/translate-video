use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

pub(crate) const MAX_LINE_CHARS: usize = 75;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Cue {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct TranslatedLine {
    pub start: f64,
    pub end: f64,
    pub source: String,
    pub translation: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Term {
    pub src: String,
    pub tgt: String,
    pub note: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Glossary {
    pub theme: String,
    pub terms: Vec<Term>,
}

#[derive(Debug)]
pub(crate) struct Job {
    pub root: PathBuf,
    pub video: PathBuf,
    // File hình để ghép sau. Trống khi mới lấy phụ đề hoặc tiếng.
    pub picture: PathBuf,
    // Target language code, used in the names of the result files.
    pub target: String,
}

impl Job {
    pub(crate) fn open(video: &Path, output_dir: &Path) -> Result<Self, String> {
        if video.is_dir() {
            return Self::reopen(video);
        }
        if !video.is_file() {
            return Err("Chưa thấy file video.".into());
        }
        let ext = video
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !is_media_ext(&ext) {
            return Err("Chọn một file video.".into());
        }
        if !output_dir.is_dir() {
            return Err("Folder kết quả chưa có.".into());
        }
        let root = output_dir.join(folder_name(video));
        if root.as_path() == video {
            return Err("Folder kết quả trùng file video.".into());
        }
        require_same_source(&root, video)?;
        std::fs::create_dir_all(root.join("lines")).map_err(|error| error.to_string())?;
        let previous = read_json::<VideoSource>(&root.join("source.json")).unwrap_or_default();
        let picture = if is_picture_ext(&ext) {
            video.to_path_buf()
        } else {
            PathBuf::from(&previous.picture)
        };
        let job = Self {
            root,
            video: video.to_path_buf(),
            picture,
            target: "vi".into(),
        };
        if job.result() == job.video {
            return Err("Video kết quả trùng file gốc.".into());
        }
        write_source(&job)?;
        Ok(job)
    }

    // Việc chỉ audio: folder đã có source.json, file hình có thể chưa tải.
    pub(crate) fn reopen(root: &Path) -> Result<Self, String> {
        if !root.join("source.json").is_file() {
            return Err("Chưa thấy file video.".into());
        }
        let saved = read_json::<VideoSource>(&root.join("source.json"))?;
        let name = root
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("video");
        let video = if saved.video.is_empty() {
            root.join(format!("{name}.mp4"))
        } else {
            PathBuf::from(saved.video)
        };
        std::fs::create_dir_all(root.join("lines")).map_err(|error| error.to_string())?;
        Ok(Self {
            root: root.to_path_buf(),
            video,
            picture: PathBuf::from(saved.picture),
            target: "vi".into(),
        })
    }

    // Tạo folder việc từ link, chưa có file hình. Giữ link đã lưu nếu mở lại cùng tên.
    pub(crate) fn open_listen(output_dir: &Path, stem: &str, url: &str) -> Result<Self, String> {
        if !output_dir.is_dir() {
            return Err("Folder kết quả chưa có.".into());
        }
        let stem = folder_name(Path::new(stem));
        let root = output_dir.join(&stem);
        std::fs::create_dir_all(root.join("lines")).map_err(|error| error.to_string())?;
        let mut saved = read_json::<VideoSource>(&root.join("source.json")).unwrap_or_default();
        saved.url = url.to_string();
        write_json(&root.join("source.json"), &saved)?;
        Self::reopen(&root)
    }

    pub(crate) fn timing_log(&self) -> PathBuf {
        self.root.join("timing.log")
    }

    pub(crate) fn audio(&self) -> PathBuf {
        self.root.join("audio.wav")
    }

    pub(crate) fn transcript(&self) -> PathBuf {
        self.root.join("transcript.json")
    }

    // Target language and voice the intermediate files in this folder were made with.
    pub(crate) fn settings_path(&self) -> PathBuf {
        self.root.join("settings.json")
    }

    pub(crate) fn lines_path(&self) -> PathBuf {
        self.root.join("lines.json")
    }

    pub(crate) fn terms_path(&self) -> PathBuf {
        self.root.join("terms.json")
    }

    pub(crate) fn names_path(&self) -> PathBuf {
        self.root.join("names.json")
    }

    pub(crate) fn translation_path(&self) -> PathBuf {
        self.root.join("translation.json")
    }

    pub(crate) fn line_wav(&self, index: usize) -> PathBuf {
        line_wav_path(&self.root, index)
    }

    pub(crate) fn fit_report(&self) -> PathBuf {
        self.root.join("fit-report.json")
    }

    pub(crate) fn dub(&self) -> PathBuf {
        self.root.join("dub.wav")
    }

    // File audio để tải về, cùng tên với video kết quả nhưng là .m4a.
    pub(crate) fn spoken(&self) -> PathBuf {
        self.root
            .join(format!("{}.{}.m4a", folder_name(&self.video), self.target))
    }

    pub(crate) fn subtitles(&self) -> PathBuf {
        self.root
            .join(format!("{}.{}.srt", folder_name(&self.video), self.target))
    }

    pub(crate) fn result(&self) -> PathBuf {
        self.root
            .join(format!("{}.{}.mp4", folder_name(&self.video), self.target))
    }

    // Bản đã đốt chữ lồng tiếng lên hình. Bản không chữ vẫn là result().
    pub(crate) fn captioned(&self) -> PathBuf {
        captioned_path(&self.result())
    }
}

// Giọng đọc của câu thứ `index` (đếm từ 1) trong folder việc `root`.
fn line_wav_path(root: &Path, index: usize) -> PathBuf {
    root.join("lines").join(format!("{index:03}.wav"))
}

// File đốt chữ nằm cạnh video kết quả: `tap.vi.mp4` -> `tap.vi.chu.mp4`.
pub(crate) fn captioned_path(result: &Path) -> PathBuf {
    let stem = result
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("video");
    result.with_file_name(format!("{stem}.chu.mp4"))
}

// File đang ghi dở, hoặc bản có chữ: danh sách video không hiện hai lần.
pub(crate) fn is_extra_video(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    name.ends_with(".chu.mp4") || name.ends_with(".partial.mp4")
}

pub(crate) fn is_partial_audio(path: &Path) -> bool {
    path.file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .ends_with(".partial.m4a")
}

// Audio nối liền từ một video đã ghép. Không phải file kết quả, nên danh sách không hiện thêm một bản.
pub(crate) fn is_listen_audio(path: &Path) -> bool {
    path.file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .ends_with(".nghe.m4a")
}

pub(crate) const KEPT_DIR: &str = "cu";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct KeptVersion {
    pub path: String,
    pub language: String,
    pub modified_ms: u64,
    pub audio: bool,
}

// Chuyển mỗi video hoặc audio đã xuất vào một folder riêng trong cu/. File đang làm ở gốc giữ nguyên.
pub(crate) fn archive_exports(root: &Path) -> Result<usize, String> {
    let groups = export_groups(root);
    if groups.is_empty() {
        return Ok(0);
    }
    let kept = root.join(KEPT_DIR);
    std::fs::create_dir_all(&kept).map_err(|error| error.to_string())?;
    for group in &groups {
        let slot = next_kept_slot(&kept);
        std::fs::create_dir_all(&slot).map_err(|error| error.to_string())?;
        for path in group {
            let name = path
                .file_name()
                .ok_or_else(|| "Không thấy tên file kết quả.".to_string())?;
            std::fs::rename(path, slot.join(name)).map_err(|error| error.to_string())?;
        }
    }
    Ok(groups.len())
}

pub(crate) fn list_kept(root: &Path) -> Result<Vec<KeptVersion>, String> {
    let kept = root.join(KEPT_DIR);
    if !kept.is_dir() {
        return Ok(Vec::new());
    }
    let mut found = Vec::new();
    for entry in std::fs::read_dir(&kept).map_err(|error| error.to_string())? {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        let slot = entry.path();
        if !slot.is_dir() {
            continue;
        }
        let Some(path) = playable_file(&slot) else {
            continue;
        };
        let modified_ms = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|elapsed| elapsed.as_millis() as u64)
            .unwrap_or(0);
        let audio = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .eq_ignore_ascii_case("m4a");
        found.push(KeptVersion {
            path: path.to_string_lossy().into_owned(),
            language: result_language(&path),
            modified_ms,
            audio,
        });
    }
    found.sort_by(|left, right| {
        right
            .modified_ms
            .cmp(&left.modified_ms)
            .then_with(|| left.path.cmp(&right.path))
    });
    Ok(found)
}

// Chỉ xóa một folder con của cu/. Bản đang dùng và bản cũ khác không nằm trong folder đó.
pub(crate) fn delete_kept(output_dir: &Path, file: &Path) -> Result<(), String> {
    if !output_dir.is_dir() {
        return Err("Folder kết quả chưa có.".into());
    }
    if !file.is_file() {
        return Err("Không thấy bản này.".into());
    }
    let output =
        std::fs::canonicalize(output_dir).map_err(|_| "Folder kết quả chưa có.".to_string())?;
    let slot = file
        .parent()
        .ok_or_else(|| "Không thấy bản này.".to_string())?;
    let slot = std::fs::canonicalize(slot).map_err(|_| "Không thấy bản này.".to_string())?;
    let kept = slot
        .parent()
        .ok_or_else(|| "Chỉ xóa được bản đã cất.".to_string())?;
    let job = kept
        .parent()
        .ok_or_else(|| "Chỉ xóa được bản đã cất.".to_string())?;
    if kept.file_name().and_then(|value| value.to_str()) != Some(KEPT_DIR)
        || job.parent() != Some(output.as_path())
    {
        return Err("Chỉ xóa được bản đã cất.".into());
    }
    std::fs::remove_dir_all(&slot).map_err(|error| format!("Không xóa được: {error}"))
}

// Video đã có file kết quả đúng chế độ đang chọn thì trả đường dẫn đó. Chưa có thì trống.
pub(crate) fn finished_result(video: &Path, output_dir: &Path, text_only: bool) -> Option<String> {
    let root = if video.is_dir() {
        video.to_path_buf()
    } else if output_dir.is_dir() {
        output_dir.join(folder_name(video))
    } else {
        return None;
    };
    if !video.is_dir() {
        require_same_source(&root, video).ok()?;
    }
    result_for_mode(&root, text_only).map(|path| path.to_string_lossy().into_owned())
}

// Không thay source.json rồi tiếp tục dùng lời chép của một video khác cùng tên.
fn require_same_source(root: &Path, video: &Path) -> Result<(), String> {
    let path = root.join("source.json");
    if !path.exists() {
        return Ok(());
    }
    let saved: VideoSource = read_json(&path)?;
    let normalize = |path: &Path| {
        path.canonicalize()
            .unwrap_or_else(|_| path.to_path_buf())
            .to_string_lossy()
            .trim_start_matches(r"\\?\")
            .replace('/', "\\")
            .to_lowercase()
    };
    let wanted = normalize(video);
    if [&saved.video, &saved.picture]
        .iter()
        .any(|source| !source.is_empty() && normalize(Path::new(source)) == wanted)
    {
        return Ok(());
    }
    Err("Folder kết quả đã có video khác cùng tên. Chọn folder kết quả khác hoặc đổi tên video mới.".into())
}

fn next_kept_slot(kept: &Path) -> PathBuf {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis())
        .unwrap_or(0);
    let mut name = millis.to_string();
    let mut extra = 2u32;
    while kept.join(&name).exists() {
        name = format!("{millis}-{extra}");
        extra += 1;
    }
    kept.join(name)
}

fn export_groups(root: &Path) -> Vec<Vec<PathBuf>> {
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };
    let mut videos = Vec::new();
    let mut captions = Vec::new();
    let mut audios = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() || !file_ready(&path) {
            continue;
        }
        let ext = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if ext == "mp4" && !is_extra_video(&path) && !result_language(&path).is_empty() {
            videos.push(path);
        } else if is_caption_video(&path) && !result_language(&path).is_empty() {
            captions.push(path);
        } else if ext == "m4a" && !is_partial_audio(&path) && !result_language(&path).is_empty() {
            audios.push(path);
        }
    }
    videos.sort();
    audios.sort();
    let plains = videos.clone();
    let mut groups = Vec::new();
    let mut packed = Vec::new();
    for video in videos {
        let mut group = Vec::new();
        for side in sidecars(&video) {
            if side
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or("")
                .eq_ignore_ascii_case("m4a")
            {
                packed.push(side.clone());
            }
            group.push(side);
        }
        group.insert(0, video);
        groups.push(group);
    }
    for caption in captions {
        if plains.iter().any(|video| video == &plain_sibling(&caption)) {
            continue;
        }
        let stem = caption
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        let stem = stem.strip_suffix(".chu").unwrap_or(stem);
        let srt = caption.with_file_name(format!("{stem}.srt"));
        let mut group = vec![caption];
        if file_ready(&srt) {
            group.push(srt);
        }
        groups.push(group);
    }
    for audio in audios {
        if packed.iter().any(|path| path == &audio) {
            continue;
        }
        let stem = audio
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        let srt = audio.with_file_name(format!("{stem}.srt"));
        let mut group = vec![audio];
        if file_ready(&srt) {
            group.push(srt);
        }
        groups.push(group);
    }
    groups
}

fn sidecars(video: &Path) -> Vec<PathBuf> {
    let stem = video
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    let parent = video.parent().unwrap_or(Path::new(""));
    [
        captioned_path(video),
        parent.join(format!("{stem}.srt")),
        parent.join(format!("{stem}.m4a")),
    ]
    .into_iter()
    .filter(|path| file_ready(path))
    .collect()
}

fn plain_sibling(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("video.chu.mp4");
    let lower = name.to_ascii_lowercase();
    let stem = lower.strip_suffix(".chu.mp4").unwrap_or(&lower);
    path.with_file_name(format!("{}.mp4", &name[..stem.len()]))
}

// Chế độ chỉ chữ chỉ nhận file .chu đã làm bằng chế độ đó. Lồng tiếng không nhận file chỉ chữ.
fn result_for_mode(dir: &Path, text_only: bool) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut videos = Vec::new();
    let mut captions = Vec::new();
    let mut audios = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() || !file_ready(&path) {
            continue;
        }
        let ext = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let modified_ms = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|elapsed| elapsed.as_millis() as u64)
            .unwrap_or(0);
        if ext == "mp4" && !is_extra_video(&path) && !result_language(&path).is_empty() {
            videos.push((path, modified_ms));
        } else if is_caption_video(&path) && !result_language(&path).is_empty() {
            captions.push((path, modified_ms));
        } else if ext == "m4a" && !is_partial_audio(&path) && !is_listen_audio(&path) {
            audios.push((path, modified_ms));
        }
    }
    let saved_text = saved_text_only(dir);
    if text_only {
        if !saved_text && !videos.is_empty() {
            return None;
        }
        return preferred_video(dir, captions);
    }
    if saved_text {
        return None;
    }
    if !videos.is_empty() {
        return preferred_video(dir, videos);
    }
    pick_audio(dir, audios).map(|(path, _)| path)
}

fn saved_text_only(dir: &Path) -> bool {
    read_json::<SavedFlags>(&dir.join("settings.json"))
        .map(|saved| saved.text_only)
        .unwrap_or(false)
}

fn playable_file(dir: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut videos = Vec::new();
    let mut captions = Vec::new();
    let mut audios = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() || !file_ready(&path) {
            continue;
        }
        let ext = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let modified_ms = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|elapsed| elapsed.as_millis() as u64)
            .unwrap_or(0);
        if ext == "mp4" && !is_extra_video(&path) {
            videos.push((path, modified_ms));
        } else if is_caption_video(&path) && !result_language(&path).is_empty() {
            captions.push((path, modified_ms));
        } else if ext == "m4a" && !is_partial_audio(&path) && !is_listen_audio(&path) {
            audios.push((path, modified_ms));
        }
    }
    if !videos.is_empty() {
        let wanted = saved_target(dir);
        return videos
            .into_iter()
            .max_by(|left, right| {
                let left_match = !wanted.is_empty() && result_language(&left.0) == wanted;
                let right_match = !wanted.is_empty() && result_language(&right.0) == wanted;
                left_match
                    .cmp(&right_match)
                    .then(left.1.cmp(&right.1))
                    .then_with(|| left.0.cmp(&right.0))
            })
            .map(|(path, _)| path);
    }
    if !captions.is_empty() {
        return preferred_video(dir, captions);
    }
    pick_audio(dir, audios).map(|(path, _)| path)
}

fn preferred_video(dir: &Path, videos: Vec<(PathBuf, u64)>) -> Option<PathBuf> {
    let wanted = saved_target(dir);
    videos
        .into_iter()
        .max_by(|left, right| {
            let left_match = !wanted.is_empty() && result_language(&left.0) == wanted;
            let right_match = !wanted.is_empty() && result_language(&right.0) == wanted;
            left_match
                .cmp(&right_match)
                .then(left.1.cmp(&right.1))
                .then_with(|| left.0.cmp(&right.0))
        })
        .map(|(path, _)| path)
}

fn is_caption_video(path: &Path) -> bool {
    path.file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .ends_with(".chu.mp4")
}

fn is_media_ext(ext: &str) -> bool {
    matches!(
        ext,
        "mp4"
            | "mkv"
            | "mov"
            | "webm"
            | "avi"
            | "m4v"
            | "m4a"
            | "mp3"
            | "opus"
            | "ogg"
            | "wav"
            | "aac"
            | "flac"
    )
}

pub(crate) fn is_picture_ext(ext: &str) -> bool {
    matches!(ext, "mp4" | "mkv" | "mov" | "avi" | "m4v")
}

fn write_source(job: &Job) -> Result<(), String> {
    let mut saved = read_json::<VideoSource>(&job.root.join("source.json")).unwrap_or_default();
    saved.video = job.video.to_string_lossy().into_owned();
    if file_ready(&job.picture) {
        saved.picture = job.picture.to_string_lossy().into_owned();
    }
    write_json(&job.root.join("source.json"), &saved)
}

pub(crate) fn source_url(dir: &Path) -> String {
    read_json::<VideoSource>(&dir.join("source.json"))
        .map(|saved| saved.url)
        .unwrap_or_default()
}

pub(crate) fn saved_picture(dir: &Path) -> Option<PathBuf> {
    let saved = read_json::<VideoSource>(&dir.join("source.json")).ok()?;
    let path = PathBuf::from(saved.picture);
    let ext = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    (is_picture_ext(&ext) && file_ready(&path)).then_some(path)
}

// Ghi file tiếng đã tải, giữ link và file hình nếu đã có.
pub(crate) fn set_source_media(root: &Path, media: &Path) -> Result<(), String> {
    let mut saved = read_json::<VideoSource>(&root.join("source.json")).unwrap_or_default();
    saved.video = media.to_string_lossy().into_owned();
    write_json(&root.join("source.json"), &saved)
}

// Ghi file hình tải sau, giữ link và file tiếng.
pub(crate) fn set_source_picture(root: &Path, picture: &Path) -> Result<(), String> {
    let mut saved = read_json::<VideoSource>(&root.join("source.json")).unwrap_or_default();
    saved.picture = picture.to_string_lossy().into_owned();
    write_json(&root.join("source.json"), &saved)
}

pub(crate) fn folder_name(video: &Path) -> String {
    let stem = video
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("video");
    let mut name: String = stem
        .chars()
        .filter(|ch| !matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'))
        .collect();
    name = name.trim().trim_matches('.').to_string();
    if name.is_empty() {
        "video".into()
    } else {
        name
    }
}

pub(crate) fn file_ready(path: &Path) -> bool {
    std::fs::metadata(path)
        .map(|meta| meta.is_file() && meta.len() > 0)
        .unwrap_or(false)
}

pub(crate) fn write_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let text = serde_json::to_string_pretty(value).map_err(|error| error.to_string())?;
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, text).map_err(|error| error.to_string())?;
    std::fs::rename(&tmp, path).map_err(|error| error.to_string())?;
    Ok(())
}

#[derive(Serialize, Deserialize, Default)]
struct VideoSource {
    #[serde(default)]
    video: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    picture: String,
}

#[derive(Deserialize)]
struct SavedTarget {
    target: String,
}

#[derive(Deserialize, Default)]
struct SavedFlags {
    #[serde(default)]
    text_only: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FinishedDub {
    pub title: String,
    pub path: String,
    pub language: String,
    pub modified_ms: u64,
    // false: việc bị dừng giữa chừng. video là file gốc để bấm chạy tiếp, không tải lại.
    pub done: bool,
    pub video: String,
    // true: đã có bản đốt chữ cạnh file này.
    pub captioned: bool,
    // true: kết quả là file audio bản dịch, không có video đã ghép.
    pub audio: bool,
    // Link đã lưu. Trống với việc cũ hoặc file chọn trên máy.
    pub url: String,
}

// Video đã ghép xong trong folder kết quả. Bỏ qua thư mục tải về và file rỗng.
pub(crate) fn list_finished(output_dir: &Path) -> Result<Vec<FinishedDub>, String> {
    if !output_dir.is_dir() {
        return Err("Folder kết quả chưa có.".into());
    }
    let mut found = Vec::new();
    for entry in std::fs::read_dir(output_dir).map_err(|error| error.to_string())? {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let title = entry.file_name().to_string_lossy().to_string();
        if title.eq_ignore_ascii_case("tai-ve") {
            continue;
        }
        let files = match std::fs::read_dir(&dir) {
            Ok(files) => files,
            Err(_) => continue,
        };
        let mut videos = Vec::new();
        let mut captions = Vec::new();
        let mut audios = Vec::new();
        for file in files.flatten() {
            let path = file.path();
            let ext = path
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if !file_ready(&path) {
                continue;
            }
            let modified_ms = file
                .metadata()
                .and_then(|meta| meta.modified())
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|elapsed| elapsed.as_millis() as u64)
                .unwrap_or(0);
            if ext == "mp4" && !is_extra_video(&path) {
                videos.push((path, modified_ms));
            } else if is_caption_video(&path) && !result_language(&path).is_empty() {
                captions.push((path, modified_ms));
            } else if ext == "m4a" && !is_partial_audio(&path) && !is_listen_audio(&path) {
                audios.push((path, modified_ms));
            }
        }
        let listed = if !videos.is_empty() { videos } else { captions };
        if !listed.is_empty() {
            for (path, modified_ms) in listed {
                found.push(FinishedDub {
                    title: title.clone(),
                    path: path.to_string_lossy().into_owned(),
                    language: result_language(&path),
                    modified_ms,
                    done: true,
                    video: String::new(),
                    captioned: is_caption_video(&path) || file_ready(&captioned_path(&path)),
                    audio: false,
                    url: source_url(&dir),
                });
            }
            continue;
        }
        if let Some((path, modified_ms)) = pick_audio(&dir, audios) {
            found.push(FinishedDub {
                title: title.clone(),
                path: path.to_string_lossy().into_owned(),
                language: result_language(&path),
                modified_ms,
                done: true,
                video: String::new(),
                captioned: false,
                audio: true,
                url: source_url(&dir),
            });
            continue;
        }
        if !job_started(&dir) {
            continue;
        }
        let video = source_video(&dir, output_dir);
        found.push(FinishedDub {
            title,
            path: dir.to_string_lossy().into_owned(),
            language: saved_target(&dir),
            modified_ms: newest_ms(&dir),
            done: false,
            video: video
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_default(),
            captioned: false,
            audio: false,
            url: source_url(&dir),
        });
    }
    found.sort_by(|left, right| {
        right
            .modified_ms
            .cmp(&left.modified_ms)
            .then_with(|| left.title.cmp(&right.title))
    });
    Ok(found)
}

// Vài con số và câu đầu của một việc dở, để thẻ việc dở cho người dùng nhớ đó là video nào.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DraftBrief {
    pub heard: usize,
    pub translated: usize,
    pub spoken: usize,
    pub text_only: bool,
    pub excerpt: Vec<String>,
    pub excerpt_translated: bool,
}

// Chỉ cần vài câu để nhớ video nói gì, không phải đọc cả bài.
const BRIEF_LINES: usize = 3;

pub(crate) fn draft_brief(root: &Path) -> Result<DraftBrief, String> {
    if !job_started(root) {
        return Err("Chưa thấy việc này.".into());
    }
    let cues = |name: &str| read_json::<Vec<Cue>>(&root.join(name)).ok();
    let heard_lines = cues("lines.json")
        .or_else(|| cues("transcript.json"))
        .unwrap_or_default();
    let done: Vec<TranslatedLine> = read_json(&root.join("translation.json")).unwrap_or_default();
    // Câu đổi chữ bị xóa bản dịch, nên chỉ tính bản dịch còn chữ.
    let translated_text = first_texts(done.iter().map(|line| line.translation.as_str()));
    let translated = done
        .iter()
        .filter(|line| !line.translation.trim().is_empty())
        .count();
    let heard = heard_lines.len().max(done.len());
    let spoken = (1..=heard)
        .filter(|index| file_ready(&line_wav_path(root, *index)))
        .count();
    let (excerpt, excerpt_translated) = if translated_text.is_empty() {
        (
            first_texts(heard_lines.iter().map(|cue| cue.text.as_str())),
            false,
        )
    } else {
        (translated_text, true)
    };
    let text_only = read_json::<SavedFlags>(&root.join("settings.json"))
        .map(|saved| saved.text_only)
        .unwrap_or(false);
    Ok(DraftBrief {
        heard,
        translated,
        spoken,
        text_only,
        excerpt,
        excerpt_translated,
    })
}

fn first_texts<'a>(texts: impl Iterator<Item = &'a str>) -> Vec<String> {
    texts
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .take(BRIEF_LINES)
        .map(str::to_string)
        .collect()
}

// Folder việc của một dòng trong danh sách: file kết quả thì lấy folder cha, việc dở thì chính đường dẫn đó.
// Chỉ nhận folder con trực tiếp của folder kết quả. Không đụng file gốc và không đụng folder tải về.
pub(crate) fn job_folder(output_dir: &Path, item_path: &Path) -> Result<PathBuf, String> {
    if !output_dir.is_dir() {
        return Err("Folder kết quả chưa có.".into());
    }
    let output =
        std::fs::canonicalize(output_dir).map_err(|_| "Folder kết quả chưa có.".to_string())?;
    if !item_path.exists() {
        return Err("Không thấy mục này trong danh sách.".into());
    }
    let candidate = if item_path.is_dir() {
        item_path.to_path_buf()
    } else {
        item_path
            .parent()
            .ok_or_else(|| "Không thấy folder việc.".to_string())?
            .to_path_buf()
    };
    let root =
        std::fs::canonicalize(&candidate).map_err(|_| "Không thấy folder việc.".to_string())?;
    if root.parent() != Some(output.as_path()) {
        return Err("Chỉ xóa được việc nằm trong folder kết quả.".into());
    }
    let name = root
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    if name.is_empty() || name.eq_ignore_ascii_case("tai-ve") {
        return Err("Không xóa folder tải về.".into());
    }
    Ok(root)
}

pub(crate) fn delete_job(output_dir: &Path, item_path: &Path) -> Result<(), String> {
    let root = job_folder(output_dir, item_path)?;
    std::fs::remove_dir_all(&root).map_err(|error| format!("Không xóa được: {error}"))
}

// Lời đã dịch nằm cạnh file video, trong translation.json của cùng folder.
pub(crate) fn read_script(result: &Path) -> Result<Vec<TranslatedLine>, String> {
    if !result.is_file() {
        return Err("Chưa thấy file kết quả.".into());
    }
    let Some(dir) = result.parent() else {
        return Ok(Vec::new());
    };
    let path = dir.join("translation.json");
    if !file_ready(&path) {
        return Ok(Vec::new());
    }
    read_json(&path)
}

// Nhiều file .m4a thì lấy bản đúng ngôn ngữ đã lưu, rồi tới bản mới hơn.
fn pick_audio(dir: &Path, audios: Vec<(PathBuf, u64)>) -> Option<(PathBuf, u64)> {
    let wanted = saved_target(dir);
    audios.into_iter().max_by(|left, right| {
        let left_match = !wanted.is_empty() && result_language(&left.0) == wanted;
        let right_match = !wanted.is_empty() && result_language(&right.0) == wanted;
        left_match
            .cmp(&right_match)
            .then(left.1.cmp(&right.1))
            .then_with(|| left.0.cmp(&right.0))
    })
}

fn job_started(dir: &Path) -> bool {
    [
        "audio.wav",
        "transcript.json",
        "lines.json",
        "translation.json",
    ]
    .iter()
    .any(|name| file_ready(&dir.join(name)))
}

fn saved_target(dir: &Path) -> String {
    read_json::<SavedTarget>(&dir.join("settings.json"))
        .map(|saved| saved.target)
        .unwrap_or_default()
}

fn newest_ms(dir: &Path) -> u64 {
    std::fs::read_dir(dir)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            entry
                .metadata()
                .and_then(|meta| meta.modified())
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|elapsed| elapsed.as_millis() as u64)
        })
        .max()
        .unwrap_or(0)
}

// File gốc đã lưu khi bắt đầu lồng. Việc cũ chưa có source.json thì lấy file cùng tên trong tai-ve.
fn source_video(dir: &Path, output_dir: &Path) -> Option<PathBuf> {
    if let Ok(saved) = read_json::<VideoSource>(&dir.join("source.json")) {
        let path = PathBuf::from(saved.video);
        if file_ready(&path) {
            return Some(path);
        }
    }
    let name = dir.file_name()?.to_string_lossy();
    let downloaded = output_dir.join("tai-ve");
    ["mp4", "mkv", "mov", "webm", "avi", "m4v"]
        .iter()
        .find_map(|ext| {
            let path = downloaded.join(format!("{name}.{ext}"));
            file_ready(&path).then_some(path)
        })
}

fn result_language(path: &Path) -> String {
    media_language(path)
}

// `tap.vi.mp4` và `tap.vi.chu.mp4` cùng mã `vi`. File không có mã thì chuỗi rỗng.
pub(crate) fn media_language(path: &Path) -> String {
    let raw = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    let stem = raw.strip_suffix(".chu").unwrap_or(raw);
    match stem.rsplit_once('.') {
        Some((_, code)) if code.len() == 2 && code.chars().all(|ch| ch.is_ascii_alphabetic()) => {
            code.to_ascii_lowercase()
        }
        _ => String::new(),
    }
}

pub(crate) fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, String> {
    let text =
        std::fs::read_to_string(path).map_err(|_| format!("Không đọc được {}.", path.display()))?;
    let text = text.trim_start_matches('\u{feff}');
    serde_json::from_str(text).map_err(|_| format!("Không đọc được {}.", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("job-folder-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn folder_name_strips_illegal_characters() {
        assert_eq!(
            folder_name(Path::new(r"C:\phim\tap 1: demo?.mp4")),
            "tap 1 demo"
        );
        assert_eq!(folder_name(Path::new("...")), "video");
    }

    #[test]
    fn same_name_from_another_folder_never_replaces_source_or_reuses_result() {
        let dir = scratch("same-name");
        for sub in ["a", "b", "out"] {
            fs::create_dir_all(dir.join(sub)).unwrap();
        }
        let first = dir.join("a/clip.mp4");
        let other = dir.join("b/clip.mp4");
        fs::write(&first, b"a").unwrap();
        fs::write(&other, b"b").unwrap();
        let output = dir.join("out");
        let job = Job::open(&first, &output).unwrap();
        fs::write(job.transcript(), b"first transcript").unwrap();
        fs::write(job.result(), b"first result").unwrap();
        let source = fs::read(job.root.join("source.json")).unwrap();
        assert!(Job::open(&other, &output).unwrap_err().contains("cùng tên"));
        assert!(finished_result(&other, &output, false).is_none());
        assert_eq!(fs::read(job.root.join("source.json")).unwrap(), source);
        assert_eq!(fs::read(job.transcript()).unwrap(), b"first transcript");
        assert!(Job::open(&first, &output).is_ok());
        assert!(Job::open(&job.root, &output).is_ok());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn job_folder_uses_video_name_and_keeps_source() {
        let dir = scratch("open");
        let video = dir.join("tap 1 demo.mp4");
        fs::write(&video, b"video").unwrap();
        let output = dir.join("out");
        fs::create_dir_all(&output).unwrap();
        let job = Job::open(&video, &output).unwrap();
        assert_eq!(job.root, output.join("tap 1 demo"));
        assert!(job.root.join("lines").is_dir());
        assert_eq!(job.audio(), job.root.join("audio.wav"));
        assert_eq!(job.line_wav(1), job.root.join("lines").join("001.wav"));
        assert_eq!(job.line_wav(12), job.root.join("lines").join("012.wav"));
        assert_eq!(job.fit_report(), job.root.join("fit-report.json"));
        assert_eq!(job.dub(), job.root.join("dub.wav"));
        assert_eq!(job.spoken(), job.root.join("tap 1 demo.vi.m4a"));
        assert_eq!(job.result(), job.root.join("tap 1 demo.vi.mp4"));
        assert_eq!(job.subtitles(), job.root.join("tap 1 demo.vi.srt"));
        assert_eq!(job.captioned(), job.root.join("tap 1 demo.vi.chu.mp4"));
        assert_eq!(captioned_path(&job.result()), job.captioned());
        assert!(is_extra_video(&job.captioned()));
        assert!(!is_extra_video(&job.result()));
        assert_ne!(job.result(), video);
        assert!(file_ready(&video));
        assert!(!file_ready(&job.audio()));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_missing_video_and_non_video() {
        let dir = scratch("reject");
        let output = dir.join("out");
        fs::create_dir_all(&output).unwrap();
        let err = Job::open(&dir.join("khong-co.mp4"), &output).unwrap_err();
        assert_eq!(err, "Chưa thấy file video.");
        let text = dir.join("loi.txt");
        fs::write(&text, b"x").unwrap();
        let err = Job::open(&text, &output).unwrap_err();
        assert_eq!(err, "Chọn một file video.");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn listen_job_keeps_the_link_until_a_picture_is_saved() {
        let dir = scratch("listen");
        let output = dir.join("out");
        fs::create_dir_all(&output).unwrap();
        let job = Job::open_listen(&output, "Talk [abc]", "https://youtu.be/abcdefghijk").unwrap();
        assert_eq!(job.root, output.join("Talk [abc]"));
        assert_eq!(source_url(&job.root), "https://youtu.be/abcdefghijk");
        assert!(saved_picture(&job.root).is_none());
        let audio = output.join("tai-ve").join("Talk [abc].m4a");
        fs::create_dir_all(audio.parent().unwrap()).unwrap();
        fs::write(&audio, b"audio").unwrap();
        set_source_media(&job.root, &audio).unwrap();
        let again = Job::open(&job.root, &output).unwrap();
        assert_eq!(again.video, audio);
        assert_eq!(source_url(&again.root), "https://youtu.be/abcdefghijk");
        let picture = output.join("tai-ve").join("Talk [abc].mp4");
        fs::write(&picture, b"video").unwrap();
        set_source_picture(&job.root, &picture).unwrap();
        let merged = Job::reopen(&job.root).unwrap();
        assert_eq!(merged.picture, picture);
        assert_eq!(merged.video, audio);
        assert_eq!(merged.spoken(), job.root.join("Talk [abc].vi.m4a"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_finished_keeps_dubbed_videos_only() {
        let dir = scratch("list");
        let out = dir.join("out");
        let downloaded = out.join("tai-ve");
        fs::create_dir_all(&downloaded).unwrap();
        fs::write(downloaded.join("goc.mp4"), b"source").unwrap();
        let job = out.join("tap 1");
        fs::create_dir_all(&job).unwrap();
        fs::write(job.join("tap 1.vi.mp4"), b"dub").unwrap();
        fs::write(job.join("tap 1.vi.m4a"), b"audio").unwrap();
        fs::write(job.join("tap 1.vi.chu.mp4"), b"text").unwrap();
        fs::write(job.join("tap 1.vi.chu.partial.mp4"), b"part").unwrap();
        fs::write(job.join("tap 1.vi.partial.m4a"), b"part-audio").unwrap();
        fs::write(job.join("empty.mp4"), b"").unwrap();
        fs::write(job.join("note.txt"), b"x").unwrap();
        let found = list_finished(&out).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].title, "tap 1");
        assert_eq!(found[0].language, "vi");
        assert!(found[0].path.ends_with("tap 1.vi.mp4"));
        assert!(found[0].done);
        assert!(found[0].captioned);
        assert!(!found[0].audio);
        let paused = out.join("tap do");
        fs::create_dir_all(&paused).unwrap();
        fs::write(paused.join("translation.json"), b"{}").unwrap();
        fs::write(downloaded.join("tap do.mp4"), b"source").unwrap();
        let found = list_finished(&out).unwrap();
        let open = found.iter().find(|item| item.title == "tap do").unwrap();
        assert!(!open.done);
        assert!(!open.audio);
        assert!(open.video.ends_with("tap do.mp4"));
        let audio_only = out.join("chi audio");
        fs::create_dir_all(&audio_only).unwrap();
        fs::write(
            audio_only.join("settings.json"),
            br#"{"target":"vi","voice":"a","use_terms":false}"#,
        )
        .unwrap();
        fs::write(audio_only.join("chi audio.en.m4a"), b"old").unwrap();
        fs::write(audio_only.join("chi audio.vi.m4a"), b"spoken").unwrap();
        fs::write(audio_only.join("chi audio.vi.partial.m4a"), b"part").unwrap();
        let found = list_finished(&out).unwrap();
        let spoken = found.iter().find(|item| item.title == "chi audio").unwrap();
        assert!(spoken.done);
        assert!(spoken.audio);
        assert!(spoken.path.ends_with("chi audio.vi.m4a"));
        assert_eq!(spoken.language, "vi");
        let lines = vec![TranslatedLine {
            start: 0.0,
            end: 1.5,
            source: "Hi".into(),
            translation: "Chào".into(),
        }];
        write_json(&job.join("translation.json"), &lines).unwrap();
        let finished = found.iter().find(|item| item.title == "tap 1").unwrap();
        let script = read_script(Path::new(&finished.path)).unwrap();
        assert_eq!(script[0].translation, "Chào");
        assert!(list_finished(&dir.join("khong-co")).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    fn heard_cues(texts: &[&str]) -> Vec<Cue> {
        texts
            .iter()
            .enumerate()
            .map(|(index, text)| Cue {
                start: index as f64,
                end: index as f64 + 0.9,
                text: (*text).into(),
            })
            .collect()
    }

    fn translated_lines(pairs: &[(&str, &str)]) -> Vec<TranslatedLine> {
        pairs
            .iter()
            .enumerate()
            .map(|(index, (source, translation))| TranslatedLine {
                start: index as f64,
                end: index as f64 + 0.9,
                source: (*source).into(),
                translation: (*translation).into(),
            })
            .collect()
    }

    #[test]
    fn draft_brief_counts_only_what_each_stage_really_finished() {
        let dir = scratch("brief-counts");
        let root = dir.join("tap do");
        fs::create_dir_all(root.join("lines")).unwrap();
        write_json(
            &root.join("lines.json"),
            &heard_cues(&["một", "hai", "ba", "bốn", "năm"]),
        )
        .unwrap();
        // Câu đổi chữ bị xóa bản dịch (hoặc chỉ còn khoảng trắng) thì chưa tính là đã dịch.
        write_json(
            &root.join("translation.json"),
            &translated_lines(&[
                ("một", "one"),
                ("hai", ""),
                ("ba", "three"),
                ("bốn", "   "),
                ("năm", "five"),
            ]),
        )
        .unwrap();
        // 001 và 003 có tiếng, 002 rỗng, 007 nằm ngoài năm câu.
        fs::write(root.join("lines").join("001.wav"), b"wav").unwrap();
        fs::write(root.join("lines").join("002.wav"), b"").unwrap();
        fs::write(root.join("lines").join("003.wav"), b"wav").unwrap();
        fs::write(root.join("lines").join("007.wav"), b"wav").unwrap();
        let brief = draft_brief(&root).unwrap();
        assert_eq!(brief.heard, 5);
        assert_eq!(brief.translated, 3);
        assert_eq!(brief.spoken, 2);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn draft_brief_excerpt_is_the_first_translated_lines() {
        let dir = scratch("brief-excerpt");
        let root = dir.join("tap do");
        fs::create_dir_all(&root).unwrap();
        write_json(
            &root.join("lines.json"),
            &heard_cues(&["a", "b", "c", "d", "e"]),
        )
        .unwrap();
        write_json(
            &root.join("translation.json"),
            &translated_lines(&[
                ("a", "Xin chào"),
                ("b", ""),
                ("c", "  Tạm biệt  "),
                ("d", "Cảm ơn"),
                ("e", "Hẹn gặp lại"),
            ]),
        )
        .unwrap();
        let brief = draft_brief(&root).unwrap();
        assert_eq!(brief.excerpt, vec!["Xin chào", "Tạm biệt", "Cảm ơn"]);
        assert!(brief.excerpt_translated);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn draft_brief_excerpt_falls_back_to_the_heard_text_before_translation() {
        let dir = scratch("brief-heard");
        let root = dir.join("tap do");
        fs::create_dir_all(&root).unwrap();
        write_json(
            &root.join("lines.json"),
            &heard_cues(&["你好", " ", "再见"]),
        )
        .unwrap();
        let brief = draft_brief(&root).unwrap();
        assert_eq!((brief.heard, brief.translated), (3, 0));
        assert_eq!(brief.excerpt, vec!["你好", "再见"]);
        assert!(!brief.excerpt_translated);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn draft_brief_counts_the_whisper_transcript_before_lines_are_split() {
        let dir = scratch("brief-transcript");
        let root = dir.join("tap do");
        fs::create_dir_all(&root).unwrap();
        write_json(&root.join("transcript.json"), &heard_cues(&["một", "hai"])).unwrap();
        let brief = draft_brief(&root).unwrap();
        assert_eq!(brief.heard, 2);
        assert_eq!(brief.excerpt, vec!["một", "hai"]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn draft_brief_knows_a_text_only_job() {
        let dir = scratch("brief-text-only");
        let root = dir.join("tap do");
        fs::create_dir_all(&root).unwrap();
        write_json(&root.join("lines.json"), &heard_cues(&["một"])).unwrap();
        assert!(!draft_brief(&root).unwrap().text_only);
        fs::write(
            root.join("settings.json"),
            br#"{"target":"vi","voice":"a","use_terms":false,"text_only":true}"#,
        )
        .unwrap();
        assert!(draft_brief(&root).unwrap().text_only);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn draft_brief_of_a_job_that_only_extracted_audio_is_empty_but_valid() {
        let dir = scratch("brief-audio");
        let root = dir.join("tap do");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("audio.wav"), b"wav").unwrap();
        let brief = draft_brief(&root).unwrap();
        assert_eq!((brief.heard, brief.translated, brief.spoken), (0, 0, 0));
        assert!(brief.excerpt.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn draft_brief_refuses_a_folder_that_is_not_a_job() {
        let dir = scratch("brief-none");
        assert!(draft_brief(&dir.join("khong-co")).is_err());
        let empty = dir.join("trong");
        fs::create_dir_all(&empty).unwrap();
        fs::write(empty.join("ghi-chu.txt"), b"x").unwrap();
        assert!(draft_brief(&empty).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn delete_job_removes_the_work_folder_only() {
        let dir = scratch("delete");
        let out = dir.join("out");
        let downloaded = out.join("tai-ve");
        fs::create_dir_all(&downloaded).unwrap();
        let source = downloaded.join("tap 1.mp4");
        fs::write(&source, b"source").unwrap();
        let job = out.join("tap 1");
        fs::create_dir_all(job.join("lines")).unwrap();
        fs::write(job.join("tap 1.vi.mp4"), b"dub").unwrap();
        fs::write(job.join("tap 1.vi.m4a"), b"audio").unwrap();
        fs::write(job.join("translation.json"), b"[]").unwrap();
        let other = out.join("tap 2");
        fs::create_dir_all(&other).unwrap();
        fs::write(other.join("translation.json"), b"{}").unwrap();
        delete_job(&out, &job.join("tap 1.vi.mp4")).unwrap();
        assert!(!job.exists());
        assert!(source.is_file());
        assert!(downloaded.is_dir());
        assert!(other.join("translation.json").is_file());
        let paused = out.join("tap do");
        fs::create_dir_all(&paused).unwrap();
        fs::write(paused.join("lines.json"), b"[]").unwrap();
        delete_job(&out, &paused).unwrap();
        assert!(!paused.exists());
        assert!(delete_job(&out, &downloaded).is_err());
        assert!(downloaded.is_dir());
        assert!(delete_job(&out, &out).is_err());
        assert!(delete_job(&out, &source).is_err());
        assert!(source.is_file());
        let outside = dir.join("ngoai");
        fs::create_dir_all(&outside).unwrap();
        assert!(delete_job(&out, &outside).is_err());
        assert!(outside.is_dir());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn archive_moves_the_old_video_out_of_the_list() {
        let dir = scratch("archive");
        let out = dir.join("out");
        let job = out.join("tap 1");
        fs::create_dir_all(&job).unwrap();
        fs::write(job.join("transcript.json"), b"[{}]").unwrap();
        fs::write(job.join("translation.json"), b"[{\"source\":\"a\"}]").unwrap();
        fs::write(job.join("tap 1.vi.mp4"), b"viet").unwrap();
        fs::write(job.join("tap 1.vi.srt"), b"sub").unwrap();
        fs::write(job.join("tap 1.vi.chu.mp4"), b"chu").unwrap();
        fs::write(job.join("tap 1.vi.m4a"), b"audio").unwrap();
        fs::write(job.join("source.mp4"), b"goc").unwrap();
        assert_eq!(archive_exports(&job).unwrap(), 1);
        assert!(job.join("transcript.json").is_file());
        assert!(job.join("translation.json").is_file());
        assert!(job.join("source.mp4").is_file());
        assert!(!job.join("tap 1.vi.mp4").exists());
        let _ = fs::remove_file(job.join("source.mp4"));
        let kept = list_kept(&job).unwrap();
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].language, "vi");
        assert!(kept[0].path.ends_with("tap 1.vi.mp4"));
        assert!(Path::new(&kept[0].path)
            .parent()
            .unwrap()
            .join("tap 1.vi.srt")
            .is_file());
        assert!(Path::new(&kept[0].path)
            .parent()
            .unwrap()
            .join("tap 1.vi.chu.mp4")
            .is_file());
        fs::write(job.join("tap 1.en.mp4"), b"anh").unwrap();
        let found = list_finished(&out).unwrap();
        assert_eq!(found.len(), 1);
        assert!(found[0].path.ends_with("tap 1.en.mp4"));
        assert_eq!(found[0].language, "en");
        fs::write(job.join("tap 1.en.mp4"), b"anh-moi").unwrap();
        // Bản tiếng Anh đang dùng được cất riêng, không đè bản Việt đã có.
        let _ = fs::remove_file(job.join("tap 1.en.mp4"));
        fs::write(job.join("tap 1.fr.mp4"), b"phap").unwrap();
        assert_eq!(archive_exports(&job).unwrap(), 1);
        fs::write(job.join("tap 1.en.mp4"), b"dang-dung").unwrap();
        let kept = list_kept(&job).unwrap();
        assert_eq!(kept.len(), 2);
        let old = kept.iter().find(|item| item.language == "vi").unwrap();
        let french = kept.iter().find(|item| item.language == "fr").unwrap();
        delete_kept(&out, Path::new(&french.path)).unwrap();
        assert!(!Path::new(&french.path).exists());
        assert!(Path::new(&old.path).is_file());
        assert!(job.join("tap 1.en.mp4").is_file());
        assert!(delete_kept(&out, &job.join("tap 1.en.mp4")).is_err());
        assert!(job.join("tap 1.en.mp4").is_file());
        assert_eq!(
            finished_result(&out.join("tai-ve").join("tap 1.mp4"), &out, false).as_deref(),
            Some(job.join("tap 1.en.mp4").to_str().unwrap())
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_caption_only_file_is_the_finished_video() {
        let dir = scratch("chu-only");
        let out = dir.join("out");
        let job = out.join("tap 1");
        fs::create_dir_all(&job).unwrap();
        fs::write(job.join("tap 1.vi.chu.mp4"), b"text").unwrap();
        fs::write(job.join("tap 1.vi.chu.partial.mp4"), b"part").unwrap();
        let found = list_finished(&out).unwrap();
        assert_eq!(found.len(), 1);
        assert!(found[0].path.ends_with("tap 1.vi.chu.mp4"));
        assert_eq!(found[0].language, "vi");
        assert!(found[0].captioned);
        assert!(found[0].done);
        let video = out.join("tai-ve").join("tap 1.mp4");
        fs::create_dir_all(video.parent().unwrap()).unwrap();
        fs::write(&video, b"source").unwrap();
        let ready = finished_result(&video, &out, true).unwrap();
        assert!(ready.ends_with("tap 1.vi.chu.mp4"));
        assert!(finished_result(&video, &out, false).is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn finished_result_follows_the_requested_mode() {
        let dir = scratch("mode-result");
        let out = dir.join("out");
        let job = out.join("tap 1");
        fs::create_dir_all(&job).unwrap();
        fs::write(job.join("tap 1.vi.mp4"), b"dub").unwrap();
        fs::write(job.join("tap 1.vi.chu.mp4"), b"burned-dub").unwrap();
        fs::write(
            job.join("settings.json"),
            br#"{"target":"vi","voice":"a","use_terms":false,"text_only":false}"#,
        )
        .unwrap();
        let video = out.join("tai-ve").join("tap 1.mp4");
        fs::create_dir_all(video.parent().unwrap()).unwrap();
        fs::write(&video, b"source").unwrap();
        assert!(finished_result(&video, &out, true).is_none());
        let dubbed = finished_result(&video, &out, false).unwrap();
        assert!(dubbed.ends_with("tap 1.vi.mp4"));

        fs::write(
            job.join("settings.json"),
            br#"{"target":"vi","voice":"","use_terms":false,"text_only":true}"#,
        )
        .unwrap();
        let _ = fs::remove_file(job.join("tap 1.vi.mp4"));
        let text = finished_result(&video, &out, true).unwrap();
        assert!(text.ends_with("tap 1.vi.chu.mp4"));
        assert!(finished_result(&video, &out, false).is_none());
        fs::write(job.join("tap 1.vi.mp4"), b"stale").unwrap();
        assert!(finished_result(&video, &out, false).is_none());
        let still = finished_result(&video, &out, true).unwrap();
        assert!(still.ends_with("tap 1.vi.chu.mp4"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn archive_moves_a_caption_only_video() {
        let dir = scratch("archive-chu");
        let job = dir.join("tap 1");
        fs::create_dir_all(&job).unwrap();
        fs::write(job.join("tap 1.vi.chu.mp4"), b"text").unwrap();
        fs::write(job.join("tap 1.vi.srt"), b"sub").unwrap();
        fs::write(job.join("translation.json"), b"[]").unwrap();
        assert_eq!(archive_exports(&job).unwrap(), 1);
        assert!(!job.join("tap 1.vi.chu.mp4").exists());
        assert!(!job.join("tap 1.vi.srt").exists());
        assert!(job.join("translation.json").is_file());
        let kept = list_kept(&job).unwrap();
        assert_eq!(kept.len(), 1);
        assert!(kept[0].path.ends_with("tap 1.vi.chu.mp4"));
        assert_eq!(kept[0].language, "vi");
        assert!(Path::new(&kept[0].path)
            .parent()
            .unwrap()
            .join("tap 1.vi.srt")
            .is_file());
        let _ = fs::remove_dir_all(&dir);
    }
}
