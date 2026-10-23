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
#[derive(Clone, Deserialize)]
pub(crate) struct LinePatch {
    pub index: usize,
    pub text: String,
}

#[derive(Serialize, Deserialize)]
struct UndoNote {
    parked: Vec<String>,
    wavs: Vec<usize>,
    missing_wavs: Vec<usize>,
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
fn folded_edits(
    lines: &[TranslatedLine],
    edits: &[LinePatch],
) -> Result<Vec<(usize, String)>, String> {
    if edits.is_empty() {
        return Err("Chưa chọn câu nào để sửa.".into());
    }
    let mut folded = Vec::new();
    for edit in edits {
        let text = edit.text.trim().to_string();
        if text.chars().count() > 4000 {
            return Err(format!("Câu {} dài quá.", edit.index + 1));
        }
        if edit.index >= lines.len() {
            return Err(format!("Không có câu {}.", edit.index + 1));
        }
        if let Some((_, slot)) = folded.iter_mut().find(|(index, _)| *index == edit.index) {
            *slot = text;
        } else {
            folded.push((edit.index, text));
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
    let saved = undo.join("translation.json");
    if file_ready(&saved) {
        copy_file(&saved, &root.join("translation.json"))?;
    }
    for name in &note.parked {
        put_back(root, &undo, name)?;
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
    let _ = std::fs::write(undo.join("done"), b"ok");
    std::fs::remove_dir_all(&undo).map_err(|error| error.to_string())?;
    Ok(())
}

// Đổi lời các câu được chọn, cất video/audio cũ sang một bên, xóa wav của đúng những câu đó.
fn begin_retake(job: &Job, edits: &[LinePatch]) -> Result<usize, String> {
    let undo = undo_dir(&job.root);
    if undo.join("done").is_file() {
        discard_undo(&job.root)?;
    } else if undo.join("manifest.json").is_file() {
        restore_retake(&job.root)?;
    } else if undo.is_dir() {
        let _ = std::fs::remove_dir_all(&undo);
    }
    let saved: Vec<TranslatedLine> =
        read_json(&job.translation_path()).map_err(|_| "Chưa có bản dịch để sửa.".to_string())?;
    let folded = folded_edits(&saved, edits)?;
    let mut lines = saved;
    let mut wavs = Vec::new();
    let mut missing_wavs = Vec::new();
    std::fs::create_dir_all(undo.join("wav")).map_err(|error| error.to_string())?;
    copy_file(&job.translation_path(), &undo.join("translation.json"))?;
    for (index, text) in &folded {
        lines[*index].translation = text.clone();
        let number = index + 1;
        let wav = job.line_wav(number);
        if file_ready(&wav) {
            copy_file(&wav, &undo.join("wav").join(format!("{number:03}.wav")))?;
            wavs.push(number);
        } else {
            missing_wavs.push(number);
        }
    }
    let mut parked = Vec::new();
    let note = UndoNote {
        parked: Vec::new(),
        wavs: wavs.clone(),
        missing_wavs: missing_wavs.clone(),
    };
    write_json(&undo.join("manifest.json"), &note)?;
    let staged = (|| {
        for path in [
            job.fit_report(),
            job.dub(),
            job.subtitles(),
            job.spoken(),
            job.result(),
            job.captioned(),
        ] {
            park_file(&path, &undo, &mut parked)?;
        }
        write_json(
            &undo.join("manifest.json"),
            &UndoNote {
                parked: parked.clone(),
                wavs: wavs.clone(),
                missing_wavs: missing_wavs.clone(),
            },
        )?;
        write_json(&job.translation_path(), &lines)?;
        for (index, _) in &folded {
            let wav = job.line_wav(index + 1);
            if !wav.is_file() {
                continue;
            }
            std::fs::remove_file(&wav).map_err(|_| {
                format!(
                    "Câu {} đang được dùng. Đóng trình phát rồi thử lại.",
                    index + 1
                )
            })?;
        }
        Ok(folded.len())
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
    let saved: Vec<TranslatedLine> = read_json(&job.translation_path()).unwrap_or_default();
    Ok(saved
        .iter()
        .enumerate()
        .filter_map(|(index, line)| {
            let text = apply_names(&line.translation, &names);
            (text != line.translation).then(|| LinePatch { index, text })
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
    let mut lines: Vec<TranslatedLine> = read_json(&job.translation_path())?;
    let mut touched = false;
    for line in &mut lines {
        let renamed = apply_names(&line.source, &names);
        if renamed != line.source {
            line.source = renamed;
            touched = true;
        }
    }
    if touched {
        write_json(&job.translation_path(), &lines)?;
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
    let (job, audio_only, burn) = open_finished(result)?;
    let text_only = read_json::<RunSettings>(&job.settings_path())
        .ok()
        .is_some_and(|saved| saved.text_only);
    let (target_code, target_name) = clean_target(&job.target);
    if !text_only {
        crate::selected_voice(app, target_code).map_err(|_| {
            format!(
                "Chưa có giọng đọc {target_name}. Bỏ cặp file giọng Piper (.onnx và .onnx.json) vào folder voices\\{target_code}."
            )
        })?;
    }
    require_tool("ffmpeg")?;
    require_tool("ffprobe")?;
    let count = begin_retake(&job, edits)?;
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
        let lines: Vec<TranslatedLine> = read_json(&job.translation_path())?;
        match retake_path(text_only) {
            RetakePath::Burn => {
                report(app, "speak", "skipped", "Không đọc giọng.");
                report(app, "fit", "skipped", "Không co độ dài.");
                merge_text(app, &job, &lines, cancel)?;
                return Ok(job.captioned());
            }
            RetakePath::Speak => {}
        }
        report(
            app,
            "speak",
            "running",
            format!("Đang đọc lại {count} câu."),
        );
        synthesize_missing(app, &job, &lines, cancel)?;
        report(app, "speak", "done", format!("Đã đọc lại {count} câu."));
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
            // Bản mới đã nằm đúng chỗ. Cờ done khiến lần mở sau không kéo bản cũ đè lên.
            let _ = discard_undo(&job.root);
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

        let count = begin_retake(
            &job,
            &[LinePatch {
                index: 1,
                text: " hai đã sửa ".into(),
            }],
        )
        .unwrap();
        assert_eq!(count, 1);
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
