mod assemble;
mod download;
mod media;
mod retake;
mod speak;
mod translate;

pub(crate) use assemble::{burn_saved, join_listen, save_spoken, script_for};
pub(crate) use download::{attach_video, download, link_title, prepare_listen, DownloadEvent};
pub(crate) use media::{
    detect_fast_path, extract, launch_for, media_duration, parse_clock, read_cues, reclaim_output,
    run_tool_with, settle_transcript, write_silence,
};
pub(crate) use retake::{retake_saved_names, revise, saved_name_edits, LinePatch};
#[cfg(test)]
pub(crate) use speak::preview_engine;
pub(crate) use speak::speak_preview;
pub(crate) use translate::{prepare_again, translate_work};

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::DubControl;

use crate::job::{file_ready, Glossary, Job};

use assemble::{export_audio, fit, merge};
use media::{split, transcribe};
use speak::speak;
use translate::{keep_source, names_step, terms, translate};

#[derive(Clone, Serialize)]
pub(crate) struct StepEvent {
    id: &'static str,
    label: String,
    state: &'static str,
    detail: String,
    // Fraction 0..1 of this step, or None while it cannot be measured.
    progress: Option<f64>,
}

#[derive(Serialize, serde::Deserialize, PartialEq)]
pub(super) struct RunSettings {
    pub(super) target: String,
    pub(super) voice: String,
    // false: bỏ bước thuật ngữ. File job cũ không có trường này thì cũng là bỏ.
    #[serde(default)]
    pub(super) use_terms: bool,
    // true: giữ nguyên lời đã chép, không gọi model dịch. File job cũ không có trường này thì vẫn dịch.
    #[serde(default)]
    pub(super) skip_translate: bool,
    // true: đốt chữ lên video gốc, không đọc giọng. File job cũ không có trường này thì vẫn lồng tiếng.
    #[serde(default)]
    pub(super) text_only: bool,
}

// Target languages the translation can be asked for: (code, name used inside the prompts).
const TARGETS: [(&str, &str); 12] = [
    ("vi", "tiếng Việt"),
    ("en", "tiếng Anh"),
    ("zh", "tiếng Trung"),
    ("ja", "tiếng Nhật"),
    ("ko", "tiếng Hàn"),
    ("fr", "tiếng Pháp"),
    ("de", "tiếng Đức"),
    ("es", "tiếng Tây Ban Nha"),
    ("ru", "tiếng Nga"),
    ("th", "tiếng Thái"),
    ("id", "tiếng Indonesia"),
    ("pt", "tiếng Bồ Đào Nha"),
];

// Unknown code falls back to Vietnamese.
pub(crate) fn clean_target(raw: &str) -> (&'static str, &'static str) {
    let code = raw.trim().to_ascii_lowercase();
    TARGETS
        .iter()
        .copied()
        .find(|(known, _)| *known == code)
        .unwrap_or(TARGETS[0])
}

// A folder reused with another target language or voice must not keep the files made with the old one.
fn drop_stale(job: &Job, now: &RunSettings) {
    let before: Option<RunSettings> = crate::job::read_json(&job.settings_path()).ok();
    if let Some(before) = before {
        let remove = |path: PathBuf| {
            let _ = std::fs::remove_file(path);
        };
        let voice_changed = before.voice != now.voice;
        let text_changed = before.target != now.target
            || before.use_terms != now.use_terms
            || before.skip_translate != now.skip_translate;
        let mode_changed = before.text_only != now.text_only;
        if mode_changed {
            for path in [
                job.result(),
                job.captioned(),
                job.dub(),
                job.spoken(),
                job.fit_report(),
            ] {
                remove(path);
            }
        }
        if text_changed {
            for path in [
                job.terms_path(),
                job.translation_path(),
                job.subtitles(),
                job.result(),
                job.captioned(),
                job.spoken(),
            ] {
                remove(path);
            }
        }
        if voice_changed || text_changed {
            remove(job.fit_report());
            remove(job.dub());
            remove(job.spoken());
            if let Ok(entries) = std::fs::read_dir(job.root.join("lines")) {
                for entry in entries.flatten() {
                    if entry.path().extension().is_some_and(|ext| ext == "wav") {
                        remove(entry.path());
                    }
                }
            }
        }
    }
    let _ = crate::job::write_json(&job.settings_path(), now);
}

pub(crate) fn remember_settings(
    job: &Job,
    target: &str,
    voice: &str,
    use_terms: bool,
    skip_translate: bool,
    text_only: bool,
) {
    drop_stale(
        job,
        &RunSettings {
            target: target.to_string(),
            voice: voice.to_string(),
            use_terms,
            skip_translate,
            text_only,
        },
    );
}

pub(super) fn reject_both_modes(audio_only: bool, text_only: bool) -> Result<(), String> {
    if audio_only && text_only {
        Err("Chỉ chọn một: chỉ audio hoặc chỉ hiện chữ.".into())
    } else {
        Ok(())
    }
}
fn drop_translation(job: &Job) {
    let remove = |path: PathBuf| {
        let _ = std::fs::remove_file(path);
    };
    for path in [
        job.translation_path(),
        job.fit_report(),
        job.dub(),
        job.spoken(),
        job.subtitles(),
        job.result(),
        job.captioned(),
    ] {
        remove(path);
    }
    if let Ok(entries) = std::fs::read_dir(job.root.join("lines")) {
        for entry in entries.flatten() {
            if entry.path().extension().is_some_and(|ext| ext == "wav") {
                remove(entry.path());
            }
        }
    }
}

// Đổi tên sau khi đã dịch: bỏ bản dịch và bảng thuật ngữ cũ. Lời chép và names.json giữ nguyên.
pub(crate) fn drop_after_names(job: &Job) {
    drop_translation(job);
    let _ = std::fs::remove_file(job.terms_path());
}

pub(crate) fn run(
    app: &AppHandle,
    video: &Path,
    output_dir: &Path,
    target: &str,
    redo: bool,
    use_terms: bool,
    burn: bool,
    audio_only: bool,
    text_only: bool,
    skip_translate: bool,
    cancel: &AtomicBool,
) -> Result<String, String> {
    let mut job = Job::open(video, output_dir)?;
    let (target_code, target_name) = clean_target(target);
    job.target = target_code.into();
    let choice = crate::choice_path(app)
        .ok()
        .map(|path| crate::read_choice(&path))
        .unwrap_or_default();
    // Báo ngay khi DeepSeek local chưa chạy, trước khi xóa bản cũ hay tốn thời gian chép lời.
    crate::ollama::require_deepseek_local(&choice, skip_translate)?;
    if redo {
        drop_translation(&job);
    }
    reject_both_modes(audio_only, text_only)?;
    if !text_only {
        crate::selected_voice(app, target_code).map_err(|_| {
            format!(
                "Chưa có giọng đọc {target_name}. Bỏ cặp file giọng Piper (.onnx và .onnx.json) vào folder voices\\{target_code}."
            )
        })?;
    }
    drop_stale(
        &job,
        &RunSettings {
            target: target_code.into(),
            voice: crate::selected_voice(app, target_code).unwrap_or_default(),
            use_terms,
            skip_translate,
            text_only,
        },
    );
    require_tool("ffmpeg")?;
    require_tool("ffprobe")?;
    stopped(cancel)?;

    let mut clock = RunClock {
        job: &job,
        started: Instant::now(),
        steps: Vec::new(),
    };
    timed(&mut clock, "extract", || extract(app, &job, cancel))
        .map_err(|error| fail(app, "extract", error))?;
    let cues = timed(&mut clock, "transcribe", || {
        transcribe(app, &job, "auto", cancel)
    })
    .map_err(|error| fail(app, "transcribe", error))?;
    let lines = timed(&mut clock, "split", || split(app, &job, &cues, cancel))
        .map_err(|error| fail(app, "split", error))?;
    let lines = timed(&mut clock, "names", || {
        names_step(app, &job, &lines, &choice, cancel)
    })
    .map_err(|error| fail(app, "names", error))?;
    let glossary = timed(&mut clock, "terms", || {
        if skip_translate {
            report(app, "terms", "skipped", "Không dịch, bỏ qua thuật ngữ.");
            return Ok(Glossary {
                theme: String::new(),
                terms: Vec::new(),
            });
        }
        terms(app, &job, &lines, &choice, target_name, use_terms, cancel)
    })
    .map_err(|error| fail(app, "terms", error))?;
    let mut translated = timed(&mut clock, "translate", || {
        if skip_translate {
            return keep_source(app, &job, &lines);
        }
        translate(
            app,
            &job,
            &lines,
            &glossary,
            &choice,
            target_name,
            target_code,
            cancel,
        )
    })
    .map_err(|error| fail(app, "translate", error))?;
    if text_only {
        report(app, "speak", "skipped", "Không đọc giọng.");
        report(app, "fit", "skipped", "Không co độ dài.");
        timed(&mut clock, "merge", || {
            assemble::merge_text(app, &job, &translated, cancel)
        })
        .map_err(|error| fail(app, "merge", error))?;
        drop(clock);
        let captioned = job.captioned();
        if !file_ready(&captioned) {
            return Err("Không ghi được video có chữ.".into());
        }
        return Ok(captioned.to_string_lossy().into_owned());
    }
    timed(&mut clock, "speak", || {
        speak(
            app,
            &job,
            &mut translated,
            &choice,
            target_name,
            audio_only,
            skip_translate,
            cancel,
        )
    })
    .map_err(|error| fail(app, "speak", error))?;
    timed(&mut clock, "fit", || {
        fit(app, &job, &translated, audio_only, cancel)
    })
    .map_err(|error| fail(app, "fit", error))?;
    if audio_only {
        timed(&mut clock, "merge", || {
            export_audio(app, &job, &translated, cancel)
        })
        .map_err(|error| {
            report_named(app, "merge", "Xuất audio", "error", error.clone());
            error
        })?;
        drop(clock);
        let spoken = job.spoken();
        if !file_ready(&spoken) {
            return Err("Không ghi được audio bản dịch.".into());
        }
        return Ok(spoken.to_string_lossy().into_owned());
    }
    timed(&mut clock, "merge", || {
        merge(app, &job, &translated, burn, cancel)
    })
    .map_err(|error| fail(app, "merge", error))?;
    drop(clock);
    let played = if burn && file_ready(&job.captioned()) {
        job.captioned()
    } else {
        job.result()
    };
    Ok(played.to_string_lossy().into_owned())
}

struct StepTime {
    id: &'static str,
    took: Duration,
}

// Logs each step as it finishes, then the total and which steps took the most time.
struct RunClock<'a> {
    job: &'a Job,
    started: Instant,
    steps: Vec<StepTime>,
}

impl Drop for RunClock<'_> {
    fn drop(&mut self) {
        log_total(self.job, self.started.elapsed(), &self.steps);
    }
}

fn readable(took: Duration) -> String {
    let secs = took.as_secs();
    let hours = secs / 3600;
    let minutes = (secs % 3600) / 60;
    let seconds = secs % 60;
    if hours > 0 {
        format!("{hours} giờ {minutes} phút {seconds} giây")
    } else if minutes > 0 {
        format!("{minutes} phút {seconds} giây")
    } else {
        format!("{:.1} giây", took.as_secs_f64())
    }
}

fn log_total(job: &Job, took: Duration, steps: &[StepTime]) {
    let name = job
        .video
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "video".into());
    log_line(
        job,
        &format!(
            "tổng: {} ({:.1}s) — {name}",
            readable(took),
            took.as_secs_f64()
        ),
    );
    let whole = took.as_secs_f64().max(0.001);
    let mut ranked: Vec<&StepTime> = steps
        .iter()
        .filter(|step| step.took.as_secs_f64() >= 0.5)
        .collect();
    ranked.sort_by(|left, right| right.took.cmp(&left.took));
    if ranked.is_empty() {
        return;
    }
    let slow: Vec<String> = ranked
        .iter()
        .map(|step| {
            format!(
                "{} {:.0}%",
                label(step.id),
                step.took.as_secs_f64() / whole * 100.0
            )
        })
        .collect();
    log_line(job, &format!("lâu nhất: {}", slow.join(" · ")));
}

// Appends one line per step to timing.log in the job folder, so slow steps show up in real runs.
fn log_timing(job: &Job, step: &str, took: Duration) {
    let shown = readable(took);
    let note = if took.as_secs_f64() < 0.05 {
        "bỏ qua"
    } else {
        shown.as_str()
    };
    log_line(
        job,
        &format!("{}: {note} ({:.1}s)", label(step), took.as_secs_f64()),
    );
}

fn log_line(job: &Job, line: &str) {
    use std::io::Write;
    eprintln!("[timing] {line}");
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(job.timing_log())
    {
        let _ = writeln!(file, "{line}");
    }
}

fn timed<T>(clock: &mut RunClock, step: &'static str, work: impl FnOnce() -> T) -> T {
    let started = Instant::now();
    let result = work();
    let took = started.elapsed();
    clock.steps.push(StepTime { id: step, took });
    log_timing(clock.job, step, took);
    result
}

fn label(id: &str) -> &'static str {
    match id {
        "extract" => "Tách tiếng",
        "transcribe" => "Chép lời",
        "split" => "Ngắt câu",
        "names" => "Tên",
        "terms" => "Thuật ngữ",
        "translate" => "Dịch",
        "speak" => "Đọc giọng",
        "fit" => "Co độ dài",
        "merge" => "Ghép video",
        _ => "Bước",
    }
}

fn report(app: &AppHandle, id: &'static str, state: &'static str, detail: impl Into<String>) {
    emit(app, id, state, detail.into(), None);
}

fn report_named(
    app: &AppHandle,
    id: &'static str,
    step_label: &'static str,
    state: &'static str,
    detail: impl Into<String>,
) {
    emit_named(app, id, step_label, state, detail.into(), None);
}

fn report_progress(
    app: &AppHandle,
    id: &'static str,
    detail: impl Into<String>,
    done: f64,
    total: f64,
) {
    let fraction = if total > 0.0 {
        (done / total).clamp(0.0, 1.0)
    } else {
        0.0
    };
    emit(app, id, "running", detail.into(), Some(fraction));
}

fn report_named_progress(
    app: &AppHandle,
    id: &'static str,
    step_label: &'static str,
    detail: impl Into<String>,
    fraction: f64,
) {
    emit_named(
        app,
        id,
        step_label,
        "running",
        detail.into(),
        Some(fraction.clamp(0.0, 1.0)),
    );
}

fn emit(
    app: &AppHandle,
    id: &'static str,
    state: &'static str,
    detail: String,
    progress: Option<f64>,
) {
    emit_named(app, id, label(id), state, detail, progress);
}

fn emit_named(
    app: &AppHandle,
    id: &'static str,
    step_label: &'static str,
    state: &'static str,
    detail: String,
    progress: Option<f64>,
) {
    let mut event = StepEvent {
        id,
        label: step_label.to_string(),
        state,
        detail,
        progress,
    };
    app.state::<DubControl>().update(|snap| {
        match snap.steps.iter_mut().find(|step| step.id == id) {
            Some(slot) => {
                // Same rule as the window: a message without a fraction keeps the last one while the step runs.
                if state == "running" && event.progress.is_none() {
                    event.progress = slot.progress;
                }
                *slot = event.clone();
            }
            None => snap.steps.push(event.clone()),
        }
    });
    let _ = app.emit("dub-progress", event);
}

fn fail(app: &AppHandle, id: &'static str, error: String) -> String {
    report(app, id, "error", error.clone());
    error
}

pub(crate) fn stopped(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        Err("Đã dừng.".into())
    } else {
        Ok(())
    }
}

pub(crate) fn require_tool(name: &str) -> Result<(), String> {
    if crate::program_path(name).is_some() {
        Ok(())
    } else {
        Err(format!("Chưa thấy {name} trong PATH."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_job(name: &str) -> Job {
        let root = std::env::temp_dir().join(format!(
            "text-only-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("lines")).unwrap();
        Job {
            root: root.clone(),
            video: root.join("tap.mp4"),
            picture: root.join("tap.mp4"),
            target: "vi".into(),
        }
    }

    #[test]
    fn flipping_text_only_drops_finished_media_and_keeps_the_translation() {
        let job = scratch_job("flip");
        crate::job::write_json(
            &job.settings_path(),
            &RunSettings {
                target: "vi".into(),
                voice: "a".into(),
                use_terms: false,
                skip_translate: false,
                text_only: false,
            },
        )
        .unwrap();
        for path in [
            job.result(),
            job.captioned(),
            job.dub(),
            job.spoken(),
            job.fit_report(),
            job.translation_path(),
            job.line_wav(1),
        ] {
            std::fs::write(&path, b"x").unwrap();
        }
        remember_settings(&job, "vi", "a", false, false, true);
        assert!(!job.result().is_file());
        assert!(!job.captioned().is_file());
        assert!(!job.dub().is_file());
        assert!(!job.spoken().is_file());
        assert!(!job.fit_report().is_file());
        assert!(file_ready(&job.translation_path()));
        assert!(file_ready(&job.line_wav(1)));
        let saved: RunSettings = crate::job::read_json(&job.settings_path()).unwrap();
        assert!(saved.text_only);
        let _ = std::fs::remove_dir_all(&job.root);
    }

    #[test]
    fn old_settings_without_text_only_read_as_off() {
        let job = scratch_job("old");
        std::fs::write(
            job.settings_path(),
            r#"{"target":"vi","voice":"a","use_terms":false,"skip_translate":false}"#,
        )
        .unwrap();
        let saved: RunSettings = crate::job::read_json(&job.settings_path()).unwrap();
        assert!(!saved.text_only);
        let _ = std::fs::remove_dir_all(&job.root);
    }

    #[test]
    fn both_output_modes_are_rejected() {
        let error = reject_both_modes(true, true).unwrap_err();
        assert_eq!(error, "Chỉ chọn một: chỉ audio hoặc chỉ hiện chữ.");
        assert!(reject_both_modes(false, true).is_ok());
        assert!(reject_both_modes(true, false).is_ok());
    }
}
