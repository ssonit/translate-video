//! Source/draft operations. Opening, ASR, saving and sentence listening never
//! enter fit/merge; only explicit export owns the complete pipeline.
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use super::{clean_target, media_duration, stopped, RunSettings};
use crate::editing::{EditedCue, Editing, Structure};
use crate::job::{read_json, write_json, Cue, Job, TranslatedLine};

#[derive(Clone)]
struct Session {
    root: String,
    stamp: String,
    source: crate::draft::Source,
    config: String,
    voice_stamp: String,
    voice_ready: bool,
}
static SESSION: Mutex<Option<Session>> = Mutex::new(None);

fn file_stamp(path: &Path) -> String {
    std::fs::metadata(path)
        .ok()
        .map(|m| {
            format!(
                "{}:{}",
                m.len(),
                m.modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_nanos())
                    .unwrap_or(0)
            )
        })
        .unwrap_or_default()
}

fn source_snapshot(
    job: &Job,
    cancel: &AtomicBool,
) -> Result<(String, crate::draft::Source), String> {
    let stamp = format!(
        "{}:{}:{}",
        job.video.display(),
        file_stamp(&job.video),
        crate::job::source_url(&job.root)
    );
    if let Some(old) = SESSION
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .filter(|s| s.root == job.root.to_string_lossy() && s.stamp == stamp)
    {
        return Ok((stamp, old.source.clone()));
    }
    Ok((stamp, crate::draft::inspect_source(job, cancel)?))
}

fn begin_session(
    job: &Job,
    config: &Options,
    voice: bool,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let (stamp, source) = source_snapshot(job, cancel)?;
    let key = serde_json::to_string(config).map_err(|e| e.to_string())?;
    let voice_stamp = if voice {
        crate::bundled_voice_dir(&config.target)
            .map(|dir| {
                format!(
                    "{}:{}",
                    file_stamp(&dir.join(format!("{}.onnx", config.voice))),
                    file_stamp(&dir.join(format!("{}.onnx.json", config.voice)))
                )
            })
            .unwrap_or_default()
    } else {
        String::new()
    };
    let previous = SESSION.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let same = previous.as_ref().is_some_and(|s| {
        s.root == job.root.to_string_lossy() && s.stamp == stamp && s.config == key
    });
    let ready = same
        && previous
            .as_ref()
            .is_some_and(|s| s.voice_ready && (!voice || s.voice_stamp == voice_stamp));
    if !same || (voice && !ready) || !job.root.join("draft.json").exists() {
        if voice {
            crate::draft::begin(job, source.clone(), config.context(), cancel)?;
        } else {
            crate::draft::begin_text(job, source.clone(), config.context(), cancel)?;
        }
    } else {
        crate::draft::checkpoint(&job.root, Some(crate::draft::Phase::Preparing))?;
    }
    *SESSION.lock().unwrap_or_else(|e| e.into_inner()) = Some(Session {
        root: job.root.to_string_lossy().into_owned(),
        stamp,
        source,
        config: key,
        voice_stamp: if voice {
            voice_stamp
        } else {
            previous
                .as_ref()
                .filter(|_| ready)
                .map(|s| s.voice_stamp.clone())
                .unwrap_or_default()
        },
        voice_ready: voice || ready,
    });
    Ok(())
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Intent {
    pub root: String,
    pub serial: String,
    pub start: f64,
    pub end: f64,
    pub listen: bool,
    pub playing: bool,
    #[serde(default)]
    pub cue: Option<String>,
}
static INTENT: Mutex<Option<Intent>> = Mutex::new(None);
static ACTIVE_REGION: Mutex<Option<(String, String)>> = Mutex::new(None);
struct ActiveRegion;
impl Drop for ActiveRegion {
    fn drop(&mut self) {
        *ACTIVE_REGION.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}
pub(crate) fn active_region(root: &str, serial: &str) -> bool {
    ACTIVE_REGION
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .is_some_and(|(r, s)| r == root && s == serial)
}

pub(crate) fn client_intent(intent: Option<Intent>, owner: &str) -> Result<(), String> {
    if owner.is_empty() || owner.len() > 100 {
        return Err("Mã cửa sổ nháp không hợp lệ.".into());
    }
    if let Some(intent) = intent {
        if !intent.serial.starts_with(&format!("{owner}:")) {
            return Err("Mã vùng không thuộc cửa sổ nháp.".into());
        }
        set_intent(Some(intent))
    } else {
        let mut current = INTENT.lock().unwrap_or_else(|e| e.into_inner());
        if current
            .as_ref()
            .is_some_and(|i| i.serial.starts_with(&format!("{owner}:")))
        {
            *current = None;
        }
        Ok(())
    }
}

pub(crate) fn set_intent(intent: Option<Intent>) -> Result<(), String> {
    if intent.as_ref().is_some_and(|i| {
        i.serial.len() > 120
            || !i.start.is_finite()
            || !i.end.is_finite()
            || i.start < 0.0
            || i.end <= i.start
            || i.end - i.start > 60.001
    }) {
        return Err("Vùng chuẩn bị phải hợp lệ và tối đa 60 giây.".into());
    }
    *INTENT.lock().unwrap_or_else(|e| e.into_inner()) = intent;
    Ok(())
}

fn same_source(job: &Job) -> bool {
    INTENT
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .is_some_and(|i| i.root == job.root.to_string_lossy())
}
fn current(job: &Job, serial: &str) -> bool {
    INTENT
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .is_some_and(|i| i.root == job.root.to_string_lossy() && i.serial == serial && i.playing)
}

pub(crate) fn selected(
    lines: &[TranslatedLine],
    start: f64,
    end: f64,
    context: bool,
) -> Vec<usize> {
    let mut indices: Vec<_> = lines
        .iter()
        .enumerate()
        .filter_map(|(i, l)| (l.start < end && l.end > start).then_some(i))
        .collect();
    if context && !indices.is_empty() {
        let first = indices[0];
        let last = *indices.last().unwrap();
        if first > 0 {
            indices.insert(0, first - 1);
        }
        if last + 1 < lines.len() {
            indices.push(last + 1);
        }
    }
    indices
}

fn align_generation(
    job: &Job,
    mut doc: Editing,
    config: &Options,
    official: bool,
) -> Result<Editing, String> {
    let guard = crate::draft::text_guard(&job.root, false)?.map(|(_, key)| {
        if !official && config.use_terms {
            format!("quick:{key}")
        } else {
            key
        }
    });
    if doc.generation_basis != guard {
        for cue in &mut doc.cues {
            if !cue.authored {
                cue.line.translation = if config.skip_translate {
                    cue.line.source.clone()
                } else {
                    String::new()
                };
            }
        }
        doc.generation_basis = guard;
        doc.save(&job.root)?;
    }
    Ok(doc)
}

fn preview_glossary(
    job: &Job,
    doc: &Editing,
    config: &Options,
    terms_skipped: bool,
) -> Result<(bool, crate::job::Glossary), String> {
    let empty = crate::job::Glossary {
        theme: String::new(),
        terms: vec![],
    };
    if config.skip_translate || !config.use_terms || terms_skipped {
        return Ok((true, empty));
    }
    let heard: Vec<_> = doc
        .cues
        .iter()
        .map(|c| Cue {
            start: c.line.start,
            end: c.line.end,
            text: c.line.source.clone(),
        })
        .collect();
    if let Some(lease) =
        crate::text_cache::lease(job, crate::text_cache::Kind::Terms, &heard, None, false)?
    {
        if let Some(glossary) = crate::text_cache::cached_terms(job, &lease)? {
            return Ok((true, glossary));
        }
    }
    Ok((false, empty))
}

fn translate_indices(
    app: &AppHandle,
    job: &Job,
    config: &Options,
    indices: &[usize],
    glossary: &crate::job::Glossary,
    official: bool,
    serial: Option<&str>,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let doc = align_generation(job, document(job)?, config, official)?;
    let subset: Vec<_> = indices.iter().map(|&i| doc.cues[i].line.clone()).collect();
    let cues: Vec<_> = subset
        .iter()
        .map(|l| Cue {
            start: l.start,
            end: l.end,
            text: l.source.clone(),
        })
        .collect();
    if subset.iter().all(|l| !l.translation.trim().is_empty()) {
        return Ok(());
    }
    if config.skip_translate {
        let mut doc = doc;
        for &i in indices {
            doc.cues[i].line.translation = doc.cues[i].line.source.clone();
        }
        return doc.save(&job.root);
    }
    let choice = choice(app, config)?;
    crate::ollama::require_deepseek_local(&choice, false)?;
    let mut trace = crate::ollama::TranslateTrace::logging();
    let windows = (official && !config.audio_only && !config.text_only)
        .then(|| super::speech_plan::windows(&cues));
    super::translate::translate_with_timing(
        &cues,
        subset,
        glossary,
        &choice,
        clean_target(&config.target).1,
        &config.target,
        cancel,
        None,
        &mut trace,
        &mut |detail, done, total| super::report_progress(app, "translate", detail, done, total),
        &mut |done| {
            stopped(cancel)?;
            if serial.is_some() && !same_source(job) {
                return Err("Đã dừng.".into());
            }
            let mut now = document(job)?;
            if now.generation_basis != doc.generation_basis {
                return Err("Dữ liệu dịch đã đổi; bỏ phản hồi cũ.".into());
            }
            for (&i, line) in indices.iter().zip(done) {
                if now.cues[i].id != doc.cues[i].id || now.cues[i].line.source != line.source {
                    return Err("ID câu đã đổi; bỏ phản hồi cũ.".into());
                }
                if !now.cues[i].authored || now.cues[i].line.translation.trim().is_empty() {
                    now.cues[i].line.translation = line.translation.clone();
                }
            }
            now.save(&job.root)
        },
        windows.as_deref(),
    )?;
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Clip {
    pub id: String,
    pub path: String,
    pub version: String,
    pub start: f64,
    pub speed: f64,
    pub played: f64,
    pub tentative: bool,
}

// Preview and trial exports must ignore the same stale/missing raw audio.
// Legacy trial exports keep their existing file-based recovery behavior.
pub(super) fn speech_slots(
    job: &Job,
    lines: &[TranslatedLine],
) -> Result<Vec<crate::timeline::Slot>, String> {
    let status = crate::draft::status(&job.root)?;
    speech_slots_verified(job, lines, status.as_ref())
}

fn speech_slots_verified(
    job: &Job,
    lines: &[TranslatedLine],
    status: Option<&crate::draft::Status>,
) -> Result<Vec<crate::timeline::Slot>, String> {
    lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            let seconds = if let Some(state) = status {
                state.manifest.cues.get(i)
                    .and_then(|cue| state.speech_seconds.get(&cue.id))
                    .copied().unwrap_or(0.0)
            } else {
                crate::wav::read_wav(&job.line_wav(i + 1))
                    .ok()
                    .map(|pcm| pcm.samples.len() as f64 / f64::from(pcm.rate))
                    .unwrap_or(0.0)
            };
            Ok(crate::timeline::Slot {
                start: line.start,
                end: line.end,
                wav_secs: seconds,
            })
        })
        .collect()
}

pub(crate) fn clips(job: &Job, duration: f64) -> Result<Vec<Clip>, String> {
    let state = crate::draft::status(&job.root)?;
    clips_verified(job, duration, state.as_ref())
}

fn clips_verified(
    job: &Job,
    duration: f64,
    state: Option<&crate::draft::Status>,
) -> Result<Vec<Clip>, String> {
    let Some(state) = state else {
        return Ok(Vec::new());
    };
    let words = crate::timing::read(&job.root)?;
    let doc = document(job)?;
    let mut unknown = false;
    let mut tentative = Vec::new();
    for i in 0..words.len() {
        let known = state.speech_ready.contains(&doc.cues[i].id);
        if !known {
            unknown = true;
        }
        tentative.push(unknown);
    }
    let slots = speech_slots_verified(job, &words, Some(state))?;
    let placed = crate::timeline::place_all(&slots, duration);
    Ok(placed
        .into_iter()
        .enumerate()
        .filter_map(|(i, p)| {
            state.speech_ready.contains(&doc.cues[i].id).then(|| Clip {
                id: doc.cues[i].id.clone(),
                path: job.line_wav(i + 1).to_string_lossy().into_owned(),
                version: state
                    .manifest
                    .speech_version(&doc.cues[i].id)
                    .unwrap_or_default()
                    .into(),
                start: p.start,
                speed: p.speed,
                played: p.played,
                tentative: tentative[i],
            })
        })
        .collect())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Region {
    pub view: View,
    pub clips: Vec<Clip>,
}

fn publish_region(
    app: &AppHandle,
    job: &Job,
    serial: &str,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let state = crate::draft::status(&job.root)?;
    let view = view_verified(job, cancel, state.as_ref())?;
    let clips = clips_verified(job, view.duration, state.as_ref())?;
    for clip in &clips {
        app.asset_protocol_scope()
            .allow_file(&clip.path)
            .map_err(|e| e.to_string())?;
    }
    app.emit(
        "draft-region-progress",
        serde_json::json!({"serial":serial,"view":view,"clips":clips}),
    )
    .map_err(|e| e.to_string())
}

pub(crate) fn region(
    app: &AppHandle,
    root: &Path,
    serial: &str,
    cancel: &AtomicBool,
) -> Result<Region, String> {
    let job = reopen(root)?;
    let config = options(&job)?;
    let intent = INTENT
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .filter(|i| i.root == job.root.to_string_lossy() && i.serial == serial)
        .ok_or("Vùng yêu cầu đã đổi.")?;
    *ACTIVE_REGION.lock().unwrap_or_else(|e| e.into_inner()) =
        Some((root.to_string_lossy().into_owned(), serial.into()));
    let _active = ActiveRegion;
    if intent.listen && config.text_only {
        return Err("Video phụ đề không tạo giọng đọc.".into());
    }
    if !config.skip_translate {
        crate::ollama::require_deepseek_local(&choice(app, &config)?, false)?;
    }
    begin_session(&job, &config, intent.listen, cancel)?;
    if !job.translation_path().exists() {
        prepare(app, root, cancel)?;
    }
    let result = (|| {
        let doc = document(&job)?;
        let terms_skipped =
            !config.skip_translate && super::translate::skips_terms(&choice(app, &config)?);
        let (official, glossary) = preview_glossary(&job, &doc, &config, terms_skipped)?;
        let doc = align_generation(&job, doc, &config, official)?;
        let indexes = if let Some(id) = &intent.cue {
            vec![doc
                .cues
                .iter()
                .position(|c| &c.id == id)
                .ok_or("Câu cần nghe đã đổi.")?]
        } else {
            selected(&doc.lines(), intent.start, intent.end, intent.listen)
        };
        // Bounded groups allow a seek/pause to take priority before the next
        // untranslated group or spoken sentence has started.
        for chunk in indexes.chunks(6) {
            if !current(&job, serial) {
                break;
            }
            translate_indices(
                app,
                &job,
                &config,
                chunk,
                &glossary,
                official,
                Some(serial),
                cancel,
            )?;
            publish_region(app, &job, serial, cancel)?;
            if intent.listen {
                for &index in chunk {
                    if !current(&job, serial) {
                        break;
                    }
                    stopped(cancel)?;
                    let doc = document(&job)?;
                    let line = &doc.cues[index].line;
                    if line.translation.trim().is_empty() {
                        continue;
                    }
                    let state = crate::draft::status(&job.root)?.ok_or("Chưa có nháp.")?;
                    if state.speech_ready.contains(&doc.cues[index].id) {
                        continue;
                    }
                    let text = crate::speech::clean_for_speech(&line.translation);
                    if crate::speech::is_speakable(&text) {
                        super::speak::speak_ready(
                            app,
                            &job,
                            doc.cues.len(),
                            vec![(index, text, job.line_wav(index + 1))],
                            cancel,
                        )?;
                    } else {
                        super::write_silence(&job.line_wav(index + 1), cancel)?;
                        let spoken =
                            crate::render::spoken_text(&config.render, &config.target, &text);
                        if let Some(ticket) = crate::draft::speech_ticket(
                            &job.line_wav(index + 1),
                            &spoken,
                            &config.target,
                            &config.voice,
                        )? {
                            crate::draft::record_existing(&ticket, &job.line_wav(index + 1))?;
                        }
                    }
                    publish_region(app, &job, serial, cancel)?;
                }
            }
        }
        let view = view(&job, cancel)?;
        let ready = clips(&job, view.duration)?;
        for clip in &ready {
            app.asset_protocol_scope()
                .allow_file(&clip.path)
                .map_err(|e| e.to_string())?;
        }
        Ok(Region { view, clips: ready })
    })();
    let settled = crate::draft::checkpoint(&job.root, Some(crate::draft::Phase::Idle));
    match result {
        Ok(view) => {
            settled?;
            Ok(view)
        }
        Err(e) => Err(e),
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Options {
    pub target: String,
    pub voice: String,
    pub provider: String,
    pub model: String,
    pub audio_only: bool,
    pub text_only: bool,
    pub burn: bool,
    pub skip_translate: bool,
    pub use_terms: bool,
    pub render: crate::render::RenderSettings,
}

impl Options {
    pub(crate) fn settings(&self) -> RunSettings {
        RunSettings {
            target: self.target.clone(),
            voice: self.voice.clone(),
            use_terms: self.use_terms,
            skip_translate: self.skip_translate,
            text_only: self.text_only,
        }
    }
    pub(crate) fn context(&self) -> crate::draft::RunContext {
        crate::draft::RunContext {
            provider: self.provider.clone(),
            model: crate::ollama::model_identity(&self.provider, &self.model),
            audio_only: self.audio_only,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct View {
    pub root: String,
    pub source: String,
    pub revision: String,
    pub duration: f64,
    pub options: Options,
    pub cues: Vec<EditedCue>,
    pub can_undo: bool,
    pub names_reviewed: bool,
    pub ready: Vec<String>,
    pub last_export: Option<String>,
    pub warning: Option<String>,
}

pub(crate) fn reopen(root: &Path) -> Result<Job, String> {
    let mut job = Job::reopen(root)?;
    recover_config(&job)?;
    let settings: RunSettings = read_json(&job.settings_path())?;
    job.target = settings.target;
    Ok(job)
}

#[derive(Serialize, Deserialize)]
struct ConfigCommit {
    version: u32,
    options: Options,
    document: Option<Editing>,
}

fn recover_config(job: &Job) -> Result<(), String> {
    let pending = job.root.join("draft-config-pending.json");
    if !pending.exists() {
        return Ok(());
    }
    let commit: ConfigCommit = read_json(&pending)?;
    if commit.version != 1 {
        return Err("Phiên bản giao dịch cấu hình chưa hỗ trợ; giữ dữ liệu để phục hồi.".into());
    }
    commit.options.render.validate()?;
    let source = crate::draft::inspect_source(job, &AtomicBool::new(false))?;
    write_json(&job.settings_path(), &commit.options.settings())?;
    write_json(&job.root.join("render.json"), &commit.options.render)?;
    write_json(&job.root.join("draft-options.json"), &commit.options)?;
    if let Some(doc) = commit.document {
        doc.save(&job.root)?;
    }
    invalidate_mix(job)?;
    crate::draft::begin_text(
        job,
        source,
        commit.options.context(),
        &AtomicBool::new(false),
    )?;
    crate::draft::checkpoint(&job.root, Some(crate::draft::Phase::Idle))?;
    std::fs::remove_file(pending).map_err(|e| e.to_string())
}

pub(crate) fn configure(job: &Job, revision: &str, mut next: Options) -> Result<(), String> {
    if crate::timing::revision(&job.root)? != revision {
        return Err("Cấu hình nháp đã đổi. Nạp lại trước khi lưu.".into());
    }
    if clean_target(&next.target).0 != next.target {
        return Err("Ngôn ngữ đích không hợp lệ.".into());
    }
    super::reject_both_modes(next.audio_only, next.text_only)?;
    next.model = crate::clean_model(&next.provider, &next.model)?;
    next.render.validate()?;
    let before = options(job)?;
    let language = before.target != next.target || before.skip_translate != next.skip_translate;
    let text = language
        || before.provider != next.provider
        || before.model != next.model
        || before.use_terms != next.use_terms;
    let mut doc = if text && job.translation_path().exists() {
        Some(document(job)?)
    } else {
        Editing::load(&job.root)?
    };
    if let Some(doc) = &mut doc {
        if text {
            for cue in &mut doc.cues {
                if language || !cue.authored {
                    cue.line.translation = if next.skip_translate {
                        cue.line.source.clone()
                    } else {
                        String::new()
                    };
                    cue.authored = false;
                }
            }
            doc.generation_basis = None;
        }
        if language {
            doc.clear_draft_history();
        }
    }
    crate::history::ensure_previous(&job.root)?;
    let folder = job.root.join("cache-stale").join(crate::history::new_id());
    for name in [
        "settings.json",
        "render.json",
        "draft-options.json",
        "editing.json",
        "draft.json",
    ] {
        match std::fs::read(job.root.join(name)) {
            Ok(bytes) => crate::atomic_file::write(&folder.join(name), &bytes)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    crate::media_assets::stage(job, &mut next.render)?;
    write_json(
        &job.root.join("draft-config-pending.json"),
        &ConfigCommit {
            version: 1,
            options: next,
            document: doc,
        },
    )?;
    recover_config(job)
}

pub(crate) fn options(job: &Job) -> Result<Options, String> {
    if job.root.join("draft-options.json").exists() {
        return read_json(&job.root.join("draft-options.json"));
    }
    let settings: RunSettings = read_json(&job.settings_path())?;
    let context = crate::draft::context(&job.root)?;
    Ok(Options {
        target: settings.target,
        voice: settings.voice,
        use_terms: settings.use_terms,
        skip_translate: settings.skip_translate,
        text_only: settings.text_only,
        provider: context
            .as_ref()
            .map(|c| c.provider.clone())
            .unwrap_or_else(|| "local".into()),
        model: context
            .as_ref()
            .map(|c| c.model.clone())
            .unwrap_or_default(),
        audio_only: context.is_some_and(|c| c.audio_only),
        burn: job.captioned().is_file(),
        render: crate::render::RenderSettings::read(&job.root)?,
    })
}

pub(crate) fn initialize_legacy_options(app: &AppHandle, job: &Job) -> Result<(), String> {
    if job.root.join("draft-options.json").exists() {
        return Ok(());
    }
    let mut config = options(job)?;
    if crate::draft::context(&job.root)?.is_none() {
        let saved = crate::read_choice(&crate::choice_path(app)?);
        config.provider = saved.provider;
        config.model = saved.model;
    }
    write_json(&job.root.join("draft-options.json"), &config)
}

pub(crate) fn choice(
    app: &AppHandle,
    options: &Options,
) -> Result<crate::TranslationChoice, String> {
    let current = crate::read_choice(&crate::choice_path(app)?);
    crate::TranslationRun {
        provider: options.provider.clone(),
        model: options.model.clone(),
        use_terms: options.use_terms,
    }
    .resolve(current)
}

pub(crate) fn view(job: &Job, cancel: &AtomicBool) -> Result<View, String> {
    let state = crate::draft::status(&job.root)?;
    view_verified(job, cancel, state.as_ref())
}

fn view_verified(
    job: &Job,
    cancel: &AtomicBool,
    state: Option<&crate::draft::Status>,
) -> Result<View, String> {
    let options = options(job)?;
    let warning = if !options.skip_translate
        && job.translation_path().exists()
        && read_json::<Vec<TranslatedLine>>(&job.translation_path())?
            .iter()
            .any(|line| !line.translation.trim().is_empty())
        && Editing::load(&job.root)?.is_none()
        && crate::text_cache::generation_basis(&job.root)?.is_none()
    {
        Some("Lời dịch cũ chưa có xác nhận nguồn/cấu hình. File cũ vẫn giữ; vùng yêu cầu sẽ được dịch lại trước khi dùng.".into())
    } else {
        None
    };
    let cues = if job.translation_path().exists() {
        if let Some(doc) = Editing::load(&job.root)? {
            doc.cues
        } else {
            crate::timing::read(&job.root)?
                .into_iter()
                .enumerate()
                .map(|(i, line)| EditedCue {
                    id: crate::editing::legacy_id(i),
                    sources: vec![i],
                    slot: i + 1,
                    line,
                    authored: false,
                    keep_speech: false,
                })
                .collect()
        }
    } else {
        Vec::new()
    };
    let source = if job.video.is_file() {
        &job.video
    } else {
        &job.picture
    };
    let duration = if source.is_file() {
        media_duration(source, cancel)?
    } else {
        super::read_cues(&job.transcript())?
            .iter()
            .map(|c| c.end)
            .fold(0.0, f64::max)
    };
    let last_export = [
        if options.audio_only {
            job.spoken()
        } else if options.burn || options.text_only {
            job.captioned()
        } else {
            job.result()
        },
        job.result(),
        job.captioned(),
        job.spoken(),
    ]
    .into_iter()
    .find(|path| path.is_file())
    .map(|p| p.to_string_lossy().into_owned())
    .or_else(|| {
        crate::job::list_kept(&job.root)
            .ok()?
            .first()
            .map(|v| v.path.clone())
    });
    Ok(View {
        root: job.root.to_string_lossy().into_owned(),
        source: if source.is_file() {
            source.to_string_lossy().into_owned()
        } else {
            String::new()
        },
        revision: crate::timing::revision(&job.root)?,
        duration,
        options,
        cues,
        can_undo: Editing::load(&job.root)?.is_some_and(|doc| doc.can_undo_draft()),
        names_reviewed: read_json::<crate::names::NameList>(&job.names_path())
            .ok()
            .is_some_and(|n| n.confirmed),
        ready: state.map(|s| s.speech_ready.clone()).unwrap_or_default(),
        last_export,
        warning,
    })
}

pub(crate) fn open(
    app: &AppHandle,
    video: &Path,
    output: &Path,
    mut config: Options,
    cancel: &AtomicBool,
) -> Result<View, String> {
    let mut job = Job::open(video, output)?;
    crate::draft::inspect_source(&job, cancel)?;
    if job.settings_path().exists() {
        let saved: RunSettings = read_json(&job.settings_path())?;
        job.target = saved.target;
        if !job.root.join("draft-options.json").exists() {
            config.target = job.target.clone();
            config.voice = saved.voice;
            config.skip_translate = saved.skip_translate;
            config.use_terms = saved.use_terms;
            config.text_only = saved.text_only;
            config.render = crate::render::RenderSettings::read(&job.root)?;
            write_json(&job.root.join("draft-options.json"), &config)?;
        }
    } else {
        config.target = clean_target(&config.target).0.into();
        job.target = config.target.clone();
        if config.voice.is_empty() {
            config.voice = crate::selected_voice(app, &job.target).unwrap_or_default();
        }
        config.render.validate()?;
        write_json(&job.settings_path(), &config.settings())?;
        write_json(&job.root.join("render.json"), &config.render)?;
        write_json(&job.root.join("draft-options.json"), &config)?;
    }
    let source = if job.video.is_file() {
        &job.video
    } else {
        &job.picture
    };
    if source.is_file() {
        app.asset_protocol_scope()
            .allow_file(source)
            .map_err(|e| e.to_string())?;
    }
    view(&job, cancel)
}

pub(crate) fn prepare(app: &AppHandle, root: &Path, cancel: &AtomicBool) -> Result<View, String> {
    let job = reopen(root)?;
    let config = options(&job)?;
    let source = crate::draft::inspect_source(&job, cancel)?;
    crate::draft::begin_text(&job, source, config.context(), cancel)?;
    let result = (|| {
        super::extract(app, &job, cancel)?;
        let heard = super::media::transcribe(app, &job, "auto", cancel)?;
        let lines = super::media::split(app, &job, &heard, cancel)?;
        if !job.translation_path().exists() {
            let words: Vec<_> = lines
                .iter()
                .map(|line| TranslatedLine {
                    start: line.start,
                    end: line.end,
                    source: line.text.clone(),
                    translation: if config.skip_translate {
                        line.text.clone()
                    } else {
                        String::new()
                    },
                })
                .collect();
            write_json(&job.translation_path(), &words)?;
        }
        super::settle_transcript(&job);
        view(&job, cancel)
    })();
    let settled = crate::draft::checkpoint(&job.root, Some(crate::draft::Phase::Idle));
    match result {
        Ok(view) => {
            settled?;
            Ok(view)
        }
        Err(e) => Err(e),
    }
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Patch {
    pub id: String,
    pub source: Option<String>,
    pub text: Option<String>,
    pub start: Option<f64>,
    pub end: Option<f64>,
}

fn document(job: &Job) -> Result<Editing, String> {
    if let Some(doc) = Editing::load(&job.root)? {
        return Ok(doc);
    }
    let base: Vec<TranslatedLine> = read_json(&job.translation_path())?;
    let effective = crate::timing::read(&job.root)?;
    Editing::initial(&job.root, &base, &effective)
}

fn invalidate_mix(job: &Job) -> Result<(), String> {
    for name in [
        "dub.wav",
        "fit-report.json",
        "frame-cache.json",
        "voice-spans.json",
    ] {
        match std::fs::remove_file(job.root.join(name)) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(())
}

pub(crate) fn save_patches(
    job: &Job,
    revision: &str,
    patches: &[Patch],
    duration: f64,
) -> Result<(), String> {
    if crate::timing::revision(&job.root)? != revision {
        return Err("Nháp đã đổi. Nạp lại trước khi lưu.".into());
    }
    let mut doc = document(job)?;
    let old = doc.lines();
    let mut next = old.clone();
    let mut changes = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for patch in patches {
        if !seen.insert(&patch.id) {
            return Err("Một câu được gửi sửa hai lần.".into());
        }
        let i = doc
            .cues
            .iter()
            .position(|c| c.id == patch.id)
            .ok_or("ID câu không còn tồn tại.")?;
        for text in [&patch.source, &patch.text].into_iter().flatten() {
            if text.chars().count() > 4000 {
                return Err("Lời mỗi câu tối đa 4.000 ký tự.".into());
            }
        }
        if let Some(source) = &patch.source {
            if source.trim().is_empty() {
                return Err("Lời gốc không được để trống.".into());
            }
            if next[i].source != source.trim() && patch.text.is_none() {
                next[i].translation.clear();
            }
            next[i].source = source.trim().into();
        }
        if let Some(text) = &patch.text {
            next[i].translation = text.trim().into();
        }
        match (patch.start, patch.end) {
            (Some(start), Some(end)) => {
                next[i].start = start;
                next[i].end = end;
                changes.push(crate::timing::TimeChange {
                    index: i,
                    start,
                    end,
                });
            }
            (None, None) => {}
            _ => return Err("Gửi đủ mốc bắt đầu và kết thúc.".into()),
        }
    }
    crate::timing::validate_changes(&old, &next, &changes, duration)?;
    if next == old {
        return Ok(());
    }
    crate::history::ensure_previous(&job.root)?;
    doc.remember_draft();
    for (cue, line) in doc.cues.iter_mut().zip(next) {
        if cue.line.source != line.source || cue.line.translation != line.translation {
            cue.authored = true;
        }
        if cue.line.start != line.start || cue.line.end != line.end {
            cue.keep_speech = true;
        }
        cue.line = line;
    }
    doc.save(&job.root)?;
    invalidate_mix(job)
}

pub(crate) fn restructure(
    job: &Job,
    revision: &str,
    action: Option<&Structure>,
    duration: f64,
) -> Result<(), String> {
    if crate::timing::revision(&job.root)? != revision {
        return Err("Nháp đã đổi. Nạp lại trước khi sửa cấu trúc.".into());
    }
    let mut doc = document(job)?;
    crate::history::ensure_previous(&job.root)?;
    if let Some(action) = action {
        if action.revision() != revision {
            return Err("Phiên bản tách/gộp không khớp nháp.".into());
        }
        doc.remember_draft();
        doc.restructure(&job.root, action, duration)?;
    } else {
        doc.undo_draft()?;
    }
    doc.save(&job.root)?;
    invalidate_mix(job)
}

pub(super) fn finish_edited(
    app: &AppHandle,
    job: &Job,
    choice: &crate::TranslationChoice,
    audio_only: bool,
    cancel: &AtomicBool,
) -> Result<Vec<TranslatedLine>, String> {
    let settings: RunSettings = read_json(&job.settings_path())?;
    let mut config = options(job)?;
    config.target = settings.target;
    config.voice = settings.voice;
    config.skip_translate = settings.skip_translate;
    config.use_terms = settings.use_terms;
    config.text_only = settings.text_only;
    config.audio_only = audio_only;
    config.provider = choice.provider.clone();
    config.model = choice.model.clone();
    if job.root.join("draft-options.json").exists() {
        write_json(&job.root.join("draft-options.json"), &config)?;
    }
    let mut doc = document(job)?;
    let heard: Vec<_> = doc
        .cues
        .iter()
        .map(|c| Cue {
            start: c.line.start,
            end: c.line.end,
            text: c.line.source.clone(),
        })
        .collect();
    let names = if config.skip_translate {
        super::report(
            app,
            "names",
            "skipped",
            "Không dịch, giữ tên đã xác nhận; không gọi AI.",
        );
        read_json::<crate::names::NameList>(&job.names_path())
            .ok()
            .filter(|n| n.confirmed)
            .map(|n| n.names)
            .unwrap_or_default()
    } else {
        super::translate::review_names(app, job, &heard, choice, cancel)?
    };
    for cue in &mut doc.cues {
        cue.line.source = crate::names::apply_names(&cue.line.source, &names);
        if cue.authored {
            cue.line.translation = crate::names::apply_names(&cue.line.translation, &names);
        }
    }
    doc.save(&job.root)?;
    let heard: Vec<_> = doc
        .cues
        .iter()
        .map(|c| Cue {
            start: c.line.start,
            end: c.line.end,
            text: c.line.source.clone(),
        })
        .collect();
    let glossary = if config.skip_translate {
        crate::job::Glossary {
            theme: String::new(),
            terms: vec![],
        }
    } else {
        super::translate::terms(
            app,
            job,
            &heard,
            choice,
            clean_target(&config.target).1,
            config.use_terms,
            cancel,
        )?
    };
    let doc = align_generation(job, doc, &config, true)?;
    translate_indices(
        app,
        job,
        &config,
        &(0..doc.cues.len()).collect::<Vec<_>>(),
        &glossary,
        true,
        None,
        cancel,
    )?;
    let words = document(job)?.lines();
    let missing = words
        .iter()
        .filter(|l| !l.source.trim().is_empty() && l.translation.trim().is_empty())
        .count();
    if missing > 0 {
        return Err(format!(
            "Còn {missing} câu chưa dịch được. Đã giữ nháp; thử lại phần thiếu trước khi xuất."
        ));
    }
    Ok(words)
}

#[derive(Serialize, Deserialize)]
struct ExportMark {
    version: u32,
    revision: String,
    result: String,
}

pub(crate) fn mark_exported(job: &Job, path: &Path) -> Result<(), String> {
    if job.root.join("editing.json").exists() {
        write_json(
            &job.root.join("draft-export.json"),
            &ExportMark {
                version: 1,
                revision: crate::timing::revision(&job.root)?,
                result: path.to_string_lossy().into_owned(),
            },
        )?;
    }
    Ok(())
}

pub(crate) fn export_matches(root: &Path) -> Result<bool, String> {
    let marker = match std::fs::read(root.join("draft-export.json")) {
        Ok(bytes) => serde_json::from_slice::<ExportMark>(&bytes)
            .map_err(|_| "Không đọc được dấu bản xuất của nháp.")?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e.to_string()),
    };
    Ok(marker.version == 1
        && marker.revision == crate::timing::revision(root)?
        && Path::new(&marker.result).is_file())
}

pub(crate) fn export(app: &AppHandle, root: &Path, cancel: &AtomicBool) -> Result<String, String> {
    let job = reopen(root)?;
    let config = options(&job)?;
    let choice = if config.skip_translate {
        let mut current = crate::read_choice(&crate::choice_path(app)?);
        current.provider = config.provider.clone();
        current.model = config.model.clone();
        current
    } else {
        choice(app, &config)?
    };
    crate::ollama::require_deepseek_local(&choice, config.skip_translate)?;
    if !config.text_only {
        crate::save_voice(app.clone(), config.target.clone(), config.voice.clone())?;
    }
    if !job.translation_path().exists() {
        prepare(app, root, cancel)?;
    }
    crate::draft::inspect_source(&job, cancel)?;
    let doc = document(&job)?;
    doc.save(root)?;
    invalidate_mix(&job)?;
    super::run(
        app,
        root,
        root.parent().ok_or("Không thấy folder kết quả.")?,
        &config.target,
        false,
        config.use_terms,
        config.burn,
        config.audio_only,
        config.text_only,
        config.skip_translate,
        Some(choice),
        config.render,
        cancel,
    )
}

pub(crate) fn sample(
    app: &AppHandle,
    root: &Path,
    start: f64,
    seconds: f64,
    cancel: &AtomicBool,
) -> Result<String, String> {
    if !start.is_finite() || start < 0.0 || !seconds.is_finite() || !(1.0..=10.0).contains(&seconds)
    {
        return Err("Đoạn thử dài từ 1 đến 10 giây, mốc không âm.".into());
    }
    let job = reopen(root)?;
    let config = options(&job)?;
    if config.audio_only {
        return Err("Audio dùng Nghe câu để kiểm tra; không dựng hình.".into());
    }
    let serial = crate::history::new_id();
    set_intent(Some(Intent {
        root: root.to_string_lossy().into_owned(),
        serial: serial.clone(),
        start,
        end: start + seconds,
        listen: !config.text_only,
        playing: true,
        cue: None,
    }))?;
    region(app, root, &serial, cancel)?;
    let job = reopen(root)?;
    let out = super::sample::export(
        &job,
        start,
        seconds,
        config.text_only,
        config.burn,
        config.render,
        cancel,
    )?;
    app.asset_protocol_scope()
        .allow_file(&out)
        .map_err(|e| e.to_string())?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wav::Pcm;
    struct Fixture {
        root: std::path::PathBuf,
        job: Job,
        config: Options,
        words: Vec<TranslatedLine>,
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
    fn fixture() -> Fixture {
        fixture_with_count(3)
    }
    fn fixture_with_count(count: usize) -> Fixture {
        let root = std::env::temp_dir().join(format!("demand-draft-{}", crate::history::new_id()));
        std::fs::create_dir_all(root.join("output")).unwrap();
        std::fs::write(root.join("clip.mp4"), b"source").unwrap();
        let job = Job::open(&root.join("clip.mp4"), &root.join("output")).unwrap();
        let config = Options {
            target: "vi".into(),
            voice: "fixture".into(),
            provider: "local".into(),
            model: String::new(),
            audio_only: false,
            text_only: false,
            burn: false,
            skip_translate: false,
            use_terms: false,
            render: Default::default(),
        };
        write_json(&job.settings_path(), &config.settings()).unwrap();
        write_json(&job.root.join("draft-options.json"), &config).unwrap();
        let words: Vec<_> = (0..count)
            .map(|i| TranslatedLine {
                start: i as f64 * 3.0,
                end: i as f64 * 3.0 + 2.0,
                source: format!("source {i}"),
                translation: format!("lời {i}"),
            })
            .collect();
        let lines: Vec<_> = words
            .iter()
            .map(|l| Cue {
                start: l.start,
                end: l.end,
                text: l.source.clone(),
            })
            .collect();
        write_json(&job.lines_path(), &lines).unwrap();
        let source = crate::draft::inspect_source(&job, &AtomicBool::new(false)).unwrap();
        crate::draft::begin_text(&job, source, config.context(), &AtomicBool::new(false)).unwrap();
        let lease = crate::text_cache::lease(
            &job,
            crate::text_cache::Kind::Translation,
            &lines,
            Some(&crate::job::Glossary {
                theme: String::new(),
                terms: vec![],
            }),
            false,
        )
        .unwrap()
        .unwrap();
        lease.commit_translation(&words).unwrap();
        if count > 3 {
            crate::draft::benchmark_speech(&job, &words);
        }
        for i in 0..if count > 3 { 0 } else { count } {
            let path = job.line_wav(i + 1);
            let ticket = crate::draft::speech_ticket(&path, &words[i].translation, "vi", "fixture")
                .unwrap()
                .unwrap();
            crate::draft::commit_speech(
                &ticket,
                &path,
                &Pcm {
                    rate: 22050,
                    samples: vec![i as i16 + 1; 2205],
                },
            )
            .unwrap();
        }
        Fixture {
            root,
            job,
            config,
            words,
        }
    }
    fn text(id: &str, value: &str) -> Patch {
        Patch {
            id: id.into(),
            source: None,
            text: Some(value.into()),
            start: None,
            end: None,
        }
    }
    #[test]
    fn saving_words_is_only_a_draft_commit_and_undo_survives_reopen() {
        let f = fixture();
        std::fs::write(f.job.result(), b"old export").unwrap();
        let base = std::fs::read(f.job.translation_path()).unwrap();
        let wav = std::fs::read(f.job.line_wav(1)).unwrap();
        save_patches(
            &f.job,
            &crate::timing::revision(&f.job.root).unwrap(),
            &[text("cue-000001", "lời đã sửa")],
            10.0,
        )
        .unwrap();
        assert_eq!(std::fs::read(f.job.result()).unwrap(), b"old export");
        assert_eq!(std::fs::read(f.job.translation_path()).unwrap(), base);
        assert_eq!(std::fs::read(f.job.line_wav(1)).unwrap(), wav);
        let doc = document(&f.job).unwrap();
        assert!(doc.cues[0].authored);
        assert!(!doc.cues[1].authored);
        assert!(doc.can_undo_draft());
        let ready = crate::draft::status(&f.job.root)
            .unwrap()
            .unwrap()
            .speech_ready;
        assert_eq!(ready, vec!["cue-000002", "cue-000003"]);
        restructure(
            &f.job,
            &crate::timing::revision(&f.job.root).unwrap(),
            None,
            10.0,
        )
        .unwrap();
        assert_eq!(document(&f.job).unwrap().lines(), f.words);
        assert_eq!(
            crate::draft::status(&f.job.root)
                .unwrap()
                .unwrap()
                .speech_ready
                .len(),
            3
        );
    }
    #[test]
    fn changing_only_time_keeps_wav_and_protects_it_from_rewriting() {
        let f = fixture();
        document(&f.job).unwrap().save(&f.job.root).unwrap();
        std::fs::write(f.job.result(), b"old export").unwrap();
        mark_exported(&f.job, &f.job.result()).unwrap();
        assert!(export_matches(&f.job.root).unwrap());
        let bytes = std::fs::read(f.job.line_wav(1)).unwrap();
        save_patches(
            &f.job,
            &crate::timing::revision(&f.job.root).unwrap(),
            &[Patch {
                id: "cue-000001".into(),
                source: None,
                text: None,
                start: Some(0.1),
                end: Some(2.1),
            }],
            10.0,
        )
        .unwrap();
        let doc = document(&f.job).unwrap();
        assert!(doc.cues[0].keep_speech);
        assert!(!doc.cues[0].authored);
        assert_eq!(std::fs::read(f.job.line_wav(1)).unwrap(), bytes);
        assert_eq!(
            crate::draft::status(&f.job.root)
                .unwrap()
                .unwrap()
                .speech_ready
                .len(),
            3
        );
        assert!(!export_matches(&f.job.root).unwrap());
        assert_eq!(std::fs::read(f.job.result()).unwrap(), b"old export");
    }
    #[test]
    fn stale_revision_bad_id_and_overlapping_times_never_write_an_edit() {
        let f = fixture();
        let revision = crate::timing::revision(&f.job.root).unwrap();
        assert!(save_patches(&f.job, &revision, &[text("missing", "wrong")], 10.0).is_err());
        assert!(save_patches(
            &f.job,
            &revision,
            &[Patch {
                id: "cue-000001".into(),
                source: None,
                text: None,
                start: Some(0.0),
                end: Some(5.0)
            }],
            10.0
        )
        .is_err());
        assert!(!f.job.root.join("editing.json").exists());
        save_patches(&f.job, &revision, &[text("cue-000002", "saved")], 10.0).unwrap();
        let bytes = std::fs::read(f.job.root.join("editing.json")).unwrap();
        assert!(save_patches(
            &f.job,
            &revision,
            &[text("cue-000001", "late response")],
            10.0
        )
        .is_err());
        assert_eq!(
            std::fs::read(f.job.root.join("editing.json")).unwrap(),
            bytes
        );
    }
    #[test]
    fn config_changes_never_speak_or_render_and_language_reset_keeps_ids() {
        let f = fixture();
        std::fs::write(f.job.result(), b"old export").unwrap();
        let bytes = std::fs::read(f.job.line_wav(1)).unwrap();
        let mut config = f.config.clone();
        config.voice = "new voice".into();
        config.render.size = 5.0;
        configure(
            &f.job,
            &crate::timing::revision(&f.job.root).unwrap(),
            config.clone(),
        )
        .unwrap();
        assert_eq!(std::fs::read(f.job.line_wav(1)).unwrap(), bytes);
        assert_eq!(std::fs::read(f.job.result()).unwrap(), b"old export");
        assert_eq!(crate::timing::read(&f.job.root).unwrap(), f.words);
        config.target = "en".into();
        configure(
            &f.job,
            &crate::timing::revision(&f.job.root).unwrap(),
            config,
        )
        .unwrap();
        let doc = document(&f.job).unwrap();
        assert_eq!(doc.cues[0].id, "cue-000001");
        assert_eq!(doc.cues[0].slot, 1);
        assert!(doc.cues.iter().all(|c| c.line.translation.is_empty()));
        assert_eq!(std::fs::read(f.job.result()).unwrap(), b"old export");
        assert!(!f.job.root.join("draft-config-pending.json").exists());
    }
    #[test]
    fn interrupted_configuration_commit_is_completed_from_its_journal() {
        let f = fixture();
        let mut config = f.config.clone();
        config.render.size = 5.5;
        config.voice = "next".into();
        write_json(
            &f.job.root.join("draft-config-pending.json"),
            &ConfigCommit {
                version: 1,
                options: config,
                document: None,
            },
        )
        .unwrap();
        let job = reopen(&f.job.root).unwrap();
        assert_eq!(options(&job).unwrap().voice, "next");
        assert_eq!(
            crate::render::RenderSettings::read(&job.root).unwrap().size,
            5.5
        );
        assert!(!job.root.join("draft-config-pending.json").exists());
    }
    #[test]
    fn splitting_untranslated_source_keeps_other_ids_and_can_undo() {
        let f = fixture();
        let revision = crate::timing::revision(&f.job.root).unwrap();
        restructure(
            &f.job,
            &revision,
            Some(&Structure::Split {
                revision: revision.clone(),
                id: "cue-000001".into(),
                at: 1.0,
                left_source: "left".into(),
                right_source: "right".into(),
                left_text: String::new(),
                right_text: String::new(),
            }),
            10.0,
        )
        .unwrap();
        let doc = document(&f.job).unwrap();
        assert_eq!(doc.cues.len(), 4);
        assert!(!doc.cues[0].authored);
        assert!(doc.cues[0].line.translation.is_empty());
        assert_eq!(doc.cues[3].id, "cue-000003");
        assert_eq!(doc.cues[3].slot, 3);
        restructure(
            &f.job,
            &crate::timing::revision(&f.job.root).unwrap(),
            None,
            10.0,
        )
        .unwrap();
        assert_eq!(document(&f.job).unwrap().lines(), f.words);
    }
    #[test]
    fn regional_selection_has_bounded_neighbor_context_and_gaps_do_not_request_everything() {
        let f = fixture();
        assert_eq!(selected(&f.words, 3.1, 4.0, false), vec![1]);
        assert_eq!(selected(&f.words, 3.1, 4.0, true), vec![0, 1, 2]);
        assert!(selected(&f.words, 2.2, 2.8, true).is_empty());
        assert_eq!(selected(&f.words, 0.0, 0.2, true), vec![0, 1]);
    }
    #[test]
    fn local_preview_uses_the_export_timeline_and_marks_missing_context() {
        let f = fixture();
        let ready = clips(&f.job, 10.0).unwrap();
        let slots: Vec<_> = f
            .words
            .iter()
            .map(|l| crate::timeline::Slot {
                start: l.start,
                end: l.end,
                wav_secs: 0.1,
            })
            .collect();
        let full = crate::timeline::place_all(&slots, 10.0);
        assert_eq!(ready.len(), 3);
        for (clip, placement) in ready.iter().zip(full) {
            assert_eq!(clip.start, placement.start);
            assert_eq!(clip.speed, placement.speed);
            assert!(!clip.tentative);
        }
        std::fs::remove_file(f.job.line_wav(1)).unwrap();
        let ready = clips(&f.job, 10.0).unwrap();
        assert_eq!(ready.len(), 2);
        assert!(ready.iter().all(|c| c.tentative));
    }
    #[test]
    #[ignore]
    fn benchmark_verified_preview_snapshot() {
        let fixture = fixture_with_count(2185);
        let job = &fixture.job;
        let duration = 6555.0;
        let mut rows = Vec::new();
        for round in 0..9 {
            for shared in if round % 2 == 0 {
                [false, true]
            } else {
                [true, false]
            } {
                let started = std::time::Instant::now();
                let state = crate::draft::status(&job.root).unwrap().unwrap();
                let status_ms = started.elapsed().as_secs_f64() * 1000.0;
                let ready = state.speech_ready.clone();
                let result = if shared {
                    clips_verified(job, duration, Some(&state)).unwrap()
                } else {
                    let second = crate::draft::status(&job.root).unwrap().unwrap();
                    let words = crate::timing::read(&job.root).unwrap();
                    let slots = speech_slots(job, &words).unwrap();
                    let placements = crate::timeline::place_all(&slots, duration);
                    let doc = document(job).unwrap();
                    let mut unknown = false;
                    placements
                        .into_iter()
                        .enumerate()
                        .filter_map(|(index, placement)| {
                            let cue = &doc.cues[index];
                            let known = second.speech_ready.contains(&cue.id);
                            unknown |= !known;
                            known.then(|| Clip {
                                id: cue.id.clone(),
                                path: job.line_wav(index + 1).to_string_lossy().into_owned(),
                                version: second
                                    .manifest
                                    .speech_version(&cue.id)
                                    .unwrap_or_default()
                                    .into(),
                                start: placement.start,
                                speed: placement.speed,
                                played: placement.played,
                                tentative: unknown,
                            })
                        })
                        .collect()
                };
                let elapsed = started.elapsed().as_secs_f64() * 1000.0;
                assert_eq!(ready.len(), 2185);
                assert_eq!(
                    serde_json::to_value(&result).unwrap(),
                    serde_json::to_value(clips_verified(job, duration, Some(&state)).unwrap())
                        .unwrap()
                );
                rows.push(serde_json::json!({"round":round,"shared":shared,"milliseconds":elapsed,"status_ms":status_ms,"remaining_ms":elapsed-status_ms}));
            }
        }
        std::fs::write(job.line_wav(1), b"broken").unwrap();
        let state = crate::draft::status(&job.root).unwrap().unwrap();
        assert_eq!(state.speech_ready.len(), 2184);
        assert_eq!(
            clips_verified(job, duration, Some(&state)).unwrap().len(),
            2184
        );
        println!("PREVIEW_BENCH {}", serde_json::to_string(&rows).unwrap());
    }
    #[test]
    fn shared_snapshot_matches_clips_and_next_operation_rechecks_modified_pcm() {
        let fixture = fixture();
        let job = &fixture.job;
        let state = crate::draft::status(&job.root).unwrap().unwrap();
        let expected = serde_json::to_value(clips(job, 10.0).unwrap()).unwrap();
        assert_eq!(state.speech_seconds.len(), 3);
        assert!(state.speech_seconds.values().all(|seconds| *seconds == 0.1));
        assert!(serde_json::to_value(&state).unwrap().get("speechSeconds").is_none());
        assert_eq!(
            serde_json::to_value(clips_verified(job, 10.0, Some(&state)).unwrap()).unwrap(),
            expected
        );
        let path = job.line_wav(2);
        let mut bytes = std::fs::read(&path).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        std::fs::write(&path, &bytes).unwrap();
        let words = crate::timing::read(&job.root).unwrap();
        assert_eq!(speech_slots_verified(job, &words, Some(&state)).unwrap()[1].wav_secs, 0.1);
        let refreshed = crate::draft::status(&job.root).unwrap().unwrap();
        assert!(!refreshed.speech_seconds.contains_key("cue-000002"));
        let clips = clips(job, 10.0).unwrap();
        assert_eq!(clips.len(), 2);
        assert!(clips.iter().all(|clip| clip.id != "cue-000002"));
        std::fs::write(job.line_wav(1), b"truncated").unwrap();
        assert_eq!(super::clips(job, 10.0).unwrap().len(), 1);
    }

    #[test]
    #[ignore]
    fn benchmark_preview_duration_snapshot() {
        let fixture = fixture_with_count(2185);
        let job = &fixture.job;
        let words = crate::timing::read(&job.root).unwrap();
        let mut rows = Vec::new();
        for round in 0..9 {
            for reread in if round % 2 == 0 { [true, false] } else { [false, true] } {
                let started = std::time::Instant::now();
                let state = crate::draft::status(&job.root).unwrap().unwrap();
                let status_ms = started.elapsed().as_secs_f64() * 1000.0;
                if reread {
                    for (index, cue) in state.manifest.cues.iter().enumerate() {
                        let pcm = crate::wav::read_wav(&job.line_wav(index + 1)).unwrap();
                        assert_eq!(state.speech_seconds[&cue.id], pcm.samples.len() as f64 / f64::from(pcm.rate));
                    }
                }
                let clips = clips_verified(job, 6555.0, Some(&state)).unwrap();
                let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
                assert_eq!(clips.len(), words.len());
                let serialize = std::time::Instant::now();
                let payload = serde_json::to_vec(&serde_json::json!({
                    "serial":"benchmark:1", "clips":clips,
                    "view":{"cues":document(job).unwrap().cues,"ready":state.speech_ready,
                        "revision":"fixed","root":job.root,"duration":6555.0,
                        "options":options(job).unwrap()}
                })).unwrap();
                rows.push(serde_json::json!({"round":round,"reread":reread,"status_ms":status_ms,
                    "total_ms":elapsed_ms,"payload_bytes":payload.len(),
                    "serialize_with_document_ms":serialize.elapsed().as_secs_f64()*1000.0}));
            }
        }
        println!("DURATION_SNAPSHOT {}", serde_json::to_string(&rows).unwrap());
    }

    #[test]
    #[ignore]
    fn benchmark_preview_components() {
        let fixture = fixture_with_count(2185);
        for round in 0..3 {
            let started = std::time::Instant::now();
            let state = crate::draft::status(&fixture.job.root).unwrap().unwrap();
            let status_ms = started.elapsed().as_secs_f64() * 1000.0;
            let started = std::time::Instant::now();
            let clips = clips_verified(&fixture.job, 6555.0, Some(&state)).unwrap();
            let clips_ms = started.elapsed().as_secs_f64() * 1000.0;
            assert_eq!(clips.len(), 2185);
            let started = std::time::Instant::now();
            let mut matched = 0;
            for cue in &state.manifest.cues {
                matched += usize::from(state.speech_ready.contains(&cue.id));
                matched += usize::from(
                    state
                        .manifest
                        .cues
                        .iter()
                        .find(|old| old.id == cue.id)
                        .is_some(),
                );
            }
            std::hint::black_box(matched);
            let vec_ms = started.elapsed().as_secs_f64() * 1000.0;
            let started = std::time::Instant::now();
            let ready: std::collections::HashSet<_> = state.speech_ready.iter().collect();
            let cues: std::collections::HashMap<_, _> = state
                .manifest
                .cues
                .iter()
                .map(|cue| (&cue.id, cue))
                .collect();
            let mut indexed = 0;
            for cue in &state.manifest.cues {
                indexed += usize::from(ready.contains(&cue.id));
                indexed += usize::from(cues.contains_key(&cue.id));
            }
            assert_eq!(matched, indexed);
            println!(
                "PREVIEW_COMPONENTS {}",
                serde_json::json!({"round":round,"status_ms":status_ms,"clips_ms":clips_ms,"vec_ms":vec_ms,"indexed_ms":started.elapsed().as_secs_f64()*1000.0})
            );
        }
    }
    #[test]
    fn stale_existing_wavs_do_not_influence_trial_or_preview_timing() {
        let f = fixture();
        save_patches(
            &f.job,
            &crate::timing::revision(&f.job.root).unwrap(),
            &[text("cue-000001", "lời mới chưa đọc")],
            10.0,
        )
        .unwrap();
        assert!(f.job.line_wav(1).is_file());
        let lines = crate::timing::read(&f.job.root).unwrap();
        let slots = speech_slots(&f.job, &lines).unwrap();
        assert_eq!(slots[0].wav_secs, 0.0);
        assert_eq!(slots[1].wav_secs, 0.1);
        let preview = clips(&f.job, 10.0).unwrap();
        let placed = crate::timeline::place_all(&slots, 10.0);
        assert_eq!(preview[0].start, placed[1].start);
        assert!(preview[0].tentative);
    }
    #[test]
    fn a_closed_window_cannot_clear_a_new_windows_intent() {
        let f = fixture();
        let root = f.job.root.to_string_lossy().into_owned();
        let intent = |serial: &str| Intent {
            root: root.clone(),
            serial: serial.into(),
            start: 0.0,
            end: 30.0,
            listen: false,
            playing: true,
            cue: None,
        };
        client_intent(Some(intent("old:1")), "old").unwrap();
        client_intent(Some(intent("new:1")), "new").unwrap();
        client_intent(None, "old").unwrap();
        assert!(current(&f.job, "new:1"));
        let mut bad = intent("new:2");
        bad.end = 90.0;
        assert!(client_intent(Some(bad), "new").is_err());
        assert!(current(&f.job, "new:1"));
        client_intent(None, "new").unwrap();
        assert!(!same_source(&f.job));
    }
    #[test]
    fn importing_unverified_legacy_words_does_not_certify_them_as_authored() {
        let f = fixture();
        std::fs::remove_file(f.job.root.join("translation-cache.json")).unwrap();
        let original = std::fs::read(f.job.translation_path()).unwrap();
        let doc = document(&f.job).unwrap();
        assert!(doc.cues.iter().all(|c| !c.authored));
        let doc = align_generation(&f.job, doc, &f.config, false).unwrap();
        assert!(doc.cues.iter().all(|c| c.line.translation.is_empty()));
        assert_eq!(std::fs::read(f.job.translation_path()).unwrap(), original);
    }
    #[test]
    fn preview_after_official_terms_reuses_words_instead_of_reverting_to_quick_generation() {
        let f = fixture();
        let mut config = f.config.clone();
        config.use_terms = true;
        configure(
            &f.job,
            &crate::timing::revision(&f.job.root).unwrap(),
            config.clone(),
        )
        .unwrap();
        let mut doc = document(&f.job).unwrap();
        for (cue, word) in doc.cues.iter_mut().zip(&f.words) {
            cue.line.translation = word.translation.clone();
        }
        let heard: Vec<_> = doc
            .cues
            .iter()
            .map(|c| Cue {
                start: c.line.start,
                end: c.line.end,
                text: c.line.source.clone(),
            })
            .collect();
        let glossary = crate::job::Glossary {
            theme: "fixture".into(),
            terms: vec![],
        };
        let lease =
            crate::text_cache::lease(&f.job, crate::text_cache::Kind::Terms, &heard, None, false)
                .unwrap()
                .unwrap();
        lease.commit_terms(&glossary).unwrap();
        doc.generation_basis = crate::draft::text_guard(&f.job.root, false)
            .unwrap()
            .map(|(_, key)| key);
        doc.save(&f.job.root).unwrap();
        let (official, cached) = preview_glossary(&f.job, &doc, &config, false).unwrap();
        assert!(official);
        assert_eq!(cached.theme, "fixture");
        let doc = align_generation(&f.job, doc, &config, official).unwrap();
        assert_eq!(doc.lines(), f.words);
    }
    #[test]
    fn a_rebuilt_sentence_has_a_new_media_version_while_other_clips_keep_theirs() {
        let f = fixture();
        let before = clips(&f.job, 10.0).unwrap();
        save_patches(
            &f.job,
            &crate::timing::revision(&f.job.root).unwrap(),
            &[text("cue-000001", "lời mới")],
            10.0,
        )
        .unwrap();
        let path = f.job.line_wav(1);
        let ticket = crate::draft::speech_ticket(&path, "lời mới", "vi", "fixture")
            .unwrap()
            .unwrap();
        crate::draft::commit_speech(
            &ticket,
            &path,
            &Pcm {
                rate: 22050,
                samples: vec![32; 2205],
            },
        )
        .unwrap();
        let after = clips(&f.job, 10.0).unwrap();
        assert_eq!(before[0].id, after[0].id);
        assert_eq!(before[0].path, after[0].path);
        assert_ne!(before[0].version, after[0].version);
        assert_eq!(before[1].version, after[1].version);
    }
}
