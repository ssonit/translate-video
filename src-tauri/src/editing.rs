use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::job::{read_json, write_json, Job, TranslatedLine};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct EditedCue {
    pub id: String,
    pub sources: Vec<usize>,
    pub slot: usize,
    pub line: TranslatedLine,
    #[serde(default = "legacy_authored")]
    pub authored: bool,
    #[serde(default)]
    pub keep_speech: bool,
}

fn legacy_authored() -> bool { true }

#[derive(Clone, Serialize, Deserialize)]
struct DraftUndo {
    cues: Vec<EditedCue>,
    history: Vec<Change>,
    generation_basis: Option<String>,
    #[serde(default)]
    after: Option<Vec<EditedCue>>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Change {
    before: Vec<EditedCue>,
    after: Vec<EditedCue>,
    #[serde(default)]
    speech: String,
}

fn speech_signature(root: &Path) -> String {
    let settings: serde_json::Value = read_json(&root.join("settings.json")).unwrap_or_default();
    let render: serde_json::Value = read_json(&root.join("render.json")).unwrap_or_default();
    let names: serde_json::Value = read_json(&root.join("names.json")).unwrap_or_default();
    let bytes = serde_json::to_vec(&(
        &settings["target"],
        &settings["voice"],
        &render["pronunciations"],
        names,
    ))
    .unwrap_or_default();
    format!("{:x}", Sha256::digest(bytes))
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Editing {
    version: u32,
    basis: String,
    next: usize,
    pub cues: Vec<EditedCue>,
    history: Vec<Change>,
    #[serde(default)]
    word_history: Vec<DraftUndo>,
    #[serde(default)]
    pub generation_basis: Option<String>,
}

#[derive(Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub(crate) enum Structure {
    Split {
        revision: String,
        id: String,
        at: f64,
        left_source: String,
        left_text: String,
        right_source: String,
        right_text: String,
    },
    Merge {
        revision: String,
        id: String,
        next_id: String,
        source: String,
        text: String,
    },
    Undo {
        revision: String,
    },
}

impl Structure {
    pub(crate) fn revision(&self) -> &str {
        match self {
            Self::Split { revision, .. }
            | Self::Merge { revision, .. }
            | Self::Undo { revision } => revision,
        }
    }
}

fn basis(lines: &[TranslatedLine]) -> Result<String, String> {
    let cues: Vec<_> = lines.iter().map(|l| (l.start, l.end, &l.source)).collect();
    let bytes = serde_json::to_vec(&cues).map_err(|e| e.to_string())?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

pub(crate) fn legacy_id(index: usize) -> String {
    format!("cue-{:06}", index + 1)
}

impl Editing {
    pub(crate) fn load(root: &Path) -> Result<Option<Self>, String> {
        let bytes = match std::fs::read(root.join("editing.json")) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.to_string()),
        };
        let doc: Self = serde_json::from_slice(&bytes).map_err(|_| {
            "Không đọc được editing.json. Giữ file để kiểm tra, không đoán file tiếng.".to_string()
        })?;
        let base: Vec<TranslatedLine> = read_json(&root.join("translation.json"))?;
        doc.validate(&base)?;
        Ok(Some(doc))
    }

    fn validate(&self, base: &[TranslatedLine]) -> Result<(), String> {
        if self.version != 1
            || self.basis != basis(base)?
            || self.cues.is_empty()
            || self.cues.len() > 10000
            || self.history.len() > 20
            || self.word_history.len() > 20
        {
            return Err("Cấu trúc câu đã sửa không khớp bản dịch gốc hoặc phiên bản không hỗ trợ. Không tự ghép nhầm tiếng.".into());
        }
        let mut ids = HashSet::new();
        let mut slots = HashSet::new();
        for cue in &self.cues {
            if !ids.insert(&cue.id)
                || !slots.insert(cue.slot)
                || cue.id.is_empty()
                || cue.id.len() > 80
                || cue.slot == 0
                || cue.slot >= self.next
                || cue.sources.is_empty()
                || cue.sources.iter().any(|i| *i >= base.len())
                || !cue.line.start.is_finite()
                || !cue.line.end.is_finite()
                || cue.line.start < 0.0
                || cue.line.end <= cue.line.start
            {
                return Err(
                    "ID, liên kết lời gốc hoặc file tiếng trong editing.json không hợp lệ.".into(),
                );
            }
        }
        // Undo snapshots can become live again, so validate their paths/IDs too.
        for change in &self.history {
            if change.before.is_empty()
                || change.after.is_empty()
                || change.before.len() > 2
                || change.after.len() > 2
            {
                return Err("Lịch sử tách/gộp trong editing.json không hợp lệ.".into());
            }
            for cue in change.before.iter().chain(&change.after) {
                if cue.slot == 0
                    || cue.slot >= self.next
                    || cue.sources.is_empty()
                    || cue.sources.iter().any(|i| *i >= base.len())
                    || cue.id.is_empty()
                    || cue.id.len() > 80
                    || !cue.line.start.is_finite()
                    || !cue.line.end.is_finite()
                    || cue.line.start < 0.0
                    || cue.line.end <= cue.line.start
                {
                    return Err("Lịch sử tách/gộp trong editing.json không hợp lệ.".into());
                }
            }
        }
        Ok(())
    }

    pub(crate) fn initial(
        root: &Path,
        base: &[TranslatedLine],
        effective: &[TranslatedLine],
    ) -> Result<Self, String> {
        let mut next = base.len() + 1;
        if let Ok(entries) = std::fs::read_dir(root.join("lines")) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("wav"))
                {
                    if let Some(number) = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .and_then(|s| s.parse::<usize>().ok())
                    {
                        next = next.max(number.checked_add(1).ok_or("Số file tiếng quá lớn.")?);
                    }
                }
            }
        }
        let authored = crate::text_cache::authored_flags(root, base)?;
        Ok(Self {
            version: 1,
            basis: basis(base)?,
            next,
            cues: effective
                .iter()
                .enumerate()
                .map(|(i, l)| EditedCue {
                    id: legacy_id(i),
                    sources: vec![i],
                    slot: i + 1,
                    line: l.clone(),
                    authored: authored.get(i).copied().unwrap_or(false),
                    keep_speech: false,
                })
                .collect(),
            history: Vec::new(),
            word_history: Vec::new(),
            generation_basis: crate::text_cache::generation_basis(root)?,
        })
    }

    pub(crate) fn lines(&self) -> Vec<TranslatedLine> {
        self.cues.iter().map(|c| c.line.clone()).collect()
    }

    pub(crate) fn remember_draft(&mut self) {
        self.word_history.push(DraftUndo { cues: self.cues.clone(), history: self.history.clone(), generation_basis: self.generation_basis.clone(), after: None });
        if self.word_history.len() > 20 { self.word_history.remove(0); }
    }

    pub(crate) fn undo_draft(&mut self) -> Result<(), String> {
        if !self.can_undo_draft() { return Err("Nháp đã có thay đổi tiếp theo; không ghi đè để hoàn tác.".into()); }
        let old = self.word_history.pop().ok_or("Chưa có thay đổi nháp để hoàn tác.")?;
        self.cues = old.cues;
        self.history = old.history;
        self.generation_basis = old.generation_basis;
        Ok(())
    }

    pub(crate) fn can_undo_draft(&self) -> bool { self.word_history.last().is_some_and(|old|old.after.as_ref()==Some(&self.cues)) }

    pub(crate) fn clear_draft_history(&mut self) { self.word_history.clear(); self.history.clear(); }
    pub(crate) fn can_undo(&self) -> bool {
        self.history.last().is_some_and(|change| {
            self.cues
                .iter()
                .position(|c| c.id == change.after[0].id)
                .is_some_and(|index| self.matches(index, &change.after))
        })
    }
    pub(crate) fn undo_blocked(&self) -> bool {
        !self.history.is_empty() && !self.can_undo()
    }
    fn matches(&self, index: usize, after: &[EditedCue]) -> bool {
        self.cues
            .get(index..index + after.len())
            .is_some_and(|cues| {
                cues.iter()
                    .zip(after)
                    .all(|(a, b)| a.id == b.id && a.sources == b.sources && a.line == b.line)
            })
    }

    pub(crate) fn save(&self, root: &Path) -> Result<(), String> {
        let base: Vec<TranslatedLine> = read_json(&root.join("translation.json"))?;
        self.validate(&base)?;
        let mut next = self.clone();
        if let Some(last)=next.word_history.last_mut().filter(|last|last.after.is_none()) {last.after=Some(next.cues.clone());}
        write_json(&root.join("editing.json"), &next)?;
        invalidate(root);
        Ok(())
    }

    fn allocate(
        &mut self,
        root: &Path,
        sources: Vec<usize>,
        line: TranslatedLine,
    ) -> Result<EditedCue, String> {
        while root
            .join("lines")
            .join(format!("{:03}.wav", self.next))
            .exists()
        {
            self.next = self.next.checked_add(1).ok_or("Hết số file tiếng.")?;
        }
        let slot = self.next;
        self.next = self.next.checked_add(1).ok_or("Hết số file tiếng.")?;
        let authored=!line.translation.trim().is_empty();
        Ok(EditedCue {
            id: format!("edit-{slot}"),
            sources,
            slot,
            line,
            authored,
            keep_speech: false,
        })
    }

    pub(crate) fn restructure(
        &mut self,
        root: &Path,
        action: &Structure,
        duration: f64,
    ) -> Result<(), String> {
        if !duration.is_finite() || duration <= 0.0 {
            return Err("Không đọc được độ dài video.".into());
        }
        let clean = |s: &str| -> Result<String, String> {
            let text = s.trim();
            if text.is_empty() || text.chars().count() > 4000 {
                return Err("Mỗi phần cần có lời gốc và lời đọc, tối đa 4.000 ký tự.".into());
            }
            Ok(text.to_string())
        };
        let clean_read = |s: &str| -> Result<String,String> {
            if s.trim().chars().count()>4000 {return Err("Lời đọc mỗi phần tối đa 4.000 ký tự.".into());}
            Ok(s.trim().into())
        };
        let (index, before, after) = match action {
            Structure::Split {
                id,
                at,
                left_source,
                left_text,
                right_source,
                right_text,
                ..
            } => {
                if self.cues.len() >= 10000 {
                    return Err("Đã đạt giới hạn 10.000 câu biên tập.".into());
                }
                let index = self
                    .cues
                    .iter()
                    .position(|c| &c.id == id)
                    .ok_or("Câu được chọn không còn tồn tại.")?;
                let cue = self.cues[index].clone();
                if !at.is_finite()
                    || at - cue.line.start < 0.079999
                    || cue.line.end - at < 0.079999
                    || cue.line.end > duration + 0.001
                {
                    return Err(
                        "Đặt điểm tách trong câu, để mỗi phần dài ít nhất 0,08 giây.".into(),
                    );
                }
                let left = self.allocate(
                    root,
                    cue.sources.clone(),
                    TranslatedLine {
                        start: cue.line.start,
                        end: *at,
                        source: clean(left_source)?,
                        translation: clean_read(left_text)?,
                    },
                )?;
                let right = self.allocate(
                    root,
                    cue.sources.clone(),
                    TranslatedLine {
                        start: *at,
                        end: cue.line.end,
                        source: clean(right_source)?,
                        translation: clean_read(right_text)?,
                    },
                )?;
                (index, vec![cue], vec![left, right])
            }
            Structure::Merge {
                id,
                next_id,
                source,
                text,
                ..
            } => {
                let index = self
                    .cues
                    .iter()
                    .position(|c| &c.id == id)
                    .ok_or("Câu được chọn không còn tồn tại.")?;
                let left = self.cues[index].clone();
                let right = self
                    .cues
                    .get(index + 1)
                    .filter(|c| &c.id == next_id)
                    .cloned()
                    .ok_or("Chỉ gộp được hai câu liền nhau hiện tại.")?;
                let gap = right.line.start - left.line.end;
                if !(-0.005..=2.0).contains(&gap) || right.line.end > duration + 0.001 {
                    return Err("Chỉ gộp câu không chồng mốc và cách nhau tối đa 2 giây. Khoảng nghỉ được giữ trong khoảng câu mới.".into());
                }
                let mut sources = left.sources.clone();
                for i in &right.sources {
                    if !sources.contains(i) {
                        sources.push(*i);
                    }
                }
                let merged = self.allocate(
                    root,
                    sources,
                    TranslatedLine {
                        start: left.line.start,
                        end: right.line.end,
                        source: clean(source)?,
                        translation: clean_read(text)?,
                    },
                )?;
                (index, vec![left, right], vec![merged])
            }
            Structure::Undo { .. } => {
                let change = self
                    .history
                    .last()
                    .ok_or("Chưa có thao tác tách/gộp để hoàn tác.")?;
                let index = self
                    .cues
                    .iter()
                    .position(|c| c.id == change.after[0].id)
                    .ok_or("Các câu đã thay đổi, không thể hoàn tác an toàn.")?;
                let matches = self.matches(index, &change.after);
                if !matches {
                    return Err("Câu này đã sửa lời hoặc mốc sau khi tách/gộp. Giữ thay đổi hiện tại, không tự ghi đè khi hoàn tác.".into());
                }
                let mut change = self.history.pop().unwrap();
                let names: crate::names::NameList =
                    read_json(&root.join("names.json")).unwrap_or_default();
                let fixes = crate::names::usable_names(&names.names);
                for cue in &mut change.before {
                    cue.line.source = crate::names::apply_names(&cue.line.source, &fixes);
                    cue.line.translation = crate::names::apply_names(&cue.line.translation, &fixes);
                }
                if change.speech != speech_signature(root) {
                    for cue in &mut change.before {
                        let allocated =
                            self.allocate(root, cue.sources.clone(), cue.line.clone())?;
                        cue.slot = allocated.slot; // Same logical ID, fresh speech recipe.
                    }
                }
                self.cues
                    .splice(index..index + change.after.len(), change.before);
                return Ok(());
            }
        };
        self.cues.splice(index..index + before.len(), after.clone());
        self.history.push(Change {
            before,
            after,
            speech: speech_signature(root),
        });
        if self.history.len() > 20 {
            self.history.remove(0);
        }
        Ok(())
    }
}

pub(crate) fn apply(root: &Path, lines: &mut Vec<TranslatedLine>) -> Result<(), String> {
    if let Some(doc) = Editing::load(root)? {
        *lines = doc.lines();
    }
    Ok(())
}

pub(crate) fn write_lines(root: &Path, lines: &[TranslatedLine]) -> Result<(), String> {
    let mut doc = Editing::load(root)?.ok_or("Chưa có cấu trúc câu đã sửa.")?;
    if lines.len() != doc.cues.len() {
        return Err("Số câu biên tập đã thay đổi.".into());
    }
    for (cue, line) in doc.cues.iter_mut().zip(lines) {
        cue.line = line.clone();
    }
    doc.save(root)
}

type SlotCache = HashMap<PathBuf, (SystemTime, u64, SystemTime, u64, Vec<usize>)>;
static SLOTS: OnceLock<Mutex<SlotCache>> = OnceLock::new();
fn cache() -> &'static Mutex<SlotCache> {
    SLOTS.get_or_init(|| Mutex::new(HashMap::new()))
}
pub(crate) fn invalidate(root: &Path) {
    cache()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .remove(root);
}

pub(crate) fn slot(root: &Path, number: usize) -> Result<usize, String> {
    let meta = match std::fs::metadata(root.join("editing.json")) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(number),
        Err(e) => return Err(e.to_string()),
    };
    let stamp = meta.modified().map_err(|e| e.to_string())?;
    let base = std::fs::metadata(root.join("translation.json")).map_err(|e| e.to_string())?;
    let base_stamp = base.modified().map_err(|e| e.to_string())?;
    {
        let saved = cache().lock().unwrap_or_else(|p| p.into_inner());
        if let Some((time, len, bt, bl, slots)) = saved.get(root) {
            if *time == stamp && *len == meta.len() && *bt == base_stamp && *bl == base.len() {
                return slots
                    .get(number.wrapping_sub(1))
                    .copied()
                    .ok_or("Không có ID câu để tìm tiếng.".into());
            }
        }
    }
    let doc = Editing::load(root)?.ok_or("Thiếu cấu trúc câu.")?;
    let slots: Vec<_> = doc.cues.iter().map(|c| c.slot).collect();
    let result = slots
        .get(number.wrapping_sub(1))
        .copied()
        .ok_or("Không có ID câu để tìm tiếng.".to_string());
    let mut saved = cache().lock().unwrap_or_else(|p| p.into_inner());
    if saved.len() >= 32 {
        saved.clear();
    }
    saved.insert(
        root.to_path_buf(),
        (stamp, meta.len(), base_stamp, base.len(), slots),
    );
    result
}

pub(crate) fn wav_path(root: &Path, number: usize) -> PathBuf {
    match slot(root, number) {
        Ok(slot) => root.join("lines").join(format!("{slot:03}.wav")),
        // Never silently use an ordinal WAV when the editing metadata is broken.
        Err(_) => root
            .join("lines")
            .join(format!("invalid-editing-{number}.wav")),
    }
}

pub(crate) fn archive(job: &Job) -> Result<(), String> {
    if !job.root.join("editing.json").exists() {
        return Ok(());
    }
    crate::job::archive_exports(&job.root)?;
    let slot = job.root.join("cu").join(format!(
        "editing-{}",
        SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    std::fs::create_dir_all(&slot).map_err(|e| e.to_string())?;
    std::fs::rename(job.root.join("editing.json"), slot.join("editing.json"))
        .map_err(|e| e.to_string())?;
    invalidate(&job.root);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (PathBuf, Vec<TranslatedLine>, Editing) {
        let root = std::env::temp_dir().join(format!(
            "cue-edit-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("lines")).unwrap();
        let lines: Vec<_> = (0..3)
            .map(|i| TranslatedLine {
                start: i as f64 * 3.0,
                end: i as f64 * 3.0 + 2.0,
                source: format!("source {i}"),
                translation: format!("câu {i}"),
            })
            .collect();
        write_json(&root.join("translation.json"), &lines).unwrap();
        for i in 1..=3 {
            std::fs::write(
                root.join("lines").join(format!("{i:03}.wav")),
                format!("wav{i}"),
            )
            .unwrap();
        }
        let doc = Editing::initial(&root, &lines, &lines).unwrap();
        (root, lines, doc)
    }

    fn split(doc: &Editing, index: usize, at: f64) -> Structure {
        Structure::Split {
            revision: String::new(),
            id: doc.cues[index].id.clone(),
            at,
            left_source: "First".into(),
            right_source: "Second".into(),
            left_text: "Phần đầu".into(),
            right_text: "Phần sau".into(),
        }
    }

    #[test]
    fn splitting_first_middle_last_keeps_unrelated_ids_slots_and_original_files() {
        for index in 0..3 {
            let (root, base, mut doc) = fixture();
            let raw = std::fs::read(root.join("translation.json")).unwrap();
            let initial = doc.cues.clone();
            doc.restructure(&root, &split(&doc, index, base[index].start + 1.0), 10.0)
                .unwrap();
            doc.save(&root).unwrap();
            assert_eq!(doc.cues.len(), 4);
            for (old_index, old) in initial.iter().enumerate() {
                if old_index == index {
                    continue;
                }
                let now = doc.cues.iter().find(|c| c.id == old.id).unwrap();
                assert_eq!(now, old);
                let position = doc.cues.iter().position(|c| c.id == old.id).unwrap() + 1;
                assert_eq!(
                    std::fs::read(wav_path(&root, position)).unwrap(),
                    format!("wav{}", old_index + 1).as_bytes()
                );
            }
            assert_eq!(doc.cues[index].sources, vec![index]);
            assert_eq!(doc.cues[index + 1].sources, vec![index]);
            assert_eq!(doc.cues[index].line.end, doc.cues[index + 1].line.start);
            assert_eq!(std::fs::read(root.join("translation.json")).unwrap(), raw);
            let loaded = Editing::load(&root).unwrap().unwrap();
            assert_eq!(loaded.cues, doc.cues);
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn split_merge_and_two_undos_survive_reopen_and_preserve_other_edits() {
        let (root, _, mut doc) = fixture();
        let original = doc.cues.clone();
        doc.restructure(&root, &split(&doc, 1, 4.0), 10.0).unwrap();
        let pair = (doc.cues[1].id.clone(), doc.cues[2].id.clone());
        doc.restructure(
            &root,
            &Structure::Merge {
                revision: String::new(),
                id: pair.0,
                next_id: pair.1,
                source: "First Second".into(),
                text: "Phần đầu và sau".into(),
            },
            10.0,
        )
        .unwrap();
        doc.cues[0].line.translation = "Sửa độc lập".into();
        doc.save(&root).unwrap();
        let mut doc = Editing::load(&root).unwrap().unwrap();
        doc.restructure(
            &root,
            &Structure::Undo {
                revision: String::new(),
            },
            10.0,
        )
        .unwrap();
        assert_eq!(doc.cues.len(), 4);
        doc.restructure(
            &root,
            &Structure::Undo {
                revision: String::new(),
            },
            10.0,
        )
        .unwrap();
        assert_eq!(doc.cues.len(), 3);
        assert_eq!(doc.cues[1..], original[1..]);
        assert_eq!(doc.cues[0].line.translation, "Sửa độc lập");
        assert!(!doc.can_undo());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn undo_refuses_to_erase_later_edits_to_the_same_cue() {
        let (root, _, mut doc) = fixture();
        doc.restructure(&root, &split(&doc, 1, 4.0), 10.0).unwrap();
        doc.cues[1].line.translation = "Lời mới cần giữ".into();
        let before = doc.cues.clone();
        assert!(doc
            .restructure(
                &root,
                &Structure::Undo {
                    revision: String::new()
                },
                10.0
            )
            .is_err());
        assert_eq!(doc.cues, before);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn changing_pronunciation_never_reuses_an_old_undo_wav() {
        let (root, _, mut doc) = fixture();
        let old_id = doc.cues[1].id.clone();
        doc.restructure(&root, &split(&doc, 1, 4.0), 10.0).unwrap();
        write_json(
            &root.join("render.json"),
            &serde_json::json!({"pronunciations":{"vi":[{"from":"Ann","to":"An"}]}}),
        )
        .unwrap();
        doc.restructure(
            &root,
            &Structure::Undo {
                revision: String::new(),
            },
            10.0,
        )
        .unwrap();
        assert_eq!(doc.cues[1].id, old_id);
        assert!(doc.cues[1].slot > 3);
        assert_eq!(std::fs::read(root.join("lines/002.wav")).unwrap(), b"wav2");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn undo_applies_confirmed_names_instead_of_restoring_an_old_wrong_name() {
        let (root, _, mut doc) = fixture();
        doc.cues[1].line.source = "London".into();
        doc.cues[1].line.translation = "London".into();
        doc.restructure(&root, &split(&doc, 1, 4.0), 10.0).unwrap();
        write_json(
            &root.join("names.json"),
            &crate::names::NameList {
                confirmed: true,
                names: vec![crate::names::NameFix {
                    from: "London".into(),
                    to: "Luân Đôn".into(),
                }],
            },
        )
        .unwrap();
        doc.restructure(
            &root,
            &Structure::Undo {
                revision: String::new(),
            },
            10.0,
        )
        .unwrap();
        assert_eq!(doc.cues[1].line.translation, "Luân Đôn");
        assert_eq!(doc.cues[1].line.source, "Luân Đôn");
        assert_ne!(doc.cues[1].slot, 2);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn explicit_reset_archives_structure_and_returns_to_legacy_mapping() {
        let (root, _, mut doc) = fixture();
        doc.restructure(&root, &split(&doc, 0, 1.0), 10.0).unwrap();
        doc.save(&root).unwrap();
        assert_eq!(slot(&root, 3).unwrap(), 2);
        let job = Job {
            root: root.clone(),
            video: root.join("clip.mp4"),
            picture: root.join("clip.mp4"),
            target: "vi".into(),
        };
        std::fs::write(job.result(), b"old result").unwrap();
        archive(&job).unwrap();
        assert!(!root.join("editing.json").exists());
        assert_eq!(slot(&root, 3).unwrap(), 3);
        assert_eq!(crate::timing::read(&root).unwrap().len(), 3);
        assert!(std::fs::read_dir(root.join("cu"))
            .unwrap()
            .flatten()
            .any(|entry| entry.path().join("editing.json").is_file()));
        assert_eq!(std::fs::read(root.join("lines/003.wav")).unwrap(), b"wav3");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn malformed_mapping_or_changed_basis_fails_without_guessing_ordinal_audio() {
        let (root, _, mut doc) = fixture();
        doc.restructure(&root, &split(&doc, 0, 1.0), 10.0).unwrap();
        doc.save(&root).unwrap();
        assert!(slot(&root, 4).is_ok());
        let mut changed: Vec<TranslatedLine> = read_json(&root.join("translation.json")).unwrap();
        changed[0].source = "other source".into();
        write_json(&root.join("translation.json"), &changed).unwrap();
        assert!(slot(&root, 4).is_err());
        std::fs::write(root.join("editing.json"), b"broken").unwrap();
        invalidate(&root);
        assert!(slot(&root, 4).is_err());
        assert!(wav_path(&root, 2)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("invalid-editing"));
        assert_eq!(std::fs::read(root.join("lines/002.wav")).unwrap(), b"wav2");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_splits_and_unsafe_merges_are_rejected() {
        let (root, _, doc) = fixture();
        for at in [f64::NAN, 0.0, 0.01, 1.99, 2.0, 4.0] {
            assert!(doc
                .clone()
                .restructure(&root, &split(&doc, 0, at), 10.0)
                .is_err());
        }
        let mut overlap = doc.clone();
        overlap.cues[1].line.start = 1.0;
        let merge = Structure::Merge {
            revision: String::new(),
            id: overlap.cues[0].id.clone(),
            next_id: overlap.cues[1].id.clone(),
            source: "a b".into(),
            text: "một hai".into(),
        };
        assert!(overlap.restructure(&root, &merge, 10.0).is_err());
        let mut gap = doc.clone();
        gap.cues[1].line.start = 4.1;
        assert!(gap.restructure(&root, &merge, 10.0).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
