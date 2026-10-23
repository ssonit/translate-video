use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager};

use crate::DubControl;

use crate::job::{file_ready, read_json, write_json, Cue, Glossary, Job, Term, TranslatedLine};
use crate::names::{
    apply_names, name_signature, names_from, names_prompt, present_in, usable_names, NameFix,
    NameList, NamesPending,
};
use crate::ollama::{
    batch_prompt, batch_translations, chunk_ranges, complete, complete_timed, glossary_from,
    retry_prompt, strip_stray_script, summary_prompt, terms_for_line, translated_text,
    wrong_language, CompleteError, TranslateTrace, CHUNK_CHARS, CHUNK_LINES, ONLINE_CHUNK_CHARS,
    ONLINE_CHUNK_LINES,
};

use super::retake::language_code;
use super::{drop_after_names, report, report_progress, stopped, RunSettings};

fn skips_terms(choice: &crate::TranslationChoice) -> bool {
    if choice.provider != "local" {
        return false;
    }
    let model = choice.model.trim();
    let model = if model.is_empty() {
        crate::ollama::LOCAL_MODEL
    } else {
        model
    };
    crate::ollama::is_translate_gemma(model)
}

fn commit_renames(job: &Job, lines: &[Cue], names: &[NameFix]) -> Vec<Cue> {
    let next = renamed(lines, names);
    invalidate_changed_lines(job, &next);
    next
}

// Câu nào đổi chữ thì bỏ bản dịch và wav của câu đó. Câu khác giữ nguyên.
fn invalidate_changed_lines(job: &Job, lines: &[Cue]) {
    let Ok(mut done) = read_json::<Vec<TranslatedLine>>(&job.translation_path()) else {
        return;
    };
    if done.len() != lines.len() {
        drop_after_names(job);
        return;
    }
    let mut changed = false;
    for (index, line) in lines.iter().enumerate() {
        if done[index].source == line.text {
            continue;
        }
        done[index].source = line.text.clone();
        done[index].start = line.start;
        done[index].end = line.end;
        done[index].translation.clear();
        let _ = std::fs::remove_file(job.line_wav(index + 1));
        changed = true;
    }
    if !changed {
        return;
    }
    let _ = write_json(&job.translation_path(), &done);
    drop_mix(job);
}

pub(super) fn drop_mix(job: &Job) {
    let _ = std::fs::remove_file(job.fit_report());
    let _ = std::fs::remove_file(job.dub());
}

fn remove_line_wavs(job: &Job) {
    if let Ok(entries) = std::fs::read_dir(job.root.join("lines")) {
        for entry in entries.flatten() {
            if entry.path().extension().is_some_and(|ext| ext == "wav") {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
}

fn source_media_ready(job: &Job) -> bool {
    file_ready(&job.video) || file_ready(&job.picture) || file_ready(&job.audio())
}

// Làm lại bản đã xong. Cất video đã xuất, rồi chỉ xóa phần sẽ làm lại.
pub(crate) fn prepare_again(
    job: &Job,
    target: &str,
    voice: &str,
    names: &[NameFix],
    retranscribe: bool,
    use_terms: bool,
    skip_translate: bool,
    text_only: bool,
) -> Result<(), String> {
    if retranscribe && !source_media_ready(job) {
        return Err("Không còn video gốc hoặc tiếng đã tải.".into());
    }
    let saved: Option<RunSettings> = read_json(&job.settings_path()).ok();
    let previous_names: NameList = read_json(&job.names_path()).unwrap_or_default();
    let names_changed = name_signature(&previous_names.names) != name_signature(names);
    let voice_changed = saved.as_ref().is_some_and(|item| item.voice != voice);
    let mode_changed = saved
        .as_ref()
        .is_some_and(|item| item.text_only != text_only);
    let text_changed = match &saved {
        Some(item) => {
            item.target != target
                || item.use_terms != use_terms
                || item.skip_translate != skip_translate
        }
        None => job_language(job) != target,
    };
    if !retranscribe && !text_changed && !names_changed && !voice_changed && !mode_changed {
        return Err("Không có gì để làm lại.".into());
    }
    crate::job::archive_exports(&job.root)?;
    drop_mix(job);
    if retranscribe {
        for path in [
            job.transcript(),
            job.lines_path(),
            job.names_path(),
            job.translation_path(),
            job.terms_path(),
            job.subtitles(),
        ] {
            let _ = std::fs::remove_file(path);
        }
        remove_line_wavs(job);
        return Ok(());
    }
    if text_changed {
        let _ = std::fs::remove_file(job.translation_path());
        let _ = std::fs::remove_file(job.terms_path());
        remove_line_wavs(job);
    } else if voice_changed {
        remove_line_wavs(job);
    }
    write_json(
        &job.names_path(),
        &NameList {
            confirmed: true,
            names: usable_names(names),
        },
    )?;
    Ok(())
}

fn job_language(job: &Job) -> String {
    std::fs::read_dir(&job.root)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if !path.is_file() || crate::job::is_extra_video(&path) {
                return None;
            }
            let ext = path
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or("");
            (ext.eq_ignore_ascii_case("mp4") || ext.eq_ignore_ascii_case("m4a")).then_some(path)
        })
        .find_map(|path| {
            let code = language_code(&path);
            (!code.is_empty()).then_some(code)
        })
        .unwrap_or_default()
}

pub(super) fn renamed(lines: &[Cue], names: &[NameFix]) -> Vec<Cue> {
    lines
        .iter()
        .map(|line| Cue {
            start: line.start,
            end: line.end,
            text: apply_names(&line.text, names),
        })
        .collect()
}

fn clear_name_review(app: &AppHandle) {
    app.state::<DubControl>().update(|snap| snap.names = None);
}

fn wait_for_names(
    app: &AppHandle,
    names: Vec<NameFix>,
    note: String,
    cancel: &AtomicBool,
) -> Result<crate::names::NameDecision, String> {
    let (sender, receiver) = mpsc::channel();
    let pending = NamesPending { names, note };
    app.state::<DubControl>().park_name_reply(sender);
    app.state::<DubControl>()
        .update(|snap| snap.names = Some(pending.clone()));
    let _ = app.emit("names-review", &pending);
    loop {
        if let Err(error) = stopped(cancel) {
            let _ = app.state::<DubControl>().take_name_reply();
            clear_name_review(app);
            return Err(error);
        }
        match receiver.recv_timeout(Duration::from_millis(200)) {
            Ok(decision) => {
                clear_name_review(app);
                return Ok(decision);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                clear_name_review(app);
                return Err("Đã dừng.".into());
            }
        }
    }
}

pub(super) fn names_step(
    app: &AppHandle,
    job: &Job,
    lines: &[Cue],
    choice: &crate::TranslationChoice,
    cancel: &AtomicBool,
) -> Result<Vec<Cue>, String> {
    if let Some(saved) = read_json::<NameList>(&job.names_path())
        .ok()
        .filter(|list| list.confirmed)
    {
        let count = usable_names(&saved.names).len();
        let detail = if count == 0 {
            "Không đổi tên.".to_string()
        } else {
            format!("Đã có {count} tên, áp dụng.")
        };
        report(app, "names", "skipped", detail);
        return Ok(commit_renames(job, lines, &saved.names));
    }
    stopped(cancel)?;
    report(app, "names", "running", "Đang tìm tên bị chép sai.");
    let full = lines
        .iter()
        .map(|line| line.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let sample: String = full.chars().take(6000).collect();
    let (proposed, note) = match complete(choice, &names_prompt(&sample), cancel) {
        Err(CompleteError::Stopped) => return Err("Đã dừng.".into()),
        Err(error) => (
            Vec::new(),
            format!("AI không đề xuất được ({error}). Bạn thêm tên rồi dịch tiếp."),
        ),
        Ok(value) => match names_from(&value) {
            Some(found) => {
                let found = present_in(&full, &found);
                let note = if found.is_empty() {
                    "AI không thấy tên cần sửa. Bạn thêm nếu muốn, rồi dịch tiếp.".to_string()
                } else {
                    String::new()
                };
                (found, note)
            }
            None => (
                Vec::new(),
                "AI không đề xuất được. Bạn thêm tên rồi dịch tiếp.".to_string(),
            ),
        },
    };
    stopped(cancel)?;
    report(app, "names", "running", "Đang chờ bạn sửa tên.");
    let decision = wait_for_names(app, proposed, note, cancel)?;
    let chosen = if decision.keep {
        Vec::new()
    } else {
        usable_names(&decision.names)
    };
    write_json(
        &job.names_path(),
        &NameList {
            confirmed: true,
            names: chosen.clone(),
        },
    )?;
    if decision.keep {
        report(app, "names", "skipped", "Giữ nguyên lời chép.");
    } else if chosen.is_empty() {
        report(app, "names", "done", "Không đổi tên.");
    } else {
        report(app, "names", "done", format!("{} tên.", chosen.len()));
    }
    Ok(commit_renames(job, lines, &chosen))
}

pub(super) fn terms(
    app: &AppHandle,
    job: &Job,
    lines: &[Cue],
    choice: &crate::TranslationChoice,
    target: &str,
    use_terms: bool,
    cancel: &AtomicBool,
) -> Result<Glossary, String> {
    if !use_terms {
        report(app, "terms", "skipped", "Bỏ qua, dịch luôn.");
        return Ok(Glossary {
            theme: String::new(),
            terms: Vec::new(),
        });
    }
    if skips_terms(choice) {
        report(
            app,
            "terms",
            "skipped",
            "Model này không lấy được thuật ngữ. Bỏ qua, dịch luôn.",
        );
        return Ok(Glossary {
            theme: String::new(),
            terms: Vec::new(),
        });
    }
    if file_ready(&job.terms_path()) {
        if let Ok(glossary) = read_json::<Glossary>(&job.terms_path()) {
            report(
                app,
                "terms",
                "skipped",
                format!("Đã có {} thuật ngữ, bỏ qua.", glossary.terms.len()),
            );
            return Ok(glossary);
        }
    }
    stopped(cancel)?;
    report(app, "terms", "running", "Đang lấy thuật ngữ.");
    let mut source = String::new();
    for line in lines {
        if source.chars().count() >= 6000 {
            break;
        }
        source.push_str(&line.text);
        source.push(' ');
    }
    // Thuật ngữ chỉ giúp dịch nhất quán hơn, không bắt buộc. AI lỗi hay quá tải thì bỏ qua, dịch tiếp.
    let fetched = complete(choice, &summary_prompt(&source, target), cancel)
        .map_err(|error| error.to_string())
        .and_then(|value| glossary_from(&value).map_err(|error| error.to_string()));
    stopped(cancel)?;
    let glossary = match fetched {
        Ok(glossary) => glossary,
        Err(error) => {
            report(
                app,
                "terms",
                "skipped",
                format!("Không lấy được thuật ngữ ({error}). Bỏ qua, dịch luôn."),
            );
            return Ok(Glossary {
                theme: String::new(),
                terms: Vec::new(),
            });
        }
    };
    write_json(&job.terms_path(), &glossary)?;
    report(
        app,
        "terms",
        "done",
        format!("{} thuật ngữ.", glossary.terms.len()),
    );
    Ok(glossary)
}

// Lời đã chép chính là lời cần đọc. Không gọi model, kể cả khi câu nhiều từ Hán Việt.
pub(super) fn keep_source(
    app: &AppHandle,
    job: &Job,
    lines: &[Cue],
) -> Result<Vec<TranslatedLine>, String> {
    let kept = lines_as_spoken(lines);
    if file_ready(&job.translation_path()) {
        if let Ok(done) = read_json::<Vec<TranslatedLine>>(&job.translation_path()) {
            if done == kept {
                report(
                    app,
                    "translate",
                    "skipped",
                    format!("Giữ nguyên {} câu, bỏ qua.", done.len()),
                );
                return Ok(done);
            }
        }
    }
    write_json(&job.translation_path(), &kept)?;
    report(
        app,
        "translate",
        "done",
        format!("Giữ nguyên {} câu, không dịch.", kept.len()),
    );
    Ok(kept)
}

fn lines_as_spoken(lines: &[Cue]) -> Vec<TranslatedLine> {
    lines
        .iter()
        .map(|line| TranslatedLine {
            start: line.start,
            end: line.end,
            source: line.text.clone(),
            translation: line.text.clone(),
        })
        .collect()
}

pub(super) fn translate(
    app: &AppHandle,
    job: &Job,
    lines: &[Cue],
    glossary: &Glossary,
    choice: &crate::TranslationChoice,
    target: &str,
    target_code: &str,
    cancel: &AtomicBool,
) -> Result<Vec<TranslatedLine>, String> {
    if file_ready(&job.translation_path()) {
        if let Ok(done) = read_json::<Vec<TranslatedLine>>(&job.translation_path()) {
            let in_target = done
                .iter()
                .all(|item| !wrong_language(&item.translation, target_code));
            if translation_ready(lines, &done) && in_target {
                report(
                    app,
                    "translate",
                    "skipped",
                    format!("Đã dịch {} câu, bỏ qua.", done.len()),
                );
                return Ok(done);
            }
        }
    }
    stopped(cancel)?;
    let mut done = if file_ready(&job.translation_path()) {
        read_json::<Vec<TranslatedLine>>(&job.translation_path()).unwrap_or_default()
    } else {
        Vec::new()
    };
    done = align_saved(lines, &done);
    // A saved line in the wrong language is dropped, so it gets translated again below.
    for item in done.iter_mut() {
        if wrong_language(&item.translation, target_code) {
            item.translation.clear();
        }
    }
    report(app, "translate", "running", "Đang dịch.");
    let mut trace = TranslateTrace::logging();
    let outcome = translate_work(
        lines,
        done,
        glossary,
        choice,
        target,
        target_code,
        cancel,
        None,
        &mut trace,
        &mut |detail, done_n, total| report_progress(app, "translate", detail, done_n, total),
        &mut |done| write_json(&job.translation_path(), done),
    );
    eprintln!("{}", trace.summary_line());
    let (done, skipped) = outcome?;
    if skipped.is_empty() {
        report(
            app,
            "translate",
            "done",
            format!("Đã dịch {} câu.", done.len()),
        );
    } else {
        let list = skipped
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        report(
            app,
            "translate",
            "done",
            format!("Đã dịch {} câu. Câu {list} model chưa ra {target}, tạm để trống; bấm Chạy lại để thử riêng các câu đó.", done.len() - skipped.len()),
        );
    }
    Ok(done)
}

pub(crate) fn translate_work(
    lines: &[Cue],
    mut done: Vec<TranslatedLine>,
    glossary: &Glossary,
    choice: &crate::TranslationChoice,
    target: &str,
    target_code: &str,
    cancel: &AtomicBool,
    max_batches: Option<usize>,
    trace: &mut TranslateTrace,
    progress: &mut dyn FnMut(String, f64, f64),
    save: &mut dyn FnMut(&Vec<TranslatedLine>) -> Result<(), String>,
) -> Result<(Vec<TranslatedLine>, Vec<usize>), String> {
    // Máy một GPU: từng lô một. API online gom lô và chạy nhiều lô cùng lúc.
    if choice.provider != "local" {
        return translate_online(
            lines,
            done,
            glossary,
            choice,
            target,
            target_code,
            cancel,
            max_batches,
            trace,
            progress,
            save,
        );
    }
    trace.begin(0);
    if let Err(error) = complete_timed(choice, "Trả về JSON {\"ok\":true}", cancel, trace) {
        if matches!(error, CompleteError::Stopped) {
            return Err(error.to_string());
        }
        eprintln!("[timing] warm-up lỗi: {error}");
    }
    let mut skipped: Vec<usize> = Vec::new();
    let total = lines.len();
    let texts: Vec<String> = lines.iter().map(|line| line.text.clone()).collect();
    let mut finished = 0usize;
    for (batch_index, range) in chunk_ranges(&texts, CHUNK_LINES, CHUNK_CHARS)
        .into_iter()
        .enumerate()
    {
        if max_batches.is_some_and(|limit| finished >= limit) {
            break;
        }
        stopped(cancel)?;
        let todo: Vec<usize> = range
            .clone()
            .filter(|&i| !texts[i].trim().is_empty() && done[i].translation.trim().is_empty())
            .collect();
        if todo.is_empty() {
            continue;
        }
        finished += 1;
        progress(
            format!("Câu {}-{}/{}.", range.start + 1, range.end, total),
            range.start as f64,
            total as f64,
        );
        let before: Vec<&str> = texts[range.start.saturating_sub(3)..range.start]
            .iter()
            .map(String::as_str)
            .collect();
        let after: Vec<&str> = texts[range.end..(range.end + 2).min(total)]
            .iter()
            .map(String::as_str)
            .collect();
        let mut matched: Vec<&Term> = Vec::new();
        for &i in &todo {
            for term in terms_for_line(&glossary.terms, &texts[i]) {
                if !matched.iter().any(|seen| seen.src == term.src) {
                    matched.push(term);
                }
            }
        }
        let batch: Vec<&str> = todo.iter().map(|&i| texts[i].as_str()).collect();
        trace.begin((batch_index + 1) as u32);
        let parsed = complete_timed(
            choice,
            &batch_prompt(&batch, &before, &after, &matched, &glossary.theme, target),
            cancel,
            trace,
        )
        .ok()
        .map(|value| batch_translations(&value, batch.len()));
        stopped(cancel)?;
        let mut again: Vec<usize> = Vec::new();
        match parsed {
            Some(items) => {
                for (&i, text) in todo.iter().zip(items) {
                    match text {
                        Some(text) if !wrong_language(&text, target_code) => {
                            done[i].translation = text
                        }
                        _ => again.push(i),
                    }
                }
            }
            None => again = todo.clone(),
        }
        // Lines the batch got wrong or missed are translated one by one, up to 4 tries each. A line that
        // still fails never stops the run: stray foreign words are cut out, else the line stays empty (silent)
        // and the next run tries only that line again.
        for i in again {
            let matched = terms_for_line(&glossary.terms, &texts[i]);
            let prompt = retry_prompt(&texts[i], &matched, target);
            let mut rejected: Vec<String> = Vec::new();
            let mut last_error = String::new();
            let mut accepted = false;
            for _ in 0..4 {
                stopped(cancel)?;
                match complete_timed(choice, &prompt, cancel, trace) {
                    Ok(value) => match translated_text(&value) {
                        Some(text) if !wrong_language(&text, target_code) => {
                            done[i].translation = text;
                            accepted = true;
                            break;
                        }
                        Some(text) => rejected.push(text),
                        None => last_error = format!("Model không trả lời câu {}.", i + 1),
                    },
                    Err(error) => last_error = error.to_string(),
                }
            }
            stopped(cancel)?;
            if accepted {
                continue;
            }
            if rejected.is_empty() {
                return Err(last_error);
            }
            let cleaned = rejected
                .iter()
                .rev()
                .map(|text| strip_stray_script(text, target_code))
                .find(|text| {
                    text.chars().filter(|ch| ch.is_alphabetic()).count() >= 3
                        && !wrong_language(text, target_code)
                });
            match cleaned {
                Some(text) => done[i].translation = text,
                None => skipped.push(i + 1),
            }
        }
        save(&done)?;
    }
    Ok((done, skipped))
}

// Bốn request cùng lúc. Cao hơn dễ dính 429, chờ rồi gọi lại còn chậm hơn.
const ONLINE_PARALLEL: usize = 4;

// Server Deepseek-API (DeepSeek local) xử lý từng request và giới hạn khoảng 30 lần mỗi phút.
// DeepSeek API trả phí không bị vậy nên vẫn chạy song song.
fn online_width(provider: &str) -> usize {
    if provider == "deepseek-local" {
        1
    } else {
        ONLINE_PARALLEL
    }
}
// Câu lô trả thiếu hoặc sai ngôn ngữ gom lại, gọi lại cả cụm. Không gọi từng câu.
const ONLINE_RETRY_PASSES: usize = 2;

struct PendingBatch {
    batch_no: u32,
    start: usize,
    end: usize,
    todo: Vec<usize>,
    prompt: String,
}

struct OpenLine {
    index: usize,
    rejected: Vec<String>,
    last_error: String,
}

enum BatchOutcome {
    Lines(Vec<Option<String>>),
    Failed(String),
    Stopped,
}

struct BatchReply {
    todo: Vec<usize>,
    outcome: BatchOutcome,
}

struct WaveResult {
    replies: Vec<BatchReply>,
    stopped: bool,
}

fn translate_online(
    lines: &[Cue],
    mut done: Vec<TranslatedLine>,
    glossary: &Glossary,
    choice: &crate::TranslationChoice,
    target: &str,
    target_code: &str,
    cancel: &AtomicBool,
    max_batches: Option<usize>,
    trace: &mut TranslateTrace,
    progress: &mut dyn FnMut(String, f64, f64),
    save: &mut dyn FnMut(&Vec<TranslatedLine>) -> Result<(), String>,
) -> Result<(Vec<TranslatedLine>, Vec<usize>), String> {
    let width = online_width(&choice.provider);
    struct Tick {
        started: Instant,
        width: usize,
    }
    impl Drop for Tick {
        fn drop(&mut self) {
            eprintln!(
                "[timing] dịch online chờ {:.1}s, tối đa {} lượt một lúc",
                self.started.elapsed().as_secs_f64(),
                self.width,
            );
        }
    }
    let _tick = Tick {
        started: Instant::now(),
        width,
    };
    let texts: Vec<String> = lines.iter().map(|line| line.text.clone()).collect();
    let mut next_batch = 1u32;
    let jobs = collect_jobs(
        &texts,
        &done,
        glossary,
        target,
        max_batches,
        &mut next_batch,
    );
    let mut open: Vec<OpenLine> = Vec::new();
    let mut skipped: Vec<usize> = Vec::new();
    for wave in jobs.chunks(width) {
        stopped(cancel)?;
        show_span(wave, &done, &texts, progress);
        let result = run_wave(choice, cancel, trace, wave)?;
        apply_wave(result, target_code, &mut done, &mut open, save, true)?;
        show_span(wave, &done, &texts, progress);
    }
    for _ in 0..ONLINE_RETRY_PASSES {
        let pending: Vec<usize> = open
            .iter()
            .map(|line| line.index)
            .filter(|&index| done[index].translation.trim().is_empty())
            .collect();
        if pending.is_empty() {
            break;
        }
        stopped(cancel)?;
        progress(
            format!("Dịch lại {} câu.", pending.len()),
            settled_count(&done, &texts),
            texts.len() as f64,
        );
        let mut retries = Vec::new();
        for indices in group_indices(&texts, &pending, ONLINE_CHUNK_LINES, ONLINE_CHUNK_CHARS) {
            let start = indices[0];
            let end = indices[indices.len() - 1] + 1;
            let batch_no = next_batch;
            next_batch += 1;
            retries.push(PendingBatch {
                batch_no,
                start,
                end,
                prompt: prompt_for(&texts, &indices, glossary, target),
                todo: indices,
            });
        }
        for wave in retries.chunks(width) {
            stopped(cancel)?;
            let result = run_wave(choice, cancel, trace, wave)?;
            apply_wave(result, target_code, &mut done, &mut open, save, false)?;
        }
    }
    settle_open(&open, &mut done, target_code, &mut skipped)?;
    save(&done)?;
    Ok((done, skipped))
}

fn collect_jobs(
    texts: &[String],
    done: &[TranslatedLine],
    glossary: &Glossary,
    target: &str,
    max_batches: Option<usize>,
    next_batch: &mut u32,
) -> Vec<PendingBatch> {
    let mut jobs = Vec::new();
    for range in chunk_ranges(texts, ONLINE_CHUNK_LINES, ONLINE_CHUNK_CHARS) {
        if max_batches.is_some_and(|limit| jobs.len() >= limit) {
            break;
        }
        let todo: Vec<usize> = range
            .clone()
            .filter(|&index| {
                !texts[index].trim().is_empty() && done[index].translation.trim().is_empty()
            })
            .collect();
        if todo.is_empty() {
            continue;
        }
        let batch_no = *next_batch;
        *next_batch += 1;
        jobs.push(PendingBatch {
            batch_no,
            start: range.start,
            end: range.end,
            prompt: prompt_for(texts, &todo, glossary, target),
            todo,
        });
    }
    jobs
}

fn prompt_for(texts: &[String], indices: &[usize], glossary: &Glossary, target: &str) -> String {
    let total = texts.len();
    let start = indices.first().copied().unwrap_or(0);
    let end = indices
        .last()
        .copied()
        .map(|index| index + 1)
        .unwrap_or(start);
    let before: Vec<&str> = texts[start.saturating_sub(3)..start]
        .iter()
        .map(String::as_str)
        .collect();
    let after: Vec<&str> = texts[end..(end + 2).min(total)]
        .iter()
        .map(String::as_str)
        .collect();
    let mut matched: Vec<&Term> = Vec::new();
    for &index in indices {
        for term in terms_for_line(&glossary.terms, &texts[index]) {
            if !matched.iter().any(|seen| seen.src == term.src) {
                matched.push(term);
            }
        }
    }
    let batch: Vec<&str> = indices.iter().map(|&index| texts[index].as_str()).collect();
    batch_prompt(&batch, &before, &after, &matched, &glossary.theme, target)
}

fn group_indices(
    texts: &[String],
    indices: &[usize],
    max_lines: usize,
    max_chars: usize,
) -> Vec<Vec<usize>> {
    let subset: Vec<String> = indices.iter().map(|&index| texts[index].clone()).collect();
    chunk_ranges(&subset, max_lines, max_chars)
        .into_iter()
        .map(|range| indices[range].to_vec())
        .collect()
}

fn show_span(
    wave: &[PendingBatch],
    done: &[TranslatedLine],
    texts: &[String],
    progress: &mut dyn FnMut(String, f64, f64),
) {
    let start = wave.iter().map(|job| job.start).min().unwrap_or(0);
    let end = wave.iter().map(|job| job.end).max().unwrap_or(start);
    progress(
        format!("Câu {}-{}/{}.", start + 1, end, texts.len()),
        settled_count(done, texts),
        texts.len() as f64,
    );
}

fn settled_count(done: &[TranslatedLine], texts: &[String]) -> f64 {
    done.iter()
        .zip(texts)
        .filter(|(item, text)| text.trim().is_empty() || !item.translation.trim().is_empty())
        .count() as f64
}

fn run_wave(
    choice: &crate::TranslationChoice,
    cancel: &AtomicBool,
    trace: &mut TranslateTrace,
    jobs: &[PendingBatch],
) -> Result<WaveResult, String> {
    if jobs.is_empty() {
        return Ok(WaveResult {
            replies: Vec::new(),
            stopped: false,
        });
    }
    let collected = thread::scope(|scope| {
        let trace_ref: &TranslateTrace = trace;
        let mut handles = Vec::with_capacity(jobs.len());
        for job in jobs {
            handles.push(scope.spawn(|| {
                let todo = job.todo.clone();
                let mut local = trace_ref.detached(job.batch_no);
                let outcome = match complete_timed(choice, &job.prompt, cancel, &mut local) {
                    Ok(value) => BatchOutcome::Lines(batch_translations(&value, todo.len())),
                    Err(CompleteError::Stopped) => BatchOutcome::Stopped,
                    Err(error) => BatchOutcome::Failed(error.to_string()),
                };
                (todo, outcome, local)
            }));
        }
        handles
            .into_iter()
            .map(|handle| {
                handle
                    .join()
                    .unwrap_or_else(|_| panic!("luồng dịch dừng đột ngột"))
            })
            .collect::<Vec<_>>()
    });
    let mut replies = Vec::new();
    let mut stopped = false;
    for (todo, outcome, local) in collected {
        trace.absorb(local);
        match outcome {
            BatchOutcome::Stopped => stopped = true,
            other => replies.push(BatchReply {
                todo,
                outcome: other,
            }),
        }
    }
    Ok(WaveResult { replies, stopped })
}

fn apply_wave(
    result: WaveResult,
    target_code: &str,
    done: &mut Vec<TranslatedLine>,
    open: &mut Vec<OpenLine>,
    save: &mut dyn FnMut(&Vec<TranslatedLine>) -> Result<(), String>,
    abort_when_all_fail: bool,
) -> Result<(), String> {
    for reply in &result.replies {
        fold_reply(reply, target_code, done, open);
    }
    save(done)?;
    if result.stopped {
        return Err("Đã dừng.".into());
    }
    if abort_when_all_fail {
        if let Some(message) = total_failure(&result.replies) {
            return Err(message);
        }
    }
    Ok(())
}

fn total_failure(replies: &[BatchReply]) -> Option<String> {
    let mut error = None;
    for reply in replies {
        match &reply.outcome {
            BatchOutcome::Failed(message) if error.is_none() => error = Some(message.clone()),
            BatchOutcome::Failed(_) => {}
            BatchOutcome::Lines(_) => return None,
            BatchOutcome::Stopped => {}
        }
    }
    error
}

fn fold_reply(
    reply: &BatchReply,
    target_code: &str,
    done: &mut [TranslatedLine],
    open: &mut Vec<OpenLine>,
) {
    match &reply.outcome {
        BatchOutcome::Lines(items) => {
            for (&index, text) in reply.todo.iter().zip(items) {
                match text {
                    Some(text) if !wrong_language(text, target_code) => {
                        done[index].translation = text.clone();
                        open.retain(|line| line.index != index);
                    }
                    Some(text) => reject_line(open, index, text),
                    None => miss_line(open, index),
                }
            }
        }
        BatchOutcome::Failed(message) => {
            for &index in &reply.todo {
                fail_line(open, index, message);
            }
        }
        BatchOutcome::Stopped => {}
    }
}

fn miss_line(open: &mut Vec<OpenLine>, index: usize) {
    if open.iter().any(|line| line.index == index) {
        return;
    }
    open.push(OpenLine {
        index,
        rejected: Vec::new(),
        last_error: String::new(),
    });
}

fn reject_line(open: &mut Vec<OpenLine>, index: usize, text: &str) {
    if let Some(line) = open.iter_mut().find(|line| line.index == index) {
        line.rejected.push(text.to_string());
        return;
    }
    open.push(OpenLine {
        index,
        rejected: vec![text.to_string()],
        last_error: String::new(),
    });
}

fn fail_line(open: &mut Vec<OpenLine>, index: usize, message: &str) {
    if let Some(line) = open.iter_mut().find(|line| line.index == index) {
        line.last_error = message.to_string();
        return;
    }
    open.push(OpenLine {
        index,
        rejected: Vec::new(),
        last_error: message.to_string(),
    });
}

fn settle_open(
    open: &[OpenLine],
    done: &mut [TranslatedLine],
    target_code: &str,
    skipped: &mut Vec<usize>,
) -> Result<(), String> {
    for line in open {
        if !done[line.index].translation.trim().is_empty() || !line.rejected.is_empty() {
            continue;
        }
        let message = if line.last_error.is_empty() {
            format!("Model không trả lời câu {}.", line.index + 1)
        } else {
            line.last_error.clone()
        };
        return Err(message);
    }
    for line in open {
        if !done[line.index].translation.trim().is_empty() {
            continue;
        }
        let cleaned = line
            .rejected
            .iter()
            .rev()
            .map(|text| strip_stray_script(text, target_code))
            .find(|text| {
                text.chars().filter(|ch| ch.is_alphabetic()).count() >= 3
                    && !wrong_language(text, target_code)
            });
        match cleaned {
            Some(text) => done[line.index].translation = text,
            None => skipped.push(line.index + 1),
        }
    }
    Ok(())
}

fn translation_ready(lines: &[Cue], done: &[TranslatedLine]) -> bool {
    done.len() == lines.len()
        && done.iter().zip(lines).all(|(item, line)| {
            item.source == line.text
                && (line.text.trim().is_empty() || !item.translation.trim().is_empty())
        })
}

fn align_saved(lines: &[Cue], saved: &[TranslatedLine]) -> Vec<TranslatedLine> {
    lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            let translation = saved
                .get(index)
                .filter(|item| item.source == line.text)
                .map(|item| item.translation.clone())
                .unwrap_or_default();
            TranslatedLine {
                start: line.start,
                end: line.end,
                source: line.text.clone(),
                translation,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::PathBuf;
    use std::process::Command;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::thread;

    #[test]
    fn voice_only_keeps_the_transcript_including_han_viet() {
        let lines = [Cue {
            start: 1.0,
            end: 2.5,
            text: "Quốc gia và 中國 đều là Hán Việt.".into(),
        }];
        let kept = lines_as_spoken(&lines);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].source, lines[0].text);
        assert_eq!(kept[0].translation, lines[0].text);
        assert_eq!(kept[0].start, 1.0);
    }

    #[test]
    fn translategemma_skips_term_extraction() {
        let gemma = crate::TranslationChoice {
            provider: "local".into(),
            api_key: String::new(),
            model: "translategemma:4b".into(),
            saved: Default::default(),
        };
        let qwen = crate::TranslationChoice {
            provider: "local".into(),
            api_key: String::new(),
            model: "qwen3.5:9b-q4_K_M".into(),
            saved: Default::default(),
        };
        let default_local = crate::TranslationChoice {
            provider: "local".into(),
            api_key: String::new(),
            model: String::new(),
            saved: Default::default(),
        };
        assert!(skips_terms(&gemma));
        assert!(skips_terms(&crate::TranslationChoice {
            model: "TranslateGemma:12b".into(),
            ..gemma.clone()
        }));
        assert!(!skips_terms(&qwen));
        assert!(!skips_terms(&default_local));
        assert!(!skips_terms(&crate::TranslationChoice {
            provider: "gemini".into(),
            model: "translategemma:4b".into(),
            ..gemma
        }));
    }

    #[test]
    fn only_deepseek_local_is_one_request_at_a_time() {
        assert_eq!(online_width("deepseek-local"), 1);
        for provider in ["deepseek", "gemini", "openai", "claude"] {
            assert_eq!(online_width(provider), ONLINE_PARALLEL, "{provider}");
        }
    }

    // Đo 10 lượt đầu của đúng đường dịch. Cần Ollama đang chạy. Không ghi translation.json.
    #[test]
    #[ignore = "cần Ollama local"]
    fn probe_first_translate_batches() {
        let output = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../output");
        let mut lines = None;
        for entry in std::fs::read_dir(&output).unwrap().flatten() {
            let path = entry.path().join("lines.json");
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Ok(parsed) = serde_json::from_str::<Vec<Cue>>(&text) else {
                continue;
            };
            if parsed.len() == 752 {
                lines = Some(parsed);
                break;
            }
        }
        let lines = lines.expect("không thấy lines.json 752 câu");
        let done: Vec<TranslatedLine> = lines
            .iter()
            .map(|line| TranslatedLine {
                start: line.start,
                end: line.end,
                source: line.text.clone(),
                translation: String::new(),
            })
            .collect();
        let choice = crate::TranslationChoice {
            provider: "local".into(),
            api_key: String::new(),
            model: String::new(),
            saved: Default::default(),
        };
        let cancel = AtomicBool::new(false);
        let stop = std::sync::Arc::new(AtomicBool::new(false));
        let sampler_stop = stop.clone();
        let sampler = thread::spawn(move || {
            let mut samples = Vec::new();
            while !sampler_stop.load(Ordering::Relaxed) {
                if let Ok(text) = Command::new("nvidia-smi")
                    .args([
                        "--query-gpu=utilization.gpu,memory.used,temperature.gpu",
                        "--format=csv,noheader",
                    ])
                    .output()
                {
                    samples.push(String::from_utf8_lossy(&text.stdout).trim().to_string());
                }
                thread::sleep(Duration::from_secs(3));
            }
            samples
        });
        let mut trace = TranslateTrace::logging();
        let result = translate_work(
            &lines,
            done,
            &Glossary {
                theme: String::new(),
                terms: Vec::new(),
            },
            &choice,
            "tiếng Việt",
            "vi",
            &cancel,
            Some(10),
            &mut trace,
            &mut |_, _, _| {},
            &mut |_| Ok(()),
        );
        stop.store(true, Ordering::Relaxed);
        let samples = sampler.join().unwrap_or_default();
        eprintln!("{}", trace.summary_line());
        if let Some(first) = samples.iter().find(|line| !line.is_empty()) {
            eprintln!("[timing] nvidia-smi đầu: {first}");
        }
        if let Some(last) = samples.iter().rev().find(|line| !line.is_empty()) {
            eprintln!("[timing] nvidia-smi cuối: {last}");
        }
        result.expect("dịch 10 lượt");
    }

    #[test]
    fn prepare_again_keeps_the_transcript_and_the_old_video() {
        let (root, job) = sample_job("again-lang");
        write_json(
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
        std::fs::write(
            job.transcript(),
            b"[{\"start\":0,\"end\":1,\"text\":\"hi\"}]",
        )
        .unwrap();
        write_json(
            &job.translation_path(),
            &vec![TranslatedLine {
                start: 0.0,
                end: 1.0,
                source: "hi".into(),
                translation: "chào".into(),
            }],
        )
        .unwrap();
        std::fs::write(root.join("clip.vi.mp4"), b"viet").unwrap();
        prepare_again(&job, "en", "a", &[], false, false, false, false).unwrap();
        assert!(job.transcript().is_file());
        assert!(!job.translation_path().is_file());
        assert!(!root.join("clip.vi.mp4").exists());
        let kept = crate::job::list_kept(&root).unwrap();
        assert_eq!(kept.len(), 1);
        assert!(kept[0].path.ends_with("clip.vi.mp4"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn prepare_again_voice_keeps_the_translation() {
        let (root, job) = sample_job("again-voice");
        write_json(
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
        write_json(
            &job.translation_path(),
            &vec![TranslatedLine {
                start: 0.0,
                end: 1.0,
                source: "hi".into(),
                translation: "chào".into(),
            }],
        )
        .unwrap();
        std::fs::write(job.line_wav(1), b"wav").unwrap();
        std::fs::write(root.join("clip.vi.mp4"), b"viet").unwrap();
        prepare_again(&job, "vi", "b", &[], false, false, false, false).unwrap();
        let kept: Vec<TranslatedLine> = read_json(&job.translation_path()).unwrap();
        assert_eq!(kept[0].translation, "chào");
        assert!(!job.line_wav(1).exists());
        assert!(!root.join("clip.vi.mp4").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn changed_names_keep_the_other_lines() {
        let (root, job) = sample_job("again-names");
        let lines = vec![
            Cue {
                start: 0.0,
                end: 1.0,
                text: "Hello Ann".into(),
            },
            Cue {
                start: 1.0,
                end: 2.0,
                text: "Goodbye".into(),
            },
        ];
        write_json(
            &job.translation_path(),
            &vec![
                TranslatedLine {
                    start: 0.0,
                    end: 1.0,
                    source: "Hello An".into(),
                    translation: "Xin chào An".into(),
                },
                TranslatedLine {
                    start: 1.0,
                    end: 2.0,
                    source: "Goodbye".into(),
                    translation: "Tạm biệt".into(),
                },
            ],
        )
        .unwrap();
        std::fs::write(job.line_wav(1), b"wav1").unwrap();
        std::fs::write(job.line_wav(2), b"wav2").unwrap();
        commit_renames(
            &job,
            &lines,
            &[NameFix {
                from: "Ann".into(),
                to: "Anne".into(),
            }],
        );
        let kept: Vec<TranslatedLine> = read_json(&job.translation_path()).unwrap();
        assert_eq!(kept[0].source, "Hello Anne");
        assert!(kept[0].translation.is_empty());
        assert_eq!(kept[1].translation, "Tạm biệt");
        assert!(!job.line_wav(1).exists());
        assert!(job.line_wav(2).is_file());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn online_jobs_cover_a_long_list_in_fewer_requests() {
        let texts: Vec<String> = (0..80).map(|index| format!("line {index}")).collect();
        let mut done: Vec<TranslatedLine> = texts.iter().map(|text| blank(text)).collect();
        let glossary = Glossary {
            theme: "Máy".into(),
            terms: Vec::new(),
        };
        let mut next = 1u32;
        let jobs = collect_jobs(&texts, &done, &glossary, "tiếng Việt", None, &mut next);
        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[0].todo.len(), 40);
        assert_eq!(jobs[1].todo.first().copied(), Some(40));
        assert!(jobs[0].prompt.contains("1. line 0"));
        assert!(jobs[0].prompt.contains("40. line 39"));
        assert!(jobs[1].prompt.contains("1. line 40"));
        assert_eq!(next, 3);

        done[0].translation = "xong".into();
        done[40].translation = "xong".into();
        let mut next = 1u32;
        let jobs = collect_jobs(&texts, &done, &glossary, "tiếng Việt", Some(1), &mut next);
        assert_eq!(jobs.len(), 1);
        assert!(!jobs[0].todo.contains(&0));
        assert_eq!(jobs[0].todo.len(), 39);
    }

    #[test]
    fn a_wrong_language_line_waits_for_one_batched_retry() {
        let mut done = vec![blank("a"), blank("b")];
        let mut open = Vec::new();
        fold_reply(
            &BatchReply {
                todo: vec![0, 1],
                outcome: BatchOutcome::Lines(vec![
                    Some("Xin chào bạn, hôm nay trời đẹp.".into()),
                    Some("Hello there my friend today".into()),
                ]),
            },
            "vi",
            &mut done,
            &mut open,
        );
        assert_eq!(done[0].translation, "Xin chào bạn, hôm nay trời đẹp.");
        assert!(done[1].translation.is_empty());
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].index, 1);
        assert_eq!(open[0].rejected.len(), 1);
    }

    #[test]
    fn settle_keeps_a_cleaned_line_and_skips_a_hopeless_one() {
        let mut done = vec![blank("a"), blank("b"), blank("c")];
        let open = vec![
            OpenLine {
                index: 0,
                rejected: vec!["Tôi uống một tách咖啡 mỗi sáng".into()],
                last_error: String::new(),
            },
            OpenLine {
                index: 1,
                rejected: vec!["咖啡".into()],
                last_error: String::new(),
            },
        ];
        let mut skipped = Vec::new();
        settle_open(&open, &mut done, "vi", &mut skipped).unwrap();
        assert_eq!(done[0].translation, "Tôi uống một tách mỗi sáng");
        assert!(done[1].translation.is_empty());
        assert_eq!(skipped, vec![2]);
        assert!(done[2].translation.is_empty());
    }

    #[test]
    fn settle_stops_when_the_service_never_answered() {
        let mut done = vec![blank("a")];
        let open = vec![OpenLine {
            index: 0,
            rejected: Vec::new(),
            last_error: "DeepSeek từ chối yêu cầu.".into(),
        }];
        let error = settle_open(&open, &mut done, "vi", &mut Vec::new()).unwrap_err();
        assert_eq!(error, "DeepSeek từ chối yêu cầu.");
        assert!(done[0].translation.is_empty());
    }

    #[test]
    fn retry_groups_follow_the_online_budget() {
        let texts: Vec<String> = (0..5).map(|_| "y".repeat(100)).collect();
        let groups = group_indices(&texts, &[0, 1, 2, 3, 4], 10, 250);
        assert_eq!(groups, vec![vec![0, 1], vec![2, 3], vec![4]]);
    }

    fn blank(text: &str) -> TranslatedLine {
        TranslatedLine {
            start: 0.0,
            end: 1.0,
            source: text.into(),
            translation: String::new(),
        }
    }

    #[test]
    fn retranscribe_without_a_source_deletes_nothing() {
        let (root, job) = sample_job("again-source");
        std::fs::write(job.transcript(), b"[{}]").unwrap();
        let error = prepare_again(&job, "vi", "a", &[], true, false, false, false).unwrap_err();
        assert!(error.contains("Không còn video gốc"));
        assert!(job.transcript().is_file());
        let _ = std::fs::remove_dir_all(&root);
    }

    fn sample_job(name: &str) -> (PathBuf, Job) {
        let root = std::env::temp_dir().join(format!(
            "{name}-{}-{}",
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
            picture: PathBuf::new(),
            target: "vi".into(),
        };
        (root, job)
    }
}
