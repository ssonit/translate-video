use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use tauri::AppHandle;

use crate::job::{file_ready, Job, TranslatedLine};
use crate::ollama::{
    complete, shorten_batch_prompt, shorten_batch_texts, shorten_percent, CHUNK_LINES,
};
use crate::speech::{clean_for_speech, is_speakable};
use crate::timeline::{room, too_long, Slot};
use crate::wav::read_wav;

use super::download::{lines_end, timeline_secs};
use super::media::write_silence;
use super::translate::drop_mix;
use super::{log_line, report, report_progress, stopped};

const MAX_SHORTEN_ROUNDS: usize = 2;

pub(super) fn speak(
    app: &AppHandle,
    job: &Job,
    lines: &mut Vec<TranslatedLine>,
    choice: &crate::TranslationChoice,
    target: &str,
    _audio_only: bool,
    keep_words: bool,
    cancel: &AtomicBool,
) -> Result<(), String> {
    if file_ready(&job.result()) || file_ready(&job.spoken()) {
        report(app, "speak", "skipped", "Đã có bản dịch, bỏ qua đọc giọng.");
        return Ok(());
    }
    let total = lines.len();
    let pending = (1..=total)
        .filter(|index| !file_ready(&job.line_wav(*index)))
        .count();
    if pending == 0 && keep_words {
        report(app, "speak", "skipped", "Đã có tiếng từng câu, bỏ qua.");
        return Ok(());
    }
    stopped(cancel)?;
    if pending > 0 {
        synthesize_missing(app, job, lines, cancel)?;
    }
    if keep_words {
        report(app, "speak", "done", "Đã đọc lại lời gốc.");
        return Ok(());
    }
    let video_secs = timeline_secs(&job.video, lines_end(lines), cancel)?;
    let mut touched = pending > 0;
    for round in 1..=MAX_SHORTEN_ROUNDS {
        let slots = read_slots(job, lines)?;
        let long: Vec<usize> = (0..total)
            .filter(|&i| {
                !lines[i].translation.trim().is_empty()
                    && too_long(slots[i].wav_secs, room(&slots, i, video_secs))
            })
            .collect();
        if long.is_empty() {
            break;
        }
        touched = true;
        report(
            app,
            "speak",
            "running",
            format!("Viết lại {} câu cho vừa chỗ (lần {round}).", long.len()),
        );
        let mut changed = false;
        let long_count = long.len();
        for (chunk_index, chunk) in long.chunks(CHUNK_LINES).enumerate() {
            stopped(cancel)?;
            let done = chunk_index * CHUNK_LINES;
            report_progress(
                app,
                "speak",
                format!(
                    "Viết lại câu {}-{}/{long_count} (lần {round}).",
                    done + 1,
                    done + chunk.len()
                ),
                done as f64,
                long_count as f64,
            );
            let items: Vec<(u32, &str)> = chunk
                .iter()
                .map(|&i| {
                    (
                        shorten_percent(room(&slots, i, video_secs), slots[i].wav_secs),
                        lines[i].translation.as_str(),
                    )
                })
                .collect();
            let texts = complete(choice, &shorten_batch_prompt(&items, target), cancel)
                .ok()
                .map(|value| shorten_batch_texts(&value, chunk.len()))
                .unwrap_or_else(|| vec![None; chunk.len()]);
            stopped(cancel)?;
            for (&i, text) in chunk.iter().zip(texts) {
                let Some(text) = text else { continue };
                if text.chars().count() >= lines[i].translation.chars().count() {
                    continue;
                }
                lines[i].translation = text;
                let _ = std::fs::remove_file(job.line_wav(i + 1));
                drop_mix(job);
                changed = true;
            }
            if changed {
                crate::timing::write_words(&job.root, lines)?;
            }
        }
        if !changed {
            break;
        }
        crate::timing::write_words(&job.root, lines)?;
        synthesize_missing(app, job, lines, cancel)?;
    }
    if !touched {
        report(app, "speak", "skipped", "Đã có tiếng từng câu, bỏ qua.");
        return Ok(());
    }
    report(app, "speak", "done", "Đã đọc.");
    Ok(())
}

fn read_slots(job: &Job, lines: &[TranslatedLine]) -> Result<Vec<Slot>, String> {
    lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            let pcm = read_wav(&job.line_wav(index + 1))?;
            Ok(Slot {
                start: line.start,
                end: line.end,
                wav_secs: pcm.samples.len() as f64 / f64::from(pcm.rate),
            })
        })
        .collect()
}

pub(super) fn synthesize_missing(
    app: &AppHandle,
    job: &Job,
    lines: &[TranslatedLine],
    cancel: &AtomicBool,
) -> Result<(), String> {
    let mut batch: Vec<(usize, String, PathBuf)> = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let path = job.line_wav(index + 1);
        if file_ready(&path) {
            continue;
        }
        let text = clean_for_speech(&line.translation);
        if is_speakable(&text) {
            batch.push((index, text, path));
        } else {
            write_silence(&path, cancel)?;
        }
    }
    if batch.is_empty() {
        return Ok(());
    }
    speak_batch(app, job, lines.len(), batch, cancel)
}

pub(crate) fn speak_batch(
    app: &AppHandle,
    job: &Job,
    all_len: usize,
    mut batch: Vec<(usize, String, PathBuf)>,
    cancel: &AtomicBool,
) -> Result<(), String> {
    if batch.is_empty() {
        return Ok(());
    }
    let render=crate::render::RenderSettings::read(&job.root)?;
    for (_,text,_) in &mut batch {*text=crate::render::spoken_text(&render,&job.target,text);}
    let voice = crate::selected_voice(app, &job.target)?;
    report(
        app,
        "speak",
        "running",
        format!("Đang nạp giọng, rồi đọc {} câu.", batch.len()),
    );
    let total = batch.len();
    let already = all_len.saturating_sub(total);
    let exe = crate::piper::locate();
    let caps = exe.as_ref().and_then(|path| crate::piper::read_caps(path));
    if crate::piper::choose_engine(exe.clone(), caps) == crate::piper::SpeakEngine::Native {
        let exe = exe.expect("choose_engine Native chỉ khi có exe");
        let caps = caps.expect("choose_engine Native chỉ khi có cờ");
        let voice_dir = crate::bundled_voice_dir(&job.target).ok_or("Chưa thấy folder giọng.")?;
        let model = crate::piper::model_path(&voice_dir, &voice);
        let scales = crate::piper::scales_from_config(
            &std::fs::read_to_string(voice_dir.join(format!("{voice}.onnx.json")))
                .unwrap_or_default(),
        );
        let raw: Vec<(usize, String)> = batch
            .iter()
            .map(|(index, text, _)| (*index, text.clone()))
            .collect();
        let prepared = crate::tts::prepare(app, &job.target, &raw, cancel)?;
        let mut native_items = Vec::new();
        for (index, _, path) in &batch {
            let chunks = prepared
                .iter()
                .find(|(found, _)| found == index)
                .map(|(_, chunks)| chunks.as_slice())
                .unwrap_or(&[]);
            match crate::piper::plan_line(chunks) {
                crate::piper::LinePlan::Silence => write_silence(path, cancel)?,
                crate::piper::LinePlan::Speak(text) => {
                    native_items.push(crate::piper::SpeechLine {
                        text,
                        output: path.clone(),
                    });
                }
            }
        }
        report(
            app,
            "speak",
            "running",
            format!("Đang đọc {} câu bằng Piper trên máy.", native_items.len()),
        );
        if let Err(error) = crate::piper::speak_native(
            &exe,
            &model,
            &caps,
            scales,
            &native_items,
            cancel,
            &|done, _| {
                report_progress(
                    app,
                    "speak",
                    format!("Đã đọc {done}/{} câu.", native_items.len()),
                    (already + done) as f64,
                    all_len as f64,
                );
            },
        ) {
            if crate::piper::after_native(&error) == crate::piper::AfterNative::Stop {
                return Err(error);
            }
            log_line(
                job,
                &format!("piper.exe lỗi, đọc phần còn lại trong app: {error}"),
            );
        }
    }
    batch.retain(|(_, _, path)| !file_ready(path));
    if batch.is_empty() {
        return Ok(());
    }
    let total = batch.len();
    let already = all_len.saturating_sub(total);
    crate::tts::run(
        app,
        &voice,
        &job.target,
        &batch,
        false,
        cancel,
        &|done, _| {
            report_progress(
                app,
                "speak",
                format!("Đã đọc {done}/{total} câu."),
                (already + done) as f64,
                all_len as f64,
            );
        },
    )?;
    for (index, _, path) in &batch {
        if !file_ready(path) {
            return Err(format!("Không đọc được câu {}.", index + 1));
        }
    }
    Ok(())
}

pub(crate) fn preview_engine() -> crate::piper::SpeakEngine {
    crate::listen::listen_engine()
}

pub(crate) fn route_preview(engine: crate::piper::SpeakEngine) -> Result<(), String> {
    match engine {
        crate::piper::SpeakEngine::Window => Ok(()),
        crate::piper::SpeakEngine::Native => Err("Xem trước đọc trong cửa sổ.".into()),
    }
}

// Preview speaks in the window and reports each wav as it lands.
// Export stays on speak_batch, which can hand the whole batch to piper.exe.
pub(crate) fn speak_preview(
    app: &AppHandle,
    job: &Job,
    lines_len: usize,
    batch: Vec<(usize, String, PathBuf)>,
    cancel: &AtomicBool,
    on_saved: &dyn Fn(usize),
) -> Result<(), String> {
    if batch.is_empty() {
        return Ok(());
    }
    route_preview(preview_engine())?;
    let voice = crate::selected_voice(app, &job.target)?;
    let total = batch.len();
    let already = lines_len.saturating_sub(total);
    crate::tts::run(
        app,
        &voice,
        &job.target,
        &batch,
        false,
        cancel,
        &|saved, _| {
            report_progress(
                app,
                "speak",
                format!("Đã đọc {saved}/{total} câu."),
                (already + saved) as f64,
                lines_len as f64,
            );
            on_saved(saved);
        },
    )?;
    for (index, _, path) in &batch {
        if !file_ready(path) {
            return Err(format!("Không đọc được câu {}.", index + 1));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::route_preview;

    #[test]
    fn preview_route_rejects_the_native_engine() {
        assert!(route_preview(crate::piper::SpeakEngine::Native).is_err());
        assert!(route_preview(crate::piper::SpeakEngine::Window).is_ok());
    }
}
