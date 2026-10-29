//! Receipts for generated text. A file alone is never a receipt. Existing
//! editorial structure remains authoritative and is not migrated/retranslated.
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::job::{Cue, Glossary, Job, TranslatedLine};

static LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Kind {
    Translation,
    Terms,
}
impl Kind {
    fn output(self) -> &'static str {
        match self {
            Self::Translation => "translation.json",
            Self::Terms => "terms.json",
        }
    }
    fn receipt(self) -> &'static str {
        match self {
            Self::Translation => "translation-cache.json",
            Self::Terms => "terms-cache.json",
        }
    }
}

#[derive(Serialize, Deserialize)]
struct Receipt {
    version: u32,
    guard: String,
    basis: String,
    outputs: Vec<Option<String>>,
    #[serde(default)]
    compatible_bases: Vec<String>,
    #[serde(default)]
    authored: Vec<bool>,
}

#[derive(Clone)]
pub(crate) struct Lease {
    root: PathBuf,
    kind: Kind,
    run: String,
    guard: String,
    basis: String,
    count: usize,
    expected: Vec<Cue>,
}

fn hash(value: &impl Serialize) -> Result<String, String> {
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(value).map_err(|e| e.to_string())?)
    ))
}
fn read(root: &Path, kind: Kind) -> Result<Option<Receipt>, String> {
    let bytes = match std::fs::read(root.join(kind.receipt())) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    let receipt: Receipt = serde_json::from_slice(&bytes)
        .map_err(|_| format!("Không đọc được {}. Giữ cache để kiểm tra.", kind.receipt()))?;
    if receipt.version != 1 {
        return Err(format!(
            "Phiên bản {} chưa được hỗ trợ. Giữ cache để kiểm tra.",
            kind.receipt()
        ));
    }
    Ok(Some(receipt))
}
fn write_receipt(lease: &Lease, outputs: Vec<Option<String>>) -> Result<(), String> {
    let previous = read(&lease.root, lease.kind)?;
    let authored = outputs
        .iter()
        .enumerate()
        .map(|(i, output)| {
            previous
                .as_ref()
                .filter(|r| {
                    r.guard == lease.guard && r.outputs.get(i) == Some(output) && output.is_some()
                })
                .map(|r| r.authored.get(i).copied().unwrap_or(false))
                .unwrap_or(false)
        })
        .collect();
    let value = Receipt {
        version: 1,
        guard: lease.guard.clone(),
        basis: lease.basis.clone(),
        outputs,
        compatible_bases: vec![],
        authored,
    };
    crate::atomic_file::write(
        &lease.root.join(lease.kind.receipt()),
        &serde_json::to_vec_pretty(&value).map_err(|e| e.to_string())?,
    )
}

impl Lease {
    pub(crate) fn validate(&self) -> Result<(), String> {
        let current = crate::draft::text_guard(&self.root, self.kind == Kind::Terms)?;
        if current != Some((self.run.clone(), self.guard.clone())) {
            return Err(
                "Dữ liệu hoặc lượt dịch đã thay đổi. Không ghi phản hồi từ bản nháp cũ.".into(),
            );
        }
        Ok(())
    }

    pub(crate) fn commit_translation(&self, done: &[TranslatedLine]) -> Result<(), String> {
        let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        self.validate()?;
        if self.kind != Kind::Translation {
            return Err("Sai loại cache lời dịch.".into());
        }
        read(&self.root, self.kind)?;
        if done.len() != self.count {
            return Err("Số câu trả về không khớp lượt dịch.".into());
        }
        crate::job::write_json(&self.root.join(self.kind.output()), &done)?;
        write_receipt(self, line_hashes(done)?)
    }

    pub(crate) fn commit_terms(&self, glossary: &Glossary) -> Result<(), String> {
        let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        self.validate()?;
        if self.kind != Kind::Terms {
            return Err("Sai loại cache thuật ngữ.".into());
        }
        read(&self.root, self.kind)?;
        crate::job::write_json(&self.root.join(self.kind.output()), glossary)?;
        write_receipt(self, vec![Some(hash(glossary)?)])
    }
}

fn line_hashes(lines: &[TranslatedLine]) -> Result<Vec<Option<String>>, String> {
    lines
        .iter()
        .map(|line| {
            if line.translation.trim().is_empty() {
                Ok(None)
            } else {
                hash(line).map(Some)
            }
        })
        .collect()
}

pub(crate) fn lease(
    job: &Job,
    kind: Kind,
    lines: &[Cue],
    glossary: Option<&Glossary>,
    speech_timing: bool,
) -> Result<Option<Lease>, String> {
    let Some((run, guard)) = crate::draft::text_guard(&job.root, kind == Kind::Terms)? else {
        return Ok(None);
    };
    let basis = hash(&(1, &guard, lines, glossary, speech_timing))?;
    Ok(Some(Lease {
        root: job.root.clone(),
        kind,
        run,
        guard,
        basis,
        count: lines.len(),
        expected: lines.to_vec(),
    }))
}

// Copy the old inputs before any invalidation. No source or old export is deleted.
fn backup(root: &Path, names: &[&str]) -> Result<PathBuf, String> {
    let folder = root.join("cache-stale").join(crate::history::new_id());
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    for name in names {
        match std::fs::read(root.join(name)) {
            Ok(bytes) => crate::atomic_file::write(&folder.join(name), &bytes)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(folder)
}

fn remove(path: &Path) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

// Caller runs this before any translator/speaker starts. The previous receipt
// remains in place until every dependent cache was invalidated successfully.
pub(crate) fn prepare_translation(job: &Job, lease: &Lease) -> Result<bool, String> {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    lease.validate()?;
    let receipt = read(&job.root, Kind::Translation)?;
    let bytes = match std::fs::read(job.translation_path()) {
        Ok(bytes) => Some(bytes),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.to_string()),
    };
    let existed = bytes.is_some();
    let mut saved: Vec<TranslatedLine> = if let Some(bytes) = bytes {
        serde_json::from_slice(&bytes)
            .map_err(|_| "Không đọc được bản dịch cũ. Giữ nguyên để kiểm tra.".to_string())?
    } else {
        Vec::new()
    };
    let missing = saved.len();
    saved.extend(
        lease
            .expected
            .iter()
            .skip(missing)
            .map(|line| TranslatedLine {
                start: line.start,
                end: line.end,
                source: line.text.clone(),
                translation: String::new(),
            }),
    );
    let hashes = line_hashes(&saved)?;
    let matching = receipt.as_ref().filter(|r| {
        r.guard == lease.guard
            && (r.basis == lease.basis || r.compatible_bases.contains(&lease.basis))
    });
    let changed: Vec<_> = (0..hashes.len().max(lease.count))
        .filter(|&i| {
            let output = hashes.get(i);
            let accepted = output
                .is_some_and(|output| matching.is_some_and(|r| r.outputs.get(i) == Some(output)));
            !accepted
                && (output.is_some_and(Option::is_some)
                    || job.line_wav(i + 1).is_file()
                    || receipt
                        .as_ref()
                        .is_some_and(|r| r.outputs.get(i).is_some_and(Option::is_some)))
        })
        .collect();
    let derived = [job.result(), job.captioned(), job.spoken(), job.dub()]
        .iter()
        .any(|p| p.is_file());
    if changed.is_empty() && (matching.is_some() || (!existed && receipt.is_none() && !derived)) {
        return Ok(false);
    }
    if job.root.join("editing.json").exists() {
        return Err("Bản dịch nền đã đổi nhưng có câu đã biên tập. Giữ bản sửa; kiểm tra trước khi dịch lại.".into());
    }
    let backup = backup(&job.root, &["translation.json", "translation-cache.json"])?;
    crate::job::archive_exports(&job.root)?;
    for &index in &changed {
        // Keep each raw WAV recoverable beside the text that produced it.
        let wav = job.line_wav(index + 1);
        if wav.is_file() {
            let bytes = std::fs::read(&wav).map_err(|e| e.to_string())?;
            crate::atomic_file::write(
                &backup
                    .join("lines")
                    .join(wav.file_name().ok_or("Không thấy file tiếng.")?),
                &bytes,
            )?;
        }
        remove(&wav)?;
        remove(&wav.with_extension("fit.wav"))?;
        if let Some(line) = saved.get_mut(index) {
            line.translation.clear();
        }
    }
    for name in [
        "dub.wav",
        "fit-report.json",
        "voice-spans.json",
        "frame-cache.json",
    ] {
        remove(&job.root.join(name))?;
    }
    // Audio/story exports are archived above, including their metadata.
    crate::job::write_json(&job.translation_path(), &saved)?;
    write_receipt(lease, line_hashes(&saved)?)?;
    Ok(true)
}

pub(crate) fn cached_terms(job: &Job, lease: &Lease) -> Result<Option<Glossary>, String> {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    lease.validate()?;
    let receipt = read(&job.root, Kind::Terms)?;
    let bytes = match std::fs::read(job.terms_path()) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    let glossary: Glossary = serde_json::from_slice(&bytes)
        .map_err(|_| "Không đọc được thuật ngữ cũ. Giữ nguyên để kiểm tra.".to_string())?;
    let matching = receipt.is_some_and(|r| {
        r.guard == lease.guard
            && r.basis == lease.basis
            && r.outputs == vec![Some(hash(&glossary).unwrap_or_default())]
    });
    if matching {
        return Ok(Some(glossary));
    }
    backup(&job.root, &["terms.json", "terms-cache.json"])?;
    remove(&job.terms_path())?;
    Ok(None)
}

// Explicit edits and duration shortening are authored content, not AI cache
// corruption. Re-sign only under the unchanged translation dependency guard.
pub(crate) fn accept_words(root: &Path, words: &[TranslatedLine]) -> Result<(), String> {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let Some(mut receipt) = read(root, Kind::Translation)? else {
        return Ok(());
    };
    let Some((_, guard)) = crate::draft::text_guard(root, false)? else {
        return Ok(());
    };
    if receipt.guard != guard {
        return Ok(());
    }
    let hashes = line_hashes(words)?;
    receipt.authored = hashes
        .iter()
        .enumerate()
        .map(|(i, hash)| {
            hash.is_some()
                && (receipt.outputs.get(i) != Some(hash)
                    || receipt.authored.get(i).copied().unwrap_or(false))
        })
        .collect();
    receipt.outputs = hashes;
    crate::atomic_file::write(
        &root.join(Kind::Translation.receipt()),
        &serde_json::to_vec_pretty(&receipt).map_err(|e| e.to_string())?,
    )
}

pub(crate) fn authored_flags(root: &Path, words: &[TranslatedLine]) -> Result<Vec<bool>, String> {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let receipt = read(root, Kind::Translation)?;
    let hashes = line_hashes(words)?;
    Ok(hashes
        .iter()
        .enumerate()
        .map(|(i, hash)| {
            hash.is_some()
                && receipt.as_ref().is_some_and(|r| {
                    r.outputs.get(i) == Some(hash) && r.authored.get(i) == Some(&true)
                })
        })
        .collect())
}

pub(crate) fn generation_basis(root: &Path) -> Result<Option<String>, String> {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    Ok(read(root, Kind::Translation)?.map(|r| r.guard))
}

pub(crate) fn can_reuse_output(root: &Path) -> Result<bool, String> {
    if root.join("editing.json").exists() {
        let Some(doc) = crate::editing::Editing::load(root)? else {
            return Ok(false);
        };
        return Ok(crate::pipeline::demand::export_matches(root)?
            && doc.generation_basis == crate::draft::text_guard(root, false)?.map(|(_, key)| key)
            && doc.cues.iter().all(|c| {
                c.line.source.trim().is_empty() || !c.line.translation.trim().is_empty()
            }));
    }
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let Some(receipt) = read(root, Kind::Translation)? else {
        return Ok(false);
    };
    let Some((_, guard)) = crate::draft::text_guard(root, false)? else {
        return Ok(false);
    };
    let words: Vec<TranslatedLine> = crate::job::read_json(&root.join("translation.json"))?;
    Ok(receipt.guard == guard
        && receipt.outputs == line_hashes(&words)?
        && words
            .iter()
            .all(|line| line.source.trim().is_empty() || !line.translation.trim().is_empty()))
}

// Only fresh, completely generated preview text may be signed here. Older
// already-published previews are not retroactively certified by their marker.
pub(crate) fn record_fresh_preview(
    job: &Job,
    source: crate::draft::Source,
    context: crate::draft::RunContext,
    lines: &[Cue],
    words: &[TranslatedLine],
    skip_translate: bool,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<(), String> {
    crate::pipeline::stopped(cancel)?;
    crate::draft::begin(job, source, context, cancel)?;
    let empty = Glossary {
        theme: String::new(),
        terms: vec![],
    };
    let glossary = (!skip_translate).then_some(&empty);
    let untimed =
        lease(job, Kind::Translation, lines, glossary, false)?.ok_or("Chưa có nháp preview.")?;
    let timed =
        lease(job, Kind::Translation, lines, glossary, true)?.ok_or("Chưa có nháp preview.")?;
    untimed.commit_translation(words)?;
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut receipt = read(&job.root, Kind::Translation)?.ok_or("Chưa có dấu xác nhận preview.")?;
    // Preview has always deferred shortening/fit until export. Either export
    // mode may reuse its text when all actual text dependencies still match.
    receipt.compatible_bases = vec![timed.basis];
    crate::atomic_file::write(
        &job.root.join(Kind::Translation.receipt()),
        &serde_json::to_vec_pretty(&receipt).map_err(|e| e.to_string())?,
    )?;
    let settings: crate::pipeline::RunSettings = crate::job::read_json(&job.settings_path())?;
    let render = crate::render::RenderSettings::read(&job.root)?;
    if !settings.text_only {
        for (index, word) in words.iter().enumerate() {
            let text = crate::speech::clean_for_speech(&word.translation);
            if word.translation.trim().is_empty()
                || crate::render::spoken_text(&render, &job.target, &text) != text
            {
                continue;
            }
            let path = job.line_wav(index + 1);
            if let Some(ticket) =
                crate::draft::speech_ticket(&path, &text, &job.target, &settings.voice)?
            {
                crate::draft::record_existing(&ticket, &path)?;
            }
        }
    }
    crate::draft::checkpoint(&job.root, Some(crate::draft::Phase::Idle))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    struct Fixture {
        root: PathBuf,
        job: Job,
        lines: Vec<Cue>,
        words: Vec<TranslatedLine>,
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
    fn fixture() -> Fixture {
        let root = std::env::temp_dir().join(format!("text-receipt-{}", crate::history::new_id()));
        std::fs::create_dir_all(root.join("output")).unwrap();
        std::fs::write(root.join("clip.mp4"), b"source").unwrap();
        let job = Job::open(&root.join("clip.mp4"), &root.join("output")).unwrap();
        let settings = crate::pipeline::RunSettings {
            target: "vi".into(),
            voice: "fixture".into(),
            use_terms: false,
            skip_translate: false,
            text_only: true,
        };
        crate::job::write_json(&job.settings_path(), &settings).unwrap();
        let lines: Vec<_> = (0..3)
            .map(|i| Cue {
                start: i as f64 * 3.0,
                end: i as f64 * 3.0 + 2.0,
                text: format!("source {i}"),
            })
            .collect();
        crate::job::write_json(&job.lines_path(), &lines).unwrap();
        let words = lines
            .iter()
            .enumerate()
            .map(|(i, c)| TranslatedLine {
                start: c.start,
                end: c.end,
                source: c.text.clone(),
                translation: format!("lời {i}"),
            })
            .collect();
        let source = crate::draft::inspect_source(&job, &AtomicBool::new(false)).unwrap();
        crate::draft::begin(&job, source, context("model-a"), &AtomicBool::new(false)).unwrap();
        Fixture {
            root,
            job,
            lines,
            words,
        }
    }
    fn context(model: &str) -> crate::draft::RunContext {
        crate::draft::RunContext {
            provider: "test".into(),
            model: model.into(),
            audio_only: false,
        }
    }
    fn glossary() -> Glossary {
        Glossary {
            theme: "theme".into(),
            terms: vec![],
        }
    }
    fn ticket(f: &Fixture) -> Lease {
        lease(&f.job, Kind::Translation, &f.lines, Some(&glossary()), true)
            .unwrap()
            .unwrap()
    }

    #[test]
    fn reopening_partial_translation_reuses_only_committed_nonempty_sentences() {
        let f = fixture();
        let cache = ticket(&f);
        let mut partial = f.words.clone();
        partial[1].translation.clear();
        partial[2].translation.clear();
        cache.commit_translation(&partial).unwrap();
        assert!(!can_reuse_output(&f.job.root).unwrap());
        std::fs::write(f.job.line_wav(1), b"keep wav").unwrap();
        let source = crate::draft::inspect_source(&f.job, &AtomicBool::new(false)).unwrap();
        crate::draft::begin(&f.job, source, context("model-a"), &AtomicBool::new(false)).unwrap();
        assert!(!prepare_translation(&f.job, &ticket(&f)).unwrap());
        assert_eq!(
            crate::job::read_json::<Vec<TranslatedLine>>(&f.job.translation_path()).unwrap(),
            partial
        );
        assert_eq!(std::fs::read(f.job.line_wav(1)).unwrap(), b"keep wav");
        assert!(read(&f.job.root, Kind::Translation)
            .unwrap()
            .unwrap()
            .outputs[1]
            .is_none());
    }

    #[test]
    fn a_tampered_sentence_invalidates_only_its_voice_and_preserves_the_previous_export() {
        let f = fixture();
        let cache = ticket(&f);
        cache.commit_translation(&f.words).unwrap();
        for i in 1..=3 {
            std::fs::write(f.job.line_wav(i), format!("wav {i}")).unwrap();
        }
        std::fs::write(f.job.captioned(), b"finished export").unwrap();
        let mut changed = f.words.clone();
        changed[1].translation = "unverified replacement".into();
        crate::job::write_json(&f.job.translation_path(), &changed).unwrap();
        assert!(prepare_translation(&f.job, &cache).unwrap());
        let next: Vec<TranslatedLine> = crate::job::read_json(&f.job.translation_path()).unwrap();
        assert_eq!(next[0], f.words[0]);
        assert!(next[1].translation.is_empty());
        assert_eq!(next[2], f.words[2]);
        assert!(f.job.line_wav(1).is_file());
        assert!(!f.job.line_wav(2).is_file());
        assert!(f.job.line_wav(3).is_file());
        let kept = crate::job::list_kept(&f.job.root).unwrap();
        assert_eq!(std::fs::read(&kept[0].path).unwrap(), b"finished export");
        let backup = std::fs::read_dir(f.job.root.join("cache-stale"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        assert_eq!(
            std::fs::read(backup.join("lines/002.wav")).unwrap(),
            b"wav 2"
        );
        assert!(!prepare_translation(&f.job, &cache).unwrap());
    }

    #[test]
    fn changed_model_glossary_or_speech_budget_cannot_reuse_the_old_translation() {
        let f = fixture();
        ticket(&f).commit_translation(&f.words).unwrap();
        let new_glossary = Glossary {
            theme: "other theme".into(),
            terms: vec![],
        };
        let new = lease(
            &f.job,
            Kind::Translation,
            &f.lines,
            Some(&new_glossary),
            true,
        )
        .unwrap()
        .unwrap();
        assert!(prepare_translation(&f.job, &new).unwrap());
        new.commit_translation(&f.words).unwrap();
        let story = lease(
            &f.job,
            Kind::Translation,
            &f.lines,
            Some(&new_glossary),
            false,
        )
        .unwrap()
        .unwrap();
        assert!(prepare_translation(&f.job, &story).unwrap());
        story.commit_translation(&f.words).unwrap();
        let source = crate::draft::inspect_source(&f.job, &AtomicBool::new(false)).unwrap();
        crate::draft::begin(&f.job, source, context("model-b"), &AtomicBool::new(false)).unwrap();
        assert!(prepare_translation(&f.job, &ticket(&f)).unwrap());
    }

    #[test]
    fn style_voice_timing_and_explicit_edits_keep_authored_words() {
        let f = fixture();
        let cache = ticket(&f);
        cache.commit_translation(&f.words).unwrap();
        let mut settings: crate::pipeline::RunSettings =
            crate::job::read_json(&f.job.settings_path()).unwrap();
        settings.voice = "other-voice".into();
        crate::job::write_json(&f.job.settings_path(), &settings).unwrap();
        let mut render = crate::render::RenderSettings::default();
        render.size = 5.0;
        crate::job::write_json(&f.job.root.join("render.json"), &render).unwrap();
        let mut timing = crate::timing::Timing::load(&f.job.root, &f.words).unwrap();
        timing.set(&[crate::timing::TimeChange {
            index: 0,
            start: 0.2,
            end: 2.5,
        }]);
        timing.save(&f.job.root).unwrap();
        assert!(!prepare_translation(&f.job, &cache).unwrap());
        let mut edited = f.words.clone();
        edited[0].translation = "lời người dùng sửa".into();
        crate::timing::write_words(&f.job.root, &edited).unwrap();
        assert!(!prepare_translation(&f.job, &cache).unwrap());
        assert_eq!(
            crate::job::read_json::<Vec<TranslatedLine>>(&f.job.translation_path()).unwrap()[0]
                .translation,
            "lời người dùng sửa"
        );
    }

    #[test]
    fn obsolete_run_and_dependency_responses_cannot_commit_text() {
        let f = fixture();
        let old = ticket(&f);
        old.commit_translation(&f.words).unwrap();
        let before = std::fs::read(f.job.translation_path()).unwrap();
        crate::job::write_json(
            &f.job.names_path(),
            &crate::names::NameList {
                confirmed: true,
                names: vec![crate::names::NameFix {
                    from: "source".into(),
                    to: "correct source".into(),
                }],
            },
        )
        .unwrap();
        assert!(old.commit_translation(&[]).is_err());
        let previous = ticket(&f);
        let source = crate::draft::inspect_source(&f.job, &AtomicBool::new(false)).unwrap();
        crate::draft::begin(&f.job, source, context("model-a"), &AtomicBool::new(false)).unwrap();
        assert!(previous.commit_translation(&[]).is_err());
        assert_eq!(std::fs::read(f.job.translation_path()).unwrap(), before);
    }

    #[test]
    fn legacy_text_is_backed_up_and_future_receipts_fail_without_modifying_it() {
        let f = fixture();
        crate::job::write_json(&f.job.translation_path(), &f.words).unwrap();
        let original = std::fs::read(f.job.translation_path()).unwrap();
        assert!(prepare_translation(&f.job, &ticket(&f)).unwrap());
        let backup = std::fs::read_dir(f.job.root.join("cache-stale"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        assert_eq!(
            std::fs::read(backup.join("translation.json")).unwrap(),
            original
        );
        let mut receipt: serde_json::Value =
            crate::job::read_json(&f.job.root.join("translation-cache.json")).unwrap();
        receipt["version"] = 100.into();
        crate::job::write_json(&f.job.root.join("translation-cache.json"), &receipt).unwrap();
        let before = std::fs::read(f.job.translation_path()).unwrap();
        assert!(prepare_translation(&f.job, &ticket(&f)).is_err());
        assert_eq!(std::fs::read(f.job.translation_path()).unwrap(), before);
    }

    #[test]
    fn terms_are_reused_only_with_their_generation_receipt() {
        let f = fixture();
        let cache = lease(&f.job, Kind::Terms, &f.lines, None, false)
            .unwrap()
            .unwrap();
        crate::job::write_json(&f.job.terms_path(), &glossary()).unwrap();
        assert!(cached_terms(&f.job, &cache).unwrap().is_none());
        cache.commit_terms(&glossary()).unwrap();
        assert_eq!(cached_terms(&f.job, &cache).unwrap().unwrap(), glossary());
        let other = Glossary {
            theme: "modified".into(),
            terms: vec![],
        };
        crate::job::write_json(&f.job.terms_path(), &other).unwrap();
        assert!(cached_terms(&f.job, &cache).unwrap().is_none());
    }

    #[test]
    fn fresh_preview_keeps_the_export_contract_without_certifying_unknown_old_text() {
        let f = fixture();
        let source = crate::draft::inspect_source(&f.job, &AtomicBool::new(false)).unwrap();
        record_fresh_preview(
            &f.job,
            source,
            context("model-a"),
            &f.lines,
            &f.words,
            false,
            &AtomicBool::new(false),
        )
        .unwrap();
        crate::job::write_json(
            &f.job.names_path(),
            &crate::names::NameList {
                confirmed: true,
                names: vec![],
            },
        )
        .unwrap();
        let empty = Glossary {
            theme: String::new(),
            terms: vec![],
        };
        for timed in [false, true] {
            let export = lease(&f.job, Kind::Translation, &f.lines, Some(&empty), timed)
                .unwrap()
                .unwrap();
            assert!(!prepare_translation(&f.job, &export).unwrap());
        }
        assert!(can_reuse_output(&f.job.root).unwrap());
        assert!(crate::draft::matches_text_context(&f.job.root, "test", "model-a").unwrap());
        assert!(!crate::draft::matches_text_context(&f.job.root, "test", "model-b").unwrap());
        let changed = lease(&f.job, Kind::Translation, &f.lines, Some(&glossary()), true)
            .unwrap()
            .unwrap();
        assert!(prepare_translation(&f.job, &changed).unwrap());
    }

    #[test]
    fn cleared_or_missing_translation_cannot_export_an_old_sentence_wav() {
        let f = fixture();
        let cache = ticket(&f);
        cache.commit_translation(&f.words).unwrap();
        for i in 1..=3 {
            std::fs::write(f.job.line_wav(i), b"old speech").unwrap();
        }
        let mut words = f.words.clone();
        words[1].translation.clear();
        crate::job::write_json(&f.job.translation_path(), &words).unwrap();
        assert!(prepare_translation(&f.job, &cache).unwrap());
        assert!(f.job.line_wav(1).is_file());
        assert!(!f.job.line_wav(2).is_file());
        std::fs::remove_file(f.job.translation_path()).unwrap();
        assert!(prepare_translation(&f.job, &cache).unwrap());
        assert!(!f.job.line_wav(1).is_file());
        assert!(!f.job.line_wav(3).is_file());
        assert_eq!(
            crate::draft::status(&f.job.root)
                .unwrap()
                .unwrap()
                .manifest
                .cues
                .len(),
            3
        );
    }
}
