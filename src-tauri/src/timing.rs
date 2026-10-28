use std::path::Path;
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::job::{read_json, write_json, TranslatedLine};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct TimeChange {
    pub index: usize,
    pub start: f64,
    pub end: f64,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Timing {
    version: u32,
    // Translation words may change independently (names, language, shortening).
    // The original ordered cue ranges identify the timeline, never edited WAV IDs.
    base: Vec<[f64; 2]>,
    edits: Vec<TimeChange>,
}

fn base(lines: &[TranslatedLine]) -> Vec<[f64; 2]> {
    lines.iter().map(|l| [l.start, l.end]).collect()
}

impl Timing {
    pub(crate) fn load(root: &Path, lines: &[TranslatedLine]) -> Result<Self, String> {
        let path = root.join("timing.json");
        let saved = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice::<Self>(&bytes).map_err(|_| {
                "Không đọc được mốc đã sửa trong timing.json. Giữ file để kiểm tra.".to_string()
            })?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Self {
                version: 1,
                base: base(lines),
                edits: Vec::new(),
            },
            Err(e) => return Err(e.to_string()),
        };
        if saved.version != 1 || saved.base != base(lines) {
            return Err("Mốc đã sửa không khớp lời chép hiện tại. Không tự ghép nhầm câu; cần kiểm tra timing.json.".into());
        }
        let mut seen = std::collections::HashSet::new();
        for edit in &saved.edits {
            if edit.index >= lines.len()
                || !seen.insert(edit.index)
                || !valid_time(edit.start, edit.end)
            {
                return Err("Mốc đã sửa trong timing.json không hợp lệ.".into());
            }
        }
        Ok(saved)
    }

    pub(crate) fn apply(&self, lines: &mut [TranslatedLine]) {
        for edit in &self.edits {
            lines[edit.index].start = edit.start;
            lines[edit.index].end = edit.end;
        }
    }

    pub(crate) fn set(&mut self, changes: &[TimeChange]) {
        for change in changes {
            self.edits.retain(|e| e.index != change.index);
            if self.base[change.index] != [change.start, change.end] {
                self.edits.push(change.clone());
            }
        }
        self.edits.sort_by_key(|e| e.index);
    }

    pub(crate) fn save(&self, root: &Path) -> Result<(), String> {
        write_json(&root.join("timing.json"), self)
    }

    pub(crate) fn restore_base(&self, lines: &mut [TranslatedLine]) {
        for (line, times) in lines.iter_mut().zip(&self.base) {
            line.start = times[0];
            line.end = times[1];
        }
    }
}

fn valid_time(start: f64, end: f64) -> bool {
    start.is_finite()
        && end.is_finite()
        && start >= 0.0
        && end - start >= 0.079999
        && end <= 3_600_000_000.0
}

pub(crate) fn apply(root: &Path, lines: &mut Vec<TranslatedLine>) -> Result<(), String> {
    let saved = Timing::load(root, lines)?;
    saved.apply(lines);
    crate::editing::apply(root, lines)?;
    Ok(())
}

pub(crate) fn read(root: &Path) -> Result<Vec<TranslatedLine>, String> {
    let mut lines: Vec<TranslatedLine> = read_json(&root.join("translation.json"))?;
    apply(root, &mut lines)?;
    Ok(lines)
}

// speak may shorten translated words after the edited timeline was applied.
// Persist words with BASE times, so the overlay remains idempotent on resume.
pub(crate) fn write_words(root: &Path, lines: &[TranslatedLine]) -> Result<(), String> {
    if root.join("editing.json").exists() { return crate::editing::write_lines(root, lines); }
    let mut next = lines.to_vec();
    let original: Vec<TranslatedLine> = read_json(&root.join("translation.json"))?;
    let saved = Timing::load(root, &original)?;
    if next.len() != original.len() {
        return Err("Số câu đã đổi khi lưu lời đọc.".into());
    }
    saved.restore_base(&mut next);
    write_json(&root.join("translation.json"), &next)
}

pub(crate) fn revision(root: &Path) -> Result<String, String> {
    let mut digest = Sha256::new();
    for name in [
        "translation.json",
        "timing.json",
        "editing.json",
        "lines.json",
        "settings.json",
        "render.json",
        "names.json",
        "source.json",
    ] {
        digest.update(name.as_bytes());
        match std::fs::read(root.join(name)) {
            Ok(bytes) => {
                digest.update([1]);
                digest.update((bytes.len() as u64).to_le_bytes());
                digest.update(bytes);
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => digest.update([0]),
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(format!("{:x}", digest.finalize()))
}

pub(crate) fn validate_changes(
    old: &[TranslatedLine],
    next: &[TranslatedLine],
    changes: &[TimeChange],
    duration: f64,
) -> Result<(), String> {
    if !duration.is_finite() || duration <= 0.0 {
        return Err("Không đọc được độ dài video.".into());
    }
    for change in changes {
        let i = change.index;
        if i >= next.len() || !valid_time(change.start, change.end) || change.end > duration + 0.001
        {
            return Err(format!(
                "Câu {}: chọn mốc trong video, dài ít nhất 0,08 giây.",
                i + 1
            ));
        }
        if (i > 0 && next[i].start < next[i - 1].start)
            || (i + 1 < next.len() && next[i].start > next[i + 1].start)
        {
            return Err(format!(
                "Câu {} không được di chuyển qua câu bên cạnh.",
                i + 1
            ));
        }
        for (j, other) in next.iter().enumerate() {
            if j == i {
                continue;
            }
            let overlap = (change.end.min(other.end) - change.start.max(other.start)).max(0.0);
            let before = (old[i].end.min(old[j].end) - old[i].start.max(old[j].start)).max(0.0);
            if overlap > before + 0.005 {
                return Err(format!(
                    "Câu {} tạo thêm phần chồng với câu {}. Chỉnh lại mốc trước khi xuất.",
                    i + 1,
                    j + 1
                ));
            }
        }
    }
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TimelineData {
    pub duration: f64,
    pub revision: String,
    pub peaks: Vec<f64>,
    pub waveform_source: &'static str,
    pub note: String,
}

#[derive(Serialize, Deserialize)]
struct WaveCache {
    version: u32,
    source: String,
    bytes: u64,
    modified: u128,
    duration: f64,
    peaks: Vec<f64>,
}

pub(crate) fn timeline_data(result: &Path, waveform: bool) -> Result<TimelineData, String> {
    if result
        .extension()
        .is_none_or(|e| !e.eq_ignore_ascii_case("mp4"))
    {
        return Err("Chỉnh mốc chỉ áp dụng cho video.".into());
    }
    let root = result.parent().ok_or("Không thấy folder việc.")?;
    let revision = revision(root)?;
    let cancel = AtomicBool::new(false);
    let duration = crate::pipeline::media_duration(result, &cancel)?;
    let job = crate::job::Job::reopen(root)?;
    let source = job
        .video
        .is_file()
        .then_some(job.video)
        .or_else(|| crate::job::saved_picture(root))
        .unwrap_or(result.to_path_buf());
    let waveform_source = if source == result {
        "bản xuất"
    } else {
        "nguồn"
    };
    if !waveform {
        return Ok(TimelineData {
            duration,
            revision,
            peaks: Vec::new(),
            waveform_source,
            note: String::new(),
        });
    }
    let meta = std::fs::metadata(&source).map_err(|e| e.to_string())?;
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let key = source.to_string_lossy().into_owned();
    let cache_path = root.join("waveform.json");
    let cached = read_json::<WaveCache>(&cache_path).ok().filter(|c| {
        c.version == 1
            && c.source == key
            && c.bytes == meta.len()
            && c.modified == modified
            && c.duration == duration
            && c.peaks.len() <= 2401
            && c.peaks
                .iter()
                .all(|p| p.is_finite() && (0.0..=1.0).contains(p))
    });
    let (peaks, note) = if let Some(cache) = cached {
        (cache.peaks, String::new())
    } else {
        let samples = ((duration * 8000.0 / 2400.0).ceil() as u64).max(1);
        let mut command = Command::new("ffmpeg");
        command.args(["-hide_banner", "-v", "error", "-i"]).arg(&source).args(["-vn", "-t", &duration.to_string(), "-af"])
            .arg(format!("aresample=8000,asetnsamples=n={samples}:p=0,astats=metadata=1:reset=1:measure_perchannel=none:measure_overall=Peak_level,ametadata=print:key=lavfi.astats.Overall.Peak_level:file=-"))
            .args(["-f", "null", "-"]);
        match crate::pipeline::run_tool_with(
            command,
            Duration::from_secs(30),
            &cancel,
            &mut |_, _| {},
        ) {
            Ok(text) => {
                let peaks: Vec<f64> = text
                    .lines()
                    .filter_map(|l| l.strip_prefix("lavfi.astats.Overall.Peak_level="))
                    .take(2401)
                    .map(|s| {
                        s.parse::<f64>()
                            .ok()
                            .filter(|x| x.is_finite())
                            .map(|db| 10f64.powf(db / 20.0).clamp(0.0, 1.0))
                            .unwrap_or(0.0)
                    })
                    .collect();
                let note = if peaks.is_empty() {
                    "Không có sóng âm; vẫn chỉnh mốc được.".into()
                } else {
                    String::new()
                };
                let _ = write_json(
                    &cache_path,
                    &WaveCache {
                        version: 1,
                        source: key,
                        bytes: meta.len(),
                        modified,
                        duration,
                        peaks: peaks.clone(),
                    },
                );
                (peaks, note)
            }
            Err(_) => (
                Vec::new(),
                "Chưa lấy được sóng âm; vẫn chỉnh mốc được. Mỗi lần thử tối đa 30 giây.".into(),
            ),
        }
    };
    if self::revision(root)? != revision {
        return Err("Dữ liệu vừa thay đổi. Mở lại phần chỉnh mốc.".into());
    }
    Ok(TimelineData {
        duration,
        revision,
        peaks,
        waveform_source,
        note,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (std::path::PathBuf, Vec<TranslatedLine>) {
        let root = std::env::temp_dir().join(format!(
            "timing-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let lines = (0..3)
            .map(|i| TranslatedLine {
                start: i as f64 * 2.0,
                end: i as f64 * 2.0 + 1.0,
                source: format!("source {i}"),
                translation: format!("câu {i}"),
            })
            .collect::<Vec<_>>();
        write_json(&root.join("translation.json"), &lines).unwrap();
        (root, lines)
    }

    #[test]
    fn overlay_survives_word_changes_without_mutating_the_original_timeline() {
        let (root, lines) = fixture();
        let original = std::fs::read(root.join("translation.json")).unwrap();
        let mut saved = Timing::load(&root, &lines).unwrap();
        saved.set(&[TimeChange {
            index: 1,
            start: 1.5,
            end: 3.5,
        }]);
        saved.save(&root).unwrap();
        let effective = read(&root).unwrap();
        assert_eq!((effective[1].start, effective[1].end), (1.5, 3.5));
        assert_eq!(
            std::fs::read(root.join("translation.json")).unwrap(),
            original
        );
        let mut renamed = effective.clone();
        renamed[1].translation = "Tên đã sửa".into();
        write_words(&root, &renamed).unwrap();
        let base: Vec<TranslatedLine> = read_json(&root.join("translation.json")).unwrap();
        assert_eq!((base[1].start, base[1].end), (2.0, 3.0));
        let resumed = read(&root).unwrap();
        assert_eq!(resumed[1].translation, "Tên đã sửa");
        assert_eq!((resumed[1].start, resumed[1].end), (1.5, 3.5));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn incompatible_corrupt_or_unknown_timing_is_reported_not_guessed() {
        let (root, lines) = fixture();
        let mut saved = Timing::load(&root, &lines).unwrap();
        saved.set(&[TimeChange {
            index: 1,
            start: 1.5,
            end: 3.5,
        }]);
        saved.save(&root).unwrap();
        let mut changed = lines.clone();
        changed[1].start = 2.1;
        assert!(Timing::load(&root, &changed).is_err());
        saved.version = 2;
        saved.save(&root).unwrap();
        assert!(read(&root).is_err());
        std::fs::write(root.join("timing.json"), b"broken").unwrap();
        assert!(read(&root).is_err());
        assert_eq!(std::fs::read(root.join("timing.json")).unwrap(), b"broken");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn new_overlap_reorder_and_outside_video_are_rejected_together() {
        let (root, old) = fixture();
        for (start, end) in [
            (f64::NAN, 3.0),
            (-1.0, 3.0),
            (2.0, 2.01),
            (2.0, 6.1),
            (0.5, 3.0),
            (4.1, 5.5),
        ] {
            let mut next = old.clone();
            next[1].start = start;
            next[1].end = end;
            assert!(validate_changes(
                &old,
                &next,
                &[TimeChange {
                    index: 1,
                    start,
                    end
                }],
                6.0
            )
            .is_err());
        }
        let mut next = old.clone();
        next[1].start = 1.5;
        next[1].end = 3.5;
        assert!(validate_changes(
            &old,
            &next,
            &[TimeChange {
                index: 1,
                start: 1.5,
                end: 3.5
            }],
            6.0
        )
        .is_ok());
        // Atomic edits may share a new boundary; validation sees the whole draft.
        next[0].end = 1.5;
        assert!(validate_changes(
            &old,
            &next,
            &[
                TimeChange {
                    index: 0,
                    start: 0.0,
                    end: 1.5
                },
                TimeChange {
                    index: 1,
                    start: 1.5,
                    end: 3.5
                }
            ],
            6.0
        )
        .is_ok());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn revision_detects_word_configuration_and_timing_changes() {
        let (root, lines) = fixture();
        let initial = revision(&root).unwrap();
        assert_eq!(initial, revision(&root).unwrap());
        write_json(&root.join("render.json"), &serde_json::json!({"size": 4.2})).unwrap();
        assert_ne!(initial, revision(&root).unwrap());
        let configured = revision(&root).unwrap();
        Timing::load(&root, &lines).unwrap().save(&root).unwrap();
        assert_ne!(configured, revision(&root).unwrap());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn local_waveform_is_cached_and_silent_video_still_has_editable_metadata() {
        if crate::program_path("ffmpeg").is_none() || crate::program_path("ffprobe").is_none() {
            return;
        }
        let (dir, _) = fixture();
        for silent in [false, true] {
            let source = dir.join(if silent { "silent.mp4" } else { "sound.mp4" });
            let mut command = Command::new("ffmpeg");
            command.args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=c=black:s=160x90:r=10:d=1",
            ]);
            if !silent {
                command.args(["-f", "lavfi", "-i", "sine=frequency=440:duration=1"]);
            }
            assert!(command
                .args(["-c:v", "libx264", "-pix_fmt", "yuv420p", "-t", "1"])
                .arg(&source)
                .status()
                .unwrap()
                .success());
            let job = crate::job::Job::open(&source, &dir).unwrap();
            std::fs::copy(&source, job.result()).unwrap();
            write_json(
                &job.translation_path(),
                &vec![TranslatedLine {
                    start: 0.0,
                    end: 0.9,
                    source: "Hello".into(),
                    translation: "Xin chào".into(),
                }],
            )
            .unwrap();
            let data = timeline_data(&job.result(), false).unwrap();
            assert!(data.duration > 0.9 && data.duration < 1.2);
            assert!(data.peaks.is_empty());
            assert!(!job.root.join("waveform.json").exists());
            let wave = timeline_data(&job.result(), true).unwrap();
            assert_eq!(wave.revision, data.revision);
            if silent {
                assert!(wave.peaks.is_empty());
                assert!(!wave.note.is_empty());
                // Downloaded jobs may keep their source audio separately from
                // a silent picture. Prefer that audio instead of the picture.
                crate::job::set_source_media(&job.root, &dir.join("sound.mp4")).unwrap();
                assert!(!timeline_data(&job.result(), true).unwrap().peaks.is_empty());
            } else {
                assert!(!wave.peaks.is_empty() && wave.peaks.len() <= 2401);
                assert!(wave.peaks.iter().any(|p| *p > 0.01));
                let cache = job.root.join("waveform.json");
                let before = std::fs::metadata(&cache).unwrap().modified().unwrap();
                let cached = timeline_data(&job.result(), true).unwrap();
                assert_eq!(cached.peaks.len(), wave.peaks.len());
                assert!(
                    cached
                        .peaks
                        .iter()
                        .zip(&wave.peaks)
                        .all(|(a, b)| (a - b).abs() < 1e-10),
                    "cached amplitude changed beyond JSON roundtrip precision"
                );
                assert_eq!(
                    std::fs::metadata(&cache).unwrap().modified().unwrap(),
                    before
                );
            }
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
