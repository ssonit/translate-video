use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::job::{file_ready, read_json, write_json, Job, TranslatedLine};
use crate::names::{apply_names, usable_names, NameList};

use super::assemble::{export_audio, fit, is_m4a, merge, merge_text, plain_result};
use super::speak::synthesize_missing;
use super::{clean_target, report, require_tool, stopped, RunSettings};

// Một câu trong bản đã xong. index tính từ 0, đúng thứ tự translation.json.
#[derive(Clone, Default, Deserialize)]
pub(crate) struct LinePatch {
    pub index: usize,
    pub text: String,
    #[serde(default)]
    pub start: Option<f64>,
    #[serde(default)]
    pub end: Option<f64>,
    #[serde(default)]
    pub revision: Option<String>,
    // None retains the legacy explicit "read again" behavior.
    #[serde(default)]
    pub again: Option<bool>,
    #[serde(default)]
    pub id: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct UndoNote {
    parked: Vec<String>,
    wavs: Vec<usize>,
    missing_wavs: Vec<usize>,
    #[serde(default)]
    timing_existed: Option<bool>,
    #[serde(default)]
    missing_outputs: Vec<String>,
    #[serde(default)]
    editing_existed: Option<bool>,
    #[serde(default)]
    history_new: Option<String>,
}

fn undo_dir(root: &Path) -> PathBuf {
    root.join("lines").join("_undo")
}

pub(super) enum RetakePath {
    Speak,
    Burn,
}

pub(super) fn retake_path(text_only: bool) -> RetakePath {
    if text_only {
        RetakePath::Burn
    } else {
        RetakePath::Speak
    }
}

pub(super) fn language_code(path: &Path) -> String {
    crate::job::media_language(path)
}

fn is_captioned_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .ends_with(".chu.mp4")
}

// Ghép các lần sửa cùng một câu: lần sau đè lần trước.
fn folded_edits(lines: &[TranslatedLine], edits: &[LinePatch]) -> Result<Vec<LinePatch>, String> {
    if edits.is_empty() {
        return Err("Chưa chọn câu nào để sửa.".into());
    }
    let mut folded: Vec<LinePatch> = Vec::new();
    for edit in edits {
        let text = edit.text.trim().to_string();
        if text.chars().count() > 4000 {
            return Err(format!("Câu {} dài quá.", edit.index + 1));
        }
        if edit.index >= lines.len() {
            return Err(format!("Không có câu {}.", edit.index + 1));
        }
        let mut clean = edit.clone();
        clean.text = text;
        if let Some(slot) = folded.iter_mut().find(|e| e.index == edit.index) {
            *slot = clean;
        } else {
            folded.push(clean);
        }
    }
    Ok(folded)
}

fn copy_file(src: &Path, dest: &Path) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::copy(src, dest).map_err(|error| error.to_string())?;
    Ok(())
}

fn park_file(src: &Path, undo: &Path, parked: &mut Vec<String>) -> Result<(), String> {
    if !file_ready(src) {
        return Ok(());
    }
    let name = src
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or("Không đọc được tên file kết quả.")?
        .to_string();
    let dest = undo.join("park").join(&name);
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let _ = std::fs::remove_file(&dest);
    if let Err(error) = std::fs::rename(src, &dest) {
        let busy = error.raw_os_error() == Some(32)
            || error.kind() == std::io::ErrorKind::PermissionDenied;
        if busy {
            return Err(format!("{name} đang được mở. Đóng trình phát rồi thử lại."));
        }
        return Err(format!("Không nhường được {name}: {error}"));
    }
    parked.push(name);
    Ok(())
}

fn put_back(root: &Path, undo: &Path, name: &str) -> Result<(), String> {
    let parked = undo.join("park").join(name);
    if !parked.is_file() {
        return Ok(());
    }
    let live = root.join(name);
    if live.is_file() {
        let _ = std::fs::remove_file(&live);
    }
    std::fs::rename(&parked, &live).map_err(|error| format!("Không khôi phục được {name}: {error}"))
}

// Bản sửa dở (tắt app giữa chừng) được trả về như trước khi sửa.
pub(crate) fn restore_retake(root: &Path) -> Result<(), String> {
    let undo = undo_dir(root);
    if !undo.is_dir() {
        return Ok(());
    }
    if undo.join("done").is_file() {
        let _ = std::fs::remove_dir_all(&undo);
        return Ok(());
    }
    let note_path = undo.join("manifest.json");
    if !note_path.is_file() {
        let _ = std::fs::remove_dir_all(&undo);
        return Ok(());
    }
    let note: UndoNote = read_json(&note_path)?;
    if let Some(id) = &note.history_new { crate::history::remove(root, id)?; }
    for name in note.parked.iter().chain(&note.missing_outputs) {
        if name.contains(['/', '\\', ':']) || name == "." || name == ".." {
            return Err("Danh sách khôi phục không hợp lệ. Giữ folder _undo để kiểm tra.".into());
        }
    }
    let saved = undo.join("translation.json");
    if file_ready(&saved) {
        copy_file(&saved, &root.join("translation.json"))?;
    }
    match note.timing_existed {
        Some(true) => copy_file(&undo.join("timing.json"), &root.join("timing.json"))?,
        Some(false) => match std::fs::remove_file(root.join("timing.json")) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        },
        None => {} // Legacy retakes never touched the editing layer.
    }
    match note.editing_existed {
        Some(true) => copy_file(&undo.join("editing.json"), &root.join("editing.json"))?,
        Some(false) => match std::fs::remove_file(root.join("editing.json")) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        },
        None => {}
    }
    crate::editing::invalidate(root);
    for name in &note.parked {
        put_back(root, &undo, name)?;
    }
    for name in &note.missing_outputs {
        match std::fs::remove_file(root.join(name)) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    for number in &note.missing_wavs {
        let _ = std::fs::remove_file(root.join("lines").join(format!("{number:03}.wav")));
    }
    for number in &note.wavs {
        let saved = undo.join("wav").join(format!("{number:03}.wav"));
        if file_ready(&saved) {
            copy_file(&saved, &root.join("lines").join(format!("{number:03}.wav")))?;
        }
    }
    std::fs::remove_dir_all(&undo).map_err(|error| error.to_string())?;
    Ok(())
}

fn discard_undo(root: &Path) -> Result<(), String> {
    let undo = undo_dir(root);
    if !undo.is_dir() {
        return Ok(());
    }
    std::fs::write(undo.join("done"), b"ok").map_err(|error| error.to_string())?;
    std::fs::remove_dir_all(&undo).map_err(|error| error.to_string())?;
    Ok(())
}

// Đổi lời các câu được chọn, cất video/audio cũ sang một bên, xóa wav của đúng những câu đó.
fn begin_retake(
    job: &Job,
    edits: &[LinePatch],
    duration: Option<f64>,
) -> Result<(usize, usize), String> {
    begin_edit(job, edits, duration, None)
}

fn begin_edit(
    job: &Job,
    edits: &[LinePatch],
    duration: Option<f64>,
    structure: Option<&crate::editing::Structure>,
) -> Result<(usize, usize), String> {
    let undo = undo_dir(&job.root);
    if undo.join("done").is_file() {
        discard_undo(&job.root)?;
    } else if undo.join("manifest.json").is_file() {
        restore_retake(&job.root)?;
    } else if undo.is_dir() {
        let _ = std::fs::remove_dir_all(&undo);
    }
    let mut saved: Vec<TranslatedLine> =
        read_json(&job.translation_path()).map_err(|_| "Chưa có bản dịch để sửa.".to_string())?;
    let current_revision = crate::timing::revision(&job.root)?;
    let mut timing = crate::timing::Timing::load(&job.root, &saved)?;
    let mut effective = saved.clone();
    timing.apply(&mut effective);
    let mut document = crate::editing::Editing::load(&job.root)?;
    let editing_existed = Some(document.is_some());
    if let Some(doc) = &document {
        effective = doc.lines();
    }
    let folded = if structure.is_some() {
        Vec::new()
    } else {
        folded_edits(&effective, edits)?
    };
    let mut next = effective.clone();
    let mut changes = Vec::new();
    let mut reread = Vec::new(); // PHYSICAL WAV slots, never logical indexes.
    let count;
    if let Some(action) = structure {
        if action.revision() != current_revision {
            return Err("Dữ liệu câu đã thay đổi. Mở lại video trước khi tách/gộp.".into());
        }
        let old_ids: std::collections::HashSet<_> = if let Some(doc) = &document {
            doc.cues.iter().map(|c| c.id.clone()).collect()
        } else {
            (0..effective.len())
                .map(crate::editing::legacy_id)
                .collect()
        };
        let mut doc = match document {
            Some(doc) => doc,
            None => crate::editing::Editing::initial(&job.root, &saved, &effective)?,
        };
        doc.restructure(
            &job.root,
            action,
            duration.ok_or("Tách/gộp chỉ áp dụng cho video.")?,
        )?;
        for cue in &doc.cues {
            if !old_ids.contains(&cue.id)
                && !file_ready(&job.root.join("lines").join(format!("{:03}.wav", cue.slot)))
            {
                reread.push(cue.slot);
            }
        }
        next = doc.lines();
        count = next
            .iter()
            .enumerate()
            .filter(|(i, l)| effective.get(*i) != Some(*l))
            .count()
            .max(1);
        document = Some(doc);
    } else {
        count = folded.len();
    }
    for edit in &folded {
        if edit
            .revision
            .as_ref()
            .is_some_and(|r| *r != current_revision)
        {
            return Err("Lời hoặc cấu hình đã thay đổi. Mở lại video trước khi sửa tiếp.".into());
        }
        if let Some(doc) = &document {
            if edit.id.as_ref() != Some(&doc.cues[edit.index].id) {
                return Err(
                    "ID câu không khớp. Mở lại video để tránh sửa nhầm câu sau tách/gộp.".into(),
                );
            }
        }
        match (edit.start, edit.end) {
            (Some(start), Some(end)) => {
                if edit.revision.is_none() {
                    return Err("Thiếu phiên bản mốc câu. Mở lại video để sửa.".into());
                }
                next[edit.index].start = start;
                next[edit.index].end = end;
                changes.push(crate::timing::TimeChange {
                    index: edit.index,
                    start,
                    end,
                });
            }
            (None, None) => {}
            _ => return Err("Điền cả mốc bắt đầu và kết thúc.".into()),
        }
    }
    if !changes.is_empty() {
        crate::timing::validate_changes(
            &effective,
            &next,
            &changes,
            duration.ok_or("Chỉnh mốc chỉ áp dụng cho video.")?,
        )?;
        if document.is_none() {
            timing.set(&changes);
        }
    }
    let mut wavs = Vec::new();
    let mut missing_wavs = Vec::new();
    std::fs::create_dir_all(undo.join("wav")).map_err(|error| error.to_string())?;
    copy_file(&job.translation_path(), &undo.join("translation.json"))?;
    let timing_existed = Some(job.root.join("timing.json").is_file());
    if timing_existed == Some(true) {
        copy_file(&job.root.join("timing.json"), &undo.join("timing.json"))?;
    }
    if editing_existed == Some(true) {
        copy_file(&job.root.join("editing.json"), &undo.join("editing.json"))?;
    }
    let mut words_changed = false;
    for edit in &folded {
        let index = edit.index;
        let text_changed = effective[index].translation.trim() != edit.text;
        let needs_voice = text_changed || edit.again.unwrap_or(edit.start.is_none());
        if text_changed {
            words_changed = true;
            next[index].translation = edit.text.clone();
            if document.is_none() {
                saved[index].translation = edit.text.clone();
            }
        }
        if !needs_voice {
            continue;
        }
        reread.push(if let Some(doc) = &document {
            doc.cues[index].slot
        } else {
            index + 1
        });
    }
    if let Some(doc) = &mut document {
        for (cue, line) in doc.cues.iter_mut().zip(&next) {
            cue.line = line.clone();
        }
    }
    if let Some(doc) = &document {
        let text_only = read_json::<RunSettings>(&job.settings_path())
            .ok()
            .is_some_and(|s| s.text_only);
        if !text_only {
            for (index, cue) in doc.cues.iter().enumerate() {
                if !reread.contains(&cue.slot)
                    && !file_ready(&job.root.join("lines").join(format!("{:03}.wav", cue.slot)))
                {
                    // No mutation has happened yet; leave the finished export alone.
                    let _ = std::fs::remove_dir_all(&undo);
                    return Err(format!("Thiếu tiếng câu {} không thuộc phần đang sửa. Chạy lại việc để bổ sung trước khi tách/gộp.", index + 1));
                }
            }
        }
    }
    for number in &reread {
        let wav = job.root.join("lines").join(format!("{number:03}.wav"));
        if file_ready(&wav) {
            copy_file(&wav, &undo.join("wav").join(format!("{number:03}.wav")))?;
            wavs.push(*number);
        } else {
            missing_wavs.push(*number);
        }
    }
    let mut parked = Vec::new();
    let outputs = [
        job.fit_report(),
        job.dub(),
        job.subtitles(),
        job.spoken(),
        job.result(),
        job.captioned(),
        job.root.join("voice-spans.json"),
        job.root.join("frame-cache.json"),
        job.root.join("fit-mode.txt"),
        job.root.join("history-current.json"),
    ];
    let missing_outputs: Vec<_> = outputs
        .iter()
        .filter(|p| !file_ready(p))
        .filter_map(|p| p.file_name())
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    let note = UndoNote {
        parked: Vec::new(),
        wavs: wavs.clone(),
        missing_wavs: missing_wavs.clone(),
        timing_existed,
        missing_outputs: missing_outputs.clone(),
        editing_existed,
        history_new: Some(crate::history::new_id()),
    };
    write_json(&undo.join("manifest.json"), &note)?;
    let staged = (|| {
        for path in outputs {
            if !file_ready(&path) {
                continue;
            }
            // Journal intent BEFORE moving each file, including the last one.
            // Recovery tolerates an intent whose move did not happen yet.
            parked.push(
                path.file_name()
                    .ok_or("Không đọc được tên file.")?
                    .to_string_lossy()
                    .into_owned(),
            );
            write_json(
                &undo.join("manifest.json"),
                &UndoNote {
                    parked: parked.clone(),
                    wavs: wavs.clone(),
                    missing_wavs: missing_wavs.clone(),
                    timing_existed,
                    missing_outputs: missing_outputs.clone(),
                    editing_existed,
                    history_new: note.history_new.clone(),
                },
            )?;
            park_file(&path, &undo, &mut Vec::new())?;
        }
        if !changes.is_empty() && document.is_none() {
            timing.save(&job.root)?;
        }
        if let Some(doc) = &document {
            doc.save(&job.root)?;
        } else if words_changed {
            write_json(&job.translation_path(), &saved)?;
        }
        for slot in &reread {
            let wav = job.root.join("lines").join(format!("{slot:03}.wav"));
            if !wav.is_file() {
                continue;
            }
            std::fs::remove_file(&wav).map_err(|_| {
                format!("Câu {} đang được dùng. Đóng trình phát rồi thử lại.", slot)
            })?;
        }
        Ok((count, reread.len()))
    })();
    if let Err(error) = staged {
        let _ = restore_retake(&job.root);
        return Err(error);
    }
    staged
}

pub(super) fn open_finished(result: &Path) -> Result<(Job, bool, bool), String> {
    let audio_only = is_m4a(result);
    let burn = !audio_only && is_captioned_file(result);
    let plain = if audio_only {
        result.to_path_buf()
    } else {
        plain_result(result)
    };
    let root = plain.parent().ok_or("Chưa thấy folder việc.")?;
    let mut job = Job::reopen(root)?;
    let saved_target = read_json::<RunSettings>(&job.settings_path())
        .ok()
        .map(|saved| saved.target)
        .unwrap_or_default();
    let code = {
        let from_file = language_code(&plain);
        if from_file.is_empty() {
            saved_target
        } else {
            from_file
        }
    };
    job.target = clean_target(&code).0.to_string();
    Ok((job, audio_only, burn))
}

pub(crate) fn saved_name_edits(result: &Path) -> Result<Vec<LinePatch>, String> {
    let (job, _, _) = open_finished(result)?;
    let list = read_json::<NameList>(&job.names_path()).unwrap_or_default();
    let names = usable_names(&list.names);
    let saved = crate::timing::read(&job.root)?;
    let document = crate::editing::Editing::load(&job.root)?;
    let revision = crate::timing::revision(&job.root)?;
    Ok(saved
        .iter()
        .enumerate()
        .filter_map(|(index, line)| {
            let text = apply_names(&line.translation, &names);
            (text != line.translation).then(|| LinePatch {
                index,
                text,
                id: Some(
                    document
                        .as_ref()
                        .map(|d| d.cues[index].id.clone())
                        .unwrap_or_else(|| crate::editing::legacy_id(index)),
                ),
                revision: Some(revision.clone()),
                ..Default::default()
            })
        })
        .collect())
}

// Đọc lại đúng các câu mà names.json đổi chữ. Câu khác giữ wav cũ.
pub(crate) fn retake_saved_names(
    app: &AppHandle,
    result: &Path,
    cancel: &AtomicBool,
) -> Result<String, String> {
    let (job, _, _) = open_finished(result)?;
    let list = read_json::<NameList>(&job.names_path()).unwrap_or_default();
    let names = usable_names(&list.names);
    if names.is_empty() {
        return Err("Chưa có tên để sửa.".into());
    }
    let edits = saved_name_edits(result)?;
    if edits.is_empty() {
        return Err("Các câu đã dùng tên đúng.".into());
    }
    let path = revise(app, result, &edits, cancel)?;
    let mut lines = crate::timing::read(&job.root)?;
    let mut touched = false;
    for line in &mut lines {
        let renamed = apply_names(&line.source, &names);
        if renamed != line.source {
            line.source = renamed;
            touched = true;
        }
    }
    if touched {
        if job.root.join("editing.json").exists() {
            crate::editing::write_lines(&job.root, &lines)?;
        } else {
            crate::timing::write_words(&job.root, &lines)?;
        }
    }
    Ok(path)
}

// Video hoặc audio đã xong: đọc lại đúng câu được sửa, rồi trộn và ghép lại. Câu khác giữ wav cũ.
pub(crate) fn revise(
    app: &AppHandle,
    result: &Path,
    edits: &[LinePatch],
    cancel: &AtomicBool,
) -> Result<String, String> {
    revise_with(app, result, edits, None, cancel)
}

pub(crate) fn restructure(
    app: &AppHandle,
    result: &Path,
    action: &crate::editing::Structure,
    cancel: &AtomicBool,
) -> Result<String, String> {
    revise_with(app, result, &[], Some(action), cancel)
}

fn revise_with(
    app: &AppHandle,
    result: &Path,
    edits: &[LinePatch],
    structure: Option<&crate::editing::Structure>,
    cancel: &AtomicBool,
) -> Result<String, String> {
    let (job, audio_only, burn) = open_finished(result)?;
    crate::history::ensure_previous(&job.root)?;
    let text_only = read_json::<RunSettings>(&job.settings_path())
        .ok()
        .is_some_and(|saved| saved.text_only);
    let (target_code, target_name) = clean_target(&job.target);
    if (structure.is_some() || edits.iter().any(|e| e.start.is_some() || e.end.is_some()))
        && audio_only
    {
        return Err("Audio liền mạch không dùng mốc video. Chỉnh mốc ở bản video.".into());
    }
    require_tool("ffmpeg")?;
    require_tool("ffprobe")?;
    let duration =
        if structure.is_some() || edits.iter().any(|e| e.start.is_some() || e.end.is_some()) {
            Some(super::media_duration(result, cancel)?)
        } else {
            None
        };
    let (count, voice_count) = if structure.is_some() {
        begin_edit(&job, edits, duration, structure)?
    } else {
        begin_retake(&job, edits, duration)?
    };
    let outcome = (|| {
        for id in [
            "extract",
            "transcribe",
            "split",
            "names",
            "terms",
            "translate",
        ] {
            report(app, id, "skipped", "Giữ nguyên. Chỉ sửa câu đã chọn.");
        }
        stopped(cancel)?;
        let lines = if audio_only && !job.root.join("editing.json").exists() {
            read_json(&job.translation_path())?
        } else {
            crate::timing::read(&job.root)?
        };
        match retake_path(text_only) {
            RetakePath::Burn => {
                report(app, "speak", "skipped", "Không đọc giọng.");
                report(app, "fit", "skipped", "Không co độ dài.");
                merge_text(app, &job, &lines, cancel)?;
                return Ok(job.captioned());
            }
            RetakePath::Speak => {}
        }
        if voice_count > 0 {
            crate::selected_voice(app, target_code).map_err(|_| {
                format!("Chưa có giọng đọc {target_name}. Bỏ cặp giọng vào voices\\{target_code}.")
            })?;
            report(
                app,
                "speak",
                "running",
                format!("Đang đọc lại {voice_count} câu."),
            );
            synthesize_missing(app, &job, &lines, cancel)?;
            report(
                app,
                "speak",
                "done",
                format!("Đã đọc lại {voice_count} câu."),
            );
        } else {
            report(
                app,
                "speak",
                "skipped",
                format!("Giữ tiếng từng câu. Chỉ đổi mốc {count} câu."),
            );
        }
        fit(app, &job, &lines, audio_only, cancel)?;
        if audio_only {
            export_audio(app, &job, &lines, cancel)?;
            return Ok(job.spoken());
        }
        merge(app, &job, &lines, burn, cancel)?;
        if burn && file_ready(&job.captioned()) {
            Ok(job.captioned())
        } else {
            Ok(job.result())
        }
    })();
    match outcome {
        Ok(path) => {
            let note: UndoNote = read_json(&undo_dir(&job.root).join("manifest.json"))?;
            let mut hints = Vec::new();
            if let Some(action) = structure {
                hints.push(match action { crate::editing::Structure::Split { .. } => "Tách câu", crate::editing::Structure::Merge { .. } => "Gộp câu", crate::editing::Structure::Undo { .. } => "Hoàn tác tách/gộp" }.into());
            } else {
                if edits.iter().any(|e| e.start.is_some()) { hints.push("Đổi mốc câu".into()); }
                if edits.iter().any(|e| e.again == Some(true)) { hints.push("Đọc lại giọng".into()); }
            }
            if let Err(error) = crate::history::publish(&job.root, &path, &hints, None, note.history_new.as_deref(), cancel) {
                restore_retake(&job.root).map_err(|undo| format!("{error}. Không khôi phục được bản cũ: {undo}"))?;
                return Err(error);
            }
            // Bản mới đã nằm đúng chỗ. Cờ done khiến lần mở sau không kéo bản cũ đè lên.
            if let Err(error) = discard_undo(&job.root) {
                if !undo_dir(&job.root).join("done").is_file() {
                    restore_retake(&job.root).map_err(|undo| {
                        format!(
                            "Không chốt được bản sửa: {error}. Không khôi phục được bản cũ: {undo}"
                        )
                    })?;
                    return Err(format!(
                        "Không chốt được bản sửa: {error}. Đã khôi phục bản cũ."
                    ));
                }
                // Commit marker exists: failed cleanup can be retried on reopen.
            }
            Ok(path.to_string_lossy().into_owned())
        }
        Err(error) => {
            if let Err(undo_error) = restore_retake(&job.root) {
                return Err(format!("{error} Không khôi phục được bản cũ: {undo_error}"));
            }
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structural_retake_rolls_back_mapping_new_wavs_and_later_edits_target_physical_slots() {
        let root = std::env::temp_dir().join(format!(
            "structure-retake-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("lines")).unwrap();
        let job = Job {
            root: root.clone(),
            video: root.join("clip.mp4"),
            picture: root.join("clip.mp4"),
            target: "vi".into(),
        };
        let base: Vec<_> = (0..3)
            .map(|i| TranslatedLine {
                start: i as f64 * 3.0,
                end: i as f64 * 3.0 + 2.0,
                source: format!("source {i}"),
                translation: format!("câu {i}"),
            })
            .collect();
        write_json(&job.translation_path(), &base).unwrap();
        write_json(&job.lines_path(), &base).unwrap();
        for i in 1..=3 {
            std::fs::write(job.line_wav(i), format!("old{i}")).unwrap();
        }
        std::fs::write(job.result(), b"old video").unwrap();
        let raw = std::fs::read(job.translation_path()).unwrap();
        let action = crate::editing::Structure::Split {
            revision: crate::timing::revision(&root).unwrap(),
            id: crate::editing::legacy_id(1),
            at: 4.0,
            left_source: "First".into(),
            right_source: "Second".into(),
            left_text: "Đầu".into(),
            right_text: "Sau".into(),
        };
        assert_eq!(
            begin_edit(&job, &[], Some(10.0), Some(&action)).unwrap().1,
            2
        );
        assert_eq!(crate::timing::read(&root).unwrap().len(), 4);
        assert_eq!(crate::editing::slot(&root, 4).unwrap(), 3);
        std::fs::write(job.line_wav(2), b"split left").unwrap();
        std::fs::write(job.line_wav(3), b"split right").unwrap();
        restore_retake(&root).unwrap();
        assert!(!root.join("editing.json").exists());
        assert_eq!(job.line_wav(3), root.join("lines/003.wav"));
        assert!(!root.join("lines/004.wav").exists() && !root.join("lines/005.wav").exists());
        assert_eq!(std::fs::read(job.result()).unwrap(), b"old video");
        begin_edit(&job, &[], Some(10.0), Some(&action)).unwrap();
        std::fs::write(job.line_wav(2), b"split left").unwrap();
        std::fs::write(job.line_wav(3), b"split right").unwrap();
        std::fs::write(job.result(), b"split video").unwrap();
        discard_undo(&root).unwrap();
        assert_eq!(std::fs::read(job.translation_path()).unwrap(), raw);
        let doc = crate::editing::Editing::load(&root).unwrap().unwrap();
        let patch = LinePatch {
            index: 3,
            id: Some(doc.cues[3].id.clone()),
            text: "câu đã sửa".into(),
            revision: Some(crate::timing::revision(&root).unwrap()),
            ..Default::default()
        };
        assert_eq!(begin_retake(&job, &[patch], None).unwrap(), (1, 1));
        assert!(!root.join("lines/003.wav").exists());
        assert_eq!(
            std::fs::read(root.join("lines/004.wav")).unwrap(),
            b"split left"
        );
        assert_eq!(
            std::fs::read(root.join("lines/005.wav")).unwrap(),
            b"split right"
        );
        restore_retake(&root).unwrap();
        assert_eq!(std::fs::read(root.join("lines/003.wav")).unwrap(), b"old3");
        assert_eq!(crate::timing::read(&root).unwrap()[3].translation, "câu 2");
        assert_eq!(std::fs::read(job.result()).unwrap(), b"split video");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn timing_only_retake_keeps_wavs_and_restores_overlay_and_derived_files() {
        let root = std::env::temp_dir().join(format!(
            "retime-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("lines")).unwrap();
        let job = Job {
            root: root.clone(),
            video: root.join("clip.mp4"),
            picture: root.join("clip.mp4"),
            target: "vi".into(),
        };
        let lines: Vec<_> = (0..3)
            .map(|i| TranslatedLine {
                start: i as f64 * 2.0,
                end: i as f64 * 2.0 + 1.0,
                source: format!("s{i}"),
                translation: format!(" t{i} "),
            })
            .collect();
        write_json(&job.translation_path(), &lines).unwrap();
        // Legacy JSON formatting/unknown metadata must survive timing-only edits
        // byte-for-byte, not be silently normalized by parse/serialize.
        let mut bytes = std::fs::read(job.translation_path()).unwrap();
        bytes.push(b'\n');
        std::fs::write(job.translation_path(), bytes).unwrap();
        write_json(&job.lines_path(), &lines).unwrap();
        for i in 1..=3 {
            std::fs::write(job.line_wav(i), format!("wav{i}")).unwrap();
        }
        std::fs::write(job.result(), b"old video").unwrap();
        std::fs::write(root.join("voice-spans.json"), b"old spans").unwrap();
        let raw = std::fs::read(job.translation_path()).unwrap();
        let whispered = std::fs::read(job.lines_path()).unwrap();
        let make = |revision| LinePatch {
            index: 1,
            text: "t1".into(),
            start: Some(1.5),
            end: Some(3.5),
            revision: Some(revision),
            again: Some(false),
            id: None,
        };
        assert!(begin_retake(&job, &[make("stale".into())], Some(6.0)).is_err());
        assert!(job.result().is_file() && !undo_dir(&root).exists());
        let revision = crate::timing::revision(&root).unwrap();
        assert_eq!(
            begin_retake(&job, &[make(revision.clone())], Some(6.0)).unwrap(),
            (1, 0)
        );
        assert_eq!(std::fs::read(job.translation_path()).unwrap(), raw);
        assert_eq!(std::fs::read(job.lines_path()).unwrap(), whispered);
        for i in 1..=3 {
            assert_eq!(
                std::fs::read(job.line_wav(i)).unwrap(),
                format!("wav{i}").as_bytes()
            );
        }
        assert_eq!(crate::timing::read(&root).unwrap()[1].start, 1.5);
        // Simulate fit succeeding, then merge failing: its new dub/spans must not
        // survive rollback alongside the old timeline.
        std::fs::write(job.dub(), b"new mix").unwrap();
        std::fs::write(root.join("voice-spans.json"), b"new spans").unwrap();
        restore_retake(&root).unwrap();
        assert!(!root.join("timing.json").exists());
        assert!(!job.dub().exists());
        assert_eq!(
            std::fs::read(root.join("voice-spans.json")).unwrap(),
            b"old spans"
        );
        assert_eq!(std::fs::read(job.result()).unwrap(), b"old video");
        assert_eq!(crate::timing::revision(&root).unwrap(), revision);
        // A committed timing edit survives reopening; a later cancelled edit
        // restores that previous overlay, not the Whisper timing.
        begin_retake(&job, &[make(revision)], Some(6.0)).unwrap();
        std::fs::write(job.result(), b"new video").unwrap();
        discard_undo(&root).unwrap();
        let timing_before = std::fs::read(root.join("timing.json")).unwrap();
        let second = LinePatch {
            start: Some(1.6),
            end: Some(3.4),
            revision: Some(crate::timing::revision(&root).unwrap()),
            ..make(String::new())
        };
        begin_retake(&job, &[second], Some(6.0)).unwrap();
        restore_retake(&root).unwrap();
        assert_eq!(
            std::fs::read(root.join("timing.json")).unwrap(),
            timing_before
        );
        assert_eq!(crate::timing::read(&root).unwrap()[1].start, 1.5);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn retake_changes_only_the_chosen_line_and_restores_the_old_cut() {
        let root = std::env::temp_dir().join(format!(
            "retake-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("lines")).unwrap();
        let job = Job {
            root: root.clone(),
            video: root.join("clip.mp4"),
            picture: root.join("clip.mp4"),
            target: "vi".into(),
        };
        let original = vec![
            TranslatedLine {
                start: 0.0,
                end: 1.0,
                source: "a".into(),
                translation: "một".into(),
            },
            TranslatedLine {
                start: 1.0,
                end: 2.0,
                source: "b".into(),
                translation: "hai".into(),
            },
            TranslatedLine {
                start: 2.0,
                end: 3.0,
                source: "c".into(),
                translation: "ba".into(),
            },
        ];
        write_json(&job.translation_path(), &original).unwrap();
        write_json(&job.lines_path(), &original).unwrap();
        for number in 1..=3 {
            std::fs::write(job.line_wav(number), format!("wav{number}")).unwrap();
        }
        std::fs::write(job.result(), b"video").unwrap();
        std::fs::write(job.spoken(), b"audio").unwrap();
        std::fs::write(job.dub(), b"mix").unwrap();
        std::fs::write(job.captioned(), b"chu").unwrap();
        std::fs::write(root.join("history-current.json"), b"previous marker").unwrap();

        let count = begin_retake(
            &job,
            &[LinePatch {
                index: 1,
                text: " hai đã sửa ".into(),
                ..Default::default()
            }],
            None,
        )
        .unwrap();
        assert_eq!(count, (1, 1));
        let note: UndoNote = read_json(&undo_dir(&root).join("manifest.json")).unwrap();
        let history = root.join("cu").join(format!("ban-{}", note.history_new.as_ref().unwrap()));
        std::fs::create_dir_all(&history).unwrap();
        std::fs::write(history.join("clip.vi.mp4"), b"uncommitted version").unwrap();
        std::fs::write(root.join("history-current.json"), b"uncommitted marker").unwrap();
        let updated: Vec<TranslatedLine> = read_json(&job.translation_path()).unwrap();
        assert_eq!(updated[0].translation, "một");
        assert_eq!(updated[1].translation, "hai đã sửa");
        assert_eq!(updated[2].translation, "ba");
        assert!(file_ready(&job.line_wav(1)));
        assert!(!job.line_wav(2).is_file());
        assert!(file_ready(&job.line_wav(3)));
        assert!(!job.result().is_file());
        assert!(!job.spoken().is_file());
        assert!(!job.dub().is_file());
        assert!(!job.captioned().is_file());
        assert!(file_ready(&job.lines_path()));
        let kept: Vec<TranslatedLine> = read_json(&job.lines_path()).unwrap();
        assert_eq!(kept[1].translation, "hai");

        restore_retake(&root).unwrap();
        let restored: Vec<TranslatedLine> = read_json(&job.translation_path()).unwrap();
        assert_eq!(restored, original);
        assert_eq!(std::fs::read(job.line_wav(2)).unwrap(), b"wav2");
        assert_eq!(std::fs::read(job.result()).unwrap(), b"video");
        assert_eq!(std::fs::read(job.spoken()).unwrap(), b"audio");
        assert_eq!(std::fs::read(job.captioned()).unwrap(), b"chu");
        assert_eq!(std::fs::read(root.join("history-current.json")).unwrap(), b"previous marker");
        assert!(!history.exists());
        assert!(!undo_dir(&root).exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn caption_file_keeps_the_language_code() {
        assert_eq!(language_code(Path::new("tap.vi.chu.mp4")), "vi");
        assert_eq!(language_code(Path::new("tap.vi.mp4")), "vi");
    }

    #[test]
    fn text_only_retake_burns_instead_of_speaking() {
        assert!(matches!(retake_path(true), RetakePath::Burn));
        assert!(matches!(retake_path(false), RetakePath::Speak));
    }
}
