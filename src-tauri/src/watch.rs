use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::job::{file_ready, read_json, write_json, Cue, Glossary, Job, TranslatedLine};
use crate::ollama::{divide_time, split_line, TranslateTrace};
use crate::pipeline::{
    clean_target, extract, launch_for, media_duration, parse_clock, read_cues, remember_settings,
    require_tool, run_tool_with, settle_transcript, speak_preview, stopped, translate_work,
    write_silence,
};
use crate::speech::{clean_for_speech, is_speakable};

pub(crate) const FLUSH_COUNT: usize = 4;
pub(crate) const FLUSH_SPAN_SECS: f64 = 12.0;
pub(crate) const FLUSH_AHEAD_SECS: f64 = 8.0;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PrintedCue {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

pub(crate) struct Driver {
    duration: f64,
    stdout: String,
    stderr: String,
    cues: Vec<PrintedCue>,
    committed: usize,
    whisper_time: f64,
}

pub(crate) fn driver(duration_secs: f64) -> Driver {
    Driver {
        duration: duration_secs,
        stdout: String::new(),
        stderr: String::new(),
        cues: Vec::new(),
        committed: 0,
        whisper_time: 0.0,
    }
}

impl Driver {
    pub(crate) fn push_stdout(&mut self, chunk: &str) -> Vec<PrintedCue> {
        self.stdout.push_str(chunk);
        self.drain_stdout();
        self.take()
    }

    pub(crate) fn push_stderr(&mut self, chunk: &str) -> Vec<PrintedCue> {
        self.stderr.push_str(chunk);
        self.drain_stderr();
        self.take()
    }

    pub(crate) fn finish(&mut self) -> Vec<PrintedCue> {
        self.drain_stdout();
        self.drain_stderr();
        let rest = self.cues[self.committed..].to_vec();
        self.committed = self.cues.len();
        rest
    }

    pub(crate) fn push_cue(&mut self, start: f64, end: f64, text: &str) -> Vec<PrintedCue> {
        let text = text.trim().to_string();
        if text.is_empty() {
            return Vec::new();
        }
        self.cues.push(PrintedCue {
            start,
            end: end.max(start + 0.05),
            text,
        });
        self.take()
    }

    fn drain_stdout(&mut self) {
        let mut lines: Vec<String> = self
            .stdout
            .split(['\n', '\r'])
            .map(str::to_string)
            .collect();
        self.stdout = lines.pop().unwrap_or_default();
        for line in lines {
            if let Some(cue) = parse_print(&line) {
                self.cues.push(cue);
            }
        }
    }

    fn drain_stderr(&mut self) {
        let mut lines: Vec<String> = self
            .stderr
            .split(['\n', '\r'])
            .map(str::to_string)
            .collect();
        self.stderr = lines.pop().unwrap_or_default();
        for line in lines {
            if let Some(fraction) = progress_fraction(&line) {
                self.whisper_time = fraction * self.duration;
            }
        }
    }

    fn take(&mut self) -> Vec<PrintedCue> {
        let pending = &self.cues[self.committed..];
        if pending.is_empty() {
            return Vec::new();
        }
        let span = pending.last().unwrap().end - pending[0].start;
        let ahead = self.whisper_time - pending.last().unwrap().end;
        let ready =
            pending.len() >= FLUSH_COUNT || span >= FLUSH_SPAN_SECS || ahead >= FLUSH_AHEAD_SECS;
        if !ready {
            return Vec::new();
        }
        let out = pending.to_vec();
        self.committed = self.cues.len();
        out
    }
}

fn parse_print(line: &str) -> Option<PrintedCue> {
    let rest = line.trim().strip_prefix('[')?;
    let (times, text) = rest.split_once(']')?;
    let (from, to) = times.split_once("-->")?;
    let start = parse_clock(from.trim())?;
    let end = parse_clock(to.trim())?;
    let text = text.trim().to_string();
    if text.is_empty() {
        return None;
    }
    Some(PrintedCue {
        start,
        end: end.max(start + 0.05),
        text,
    })
}

fn progress_fraction(line: &str) -> Option<f64> {
    let rest = line.split("progress =").nth(1)?;
    let percent: f64 = rest.trim().trim_end_matches('%').trim().parse().ok()?;
    Some((percent / 100.0).clamp(0.0, 1.0))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CancelAction {
    pub delete_transcript: bool,
    pub delete_watch_dir: bool,
}

pub(crate) fn cancel_action(
    transcript_existed_at_start: bool,
    whisper_exit_ok: bool,
) -> CancelAction {
    CancelAction {
        delete_transcript: !transcript_existed_at_start && !whisper_exit_ok,
        delete_watch_dir: true,
    }
}

pub(crate) fn publish_allowed(line_count: usize, attempted: usize, spoken: usize) -> bool {
    line_count > 0 && attempted == line_count && spoken == line_count
}

pub(crate) fn publish_text_allowed(line_count: usize, attempted: usize) -> bool {
    line_count > 0 && attempted == line_count
}

// Câu đã sẵn để hiện. Có giọng thì dừng ở câu chưa có wav. Chỉ chữ thì lấy hết.
pub(crate) fn ready_watch_lines(
    lines: &[TranslatedLine],
    wav_ready: impl Fn(usize) -> bool,
    text_only: bool,
) -> (Vec<usize>, f64) {
    let mut indexes = Vec::new();
    let mut until = 0.0;
    for (offset, line) in lines.iter().enumerate() {
        let index = offset + 1;
        if !text_only && !wav_ready(index) {
            break;
        }
        indexes.push(index);
        until = line.end;
    }
    (indexes, until)
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WatchLine {
    index: usize,
    start: f64,
    end: f64,
    source: String,
    translation: String,
    wav: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WatchUpdate {
    video: String,
    ready_until: f64,
    running: bool,
    published: bool,
    detail: String,
    lines: Vec<WatchLine>,
    text_only: bool,
}

#[derive(Serialize)]
struct DoneMark {
    published: bool,
}

struct Accum {
    lines: Vec<Cue>,
    translated: Vec<TranslatedLine>,
    saved: Vec<TranslatedLine>,
}

pub(crate) fn should_warm_preview(text_only: bool, published: bool) -> bool {
    !text_only && !published
}

pub(crate) fn run_watch(
    app: &AppHandle,
    video: &Path,
    output_dir: &Path,
    target: &str,
    skip_translate: bool,
    text_only: bool,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let mut job = Job::open(video, output_dir)?;
    let draft_source = crate::draft::inspect_source(&job, cancel)?;
    if job.root.join("editing.json").exists() {
        return Err("Video này đã tách/gộp câu. Mở bản kết quả hoặc Xuất lại để dùng đúng câu đã sửa; Xem trước dịch trực tiếp chỉ dùng mốc gốc.".into());
    }
    let (target_code, target_name) = clean_target(target);
    job.target = target_code.into();
    let choice = crate::choice_path(app)
        .ok()
        .map(|path| crate::read_choice(&path))
        .unwrap_or_default();
    // Báo ngay khi DeepSeek local chưa chạy, trước khi chép lời hay ghi gì vào folder việc.
    crate::ollama::require_deepseek_local(&choice, skip_translate)?;
    let voice = if text_only {
        crate::selected_voice(app, target_code).unwrap_or_default()
    } else {
        crate::selected_voice(app, target_code).map_err(|_| {
            format!(
                "Chưa có giọng đọc {target_name}. Bỏ cặp file giọng Piper (.onnx và .onnx.json) vào folder voices\\{target_code}."
            )
        })?
    };
    let use_terms = crate::prefs_path(app)
        .ok()
        .map(|path| crate::read_prefs(&path).use_terms)
        .unwrap_or(false);
    remember_settings(
        &job,
        target_code,
        &voice,
        use_terms,
        skip_translate,
        text_only,
    );
    let done_detail = if text_only {
        "Đã dịch xong."
    } else if skip_translate {
        "Đã đọc xong."
    } else {
        "Đã dịch xong."
    };
    require_tool("ffmpeg")?;
    require_tool("ffprobe")?;
    stopped(cancel)?;

    let transcript_existed = file_ready(&job.transcript());
    let whisper_ok = AtomicBool::new(false);
    let outcome: Result<(), String> = (|| {
        extract(app, &job, cancel)?;
        let media = playback_of(&job);
        if let Some((lines, translated)) = already_published(&job, text_only) {
            emit_update(
                app,
                &media,
                &translated,
                |index| job.line_wav(index),
                false,
                true,
                done_detail,
                text_only,
            )?;
            let _ = lines;
            settle_transcript(&job);
            return Ok(());
        }
        if should_warm_preview(text_only, false) {
            let _ = app.emit(
                "tts-warm",
                serde_json::json!({ "lang": target_code, "voice": voice }),
            );
        }
        let duration = resolve_duration(
            if media.is_empty() {
                None
            } else {
                Some(media_duration(&job.video, cancel)?)
            },
            if media.is_empty() {
                settle_transcript(&job).as_deref().and_then(cue_span)
            } else {
                None
            },
        )?;
        emit_update(
            app,
            &media,
            &[],
            |_| PathBuf::new(),
            true,
            false,
            "Đang chép đoạn đầu",
            text_only,
        )?;
        let saved = read_json::<Vec<TranslatedLine>>(&job.translation_path()).unwrap_or_default();
        let mut state = Accum {
            lines: Vec::new(),
            translated: Vec::new(),
            saved,
        };
        let fresh_text = state.saved.is_empty();
        if transcript_existed {
            whisper_ok.store(true, Ordering::SeqCst);
            let cues = read_cues(&job.transcript())?;
            if cues.is_empty() {
                return Err("Không nghe được lời thoại.".into());
            }
            for batch in batches_from_cues(duration, &cues) {
                ingest(
                    app,
                    &job,
                    &choice,
                    target_name,
                    target_code,
                    skip_translate,
                    text_only,
                    cancel,
                    &mut state,
                    &batch,
                )?;
            }
        } else {
            stream_and_ingest(
                app,
                &job,
                &choice,
                target_name,
                target_code,
                skip_translate,
                text_only,
                cancel,
                duration,
                &whisper_ok,
                &mut state,
            )?;
        }
        if state.lines.is_empty() {
            return Err("Không nghe được lời thoại.".into());
        }
        publish(&job, &state.lines, &state.translated, text_only)?;
        settle_transcript(&job);
        if fresh_text {
            crate::text_cache::record_fresh_preview(&job, draft_source, crate::draft::RunContext {
                provider: choice.provider.clone(), model: crate::ollama::model_identity(&choice.provider, &choice.model), audio_only: false,
            }, &state.lines, &state.translated, skip_translate, cancel)?;
        }
        emit_update(
            app,
            &media,
            &state.translated,
            |index| job.line_wav(index),
            false,
            true,
            done_detail,
            text_only,
        )?;
        Ok(())
    })();
    if let Err(error) = outcome {
        let detail = if cancel.load(Ordering::SeqCst) || error == "Đã dừng." {
            "Đã dừng.".to_string()
        } else {
            error.clone()
        };
        let _ = app.emit(
            "watch-update",
            stopped_playback(&playback_of(&job), &detail),
        );
        let action = cancel_action(transcript_existed, whisper_ok.load(Ordering::SeqCst));
        if action.delete_transcript {
            let _ = std::fs::remove_file(job.transcript());
        }
        if action.delete_watch_dir {
            let _ = std::fs::remove_dir_all(job.root.join("watch"));
        }
        if cancel.load(Ordering::SeqCst) || error == "Đã dừng." {
            return Err("Đã dừng.".into());
        }
        return Err(error);
    }
    Ok(())
}

fn already_published(job: &Job, text_only: bool) -> Option<(Vec<Cue>, Vec<TranslatedLine>)> {
    if !file_ready(&job.lines_path()) || !file_ready(&job.translation_path()) {
        return None;
    }
    let lines: Vec<Cue> = read_json(&job.lines_path()).ok()?;
    let translated: Vec<TranslatedLine> = read_json(&job.translation_path()).ok()?;
    if lines.is_empty() || translated.len() != lines.len() {
        return None;
    }
    if text_only {
        return Some((lines, translated));
    }
    let ready = (1..=lines.len()).all(|index| file_ready(&job.line_wav(index)));
    ready.then_some((lines, translated))
}

fn batches_from_cues(duration: f64, cues: &[Cue]) -> Vec<Vec<PrintedCue>> {
    let mut driver = driver(duration);
    let mut batches = Vec::new();
    for cue in cues {
        let flushed = driver.push_cue(cue.start, cue.end, &cue.text);
        if !flushed.is_empty() {
            batches.push(flushed);
        }
    }
    let rest = driver.finish();
    if !rest.is_empty() {
        batches.push(rest);
    }
    batches
}

fn watch_wav(job: &Job, index: usize) -> PathBuf {
    job.root
        .join("watch")
        .join("lines")
        .join(format!("{index:03}.wav"))
}

fn playback_wav(job: &Job, index: usize) -> PathBuf {
    let watch = watch_wav(job, index);
    if file_ready(&watch) {
        watch
    } else {
        job.line_wav(index)
    }
}

fn kept_translation(saved: Option<&TranslatedLine>, source: &str) -> String {
    match saved {
        Some(line) if line.source == source => line.translation.clone(),
        _ => String::new(),
    }
}

fn should_replace_line(watch_ready: bool) -> bool {
    watch_ready
}

fn playable_media(ready: bool, path: &str) -> String {
    if ready {
        path.to_string()
    } else {
        String::new()
    }
}

fn playback_of(job: &Job) -> String {
    playable_media(file_ready(&job.video), &job.video.to_string_lossy())
}

fn cue_span(cues: &[Cue]) -> Option<f64> {
    let end = cues.iter().map(|cue| cue.end).fold(0.0, f64::max);
    (end > 0.0).then_some(end)
}

fn resolve_duration(media_secs: Option<f64>, cue_end: Option<f64>) -> Result<f64, String> {
    if let Some(secs) = media_secs {
        return Ok(secs);
    }
    if let Some(end) = cue_end {
        return Ok(end);
    }
    Err("Chưa có file tiếng. Bấm Tải video trước.".into())
}

fn stopped_playback(video: &str, detail: &str) -> WatchUpdate {
    WatchUpdate {
        video: video.to_string(),
        ready_until: 0.0,
        running: false,
        published: false,
        detail: detail.to_string(),
        lines: Vec::new(),
        text_only: false,
    }
}

fn ingest(
    app: &AppHandle,
    job: &Job,
    choice: &crate::TranslationChoice,
    target_name: &str,
    target_code: &str,
    skip_translate: bool,
    text_only: bool,
    cancel: &AtomicBool,
    state: &mut Accum,
    batch: &[PrintedCue],
) -> Result<(), String> {
    let start = state.lines.len();
    for cue in batch {
        let parts = split_line(&cue.text);
        state.lines.extend(divide_time(cue.start, cue.end, &parts));
    }
    if state.lines.len() == start {
        return Ok(());
    }
    while state.translated.len() < state.lines.len() {
        let index = state.translated.len();
        let line = &state.lines[index];
        state.translated.push(TranslatedLine {
            start: line.start,
            end: line.end,
            source: line.text.clone(),
            translation: kept_translation(state.saved.get(index), &line.text),
        });
    }
    let partial = job.root.join("watch").join("partial.json");
    if skip_translate {
        for item in &mut state.translated[start..] {
            item.translation = item.source.clone();
        }
    } else {
        let glossary = Glossary {
            theme: String::new(),
            terms: Vec::new(),
        };
        let mut trace = TranslateTrace::logging();
        let fresh_lines = state.lines[start..].to_vec();
        let fresh_done = state.translated[start..].to_vec();
        let prefix = state.translated[..start].to_vec();
        let (done, _) = translate_work(
            &fresh_lines,
            fresh_done,
            &glossary,
            choice,
            target_name,
            target_code,
            cancel,
            None,
            &mut trace,
            &mut |_detail, _done_n, _total| {},
            &mut |done| {
                let mut full = prefix.clone();
                full.extend(done.iter().cloned());
                write_json(&partial, &full)
            },
        )?;
        state.translated.splice(start.., done);
    }
    write_json(&partial, &state.translated)?;
    if !text_only {
        let mut speak = Vec::new();
        for index in start..state.lines.len() {
            let path = watch_wav(job, index + 1);
            if file_ready(&path) {
                continue;
            }
            let reused = state.saved.get(index).is_some_and(|saved| {
                saved.source == state.lines[index].text
                    && saved.translation == state.translated[index].translation
            });
            if reused && file_ready(&job.line_wav(index + 1)) {
                continue;
            }
            let text = clean_for_speech(&state.translated[index].translation);
            if is_speakable(&text) {
                speak.push((index, text, path));
            } else {
                write_silence(&path, cancel)?;
            }
        }
        if !speak.is_empty() {
            let media = playback_of(job);
            let detail = if skip_translate { "Đang đọc" } else { "Đang dịch" };
            speak_preview(app, job, state.lines.len(), speak, cancel, &|_| {
                let _ = emit_update(
                    app,
                    &media,
                    &state.translated,
                    |index| playback_wav(job, index),
                    true,
                    false,
                    detail,
                    text_only,
                );
            })?;
        }
    }
    let media = playback_of(job);
    emit_update(
        app,
        &media,
        &state.translated,
        |index| playback_wav(job, index),
        true,
        false,
        if text_only || !skip_translate {
            "Đang dịch"
        } else {
            "Đang đọc"
        },
        text_only,
    )
}

// Whisper chạy ở luồng riêng. Dịch trong callback đọc pipe sẽ nghẽn pipe.
fn stream_and_ingest(
    app: &AppHandle,
    job: &Job,
    choice: &crate::TranslationChoice,
    target_name: &str,
    target_code: &str,
    skip_translate: bool,
    text_only: bool,
    cancel: &AtomicBool,
    duration: f64,
    whisper_ok: &AtomicBool,
    state: &mut Accum,
) -> Result<(), String> {
    let abort = AtomicBool::new(false);
    let flag = AtomicBool::new(false);
    thread::scope(|scope| {
        let (sender, receiver) = mpsc::channel();
        let handle = scope.spawn(|| {
            let result = stream_whisper(job, duration, cancel, &abort, sender);
            if result.is_ok() {
                flag.store(true, Ordering::SeqCst);
            }
            result
        });
        let loop_result = recv_batches(
            app,
            job,
            choice,
            target_name,
            target_code,
            skip_translate,
            text_only,
            cancel,
            &abort,
            receiver,
            state,
        );
        abort.store(true, Ordering::SeqCst);
        let thread_result = handle
            .join()
            .unwrap_or_else(|_| Err("Whisper dừng đột ngột.".into()));
        whisper_ok.store(flag.load(Ordering::SeqCst), Ordering::SeqCst);
        match (loop_result, thread_result) {
            (Err(error), _) => Err(error),
            (Ok(()), Err(error)) => Err(error),
            (Ok(()), Ok(())) => Ok(()),
        }
    })?;
    Ok(())
}

fn recv_batches(
    app: &AppHandle,
    job: &Job,
    choice: &crate::TranslationChoice,
    target_name: &str,
    target_code: &str,
    skip_translate: bool,
    text_only: bool,
    cancel: &AtomicBool,
    abort: &AtomicBool,
    receiver: mpsc::Receiver<Vec<PrintedCue>>,
    state: &mut Accum,
) -> Result<(), String> {
    loop {
        match receiver.recv_timeout(Duration::from_millis(200)) {
            Ok(batch) => ingest(
                app,
                job,
                choice,
                target_name,
                target_code,
                skip_translate,
                text_only,
                cancel,
                state,
                &batch,
            )?,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if cancel.load(Ordering::Relaxed) {
                    abort.store(true, Ordering::Relaxed);
                    return Err("Đã dừng.".into());
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
        }
    }
}

fn stream_whisper(
    job: &Job,
    duration: f64,
    cancel: &AtomicBool,
    abort: &AtomicBool,
    sender: mpsc::Sender<Vec<PrintedCue>>,
) -> Result<(), String> {
    let launch = launch_for(&job.root, &job.audio(), &job.transcript(), Some(duration))?;
    let mut command = Command::new(&launch.program);
    command.args(&launch.args);
    if let Some(dir) = &launch.cwd {
        command.current_dir(dir);
    }
    let mut driver = driver(duration);
    // Dịch trong callback đọc pipe sẽ nghẽn pipe. Callback chỉ gửi cụm sang channel.
    let outcome = run_tool_with(
        command,
        Duration::from_secs(4 * 60 * 60),
        abort,
        &mut |is_err, text| {
            if cancel.load(Ordering::Relaxed) {
                abort.store(true, Ordering::Relaxed);
            }
            let batch = if is_err {
                driver.push_stderr(text)
            } else {
                driver.push_stdout(text)
            };
            if !batch.is_empty() {
                let _ = sender.send(batch);
            }
        },
    );
    let rest = driver.finish();
    if !rest.is_empty() {
        let _ = sender.send(rest);
    }
    outcome?;
    if !file_ready(&job.transcript()) {
        return Err("Whisper không ghi transcript.json.".into());
    }
    Ok(())
}

fn publish(
    job: &Job,
    lines: &Vec<Cue>,
    translated: &Vec<TranslatedLine>,
    text_only: bool,
) -> Result<(), String> {
    if text_only {
        if !publish_text_allowed(lines.len(), translated.len()) {
            return Err("Chưa dịch đủ câu để ghi file.".into());
        }
    } else {
        let spoken = (1..=translated.len())
            .filter(|index| {
                file_ready(&watch_wav(job, *index)) || file_ready(&job.line_wav(*index))
            })
            .count();
        if !publish_allowed(lines.len(), translated.len(), spoken) {
            return Err("Chưa đọc đủ câu để ghi file.".into());
        }
        for index in 1..=translated.len() {
            let src = watch_wav(job, index);
            if should_replace_line(file_ready(&src)) {
                std::fs::copy(&src, &job.line_wav(index)).map_err(|error| error.to_string())?;
            }
        }
    }
    write_json(&job.lines_path(), lines)?;
    write_json(&job.translation_path(), translated)?;
    let watch = job.root.join("watch");
    let _ = std::fs::remove_file(watch.join("partial.json"));
    let _ = std::fs::remove_dir_all(watch.join("lines"));
    write_json(&watch.join("done.json"), &DoneMark { published: true })?;
    Ok(())
}

fn emit_update(
    app: &AppHandle,
    media: &str,
    lines: &[TranslatedLine],
    wav_at: impl Fn(usize) -> PathBuf,
    running: bool,
    published: bool,
    detail: &str,
    text_only: bool,
) -> Result<(), String> {
    if !media.is_empty() {
        allow_path(app, Path::new(media))?;
    }
    let (indexes, ready_until) =
        ready_watch_lines(lines, |index| file_ready(&wav_at(index)), text_only);
    let mut payload = Vec::new();
    for index in indexes {
        let line = &lines[index - 1];
        let wav = wav_at(index);
        let wav_text = if file_ready(&wav) {
            allow_path(app, &wav)?;
            wav.to_string_lossy().into_owned()
        } else {
            String::new()
        };
        payload.push(WatchLine {
            index,
            start: line.start,
            end: line.end,
            source: line.source.clone(),
            translation: line.translation.clone(),
            wav: wav_text,
        });
    }
    app.emit(
        "watch-update",
        WatchUpdate {
            video: media.to_string(),
            ready_until,
            running,
            published,
            detail: detail.to_string(),
            lines: payload,
            text_only,
        },
    )
    .map_err(|error| error.to_string())
}

fn allow_path(app: &AppHandle, path: &Path) -> Result<(), String> {
    app.asset_protocol_scope()
        .allow_file(path)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stderr_progress_between_stdout_halves_keeps_the_cue() {
        let mut driver = driver(120.0);
        assert!(driver.push_stdout("[00:00:01.000 --> 00:00:0").is_empty());
        assert!(driver
            .push_stderr("whisper_print_progress_callback: progress = 10%\n")
            .is_empty());
        let got = driver.push_stdout("4.000]  Xin chào\n");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].text, "Xin chào");
        assert!((got[0].start - 1.0).abs() < 0.001);
        assert!((got[0].end - 4.0).abs() < 0.001);
    }

    #[test]
    fn four_cues_flush_even_when_whisper_time_is_still_zero() {
        let mut driver = driver(120.0);
        let block = "\
[00:00:00.000 --> 00:00:02.000]  một
[00:00:03.000 --> 00:00:05.000]  hai
[00:00:06.000 --> 00:00:08.000]  ba
[00:00:09.000 --> 00:00:11.000]  bốn
";
        let flushed = driver.push_stdout(block);
        assert_eq!(flushed.len(), 4);
        assert_eq!(flushed[3].text, "bốn");
    }

    #[test]
    fn one_cue_waits_until_whisper_is_eight_seconds_past_it() {
        let mut driver = driver(100.0);
        let held = driver.push_stdout("[00:00:01.000 --> 00:00:03.000]  một\n");
        assert!(held.is_empty());
        // 11% của 100 giây = 11 giây, vượt cuối câu (3 giây) đúng 8 giây.
        let flushed = driver.push_stderr("progress = 11%\n");
        assert_eq!(flushed.len(), 1);
        assert_eq!(flushed[0].text, "một");
    }

    #[test]
    fn span_of_twelve_seconds_flushes_before_the_fourth_cue() {
        let mut driver = driver(200.0);
        let block = "\
[00:00:00.000 --> 00:00:02.000]  một
[00:00:10.000 --> 00:00:12.000]  hai
";
        let flushed = driver.push_stdout(block);
        assert_eq!(flushed.len(), 2);
    }

    #[test]
    fn finish_flushes_the_cue_that_was_still_waiting() {
        let mut driver = driver(30.0);
        assert!(driver
            .push_stdout("[00:00:01.000 --> 00:00:02.000]  một\n")
            .is_empty());
        let flushed = driver.finish();
        assert_eq!(flushed.len(), 1);
        assert!(driver.finish().is_empty());
    }

    #[test]
    fn cancel_removes_a_transcript_this_run_did_not_finish() {
        let action = cancel_action(false, false);
        assert!(action.delete_transcript);
        assert!(action.delete_watch_dir);
    }

    #[test]
    fn cancel_keeps_a_transcript_whisper_already_finished() {
        let action = cancel_action(false, true);
        assert!(!action.delete_transcript);
        assert!(action.delete_watch_dir);
    }

    #[test]
    fn cancel_keeps_a_transcript_that_was_already_there() {
        let action = cancel_action(true, false);
        assert!(!action.delete_transcript);
    }

    #[test]
    fn a_partial_batch_is_not_publishable() {
        assert!(!publish_allowed(10, 4, 4));
        assert!(!publish_allowed(10, 10, 4));
        assert!(!publish_allowed(0, 0, 0));
        assert!(publish_allowed(10, 10, 10));
    }

    #[test]
    fn seeded_cues_flush_on_the_same_four_cue_rule() {
        let mut driver = driver(80.0);
        assert!(driver.push_cue(0.0, 2.0, "một").is_empty());
        assert!(driver.push_cue(3.0, 5.0, "hai").is_empty());
        assert!(driver.push_cue(6.0, 8.0, "ba").is_empty());
        let flushed = driver.push_cue(9.0, 11.0, "bốn");
        assert_eq!(flushed.len(), 4);
    }

    #[test]
    fn a_stopped_update_releases_the_player() {
        let update = stopped_playback("C:/video.mp4", "Đã dừng.");
        assert!(!update.running);
        assert!(!update.published);
        assert!(update.lines.is_empty());
        assert_eq!(update.ready_until, 0.0);
        assert_eq!(update.detail, "Đã dừng.");
    }

    #[test]
    fn a_saved_line_is_kept_only_when_the_source_matches() {
        let saved = TranslatedLine {
            start: 0.0,
            end: 1.0,
            source: "hello".into(),
            translation: "xin chào".into(),
        };
        assert_eq!(kept_translation(Some(&saved), "hello"), "xin chào");
        assert_eq!(kept_translation(Some(&saved), "other"), "");
        assert_eq!(kept_translation(None, "hello"), "");
    }

    #[test]
    fn a_new_watch_wav_replaces_a_canonical_line() {
        assert!(should_replace_line(true));
        assert!(!should_replace_line(false));
    }

    #[test]
    fn playback_path_is_empty_until_a_media_file_exists() {
        assert_eq!(playable_media(false, "C:/tap.mp4"), "");
        assert_eq!(playable_media(true, "C:/tap.m4a"), "C:/tap.m4a");
    }

    #[test]
    fn cue_span_is_the_latest_end() {
        let cues = vec![
            Cue {
                start: 1.0,
                end: 4.0,
                text: "một".into(),
            },
            Cue {
                start: 10.0,
                end: 12.5,
                text: "hai".into(),
            },
        ];
        assert_eq!(cue_span(&cues), Some(12.5));
        assert_eq!(cue_span(&[]), None);
    }

    #[test]
    fn duration_uses_cues_when_there_is_no_media_file() {
        assert_eq!(resolve_duration(Some(90.0), Some(12.0)).unwrap(), 90.0);
        assert_eq!(resolve_duration(None, Some(12.5)).unwrap(), 12.5);
        assert_eq!(
            resolve_duration(None, None).unwrap_err(),
            "Chưa có file tiếng. Bấm Tải video trước."
        );
    }

    fn line(start: f64, end: f64) -> TranslatedLine {
        TranslatedLine {
            start,
            end,
            source: "a".into(),
            translation: "một".into(),
        }
    }

    #[test]
    fn text_preview_lists_lines_that_have_no_wav() {
        let lines = vec![line(0.0, 1.0), line(1.0, 2.5)];
        let (indexes, until) = ready_watch_lines(&lines, |_| false, true);
        assert_eq!(indexes, vec![1, 2]);
        assert_eq!(until, 2.5);
        let (indexes, until) = ready_watch_lines(&lines, |_| false, false);
        assert!(indexes.is_empty());
        assert_eq!(until, 0.0);
    }

    #[test]
    fn text_preview_publishes_without_wavs() {
        assert!(publish_text_allowed(2, 2));
        assert!(!publish_text_allowed(2, 1));
        assert!(!publish_text_allowed(0, 0));
    }

    #[test]
    fn the_first_ready_line_plays_before_the_next_wav_exists() {
        let lines = vec![line(0.0, 1.2), line(1.2, 3.0)];
        let (indexes, until) = ready_watch_lines(&lines, |index| index == 1, false);
        assert_eq!(indexes, vec![1]);
        assert_eq!(until, 1.2);
    }

    #[test]
    fn silence_shows_and_a_missing_later_line_still_stops_the_list() {
        let lines = vec![line(0.0, 1.0), line(1.0, 2.0), line(2.0, 3.5)];
        let (indexes, until) = ready_watch_lines(&lines, |index| index < 3, false);
        assert_eq!(indexes, vec![1, 2]);
        assert_eq!(until, 2.0);
    }

    #[test]
    fn a_finished_preview_does_not_load_the_voice() {
        assert!(!should_warm_preview(false, true));
        assert!(!should_warm_preview(true, false));
        assert!(should_warm_preview(false, false));
    }

    #[test]
    fn preview_speech_stays_in_the_window() {
        assert_eq!(
            crate::pipeline::preview_engine(),
            crate::piper::SpeakEngine::Window
        );
    }
}
