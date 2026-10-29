use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use tauri::{AppHandle, Emitter};

use crate::fit_report::FitEntry;
use crate::job::{
    file_ready, is_listen_audio, is_picture_ext, read_json, write_json, Job, TranslatedLine,
};
use crate::listen::{group_passages, listen_engine, speak_indexes, ListenPlan, SpeakSet};
use crate::timeline::{place_all, Slot};
use crate::wav::{
    join_passages, join_story, mix_at, passage_spans, read_wav, resample_linear, story_spans,
    write_wav, PassageClip, Pcm, StoryPiece,
};

use super::download::{lines_end, timeline_secs, DownloadEvent};
use super::fit_batch::{FitBatch, FIT_BATCH_LINES};
use super::media::{drop_fit_wavs, media_duration, release_mix, run_ffmpeg_progress, write_srt};
use super::retake::open_finished;
use super::{report, report_named, report_named_progress, report_progress, require_tool, stopped};

pub(super) fn fit_filter(speed: f64, keep: Option<f64>) -> String {
    let mut parts = Vec::new();
    if (speed - 1.0).abs() > 0.001 {
        parts.push(format!("atempo={speed:.3}"));
    }
    if let Some(secs) = keep {
        let fade = secs.min(0.05);
        parts.push(format!("atrim=end={secs:.3}"));
        parts.push(format!(
            "afade=t=out:st={:.3}:d={fade:.3}",
            (secs - fade).max(0.0)
        ));
    }
    // Fade 15ms at both ends of every clip, so clips never start or stop with a click.
    parts.push("afade=t=in:d=0.015".into());
    parts.push("areverse,afade=t=in:d=0.015,areverse".into());
    parts.join(",")
}

const FIT_SYNC: &str = "sync";
const FIT_STORY: &str = "story";
const STORY_STEP: &str = "Nối câu";

fn fit_mode_path(root: &Path) -> PathBuf {
    root.join("fit-mode.txt")
}

fn read_fit_mode(root: &Path) -> String {
    std::fs::read_to_string(fit_mode_path(root))
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn write_fit_mode(job: &Job, mode: &str) {
    let _ = std::fs::write(fit_mode_path(&job.root), mode);
}

// Bản audio cũ xếp theo mốc hình. Lần chạy sau bỏ bản đó để nối lại thành một mạch.
fn settle_fit_mode(job: &Job, audio_only: bool) {
    let have = read_fit_mode(&job.root);
    let ok = if audio_only {
        have == FIT_STORY
    } else {
        have.is_empty() || have == FIT_SYNC
    };
    if ok {
        return;
    }
    for path in [job.dub(), job.spoken(), job.fit_report(), job.subtitles()] {
        let _ = std::fs::remove_file(path);
    }
    if !audio_only {
        let _ = std::fs::remove_file(job.result());
        let _ = std::fs::remove_file(job.captioned());
    }
}

fn load_story_pieces(
    job: &Job,
    lines: &[TranslatedLine],
) -> Result<(u32, Vec<StoryPiece>), String> {
    let mut pieces = Vec::with_capacity(lines.len());
    let mut rate = 0u32;
    for (index, line) in lines.iter().enumerate() {
        let path = job.line_wav(index + 1);
        if !file_ready(&path) {
            return Err(format!("Thiếu tiếng của câu {}.", index + 1));
        }
        let pcm = read_wav(&path)?;
        if rate == 0 {
            rate = pcm.rate.max(1);
        }
        let samples = if pcm.rate == rate {
            pcm.samples
        } else {
            resample_linear(&pcm.samples, pcm.rate, rate)
        };
        let gap_before = if index == 0 {
            0.0
        } else {
            (line.start - lines[index - 1].end).max(0.0)
        };
        pieces.push(StoryPiece {
            gap_before,
            samples,
        });
    }
    Ok((rate.max(1), pieces))
}

fn story_captions(job: &Job, lines: &[TranslatedLine]) -> Result<Vec<TranslatedLine>, String> {
    let (rate, pieces) = load_story_pieces(job, lines)?;
    let spans = story_spans(rate, &pieces);
    if spans.len() != lines.len() {
        return Err("Không khớp được mốc câu của audio.".into());
    }
    Ok(lines
        .iter()
        .zip(spans)
        .map(|(line, span)| TranslatedLine {
            start: span.start,
            end: span.end.max(span.start),
            source: line.source.clone(),
            translation: line.translation.clone(),
        })
        .collect())
}

fn fit_story(
    app: &AppHandle,
    job: &Job,
    lines: &[TranslatedLine],
    cancel: &AtomicBool,
) -> Result<(), String> {
    stopped(cancel)?;
    report_named(
        app,
        "fit",
        STORY_STEP,
        "running",
        "Đang nối các câu thành một mạch.",
    );
    let (rate, pieces) = load_story_pieces(job, lines)?;
    stopped(cancel)?;
    let samples = join_story(rate, &pieces);
    write_wav(&job.dub(), &Pcm { rate, samples })?;
    write_fit_mode(job, FIT_STORY);
    let _ = std::fs::remove_file(job.fit_report());
    report_named(app, "fit", STORY_STEP, "done", "Đã nối liền các câu.");
    Ok(())
}

pub(super) fn fit(
    app: &AppHandle,
    job: &Job,
    lines: &[TranslatedLine],
    audio_only: bool,
    cancel: &AtomicBool,
) -> Result<(), String> {
    settle_fit_mode(job, audio_only);
    let finished = if audio_only {
        file_ready(&job.spoken())
    } else {
        file_ready(&job.result()) || file_ready(&job.spoken())
    };
    if finished {
        release_mix(&job.root);
        let detail = if audio_only {
            "Đã có audio, bỏ qua nối câu."
        } else {
            "Đã có bản dịch, bỏ qua xếp tiếng."
        };
        if audio_only {
            report_named(app, "fit", STORY_STEP, "skipped", detail);
        } else {
            report(app, "fit", "skipped", detail);
        }
        return Ok(());
    }
    if file_ready(&job.dub()) {
        drop_fit_wavs(&job.root);
        if audio_only {
            report_named(app, "fit", STORY_STEP, "skipped", "Đã nối câu, bỏ qua.");
        } else {
            report(app, "fit", "skipped", "Đã có dub.wav, bỏ qua.");
        }
        return Ok(());
    }
    if audio_only {
        return fit_story(app, job, lines, cancel);
    }
    stopped(cancel)?;
    report(app, "fit", "running", "Đang xếp tiếng vào đúng chỗ.");
    let video_secs = timeline_secs(&job.video, lines_end(lines), cancel)?;
    let mut slots = Vec::with_capacity(lines.len());
    for (index, line) in lines.iter().enumerate() {
        let wav = job.line_wav(index + 1);
        if !file_ready(&wav) {
            return Err(format!("Thiếu tiếng của câu {}.", index + 1));
        }
        let pcm = read_wav(&wav)?;
        slots.push(Slot {
            start: line.start,
            end: line.end,
            wav_secs: pcm.samples.len() as f64 / f64::from(pcm.rate),
        });
    }
    let placements = place_all(&slots, video_secs);
    let mut canvas: Vec<i16> = Vec::new();
    let mut rate = 22050;
    let mut cuts = Vec::new();
    for start in (0..placements.len()).step_by(FIT_BATCH_LINES) {
        stopped(cancel)?;
        let end = (start + FIT_BATCH_LINES).min(placements.len());
        report_progress(
            app,
            "fit",
            format!("Câu {}-{end}/{}.", start + 1, lines.len()),
            start as f64,
            lines.len() as f64,
        );
        let sources: Vec<_> = (start..end).map(|index| job.line_wav(index + 1)).collect();
        let batch = FitBatch::render(
            &job.root.join("lines"),
            &sources,
            &placements[start..end],
            cancel,
        )?;
        for (offset, fitted) in batch.paths.iter().enumerate() {
            stopped(cancel)?;
            let index = start + offset;
            let place = &placements[index];
            let pcm = read_wav(fitted)?;
            rate = pcm.rate;
            mix_at(&mut canvas, rate, place.start, &pcm.samples);
            if let Some(entry) = FitEntry::placed(
                index + 1,
                &lines[index].translation,
                slots[index].wav_secs,
                place,
            ) {
                cuts.push(entry);
            }
        }
    }
    let wanted = (video_secs * f64::from(rate)).ceil() as usize;
    if canvas.len() < wanted {
        canvas.resize(wanted, 0);
    }
    write_wav(
        &job.dub(),
        &Pcm {
            rate,
            samples: canvas,
        },
    )?;
    let spans: Vec<_> = placements.iter().map(|p| crate::audio_mix::VoiceSpan { start:p.start, end:p.start+p.played }).collect();
    write_json(&job.root.join("voice-spans.json"), &spans)?;
    drop_fit_wavs(&job.root);
    write_fit_mode(job, FIT_SYNC);
    write_json(&job.fit_report(), &cuts)?;
    let cut_count = cuts.iter().filter(|entry| entry.is_cut()).count();
    let detail = if cuts.is_empty() {
        "Đã xếp tiếng.".to_string()
    } else {
        format!(
            "Đã xếp tiếng. {} câu đã tăng tốc hoặc cắt bớt ({} câu bị cắt). Xem lại trong danh sách lời thoại.",
            cuts.len(), cut_count
        )
    };
    report(app, "fit", "done", detail);
    Ok(())
}

const AUDIO_STEP: &str = "Xuất audio";

// Bỏ ghép video. Ghi file .m4a từ dub.wav để người dùng tải audio bản dịch.
pub(super) fn export_audio(
    app: &AppHandle,
    job: &Job,
    lines: &[TranslatedLine],
    cancel: &AtomicBool,
) -> Result<(), String> {
    let captions = story_captions(job, lines).unwrap_or_else(|_| lines.to_vec());
    write_srt(&job.subtitles(), &captions)?;
    let spoken = job.spoken();
    if file_ready(&spoken) {
        release_mix(&job.root);
        report_named(
            app,
            "merge",
            AUDIO_STEP,
            "skipped",
            "Đã có audio bản dịch, bỏ qua.",
        );
        return Ok(());
    }
    let source = if file_ready(&job.dub()) {
        job.dub()
    } else if file_ready(&job.result()) {
        job.result()
    } else {
        return Err("Chưa có tiếng đã lồng.".into());
    };
    stopped(cancel)?;
    report_named(
        app,
        "merge",
        AUDIO_STEP,
        "running",
        "Đang xuất audio bản dịch.",
    );
    encode_spoken(&source, &spoken, cancel, &mut |fraction| {
        report_named_progress(
            app,
            "merge",
            AUDIO_STEP,
            "Đang xuất audio bản dịch.",
            fraction,
        );
    })?;
    release_mix(&job.root);
    report_named(
        app,
        "merge",
        AUDIO_STEP,
        "done",
        "Audio bản dịch nằm trong folder việc.",
    );
    Ok(())
}

// File .m4a nằm cạnh video kết quả: `tap.vi.mp4` và `tap.vi.chu.mp4` đều ra `tap.vi.m4a`.
pub(crate) fn spoken_beside(result: &Path) -> PathBuf {
    plain_result(result).with_extension("m4a")
}

// Audio nối liền, cạnh video nhưng không thay file .m4a của bước xuất. `tap.vi.mp4` ra `tap.vi.nghe.m4a`.
fn listen_beside(result: &Path) -> PathBuf {
    let plain = plain_result(result);
    let stem = plain
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("video");
    plain.with_file_name(format!("{stem}.nghe.m4a"))
}

pub(super) fn drop_listen_export(job: &Job) {
    let audio = listen_beside(&job.result());
    let _ = std::fs::remove_file(audio.with_extension("srt"));
    let _ = std::fs::remove_file(audio);
}

fn listen_dir(job: &Job) -> PathBuf {
    job.root.join("listen")
}

fn listen_manifest(job: &Job) -> PathBuf {
    listen_dir(job).join("manifest.json")
}

fn listen_wav(job: &Job, index: usize) -> PathBuf {
    listen_dir(job).join(format!("{:03}.wav", index + 1))
}

fn clear_listen_wavs(job: &Job) {
    let Ok(entries) = std::fs::read_dir(listen_dir(job)) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("wav") {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn load_passage_clips(job: &Job, plan: &ListenPlan) -> Result<(u32, Vec<PassageClip>), String> {
    if plan.passages.is_empty() {
        return Err("Chưa có lời đã dịch.".into());
    }
    let mut rate = 0u32;
    let mut clips = Vec::with_capacity(plan.passages.len());
    for (index, passage) in plan.passages.iter().enumerate() {
        let pcm = read_wav(&listen_wav(job, index))
            .map_err(|_| format!("Chưa đọc xong đoạn {}.", index + 1))?;
        if rate == 0 {
            rate = pcm.rate.max(1);
        }
        let samples = if pcm.rate == rate {
            pcm.samples
        } else {
            resample_linear(&pcm.samples, pcm.rate, rate)
        };
        clips.push(PassageClip {
            rest_before: passage.rest_before,
            samples,
        });
    }
    Ok((rate.max(1), clips))
}

fn passage_cues(plan: &ListenPlan, rate: u32, clips: &[PassageClip]) -> Vec<TranslatedLine> {
    plan.passages
        .iter()
        .zip(passage_spans(rate, clips))
        .map(|(passage, span)| TranslatedLine {
            start: span.start,
            end: span.end.max(span.start),
            source: String::new(),
            translation: passage.text.clone(),
        })
        .collect()
}

// Ghép wav đoạn đã đọc. Giữ video, phụ đề theo hình, và cờ xếp tiếng theo hình.
fn assemble_listen(job: &Job, _lines: &[TranslatedLine]) -> Result<PathBuf, String> {
    let plan: ListenPlan = read_json(&listen_manifest(job))?;
    let (rate, clips) = load_passage_clips(job, &plan)?;
    let samples = join_passages(rate, &clips);
    let wav = job.root.join("_listen.wav");
    write_wav(&wav, &Pcm { rate, samples })?;
    write_srt(
        &listen_beside(&job.result()).with_extension("srt"),
        &passage_cues(&plan, rate, &clips),
    )?;
    let _ = std::fs::remove_file(job.dub());
    Ok(wav)
}

fn listen_cues(job: &Job) -> Result<Vec<TranslatedLine>, String> {
    let plan: ListenPlan = read_json(&listen_manifest(job))?;
    let (rate, clips) = load_passage_clips(job, &plan)?;
    let cues = passage_cues(&plan, rate, &clips);
    if cues.len() != plan.passages.len() {
        return Err("Không khớp được mốc đoạn.".into());
    }
    Ok(cues)
}

// Lời trên màn nghe liền mạch lấy mốc của audio nối, không lấy mốc hình.
pub(crate) fn script_for(result: &Path) -> Result<Vec<TranslatedLine>, String> {
    let saved = crate::job::read_script(result)?;
    if !is_listen_audio(result) {
        return Ok(saved);
    }
    let root = result.parent().ok_or("Chưa thấy folder việc.")?;
    let job = Job::reopen(root)?;
    Ok(listen_cues(&job).unwrap_or(saved))
}

// Video đã ghép: đọc từng đoạn một lần, ghi file nghe. Không tải lại, không dịch lại.
pub(crate) fn join_listen(
    app: &AppHandle,
    result: &Path,
    cancel: &AtomicBool,
) -> Result<String, String> {
    if is_m4a(result) {
        return Err("Bản này đã là audio.".into());
    }
    let (job, _, _) = open_finished(result)?;
    if !file_ready(&job.result()) {
        return Err("Chưa có video đã ghép.".into());
    }
    crate::history::ensure_previous(&job.root)?;
    let lines = crate::timing::read(&job.root)?;
    stopped(cancel)?;
    let passages = group_passages(&lines);
    if passages.is_empty() {
        return Err("Chưa có lời đã dịch.".into());
    }
    let voice = crate::selected_voice(app, &job.target)?;
    let saved = read_json(&listen_manifest(&job)).unwrap_or(ListenPlan {
        voice: String::new(),
        passages: Vec::new(),
        render: 0,
    });
    let ready: Vec<bool> = (0..passages.len())
        .map(|index| read_wav(&listen_wav(&job, index)).is_ok())
        .collect();
    let indexes = match speak_indexes(&saved, &voice, &passages, &ready) {
        SpeakSet::All => {
            clear_listen_wavs(&job);
            (0..passages.len()).collect::<Vec<_>>()
        }
        SpeakSet::Missing(indexes) => indexes,
    };
    std::fs::create_dir_all(listen_dir(&job)).map_err(|error| error.to_string())?;
    write_json(
        &listen_manifest(&job),
        &ListenPlan {
            voice: voice.clone(),
            passages: passages.clone(),
            render: crate::listen::PASSAGE_RENDER,
        },
    )?;
    let total = passages.len();
    if !indexes.is_empty() {
        let batch: Vec<(usize, String, PathBuf)> = indexes
            .iter()
            .map(|index| {
                (
                    *index,
                    passages[*index].text.clone(),
                    listen_wav(&job, *index),
                )
            })
            .collect();
        let already = total.saturating_sub(batch.len());
        match listen_engine() {
            crate::piper::SpeakEngine::Window => {
                crate::tts::run(
                    app,
                    &voice,
                    &job.target,
                    &batch,
                    true,
                    cancel,
                    &|done, _| {
                        let fraction = (already + done) as f64 / (total as f64 + 1.0);
                        let _ = app.emit("listen-progress", fraction);
                    },
                )?;
            }
            crate::piper::SpeakEngine::Native => {
                return Err("Nghe liền mạch đọc cả đoạn trong cửa sổ app.".into());
            }
        }
    }
    let wav = assemble_listen(&job, &lines)?;
    let out = listen_beside(&job.result());
    let encoded = encode_spoken(&wav, &out, cancel, &mut |fraction| {
        let mapped = total as f64 / (total as f64 + 1.0) + fraction / (total as f64 + 1.0);
        let _ = app.emit("listen-progress", mapped);
    });
    let _ = std::fs::remove_file(&wav);
    encoded?;
    if !file_ready(&out) {
        return Err("Không ghi được audio liền mạch.".into());
    }
    crate::history::publish(&job.root, &out, &["Xuất audio liền mạch".into()], None, None, cancel)?;
    Ok(out.to_string_lossy().into_owned())
}

// Copy audio bản dịch ra chỗ người dùng chọn. Đã nối liền thì lấy file đó. Chưa có thì lấy từ dub.wav, hết wav thì lấy từ video đã ghép.
pub(crate) fn save_spoken(result: &Path, dest: &Path) -> Result<(), String> {
    let spoken = if is_m4a(result) && file_ready(result) {
        result.to_path_buf()
    } else {
        let listen = listen_beside(result);
        if file_ready(&listen) {
            listen
        } else {
            let beside = spoken_beside(result);
            if !file_ready(&beside) {
                let parent = result.parent().ok_or("Chưa thấy folder việc.")?;
                let dub = parent.join("dub.wav");
                let video = plain_result(result);
                let source = if file_ready(&dub) {
                    dub
                } else if file_ready(&video) {
                    video
                } else {
                    return Err("Chưa có audio bản dịch. Chạy lồng tiếng trước.".into());
                };
                encode_spoken(&source, &beside, &AtomicBool::new(false), &mut |_| {})?;
            }
            beside
        }
    };
    copy_audio(&spoken, dest)
}

// Video kết quả (.mp4) chép tới chỗ người dùng chọn trong hộp thoại lưu. Tên gõ thiếu .mp4 thì nối thêm, không đè lên phần đã gõ.
pub(crate) fn save_video(result: &Path, dest: &Path) -> Result<(), String> {
    if !is_mp4(result) || !file_ready(result) {
        return Err("Chưa thấy video kết quả.".into());
    }
    let Some(name) = dest.file_name() else {
        return Err("Chưa chọn chỗ lưu video.".into());
    };
    let dest = if is_mp4(dest) {
        dest.to_path_buf()
    } else {
        let mut named = name.to_os_string();
        named.push(".mp4");
        dest.with_file_name(named)
    };
    copy_file(result, &dest, "video")
}

fn is_mp4(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .eq_ignore_ascii_case("mp4")
}

pub(super) fn is_m4a(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .eq_ignore_ascii_case("m4a")
}

fn encode_spoken(
    dub: &Path,
    out: &Path,
    cancel: &AtomicBool,
    on_fraction: &mut dyn FnMut(f64),
) -> Result<(), String> {
    let partial = audio_partial(out);
    let _ = std::fs::remove_file(&partial);
    let total = media_duration(dub, cancel).ok();
    let mut command = Command::new("ffmpeg");
    command
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-progress",
            "pipe:1",
            "-nostats",
            "-y",
            "-i",
        ])
        .arg(dub)
        .args([
            "-c:a",
            "aac",
            "-b:a",
            "128k",
            "-ac",
            "1",
            "-ar",
            "48000",
            "-movflags",
            "+faststart",
        ])
        .arg(&partial);
    let encoded = run_ffmpeg_progress(
        command,
        total,
        Duration::from_secs(60 * 60),
        cancel,
        on_fraction,
    );
    if let Err(error) = encoded {
        let _ = std::fs::remove_file(&partial);
        return Err(error);
    }
    if !file_ready(&partial) {
        let _ = std::fs::remove_file(&partial);
        return Err("FFmpeg không ghi được audio bản dịch.".into());
    }
    let _ = std::fs::remove_file(out);
    std::fs::rename(&partial, out).map_err(|error| error.to_string())?;
    Ok(())
}

fn audio_partial(out: &Path) -> PathBuf {
    let stem = out
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("audio");
    out.with_file_name(format!("{stem}.partial.m4a"))
}

fn copy_audio(from: &Path, dest: &Path) -> Result<(), String> {
    copy_file(from, dest, "audio")
}

// Chép qua file dở cạnh đích rồi đổi tên, để đích không bao giờ là file ghi dở. Lỗi nào cũng dọn file dở.
fn copy_file(from: &Path, dest: &Path, what: &str) -> Result<(), String> {
    if same_path(from, dest) {
        return Ok(());
    }
    if let Some(parent) = dest.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
    }
    // Chỉ dọn file tạm do chính lần lưu này tạo; không chiếm tên .partial của người dùng.
    let mut input = std::fs::File::open(from).map_err(|error| error.to_string())?;
    let (partial, mut output) = copy_temp(dest)?;
    let copied = std::io::copy(&mut input, &mut output).and_then(|_| output.sync_all());
    drop(output);
    if let Err(error) = copied {
        let _ = std::fs::remove_file(&partial);
        return Err(format!("Không ghi được {what}. {error}"));
    }
    if let Err(error) = std::fs::rename(&partial, dest) {
        let _ = std::fs::remove_file(&partial);
        return Err(format!("Không ghi được {what}. {error}"));
    }
    Ok(())
}

pub(super) fn write_video_output(
    mut command: Command,
    out: &Path,
    total: Option<f64>,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(f64),
) -> Result<(), String> {
    let partial = partial_path(out);
    let _ = std::fs::remove_file(&partial);
    command.arg(&partial);
    let merged = super::video_encode::run(
        command,
        total,
        Duration::from_secs(60 * 60),
        cancel,
        progress,
    );
    if let Err(error) = merged {
        let _ = std::fs::remove_file(&partial);
        return Err(error);
    }
    if !file_ready(&partial) {
        let _ = std::fs::remove_file(&partial);
        return Err("FFmpeg không ghi được video kết quả.".into());
    }
    if let Err(error) = std::fs::rename(&partial, out) {
        let _ = std::fs::remove_file(&partial);
        return Err(error.to_string());
    }
    Ok(())
}

fn copy_temp(dest: &Path) -> Result<(PathBuf, std::fs::File), String> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let name = dest
        .file_name()
        .ok_or("Chưa chọn chỗ lưu.")?
        .to_string_lossy();
    loop {
        let path = dest.with_file_name(format!(
            ".{name}.save-{}-{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.to_string()),
        }
    }
}

fn same_path(left: &Path, right: &Path) -> bool {
    let norm = |path: &Path| {
        path.canonicalize()
            .unwrap_or_else(|_| path.to_path_buf())
            .to_string_lossy()
            .trim_start_matches(r"\\?\")
            .to_ascii_lowercase()
    };
    norm(left) == norm(right)
}

pub(super) fn merge_picture(job: &Job) -> Result<PathBuf, String> {
    if file_ready(&job.picture) {
        return Ok(job.picture.clone());
    }
    let ext = job
        .video
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if is_picture_ext(&ext) && file_ready(&job.video) {
        return Ok(job.video.clone());
    }
    Err("Chưa có file video. Bấm Tải video trước.".into())
}

fn reusable_frame(job:&Job,now:&crate::render::RenderSettings,cancel:&AtomicBool)->Option<PathBuf>{
    #[derive(serde::Deserialize)] struct Saved {path:PathBuf,render:crate::render::RenderSettings}
    let saved:Saved=read_json(&job.root.join("frame-cache.json")).ok()?;
    if !saved.render.same_frame(now){return None;}
    let path=saved.path.canonicalize().ok()?;
    if !path.starts_with(job.root.join(crate::job::KEPT_DIR).canonicalize().ok()?){return None;}
    media_duration(&path,cancel).ok()?;
    Some(path)
}

fn fitted_spans(job:&Job,lines:&[TranslatedLine],cancel:&AtomicBool)->Result<Vec<crate::audio_mix::VoiceSpan>,String>{
    if let Ok(saved)=read_json::<Vec<crate::audio_mix::VoiceSpan>>(&job.root.join("voice-spans.json")){return Ok(saved);}
    let slots:Vec<_>=lines.iter().enumerate().map(|(i,l)|{let pcm=read_wav(&job.line_wav(i+1))?;Ok(Slot{start:l.start,end:l.end,wav_secs:pcm.samples.len() as f64 / f64::from(pcm.rate)})}).collect::<Result<_,String>>()?;
    Ok(place_all(&slots,media_duration(&merge_picture(job)?,cancel)?).iter().map(|p|crate::audio_mix::VoiceSpan{start:p.start,end:p.start+p.played}).collect())
}

#[cfg(test)]
fn frame_merge(command: &mut Command, picture: &Path, root: &Path, cancel: &AtomicBool) -> Result<(), String> {
    let render = crate::render::RenderSettings::read(root)?;
    if render.needs_frame() {
        let logo_input=if render.logo.enabled {command.arg("-i").arg(&render.logo.path);Some(2)} else {None};
        let (w,h) = super::captions::probe_dimensions(picture, cancel)?;
        command.args(["-filter_complex", &render.graph_with_logo(w,h,"",logo_input), "-map", "-0:v:0", "-map", "[vout]",
            "-c:v", "libx264", "-preset", "veryfast", "-crf", "20", "-pix_fmt", "yuv420p", "-threads", "2"]);
    }
    Ok(())
}

pub(super) fn merge_video_file(job:&Job,lines:&[TranslatedLine],cancel:&AtomicBool,on_fraction:&mut dyn FnMut(f64))->Result<(),String>{
    let original_picture=merge_picture(job)?;
    let render=crate::render::RenderSettings::read(&job.root)?;
    let reused=reusable_frame(job,&render,cancel);
    let picture=reused.clone().unwrap_or_else(||original_picture.clone());
    let dubbed=if file_ready(&job.dub()){job.dub()}else if file_ready(&job.spoken()){job.spoken()}else{return Err("Chưa có tiếng đã lồng.".into());};
    let total=media_duration(&picture,cancel).ok();
    let mut command=Command::new("ffmpeg");
    command.args(["-hide_banner","-loglevel","error","-progress","pipe:1","-nostats","-y","-i"]).arg(&picture).arg("-i").arg(dubbed);
    let mut next_input=2;
    let mut envelope_guard=None;
    let mut graphs=Vec::new();
    let mix_audio=render.audio!=Default::default();
    if mix_audio{
        let source_input=if render.audio.original>0.0 && picture_has_audio(&original_picture){
            command.arg("-i").arg(&original_picture);next_input+=1;Some(2)
        }else{None};
        let env_input=if source_input.is_some()&&render.audio.duck{
            let spans=fitted_spans(job,lines,cancel)?;
            envelope_guard=Some(crate::audio_mix::prepare_envelope(&job.root,&render.audio,&spans,media_duration(&original_picture,cancel)?)?);
            command.arg("-i").arg(&envelope_guard.as_ref().unwrap().0);
            let index=next_input;next_input+=1;Some(index)
        }else{None};
        graphs.push(render.audio.graph(source_input,env_input));
    }
    let frame=render.needs_frame()&&reused.is_none();
    if frame{
        let logo_input=if render.logo.enabled{command.arg("-i").arg(&render.logo.path);Some(next_input)}else{None};
        let (w,h)=super::captions::probe_dimensions(&picture,cancel)?;
        graphs.push(render.graph_with_logo(w,h,"",logo_input));
    }
    if !graphs.is_empty(){command.args(["-filter_complex",&graphs.join(";")]);}
    command.args(["-map",if frame{"[vout]"}else{"0:v:0"},"-map",if mix_audio{"[aout]"}else{"1:a:0"}]);
    if frame{command.args(["-c:v","libx264","-preset","veryfast","-threads","2","-bf","0","-crf","20","-pix_fmt","yuv420p"]);}else{command.args(["-c:v","copy"]);}
    command.args(["-c:a","aac","-ar","48000","-ac","2","-b:a","192k","-movflags","+faststart","-muxpreload","0","-muxdelay","0","-use_editlist","0"]);
    // AAC padding can differ from the cached video's last packet by a few ms.
    // With a known video duration, -shortest would discard that final frame on remux.
    if let Some(seconds)=total {command.args(["-t",&seconds.to_string()]);}else{command.arg("-shortest");}
    write_video_output(command,&job.result(),total,cancel,on_fraction)?;
    drop(envelope_guard);
    Ok(())
}

pub(super) fn merge(
    app: &AppHandle,
    job: &Job,
    lines: &[TranslatedLine],
    burn: bool,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let have_video = file_ready(&job.result());
    let have_text = file_ready(&job.captioned());
    if have_video && (!burn || have_text) {
        release_mix(&job.root);
        report(app, "merge", "skipped", "Đã có video kết quả, bỏ qua.");
        return Ok(());
    }
    stopped(cancel)?;
    write_srt(&job.subtitles(), lines)?;
    if !have_video {
        report(app,"merge","running","Đang ghép video.");
        merge_video_file(job,lines,cancel,&mut |fraction|report_progress(app,"merge","Đang ghép video.",fraction,1.0))?;
    }
    release_mix(&job.root);
    if burn && !file_ready(&job.captioned()) {
        report(app, "merge", "running", "Đang ghi chữ vào video.");
        burn_file(
            &job.result(),
            &job.subtitles(),
            &job.captioned(),
            cancel,
            &mut |fraction| {
                report_progress(app, "merge", "Đang ghi chữ vào video.", fraction, 1.0);
            },
        )?;
    }
    let detail = if burn {
        "Video có chữ lồng tiếng và file phụ đề nằm trong folder việc."
    } else {
        "Video và phụ đề nằm trong folder việc."
    };
    report(app, "merge", "done", detail);
    Ok(())
}

// Đốt phụ đề lên hình gốc và giữ tiếng gốc. Không tạo file lồng tiếng.
// Pipeline gọi `burn_original_with`; hàm này chỉ để test không cần callback tiến độ.
#[cfg(test)]
pub(super) fn burn_original(
    job: &Job,
    lines: &[TranslatedLine],
    cancel: &AtomicBool,
) -> Result<(), String> {
    burn_original_with(job, lines, cancel, &mut |_| {})
}

fn burn_original_with(
    job: &Job,
    lines: &[TranslatedLine],
    cancel: &AtomicBool,
    on_fraction: &mut dyn FnMut(f64),
) -> Result<(), String> {
    let picture = merge_picture(job)?;
    if !picture_has_audio(&picture) {
        return Err("Video không có tiếng gốc.".into());
    }
    write_srt(&job.subtitles(), lines)?;
    stopped(cancel)?;
    burn_file_with(
        &picture,
        &job.subtitles(),
        &job.captioned(),
        cancel,
        on_fraction,
        true,
    )
}

pub(super) fn merge_text(
    app: &AppHandle,
    job: &Job,
    lines: &[TranslatedLine],
    cancel: &AtomicBool,
) -> Result<(), String> {
    if file_ready(&job.captioned()) {
        report(app, "merge", "skipped", "Đã có video kết quả, bỏ qua.");
        return Ok(());
    }
    report(app, "merge", "running", "Đang ghi chữ vào video.");
    burn_original_with(job, lines, cancel, &mut |fraction| {
        report_progress(app, "merge", "Đang ghi chữ vào video.", fraction, 1.0);
    })?;
    report(
        app,
        "merge",
        "done",
        "Video giữ tiếng gốc, chữ dịch nằm trên hình. File phụ đề nằm trong folder việc.",
    );
    Ok(())
}

// Đốt phụ đề lên bản đã ghép, không chạy lại pipeline. Trả về đường dẫn bản có chữ.
pub(crate) fn burn_saved(
    app: &AppHandle,
    result: &Path,
    cancel: &AtomicBool,
) -> Result<String, String> {
    let result = plain_result(result);
    if let Some(root) = result.parent() { crate::history::ensure_previous(root)?; }
    if !file_ready(&result) {
        return Err("Chưa thấy video đã lồng.".into());
    }
    let out = crate::job::captioned_path(&result);
    if file_ready(&out) {
        if let Some(root) = out.parent() { crate::history::publish(root, &out, &["Thêm phụ đề".into()], None, None, cancel)?; }
        return Ok(out.to_string_lossy().into_owned());
    }
    require_tool("ffmpeg")?;
    let srt = ensure_srt(&result)?;
    stopped(cancel)?;
    caption_report(app, None);
    burn_file(&result, &srt, &out, cancel, &mut |fraction| {
        caption_report(app, Some(fraction))
    })?;
    if let Some(root) = out.parent() { crate::history::publish(root, &out, &["Thêm phụ đề".into()], None, None, cancel)?; }
    Ok(out.to_string_lossy().into_owned())
}

pub(super) fn plain_result(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    let lower = name.to_ascii_lowercase();
    let Some(stem) = lower.strip_suffix(".chu.mp4") else {
        return path.to_path_buf();
    };
    path.with_file_name(format!("{}.mp4", &name[..stem.len()]))
}

fn ensure_srt(result: &Path) -> Result<PathBuf, String> {
    let srt = result.with_extension("srt");
    if let Some(dir) = result.parent() {
        if dir.join("translation.json").is_file() {
            let lines = crate::timing::read(dir)?;
            write_srt(&srt, &lines)?;
        }
    }
    if srt_has_cues(&srt) {
        Ok(srt)
    } else {
        Err("Chưa có lời dịch để ghi chữ.".into())
    }
}

pub(super) fn srt_has_cues(path: &Path) -> bool {
    std::fs::read_to_string(path)
        .ok()
        .is_some_and(|text| text.contains("-->"))
}

fn burn_file(
    video: &Path,
    srt: &Path,
    out: &Path,
    cancel: &AtomicBool,
    on_fraction: &mut dyn FnMut(f64),
) -> Result<(), String> {
    burn_file_with(video, srt, out, cancel, on_fraction, false)
}

fn burn_file_with(
    video: &Path, srt: &Path, out: &Path, cancel: &AtomicBool,
    on_fraction: &mut dyn FnMut(f64), transform: bool,
) -> Result<(), String> {
    let render = crate::render::RenderSettings::read(srt.parent().unwrap_or(Path::new(".")))?;
    let source_size = super::captions::probe_dimensions(video, cancel)?;
    let prepared = if transform {
        let (w,h) = render.dimensions(source_size.0, source_size.1);
        super::captions::prepare_for(srt,w,h,&render)?
    } else { super::captions::prepare(video, srt, cancel)? };
    let name = prepared.0
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or("Chưa có phụ đề.")?;
    let partial = partial_path(out);
    let _ = std::fs::remove_file(&partial);
    let total = media_duration(video, cancel).ok();
    let mut command = Command::new("ffmpeg");
    command
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-progress",
            "pipe:1",
            "-nostats",
            "-y",
            "-i",
        ])
        .arg(video)
        .arg("-vf")
        .arg(subtitle_filter(name))
        .args([
            "-c:v", "libx264", "-preset", "veryfast", "-threads", "2", "-crf", "20", "-pix_fmt",
            "yuv420p",
        ]);
    if transform && render.needs_frame() {
        // Replace the simple overlay with a graph that frames first and adds text last.
        // FFmpeg forbids combining -vf and -filter_complex for the same stream.
        let mut framed = Command::new("ffmpeg");
        framed.args(["-hide_banner", "-loglevel", "error", "-progress", "pipe:1", "-nostats", "-y", "-i"])
            .arg(video);
        let logo_input=if render.logo.enabled {framed.arg("-i").arg(&render.logo.path);Some(1)} else {None};
        framed.args(["-filter_complex", &render.graph_with_logo(source_size.0, source_size.1, &format!(",{}", subtitle_filter(name)),logo_input),
                "-map", "[vout]", "-map", "0:a?", "-c:v", "libx264", "-preset", "veryfast", "-threads", "2", "-crf", "20", "-pix_fmt", "yuv420p"]);
        command = framed;
    }
    if mp4_can_copy_audio(video) {
        command.args(["-c:a", "copy"]);
    } else {
        command.args(["-c:a", "aac", "-b:a", "192k", "-ar", "48000", "-ac", "2"]);
    }
    command.args(["-movflags", "+faststart"]).arg(&partial);
    if let Some(dir) = srt.parent() {
        command.current_dir(dir);
    }
    let burned = super::video_encode::run(
        command,
        total,
        Duration::from_secs(6 * 60 * 60),
        cancel,
        on_fraction,
    );
    if let Err(error) = burned {
        let _ = std::fs::remove_file(&partial);
        return Err(error);
    }
    if !file_ready(&partial) {
        let _ = std::fs::remove_file(&partial);
        return Err("FFmpeg không ghi được chữ vào video.".into());
    }
    let _ = std::fs::remove_file(out);
    std::fs::rename(&partial, out).map_err(|error| error.to_string())?;
    Ok(())
}

fn mp4_can_copy_audio(path: &Path) -> bool {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "a:0",
            "-show_entries",
            "stream=codec_name",
            "-of",
            "csv=p=0",
        ])
        .arg(path)
        .output();
    let codec = output
        .ok()
        .filter(|out| out.status.success())
        .map(|out| {
            String::from_utf8_lossy(&out.stdout)
                .trim()
                .to_ascii_lowercase()
        })
        .unwrap_or_default();
    matches!(codec.as_str(), "aac" | "mp3" | "ac3" | "eac3" | "alac")
}

pub(super) fn picture_has_audio(path: &Path) -> bool {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "a",
            "-show_entries",
            "stream=codec_type",
            "-of",
            "csv=p=0",
        ])
        .arg(path)
        .output();
    output
        .ok()
        .filter(|out| out.status.success())
        .is_some_and(|out| String::from_utf8_lossy(&out.stdout).contains("audio"))
}

fn partial_path(out: &Path) -> PathBuf {
    let stem = out
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("video");
    out.with_file_name(format!("{stem}.partial.mp4"))
}

// Tên file tương đối, ffmpeg chạy trong folder chứa phụ đề. Dấu nháy thuộc cú pháp bộ lọc.
pub(super) fn subtitle_filter(file_name: &str) -> String {
    let mut escaped = String::new();
    for ch in file_name.chars() {
        if matches!(ch, '\\' | '\'' | ':') {
            escaped.push('\\');
        }
        escaped.push(ch);
    }
    format!(
        "ass=filename='{escaped}'"
    )
}

fn caption_report(app: &AppHandle, progress: Option<f64>) {
    let _ = app.emit(
        "caption-progress",
        DownloadEvent {
            detail: "Đang ghi chữ vào video.".into(),
            progress,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::process::Stdio;
    use std::sync::atomic::AtomicBool;

    // Explicit opt-in, uses existing local media only; never changes its job/cache.
    #[test]
    #[ignore]
    fn benchmark_local_video_encoders() {
        let input = PathBuf::from(std::env::var_os("VIDEO_BENCH_INPUT").expect("VIDEO_BENCH_INPUT"));
        let srt = PathBuf::from(std::env::var_os("VIDEO_BENCH_SRT").expect("VIDEO_BENCH_SRT"));
        let dir = PathBuf::from(std::env::var_os("VIDEO_BENCH_OUTPUT").expect("VIDEO_BENCH_OUTPUT"));
        std::fs::create_dir_all(&dir).unwrap();
        let cancel = AtomicBool::new(false);
        let prepared = super::super::captions::prepare(&input, &srt, &cancel).unwrap();
        let ass = dir.join("captions.ass");
        std::fs::copy(&prepared.0, &ass).unwrap();
        let mut timings = serde_json::Map::new();
        for gpu in [false, true] {
            let name = if gpu { "nvenc" } else { "cpu" };
            let out = dir.join(format!("{name}.mp4"));
            let mut c = Command::new("ffmpeg");
            c.args(["-hide_banner", "-loglevel", "error", "-progress", "pipe:1", "-nostats", "-y", "-i"])
                .arg(&input).args(["-vf", &subtitle_filter("captions.ass"), "-c:v", "libx264", "-preset", "veryfast",
                    "-threads", "2", "-crf", "20", "-pix_fmt", "yuv420p", "-c:a", "copy", "-movflags", "+faststart"])
                .arg(&out).current_dir(&dir);
            let start = std::time::Instant::now();
            if gpu {
                super::super::video_encode::run(c, None, Duration::from_secs(1800), &cancel, &mut |_| {}).unwrap();
            } else {
                run_ffmpeg_progress(c, None, Duration::from_secs(1800), &cancel, &mut |_| {}).unwrap();
            }
            let seconds = start.elapsed().as_secs_f64();
            eprintln!("{name}: {seconds:.3}s");
            timings.insert(name.into(), serde_json::json!({"seconds": seconds, "bytes": std::fs::metadata(out).unwrap().len()}));
        }
        write_json(&dir.join("timings.json"), &timings).unwrap();
    }

    #[test]
    fn fit_filter_chain_is_plain_when_nothing_to_do() {
        let edges = "afade=t=in:d=0.015,areverse,afade=t=in:d=0.015,areverse";
        assert_eq!(fit_filter(1.0, None), edges);
        assert_eq!(fit_filter(1.25, None), format!("atempo=1.250,{edges}"));
        assert_eq!(
            fit_filter(1.4, Some(1.0)),
            format!("atempo=1.400,atrim=end=1.000,afade=t=out:st=0.950:d=0.050,{edges}")
        );
    }

    #[test]
    fn fit_filter_cuts_after_speeding_up() {
        if crate::program_path("ffmpeg").is_none() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("fit-filter-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let tone = dir.join("tone.wav");
        let out = dir.join("out.wav");
        let made = Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=3",
            ])
            .args(["-ac", "1", "-ar", "22050", "-c:a", "pcm_s16le"])
            .arg(&tone)
            .status()
            .unwrap()
            .success();
        assert!(made);
        let fitted = Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
            .arg(&tone)
            .arg("-af")
            .arg(fit_filter(1.4, Some(1.0)))
            .args(["-ac", "1", "-ar", "22050", "-c:a", "pcm_s16le"])
            .arg(&out)
            .status()
            .unwrap()
            .success();
        assert!(fitted);
        let pcm = crate::wav::read_wav(&out).unwrap();
        let secs = pcm.samples.len() as f64 / f64::from(pcm.rate);
        assert!((secs - 1.0).abs() < 0.06, "got {secs}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn subtitle_filter_escapes_quotes_and_keeps_spaces() {
        let filter = subtitle_filter("tap 1 demo.vi.srt");
        assert!(filter.contains("filename='tap 1 demo.vi.srt'"));
        assert!(filter.starts_with("ass=filename="));
        let quoted = subtitle_filter("it's.srt");
        assert!(quoted.contains("filename='it\\'s.srt'"));
    }

    #[test]
    fn plain_result_strips_only_the_caption_suffix() {
        let plain = PathBuf::from(r"D:\out\tap 1.vi.mp4");
        assert_eq!(plain_result(&plain), plain);
        assert_eq!(
            plain_result(&PathBuf::from(r"D:\out\tap 1.vi.chu.mp4")),
            PathBuf::from(r"D:\out\tap 1.vi.mp4")
        );
    }

    #[test]
    fn framed_original_burn_keeps_audio_and_cleans_temporary_files() {
        let dir = save_video_dir("styled-frame");
        let source = dir.join("source.mp4");
        assert!(Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc2=s=960x540:r=4:d=1",
            "-f", "lavfi", "-i", "sine=frequency=440:duration=1", "-c:v", "libx264", "-preset", "ultrafast", "-c:a", "aac"])
            .arg(&source).status().unwrap().success());
        let job = Job { root: dir.clone(), video: source.clone(), picture: source, target: "vi".into() };
        let settings = crate::render::RenderSettings { aspect: "9:16".into(), color: "#ffcc00".into(),
            background: true, caption_y: 72.0, font: "Tahoma".into(), ..Default::default() };
        write_json(&dir.join("render.json"), &settings).unwrap();
        let lines = vec![TranslatedLine { start: 0.0, end: 1.0, source: "Hello".into(), translation: "Một câu chuyện mới bắt đầu từ đây.".into() }];
        burn_original(&job, &lines, &AtomicBool::new(false)).unwrap();
        // Exercise the same mapping used by speech merge: replace picture video
        // with the framed graph, while retaining the second input's audio.
        let mut command = Command::new("ffmpeg");
        command.args(["-v","error","-y","-i"]).arg(&job.picture).arg("-i").arg(&job.picture)
            .args(["-map","0:v:0","-map","1:a:0","-c:v","copy","-c:a","aac"]);
        frame_merge(&mut command, &job.picture, &job.root, &AtomicBool::new(false)).unwrap();
        command.arg(job.result());
        let encoded = command.output().unwrap();
        assert!(encoded.status.success(), "{}", String::from_utf8_lossy(&encoded.stderr));
        assert_eq!(super::super::captions::probe_dimensions(&job.result(), &AtomicBool::new(false)).unwrap(), (540,960));

        let probe = Command::new("ffprobe").args(["-v","error","-show_streams","-of","json"]).arg(job.captioned()).output().unwrap();
        let info: serde_json::Value = serde_json::from_slice(&probe.stdout).unwrap();
        let streams = info["streams"].as_array().unwrap();
        let v = streams.iter().find(|s| s["codec_type"] == "video").unwrap();
        assert_eq!(v["width"], 540);
        assert_eq!(v["height"], 960);
        assert!(streams.iter().any(|s| s["codec_type"] == "audio"));
        assert!(!partial_path(&job.captioned()).exists());
        assert!(!std::fs::read_dir(&dir).unwrap().flatten().any(|e| e.path().extension().is_some_and(|x| x == "ass")));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn adaptive_caption_burn_preserves_aspect_and_cleans_overlay() {
        if crate::program_path("ffmpeg").is_none() || crate::program_path("ffprobe").is_none() {
            return;
        }
        let dir = save_video_dir("caption-aspect");
        let srt = dir.join("captions.srt");
        write_srt(&srt, &[TranslatedLine {
            start: 0.0, end: 6.0, source: "Original".into(),
            translation: "Đây là câu phụ đề tiếng Việt để kiểm tra video ngang và video dọc. Hình phải giữ đúng tỉ lệ, chữ xuống dòng và không bị cắt ở hai bên.".into(),
        }]).unwrap();
        for (name, width, height) in [("landscape", 1920, 1080), ("portrait", 1080, 1920), ("square", 1080, 1080)] {
            let src = dir.join(format!("{name}.mp4"));
            let out = dir.join(format!("{name}.chu.mp4"));
            let mut source = Command::new("ffmpeg");
            source.args(["-v", "error"]);
            if let Some(real) = std::env::var_os("DUB_CAPTION_REVIEW_SOURCE") {
                // Optional manual QA uses an explicitly supplied local source,
                // copied into this test's private directory; no model/API calls.
                source.arg("-i").arg(real).args(["-t", "6", "-an", "-vf"])
                    .arg(format!("scale={width}:{height}:force_original_aspect_ratio=decrease,pad={width}:{height}:(ow-iw)/2:(oh-ih)/2,setsar=1"));
            } else {
                source.args(["-f", "lavfi", "-i"])
                    .arg(format!("color=c=0x263344:s={width}x{height}:r=2:d=6"));
            }
            assert!(source.args(["-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p"])
                .arg(&src).status().unwrap().success());
            burn_file(&src, &srt, &out, &AtomicBool::new(false), &mut |_| {}).unwrap();
            let probe = Command::new("ffprobe").args(["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=width,height,display_aspect_ratio", "-of", "json"])
                .arg(&out).output().unwrap();
            assert!(probe.status.success());
            let info: serde_json::Value = serde_json::from_slice(&probe.stdout).unwrap();
            assert_eq!(info["streams"][0]["width"], width);
            assert_eq!(info["streams"][0]["height"], height);
            assert!(!partial_path(&out).exists());
            assert!(!std::fs::read_dir(&dir).unwrap().flatten().any(|entry| entry.file_name().to_string_lossy().ends_with(".ass")));
            if std::env::var_os("DUB_CAPTION_REVIEW").is_some() {
                assert!(Command::new("ffmpeg").args(["-v", "error", "-ss", "1", "-i"]).arg(&out)
                    .args(["-frames:v", "1"]).arg(dir.join(format!("{name}.png"))).status().unwrap().success());
            }
        }
        // A failed encode must also release the overlay used only by this burn.
        // Invalid caption timing must fail before touching a finished output,
        // its source, the source SRT, or cached speech.
        let preserved = dir.join("portrait.chu.mp4");
        let result_before = std::fs::read(&preserved).unwrap();
        let source_before = std::fs::read(dir.join("portrait.mp4")).unwrap();
        let srt_before = std::fs::read(&srt).unwrap();
        let bad_srt = dir.join("invalid.srt");
        std::fs::write(&bad_srt, "1\n00:00:02,000 --> 00:00:01,000\nKhông được bỏ qua chữ").unwrap();
        let error = burn_file(&dir.join("portrait.mp4"), &bad_srt, &preserved, &AtomicBool::new(false), &mut |_| {}).unwrap_err();
        assert!(error.contains("Phụ đề 1:"), "{error}");
        assert_eq!(std::fs::read(&preserved).unwrap(), result_before);
        assert_eq!(std::fs::read(dir.join("portrait.mp4")).unwrap(), source_before);
        assert_eq!(std::fs::read(&srt).unwrap(), srt_before);
        assert!(!partial_path(&preserved).exists());
        let missing_parent = dir.join("missing-parent").join("failed.mp4");
        assert!(burn_file(&dir.join("portrait.mp4"), &srt, &missing_parent, &AtomicBool::new(false), &mut |_| {}).is_err());
        assert!(!std::fs::read_dir(&dir).unwrap().flatten().any(|entry| entry.file_name().to_string_lossy().ends_with(".ass")));
        let cancelled = dir.join("cancelled.mp4");
        assert_eq!(burn_file(&dir.join("portrait.mp4"), &srt, &cancelled, &AtomicBool::new(true), &mut |_| {}).unwrap_err(), "Đã dừng.");
        assert!(!cancelled.exists());
        if std::env::var_os("DUB_CAPTION_REVIEW").is_some() {
            println!("Caption review: {}", dir.display());
        } else {
            std::fs::remove_dir_all(&dir).unwrap();
        }
    }

    #[test]
    fn burn_file_writes_a_captioned_video_when_ffmpeg_is_installed() {
        let ffmpeg = Command::new("ffmpeg")
            .args(["-hide_banner", "-version"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if !ffmpeg.map(|status| status.success()).unwrap_or(false) {
            return;
        }
        let dir = std::env::temp_dir().join(format!("burn-cue-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("clip.vi.mp4");
        let made = Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "color=c=black:s=320x180:d=1",
                "-f",
                "lavfi",
                "-i",
                "anullsrc=r=48000:cl=stereo",
                "-shortest",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
            ])
            .arg(&src)
            .status()
            .expect("ffmpeg")
            .success();
        assert!(made, "không tạo được video thử");
        let srt = dir.join("clip.vi.srt");
        write_srt(
            &srt,
            &[TranslatedLine {
                start: 0.0,
                end: 1.0,
                source: "Hi".into(),
                translation: "Xin chào".into(),
            }],
        )
        .unwrap();
        let out = dir.join("clip.vi.chu.mp4");
        let cancel = AtomicBool::new(false);
        burn_file(&src, &srt, &out, &cancel, &mut |_| {}).unwrap();
        assert!(file_ready(&out));
        assert!(!dir.join("clip.vi.chu.partial.mp4").exists());
        let stats = Command::new("ffmpeg")
            .args(["-hide_banner", "-i"])
            .arg(&out)
            .args([
                "-vf",
                "signalstats,metadata=print:file=-",
                "-f",
                "null",
                "-",
            ])
            .output()
            .unwrap();
        let log = String::from_utf8_lossy(&stats.stdout);
        let average = log
            .lines()
            .filter_map(|line| line.trim().strip_prefix("lavfi.signalstats.YAVG="))
            .filter_map(|value| value.parse::<f64>().ok())
            .fold(0.0_f64, f64::max);
        assert!(average > 1.0, "chữ không hiện trên hình, YAVG={average}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn spoken_file_sits_beside_the_video() {
        assert_eq!(
            spoken_beside(Path::new(r"C:\out\tap\tap.vi.mp4")),
            PathBuf::from(r"C:\out\tap\tap.vi.m4a")
        );
        assert_eq!(
            spoken_beside(Path::new(r"C:\out\tap\tap.vi.chu.mp4")),
            PathBuf::from(r"C:\out\tap\tap.vi.m4a")
        );
    }

    #[test]
    fn save_spoken_copies_an_existing_audio_file() {
        let dir = std::env::temp_dir().join(format!("save-spoken-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("tap.vi.m4a");
        std::fs::write(&src, b"audio-bytes").unwrap();
        let dest = dir.join("ra").join("ban-dich.m4a");
        save_spoken(&src, &dest).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"audio-bytes");
        let video = dir.join("clip.vi.mp4");
        std::fs::write(&video, b"video").unwrap();
        let missing = dir.join("thieu.m4a");
        assert!(save_spoken(&video, &missing).is_err());
        assert!(!file_ready(&missing));
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn save_video_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("save-video-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn file_names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn save_video_copies_the_mp4_into_a_folder_that_does_not_exist_yet() {
        let dir = save_video_dir("copy");
        let src = dir.join("tap.vi.mp4");
        std::fs::write(&src, b"video-bytes").unwrap();
        let dest = dir.join("ra").join("xong").join("ban-dich.mp4");
        save_video(&src, &dest).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"video-bytes");
        assert_eq!(std::fs::read(&src).unwrap(), b"video-bytes");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_video_appends_mp4_to_what_was_typed_instead_of_replacing_it() {
        let dir = save_video_dir("name");
        let src = dir.join("tap.vi.mp4");
        std::fs::write(&src, b"video-bytes").unwrap();
        let out = dir.join("ra");
        // Tên gõ trong hộp thoại lưu. "tap 1.vi" có đuôi .vi nhưng chưa phải .mp4.
        for typed in ["ban-dich", "tap 1.vi", "Giu.MP4"] {
            save_video(&src, &out.join(typed)).unwrap();
        }
        assert_eq!(
            file_names(&out),
            ["Giu.MP4", "ban-dich.mp4", "tap 1.vi.mp4"]
        );
        assert_eq!(
            std::fs::read(out.join("tap 1.vi.mp4")).unwrap(),
            b"video-bytes"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_video_refuses_anything_but_a_finished_mp4_and_writes_nothing() {
        let dir = save_video_dir("refuse");
        let audio = dir.join("tap.vi.m4a");
        std::fs::write(&audio, b"audio").unwrap();
        let empty = dir.join("rong.vi.mp4");
        std::fs::write(&empty, b"").unwrap();
        let folder = dir.join("thu-muc.mp4");
        std::fs::create_dir_all(&folder).unwrap();
        let missing = dir.join("thieu.vi.mp4");
        let dest = dir.join("ra").join("ban-dich.mp4");
        for source in [&audio, &empty, &folder, &missing] {
            assert!(save_video(source, &dest).is_err(), "{}", source.display());
        }
        assert!(!dir.join("ra").exists());
        let good = dir.join("tap.vi.mp4");
        std::fs::write(&good, b"video-bytes").unwrap();
        assert!(save_video(&good, Path::new("")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_video_replaces_an_older_copy_and_leaves_no_partial_file() {
        let dir = save_video_dir("replace");
        let src = dir.join("tap.vi.mp4");
        std::fs::write(&src, b"moi").unwrap();
        let out = dir.join("ra");
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(out.join("ban-dich.mp4"), b"cu-va-dai-hon").unwrap();
        save_video(&src, &out.join("ban-dich.mp4")).unwrap();
        assert_eq!(std::fs::read(out.join("ban-dich.mp4")).unwrap(), b"moi");
        assert_eq!(file_names(&out), ["ban-dich.mp4"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn saving_never_deletes_an_existing_partial_or_the_source() {
        let dir = save_video_dir("unrelated-partial");
        let source = dir.join("source.vi.mp4");
        let dest = dir.join("export.mp4");
        let other = dir.join("export.partial.mp4");
        std::fs::write(&source, b"video").unwrap();
        std::fs::write(&other, b"keep me").unwrap();
        save_video(&source, &dest).unwrap();
        assert_eq!(std::fs::read(&other).unwrap(), b"keep me");
        // Kể cả khi file nguồn có đúng tên mà cách lưu cũ dùng làm file tạm.
        save_video(&other, &dest).unwrap();
        assert_eq!(std::fs::read(&other).unwrap(), b"keep me");
        assert_eq!(std::fs::read(&dest).unwrap(), b"keep me");
        assert_eq!(
            file_names(&dir),
            ["export.mp4", "export.partial.mp4", "source.vi.mp4"]
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn failed_replace_keeps_the_previous_destination_and_cleans_only_its_own_temp() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = save_video_dir("locked-dest");
        let source = dir.join("source.vi.mp4");
        let dest = dir.join("export.mp4");
        std::fs::write(&source, b"new").unwrap();
        std::fs::write(&dest, b"old").unwrap();
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&dest)
            .unwrap();
        assert!(save_video(&source, &dest).is_err());
        assert_eq!(std::fs::read(&dest).unwrap(), b"old");
        assert_eq!(file_names(&dir), ["export.mp4", "source.vi.mp4"]);
        drop(lock);
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn test_video_command(realtime: bool) -> Command {
        let mut command = Command::new("ffmpeg");
        command.args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-progress",
            "pipe:1",
            "-nostats",
            "-y",
        ]);
        if realtime {
            command.arg("-re");
        }
        command.args([
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=64x64:r=10",
            "-t",
            if realtime { "30" } else { "0.2" },
            "-c:v",
            "mpeg4",
            "-an",
        ]);
        command
    }

    #[test]
    fn interrupted_merge_never_publishes_a_finished_file() {
        if crate::program_path("ffmpeg").is_none() {
            return;
        }
        let dir = save_video_dir("cancel-merge");
        let out = dir.join("clip.vi.mp4");
        let cancel = AtomicBool::new(false);
        let mut saw_partial = false;
        let result = write_video_output(
            test_video_command(true),
            &out,
            Some(30.0),
            &cancel,
            &mut |fraction| {
                assert!(!out.exists());
                saw_partial |= file_ready(&partial_path(&out));
                if fraction > 0.0 {
                    cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            },
        );
        assert_eq!(result.unwrap_err(), "Đã dừng.");
        assert!(saw_partial);
        assert!(!out.exists());
        assert!(!partial_path(&out).exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn successful_merge_publishes_a_readable_mp4_and_no_partial() {
        if crate::program_path("ffmpeg").is_none() || crate::program_path("ffprobe").is_none() {
            return;
        }
        let dir = save_video_dir("publish-merge");
        let out = dir.join("clip.vi.mp4");
        let cancel = AtomicBool::new(false);
        write_video_output(
            test_video_command(false),
            &out,
            Some(0.2),
            &cancel,
            &mut |_| {
                assert!(!out.exists());
            },
        )
        .unwrap();
        assert!(media_duration(&out, &cancel).unwrap() > 0.0);
        assert!(!partial_path(&out).exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn save_video_that_cannot_take_the_place_leaves_no_partial_file() {
        let dir = save_video_dir("blocked");
        let src = dir.join("tap.vi.mp4");
        std::fs::write(&src, b"video-bytes").unwrap();
        let out = dir.join("ra");
        // Một folder đang giữ đúng tên đích: file không đặt vào được.
        std::fs::create_dir_all(out.join("ban-dich.mp4")).unwrap();
        assert!(save_video(&src, &out.join("ban-dich.mp4")).is_err());
        assert_eq!(file_names(&out), ["ban-dich.mp4"]);
        assert!(out.join("ban-dich.mp4").is_dir());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn join_listen_keeps_the_video_and_drops_the_picture_gaps() {
        let dir = std::env::temp_dir().join(format!("join-listen-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let root = dir.join("tap");
        std::fs::create_dir_all(root.join("lines")).unwrap();
        let source = dir.join("tap.mp4");
        std::fs::write(&source, b"goc").unwrap();
        write_json(
            &root.join("source.json"),
            &serde_json::json!({ "video": source }),
        )
        .unwrap();
        let mut job = Job::reopen(&root).unwrap();
        job.target = "vi".into();
        let video = job.result();
        std::fs::write(&video, b"video-bytes").unwrap();
        let picture_srt = job.subtitles();
        std::fs::write(&picture_srt, "mốc hình").unwrap();
        std::fs::write(job.dub(), b"cut-mix").unwrap();
        std::fs::write(fit_mode_path(&job.root), "sync").unwrap();
        let lines = vec![
            TranslatedLine {
                start: 0.0,
                end: 1.0,
                source: "One".into(),
                translation: "Một".into(),
            },
            TranslatedLine {
                start: 1.4,
                end: 2.4,
                source: "Two".into(),
                translation: "Hai".into(),
            },
        ];
        write_json(&job.translation_path(), &lines).unwrap();
        let tone = Pcm {
            rate: 1000,
            samples: vec![1000; 1000],
        };
        write_wav(&job.line_wav(1), &tone).unwrap();
        write_wav(&job.line_wav(2), &tone).unwrap();
        let passage = Pcm {
            rate: 1000,
            samples: vec![8000; 1000],
        };
        let listen_dir = job.root.join("listen");
        std::fs::create_dir_all(&listen_dir).unwrap();
        write_wav(&listen_dir.join("001.wav"), &passage).unwrap();
        write_wav(&listen_dir.join("002.wav"), &passage).unwrap();
        write_json(
            &listen_dir.join("manifest.json"),
            &crate::listen::ListenPlan {
                voice: "banmai".into(),
                passages: vec![
                    crate::listen::Passage {
                        text: "Một.".into(),
                        rest_before: false,
                    },
                    crate::listen::Passage {
                        text: "Hai.".into(),
                        rest_before: true,
                    },
                ],
                render: crate::listen::PASSAGE_RENDER,
            },
        )
        .unwrap();

        let wav = assemble_listen(&job, &lines).unwrap();
        let joined = read_wav(&wav).unwrap();
        assert_eq!(
            joined.samples.len(),
            2450,
            "đoạn sau nghỉ 0,45 giây, không thở 120 ms"
        );
        let listen = listen_beside(&video);
        let captions = std::fs::read_to_string(listen.with_extension("srt")).unwrap();
        assert!(captions.contains("00:00:01,450"), "{captions}");
        assert!(!captions.contains("00:00:01,120"), "{captions}");
        assert_eq!(std::fs::read(&video).unwrap(), b"video-bytes");
        assert_eq!(std::fs::read_to_string(&picture_srt).unwrap(), "mốc hình");
        assert_eq!(read_fit_mode(&job.root), "sync");
        assert!(!job.dub().exists());
        assert!(file_ready(&job.line_wav(1)));

        std::fs::write(&listen, b"lien-mach").unwrap();
        let saved = dir.join("tai.m4a");
        save_spoken(&video, &saved).unwrap();
        assert_eq!(std::fs::read(&saved).unwrap(), b"lien-mach");

        let on_picture = script_for(&video).unwrap();
        assert!((on_picture[1].start - 1.4).abs() < 0.001);
        std::fs::write(&listen, b"x").unwrap();
        let on_listen = script_for(&listen).unwrap();
        assert!(
            (on_listen[1].start - 1.45).abs() < 0.02,
            "mốc nghe {}",
            on_listen[1].start
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn listen_speaks_in_the_window_even_when_piper_exe_exists() {
        assert_eq!(
            crate::listen::listen_engine(),
            crate::piper::SpeakEngine::Window
        );
    }

    #[test]
    fn encode_spoken_writes_m4a_when_ffmpeg_is_installed() {
        let ffmpeg = Command::new("ffmpeg")
            .args(["-hide_banner", "-version"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if !ffmpeg.map(|status| status.success()).unwrap_or(false) {
            return;
        }
        let dir = std::env::temp_dir().join(format!("encode-spoken-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let dub = dir.join("dub.wav");
        write_wav(
            &dub,
            &Pcm {
                rate: 22050,
                samples: vec![0; 22050],
            },
        )
        .unwrap();
        let out = dir.join("tap.vi.m4a");
        let cancel = AtomicBool::new(false);
        encode_spoken(&dub, &out, &cancel, &mut |_| {}).unwrap();
        assert!(file_ready(&out));
        assert!(!dir.join("tap.vi.partial.m4a").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn ffmpeg_ok() -> bool {
        Command::new("ffmpeg")
            .args(["-hide_banner", "-version"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    fn text_job(name: &str) -> (PathBuf, Job) {
        let dir = std::env::temp_dir().join(format!(
            "{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let job = Job {
            root: dir.clone(),
            video: dir.join("tap.mp4"),
            picture: dir.join("picture.mp4"),
            target: "vi".into(),
        };
        (dir, job)
    }

    fn make_picture(path: &Path, with_audio: bool) {
        let mut command = Command::new("ffmpeg");
        command.args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=320x240:d=1",
        ]);
        if with_audio {
            command.args([
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=1",
                "-shortest",
            ]);
        }
        let made = command
            .args(["-c:v", "libx264", "-pix_fmt", "yuv420p"])
            .arg(path)
            .status()
            .unwrap()
            .success();
        assert!(made);
    }

    #[test]
    fn burn_original_keeps_the_source_audio_and_skips_the_dub_file() {
        if !ffmpeg_ok() {
            return;
        }
        let (dir, job) = text_job("burn-original");
        make_picture(&job.picture, true);
        let cancel = AtomicBool::new(false);
        let lines = vec![TranslatedLine {
            start: 0.0,
            end: 1.0,
            source: "hi".into(),
            translation: "xin chào".into(),
        }];
        burn_original(&job, &lines, &cancel).unwrap();
        assert!(file_ready(&job.captioned()));
        assert!(srt_has_cues(&job.subtitles()));
        assert!(!job.result().exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rotated_unicode_source_burns_and_saves_with_display_dimensions() {
        if !ffmpeg_ok() || crate::program_path("ffprobe").is_none() {
            return;
        }
        let (dir, mut job) = text_job("nguồn xoay có dấu");
        let original = dir.join("nguồn ban đầu.mp4");
        make_picture(&original, true);
        job.picture = dir.join("hình xoay 90 độ.mp4");
        let made = Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-y", "-display_rotation", "90", "-i"])
            .arg(&original)
            .args(["-c", "copy"])
            .arg(&job.picture)
            .status().unwrap();
        assert!(made.success());
        let cancel = AtomicBool::new(false);
        assert_eq!(super::super::captions::probe_dimensions(&job.picture, &cancel).unwrap(), (240, 320));
        burn_original(&job, &[TranslatedLine {
            start: 0.0, end: 1.0, source: "hello".into(),
            translation: "Một câu tiếng Việt".into(),
        }], &cancel).unwrap();
        assert_eq!(super::super::captions::probe_dimensions(&job.captioned(), &cancel).unwrap(), (240, 320));
        let destination = dir.join("Kết quả đã lưu").join("Tập một có chữ.mp4");
        save_video(&job.captioned(), &destination).unwrap();
        assert_eq!(std::fs::read(&destination).unwrap(), std::fs::read(job.captioned()).unwrap());
        let decoded = Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-i"])
            .arg(&destination)
            .args(["-map", "0:v:0", "-map", "0:a:0", "-f", "null", "-"])
            .output().unwrap();
        assert!(decoded.status.success(), "{}", String::from_utf8_lossy(&decoded.stderr));
        assert!(!partial_path(&job.captioned()).exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn read_only_destination_keeps_old_video_and_cleans_temp() {
        let dir = save_video_dir("readonly");
        let source = dir.join("nguồn.mp4");
        let destination = dir.join("kết quả.mp4");
        std::fs::write(&source, b"new-video").unwrap();
        std::fs::write(&destination, b"previous-video").unwrap();
        let mut permissions = std::fs::metadata(&destination).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&destination, permissions).unwrap();
        let result = save_video(&source, &destination);
        let mut permissions = std::fs::metadata(&destination).unwrap().permissions();
        permissions.set_readonly(false);
        std::fs::set_permissions(&destination, permissions).unwrap();
        assert!(result.is_err());
        assert_eq!(std::fs::read(&destination).unwrap(), b"previous-video");
        assert_eq!(std::fs::read(&source).unwrap(), b"new-video");
        assert_eq!(file_names(&dir).len(), 2);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn burn_original_rejects_a_silent_picture_and_leaves_no_partial() {
        if !ffmpeg_ok() {
            return;
        }
        let (dir, job) = text_job("burn-silent");
        make_picture(&job.picture, false);
        let cancel = AtomicBool::new(false);
        let lines = vec![TranslatedLine {
            start: 0.0,
            end: 1.0,
            source: "hi".into(),
            translation: "xin chào".into(),
        }];
        assert!(burn_original(&job, &lines, &cancel).is_err());
        assert!(!job.captioned().exists());
        let leftover = std::fs::read_dir(&dir).unwrap().any(|entry| {
            entry
                .ok()
                .and_then(|item| item.file_name().into_string().ok())
                .is_some_and(|name| name.ends_with(".partial.mp4"))
        });
        assert!(!leftover);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn burn_original_keeps_pcm_audio_from_a_matroska_source() {
        if !ffmpeg_ok() {
            return;
        }
        let (dir, mut job) = text_job("burn-pcm");
        job.picture = dir.join("picture.mkv");
        let made = Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "color=c=black:s=320x240:d=1",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=1",
                "-shortest",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "pcm_s16le",
            ])
            .arg(&job.picture)
            .status()
            .unwrap()
            .success();
        assert!(made);
        let cancel = AtomicBool::new(false);
        let lines = vec![TranslatedLine {
            start: 0.0,
            end: 1.0,
            source: "hi".into(),
            translation: "xin chào".into(),
        }];
        burn_original(&job, &lines, &cancel).unwrap();
        assert!(file_ready(&job.captioned()));
        let probe = Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-select_streams",
                "a:0",
                "-show_entries",
                "stream=codec_name",
                "-of",
                "csv=p=0",
            ])
            .arg(job.captioned())
            .output()
            .unwrap();
        assert!(probe.status.success());
        assert_eq!(String::from_utf8_lossy(&probe.stdout).trim(), "aac");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
