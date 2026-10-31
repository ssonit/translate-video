//! A versioned, non-destructive index beside the existing job files.
//! Receipts are evidence of committed speech, never evidence of a completed export.
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::job::{read_json, Job, TranslatedLine};

static LOCK: Mutex<()> = Mutex::new(());
const FILE: &str = "draft.json";
const INPUTS: &[&str] = &[
    "settings.json",
    "render.json",
    "transcript.json",
    "lines.json",
    "translation.json",
    "names.json",
    "terms.json",
    "editing.json",
    "timing.json",
    "draft-options.json",
];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Source {
    locator: String,
    media: String,
    sha256: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Phase {
    Preparing,
    Exporting,
    Idle,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Cue {
    pub id: String,
    pub slot: usize,
    pub start: f64,
    pub end: f64,
    pub source: String,
    pub translation: String,
    speech_key: String,
    speech_revision: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct Receipt {
    key: String,
    sha256: String,
    slot: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Manifest {
    version: u32,
    pub revision: u64,
    run_id: String,
    context: RunContext,
    configuration: serde_json::Value,
    source: Source,
    inputs: BTreeMap<String, String>,
    voice_fingerprint: String,
    pub phase: Phase,
    pub cues: Vec<Cue>,
    speech: BTreeMap<String, Receipt>,
}

impl Manifest {
    pub(crate) fn speech_version(&self, id: &str) -> Option<&str> {
        self.speech.get(id).map(|receipt| receipt.sha256.as_str())
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SpeechTicket {
    root: PathBuf,
    id: String,
    slot: usize,
    key: String,
    speech_revision: u64,
    run_id: String,
    source: Source,
    voice_fingerprint: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RunContext {
    pub provider: String,
    pub model: String,
    pub audio_only: bool,
}

pub(crate) fn context(root: &Path) -> Result<Option<RunContext>, String> {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    Ok(load(root)?.map(|m| m.context))
}

fn name_identity(root: &Path) -> Result<Vec<(String, String)>, String> {
    if !root.join("names.json").exists() {
        return Ok(Vec::new());
    }
    let list: crate::names::NameList = read_json(&root.join("names.json"))?;
    Ok(crate::names::name_signature(&crate::names::usable_names(
        &list.names,
    )))
}

// Only dependencies of text generation belong here: voice, layout and edited
// timing deliberately do not invalidate translation. No provider key is stored.
pub(crate) fn text_guard(root: &Path, terms: bool) -> Result<Option<(String, String)>, String> {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let Some(manifest) = load(root)? else {
        return Ok(None);
    };
    let settings: crate::pipeline::RunSettings = read_json(&root.join("settings.json"))?;
    let files = inputs(root)?;
    let names = name_identity(root)?;
    let dependency = digest(
        &serde_json::to_vec(&(
            1,
            &manifest.source,
            &manifest.context.provider,
            &manifest.context.model,
            &settings.target,
            settings.skip_translate,
            settings.use_terms,
            files.get("transcript.json"),
            files.get("lines.json"),
            names,
            if terms || !settings.use_terms {
                None
            } else {
                files.get("terms.json")
            },
        ))
        .map_err(|e| e.to_string())?,
    );
    Ok(Some((manifest.run_id, dependency)))
}

pub(crate) fn matches_text_context(
    root: &Path,
    provider: &str,
    model: &str,
) -> Result<bool, String> {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    Ok(load(root)?.is_some_and(|m| m.context.provider == provider && m.context.model == model))
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn hash_file(path: &Path, cancel: &AtomicBool) -> Result<String, String> {
    let before = std::fs::metadata(path).map_err(|e| e.to_string())?;
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0; 1024 * 1024];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("Đã dừng.".into());
        }
        let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    let after = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if before.len() != after.len() || before.modified().ok() != after.modified().ok() {
        return Err(format!(
            "File vừa thay đổi khi kiểm tra nháp: {}.",
            path.display()
        ));
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn load(root: &Path) -> Result<Option<Manifest>, String> {
    let bytes = match std::fs::read(root.join(FILE)) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    let manifest: Manifest = serde_json::from_slice(&bytes)
        .map_err(|_| "Không đọc được draft.json. Giữ nguyên nháp để kiểm tra.".to_string())?;
    if manifest.version != 1 || manifest.revision == 0 {
        return Err("Phiên bản draft.json chưa được hỗ trợ. Giữ nguyên nháp để kiểm tra.".into());
    }
    Ok(Some(manifest))
}

fn save(root: &Path, manifest: &Manifest) -> Result<(), String> {
    crate::atomic_file::write(
        &root.join(FILE),
        &serde_json::to_vec_pretty(manifest).map_err(|e| e.to_string())?,
    )
}

// Called before invalidating any old settings/cache. Local replacement in-place
// must not silently inherit the transcript of the previous file with the same name.
pub(crate) fn inspect_source(job: &Job, cancel: &AtomicBool) -> Result<Source, String> {
    let previous = load(&job.root)?;
    let url = crate::job::source_url(&job.root);
    let media = job.video.to_string_lossy().into_owned();
    let source = Source {
        locator: if url.is_empty() { media.clone() } else { url },
        media,
        sha256: if job.video.is_file() {
            Some(hash_file(&job.video, cancel)?)
        } else {
            None
        },
    };
    if let Some(old) = previous {
        if old.source.locator != source.locator
            || (old.source.media == source.media
                && old.source.sha256.is_some()
                && old.source.sha256 != source.sha256)
        {
            return Err("Nguồn đã thay đổi so với bản nháp. Chọn folder kết quả khác hoặc đổi tên nguồn; dữ liệu cũ được giữ nguyên.".into());
        }
    }
    Ok(source)
}

fn inputs(root: &Path) -> Result<BTreeMap<String, String>, String> {
    let mut found = BTreeMap::new();
    for name in INPUTS {
        match std::fs::read(root.join(name)) {
            Ok(bytes) => {
                found.insert((*name).into(), digest(&bytes));
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(found)
}

fn collect(
    root: &Path,
    source: &Source,
    voice: &str,
    dependencies: &BTreeMap<String, String>,
) -> Result<Vec<Cue>, String> {
    let settings: serde_json::Value = if root.join("settings.json").exists() {
        read_json(&root.join("settings.json"))?
    } else {
        serde_json::Value::Null
    };
    let render = crate::render::RenderSettings::read(root)?;
    let lang = settings["target"].as_str().unwrap_or("vi");
    let names = name_identity(root)?;
    let lines: Vec<TranslatedLine> = if root.join("translation.json").exists() {
        read_json(&root.join("translation.json"))?
    } else if root.join("lines.json").exists() {
        let heard: Vec<crate::job::Cue> = read_json(&root.join("lines.json"))?;
        heard
            .into_iter()
            .map(|c| TranslatedLine {
                start: c.start,
                end: c.end,
                source: c.text,
                translation: String::new(),
            })
            .collect()
    } else {
        Vec::new()
    };
    let mut effective = lines;
    let edited = crate::editing::Editing::load(root)?;
    if root.join("translation.json").exists() {
        crate::timing::apply(root, &mut effective)?;
    }
    effective
        .into_iter()
        .enumerate()
        .map(|(index, line)| {
            if !line.start.is_finite()
                || !line.end.is_finite()
                || line.start < 0.0
                || line.end <= line.start
            {
                return Err("Mốc câu trong nháp không hợp lệ.".into());
            }
            let (id, slot) = edited
                .as_ref()
                .map(|e| (e.cues[index].id.clone(), e.cues[index].slot))
                .unwrap_or_else(|| (crate::editing::legacy_id(index), index + 1));
            let text = crate::render::spoken_text(
                &render,
                lang,
                &crate::speech::clean_for_speech(&line.translation),
            );
            let terms = settings["use_terms"]
                .as_bool()
                .unwrap_or(false)
                .then(|| dependencies.get("terms.json"))
                .flatten();
            let speech_key = digest(
                &serde_json::to_vec(&(
                    1,
                    source,
                    &id,
                    &line.source,
                    lang,
                    &settings["voice"],
                    voice,
                    &text,
                    &settings["skip_translate"],
                    &settings["use_terms"],
                    dependencies.get("lines.json"),
                    dependencies.get("transcript.json"),
                    &names,
                    terms,
                ))
                .map_err(|e| e.to_string())?,
            );
            Ok(Cue {
                id,
                slot,
                start: line.start,
                end: line.end,
                source: line.source,
                translation: line.translation,
                speech_key,
                speech_revision: 0,
            })
        })
        .collect()
}

fn observe(root: &Path, manifest: &mut Manifest) -> Result<(), String> {
    let next = inputs(root)?;
    let mut cues = collect(root, &manifest.source, &manifest.voice_fingerprint, &next)?;
    let settings = if root.join("settings.json").exists() {
        let settings: crate::pipeline::RunSettings = read_json(&root.join("settings.json"))?;
        serde_json::to_value(settings).map_err(|e| e.to_string())?
    } else {
        serde_json::Value::Null
    };
    let configuration = serde_json::json!({ "settings": settings, "render": crate::render::RenderSettings::read(root)? });
    let revision = manifest
        .revision
        .checked_add(1)
        .ok_or("Revision nháp vượt giới hạn.")?;
    for cue in &mut cues {
        cue.speech_revision = manifest
            .cues
            .iter()
            .find(|old| old.id == cue.id && old.speech_key == cue.speech_key)
            .map(|old| old.speech_revision)
            .unwrap_or(revision);
    }
    if next != manifest.inputs || cues != manifest.cues {
        manifest.revision = revision;
        manifest.inputs = next;
        manifest.cues = cues;
    }
    manifest.configuration = configuration;
    Ok(())
}

fn refresh_inner(root: &Path, manifest: &mut Manifest) -> Result<(), String> {
    observe(root, manifest)?;
    save(root, manifest)
}

fn voice_fingerprint(
    settings: &crate::pipeline::RunSettings,
    cancel: &AtomicBool,
) -> Result<String, String> {
    if !settings.text_only {
        let dir = crate::bundled_voice_dir(&settings.target).ok_or("Chưa thấy folder giọng.")?;
        return hash_voice_files(&dir, &settings.voice, cancel);
    }
    Ok(String::new())
}

fn hash_voice_files(dir: &Path, name: &str, cancel: &AtomicBool) -> Result<String, String> {
    let mut voice = String::new();
    for ext in ["onnx", "onnx.json"] {
        voice.push_str(&hash_file(&dir.join(format!("{name}.{ext}")), cancel)?);
    }
    Ok(voice)
}

pub(crate) fn matches_voice(root: &Path, cancel: &AtomicBool) -> Result<bool, String> {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let Some(manifest) = load(root)? else {
        return Ok(false);
    };
    let settings: crate::pipeline::RunSettings = read_json(&root.join("settings.json"))?;
    Ok(manifest.voice_fingerprint == voice_fingerprint(&settings, cancel)?)
}

pub(crate) fn begin(
    job: &Job,
    source: Source,
    context: RunContext,
    cancel: &AtomicBool,
) -> Result<(), String> {
    begin_mode(job, source, context, cancel, true)
}

pub(crate) fn begin_text(
    job: &Job,
    source: Source,
    context: RunContext,
    cancel: &AtomicBool,
) -> Result<(), String> {
    begin_mode(job, source, context, cancel, false)
}

fn begin_mode(
    job: &Job,
    source: Source,
    context: RunContext,
    cancel: &AtomicBool,
    voice_required: bool,
) -> Result<(), String> {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let settings: crate::pipeline::RunSettings = read_json(&job.settings_path())?;
    let previous = load(&job.root)?;
    let voice = if voice_required || settings.text_only {
        voice_fingerprint(&settings, cancel)?
    } else {
        previous
            .as_ref()
            .map(|m| m.voice_fingerprint.clone())
            .unwrap_or_default()
    };
    let mut manifest = previous.unwrap_or(Manifest {
        version: 1,
        revision: 1,
        run_id: String::new(),
        context: context.clone(),
        configuration: serde_json::Value::Null,
        source: source.clone(),
        inputs: BTreeMap::new(),
        voice_fingerprint: voice.clone(),
        phase: Phase::Idle,
        cues: Vec::new(),
        speech: BTreeMap::new(),
    });
    if manifest.source != source
        || manifest.voice_fingerprint != voice
        || manifest.context != context
    {
        manifest.revision = manifest
            .revision
            .checked_add(1)
            .ok_or("Revision nháp vượt giới hạn.")?;
        manifest.source = source;
        manifest.voice_fingerprint = voice;
        manifest.context = context;
    }
    manifest.run_id = crate::history::new_id();
    manifest.phase = Phase::Preparing;
    refresh_inner(&job.root, &mut manifest)
}

pub(crate) fn checkpoint(root: &Path, phase: Option<Phase>) -> Result<(), String> {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(mut manifest) = load(root)? {
        if let Some(phase) = phase {
            manifest.phase = phase;
        }
        refresh_inner(root, &mut manifest)?;
    }
    Ok(())
}

pub(crate) fn after_json(path: &Path) -> Result<(), String> {
    if path
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| INPUTS.contains(&n))
    {
        if let Some(root) = path.parent() {
            checkpoint(root, None)?;
        }
    }
    Ok(())
}

// Preview's watch/ and voice samples deliberately remain outside this protocol.
pub(crate) fn speech_ticket(
    path: &Path,
    text: &str,
    lang: &str,
    voice: &str,
) -> Result<Option<SpeechTicket>, String> {
    let Some(lines) = path
        .parent()
        .filter(|p| p.file_name().is_some_and(|s| s == "lines"))
    else {
        return Ok(None);
    };
    let Some(root) = lines.parent() else {
        return Ok(None);
    };
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let Some(mut manifest) = load(root)? else {
        return Ok(None);
    };
    refresh_inner(root, &mut manifest)?;
    let settings: crate::pipeline::RunSettings = read_json(&root.join("settings.json"))?;
    if settings.target != lang || settings.voice != voice {
        return Err("Cấu hình giọng vừa thay đổi.".into());
    }
    let render = crate::render::RenderSettings::read(root)?;
    let cue = manifest.cues.iter().find(|c| {
        path.file_stem()
            .and_then(|s| s.to_str())
            .and_then(|s| s.parse::<usize>().ok())
            == Some(c.slot)
    });
    let Some(cue) = cue else {
        return Err("Không thấy ID câu cần đọc trong nháp.".into());
    };
    let expected = crate::render::spoken_text(
        &render,
        lang,
        &crate::speech::clean_for_speech(&cue.translation),
    );
    if expected != text {
        return Err("Lời đọc vừa thay đổi; không ghi tiếng vào nháp cũ.".into());
    }
    Ok(Some(SpeechTicket {
        root: root.into(),
        id: cue.id.clone(),
        slot: cue.slot,
        key: cue.speech_key.clone(),
        run_id: manifest.run_id,
        speech_revision: cue.speech_revision,
        source: manifest.source,
        voice_fingerprint: manifest.voice_fingerprint,
    }))
}

// Hold the draft lock through the WAV commit and its receipt. A receipt failure
// leaves an unverified WAV, which can be inspected/retried, never a false ready cue.
pub(crate) fn commit_speech(
    ticket: &SpeechTicket,
    path: &Path,
    pcm: &crate::wav::Pcm,
) -> Result<(), String> {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut manifest = load(&ticket.root)?.ok_or("Nháp đã bị thay đổi.")?;
    refresh_inner(&ticket.root, &mut manifest)?;
    if manifest.run_id != ticket.run_id
        || manifest.source != ticket.source
        || manifest.voice_fingerprint != ticket.voice_fingerprint
        || !manifest.cues.iter().any(|c| {
            c.id == ticket.id
                && c.slot == ticket.slot
                && c.speech_key == ticket.key
                && c.speech_revision == ticket.speech_revision
        })
    {
        return Err("Revision lời đọc đã cũ; không ghi WAV.".into());
    }
    let expected = ticket
        .root
        .join("lines")
        .join(format!("{:03}.wav", ticket.slot));
    if path != expected {
        return Err("Đường dẫn WAV không khớp ID câu.".into());
    }
    crate::wav::write_wav(path, pcm)?;
    manifest.speech.insert(
        ticket.id.clone(),
        Receipt {
            key: ticket.key.clone(),
            slot: ticket.slot,
            sha256: digest(&crate::wav::encode_wav(pcm)),
        },
    );
    save(&ticket.root, &manifest)
}

fn complete_pcm(bytes: &[u8]) -> Option<crate::wav::Pcm> {
    if bytes.len() < 12
        || u32::from_le_bytes(bytes[4..8].try_into().ok()?) as usize + 8 > bytes.len()
    {
        return None;
    }
    let mut offset = 12usize;
    while offset < bytes.len() {
        let header = bytes.get(offset..offset.checked_add(8)?)?;
        let size = u32::from_le_bytes(header[4..8].try_into().ok()?) as usize;
        offset = offset
            .checked_add(8)?
            .checked_add(size)?
            .checked_add(size & 1)?;
        if offset > bytes.len() {
            return None;
        }
    }
    let pcm = crate::wav::parse_wav(bytes).ok()?;
    (pcm.rate > 0 && !pcm.samples.is_empty()).then_some(pcm)
}

#[cfg(test)]
pub(crate) fn benchmark_speech(job: &Job, words: &[TranslatedLine]) {
    let mut manifest = load(&job.root).unwrap().unwrap();
    observe(&job.root, &mut manifest).unwrap();
    for (index, cue) in manifest.cues.iter().enumerate() {
        assert_eq!(cue.translation, words[index].translation);
        let pcm = crate::wav::Pcm {
            rate: 22050,
            samples: vec![1; 44100],
        };
        crate::wav::write_wav(&job.line_wav(index + 1), &pcm).unwrap();
        manifest.speech.insert(
            cue.id.clone(),
            Receipt {
                key: cue.speech_key.clone(),
                slot: cue.slot,
                sha256: digest(&crate::wav::encode_wav(&pcm)),
            },
        );
    }
    save(&job.root, &manifest).unwrap();
}

pub(crate) fn record_existing(ticket: &SpeechTicket, path: &Path) -> Result<bool, String> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e.to_string()),
    };
    let Some(pcm) = complete_pcm(&bytes) else {
        return Ok(false);
    };
    commit_speech(ticket, path, &pcm)?;
    Ok(true)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Status {
    pub manifest: Manifest,
    pub speech_ready: Vec<String>,
    pub speech_stale: Vec<String>,
    pub unverified_wavs: Vec<String>,
    pub ready_regions: Vec<[f64; 2]>,
    #[serde(skip)]
    pub speech_seconds: BTreeMap<String, f64>,
}

pub(crate) fn status(root: &Path) -> Result<Option<Status>, String> {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let Some(mut manifest) = load(root)? else {
        return Ok(None);
    };
    // A read does not rewrite the on-disk revision or turn an interrupted phase into success.
    observe(root, &mut manifest)?;
    let mut ready = Vec::new();
    let mut stale = Vec::new();
    let mut unverified = Vec::new();
    let mut intervals = Vec::new();
    let mut speech_seconds = BTreeMap::new();
    for cue in &manifest.cues {
        if !cue.translation.trim().is_empty()
            && !manifest
                .speech
                .get(&cue.id)
                .is_some_and(|r| r.key == cue.speech_key && r.slot == cue.slot)
        {
            stale.push(cue.id.clone());
        }
        let path = root.join("lines").join(format!("{:03}.wav", cue.slot));
        let valid = !cue.translation.trim().is_empty()
            && manifest.speech.get(&cue.id).is_some_and(|r| {
                r.key == cue.speech_key
                    && r.slot == cue.slot
                    && std::fs::read(&path).ok().is_some_and(|bytes| {
                        if digest(&bytes) != r.sha256 {
                            return false;
                        }
                        if let Some(pcm) = complete_pcm(&bytes) {
                            speech_seconds.insert(
                                cue.id.clone(),
                                pcm.samples.len() as f64 / f64::from(pcm.rate),
                            );
                            true
                        } else {
                            false
                        }
                    })
            });
        if valid {
            ready.push(cue.id.clone());
            intervals.push([cue.start, cue.end]);
        } else if path.is_file() {
            unverified.push(cue.id.clone());
        }
    }
    intervals.sort_by(|a, b| a[0].total_cmp(&b[0]));
    let mut regions: Vec<[f64; 2]> = Vec::new();
    for [start, end] in intervals {
        if let Some(last) = regions.last_mut().filter(|last| start <= last[1]) {
            last[1] = last[1].max(end);
        } else {
            regions.push([start, end]);
        }
    }
    Ok(Some(Status {
        manifest,
        speech_ready: ready,
        speech_stale: stale,
        unverified_wavs: unverified,
        ready_regions: regions,
        speech_seconds,
    }))
}

// Finished media may remain valid after its raw WAVs were removed. Reuse that
// media only if the speech recipes still match; when raw audio is needed,
// require both the recipe and the committed WAV digest.
pub(crate) fn prepare_speech_cache(job: &Job) -> Result<bool, String> {
    let finished = [job.result(), job.spoken()]
        .iter()
        .any(|p| crate::job::file_ready(p));
    let Some(state) = status(&job.root)? else {
        return Ok(finished);
    };
    if finished && state.speech_stale.is_empty() {
        return Ok(true);
    }
    let invalid: Vec<_> = state
        .manifest
        .cues
        .iter()
        .filter(|cue| !state.speech_ready.contains(&cue.id))
        .map(|cue| job.root.join("lines").join(format!("{:03}.wav", cue.slot)))
        .collect();
    if invalid.is_empty() {
        return Ok(false);
    }
    if !finished
        && !job.captioned().is_file()
        && !job.dub().is_file()
        && !invalid.iter().any(|p| p.is_file())
    {
        return Ok(false);
    }
    let folder = job.root.join("cache-stale").join(crate::history::new_id());
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let manifest = std::fs::read(job.root.join(FILE)).map_err(|e| e.to_string())?;
    crate::atomic_file::write(&folder.join(FILE), &manifest)?;
    for path in &invalid {
        match std::fs::read(path) {
            Ok(bytes) => crate::atomic_file::write(
                &folder
                    .join("lines")
                    .join(path.file_name().ok_or("Không thấy file tiếng.")?),
                &bytes,
            )?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    if !state.speech_stale.is_empty() {
        crate::job::archive_exports(&job.root)?;
    }
    for path in &invalid {
        for file in [path.clone(), path.with_extension("fit.wav")] {
            match std::fs::remove_file(file) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.to_string()),
            }
        }
    }
    for name in [
        "dub.wav",
        "fit-report.json",
        "voice-spans.json",
        "frame-cache.json",
    ] {
        match std::fs::remove_file(job.root.join(name)) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job::{write_json, Cue as Heard};
    use crate::wav::{write_wav, Pcm};

    struct Fixture {
        root: PathBuf,
        job: Job,
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
    fn context() -> RunContext {
        RunContext {
            provider: "test-provider".into(),
            model: "test-model".into(),
            audio_only: false,
        }
    }
    fn fixture() -> Fixture {
        let root = std::env::temp_dir().join(format!("draft-state-{}", crate::history::new_id()));
        std::fs::create_dir_all(root.join("output")).unwrap();
        let source = root.join("nguồn.mp4");
        std::fs::write(&source, b"source bytes").unwrap();
        let job = Job::open(&source, &root.join("output")).unwrap();
        // No model inference or bundled voice is needed for these storage tests.
        write_json(
            &job.settings_path(),
            &crate::pipeline::RunSettings {
                target: "vi".into(),
                voice: "fixture".into(),
                use_terms: false,
                skip_translate: false,
                text_only: true,
            },
        )
        .unwrap();
        let lines: Vec<_> = (0..3)
            .map(|i| TranslatedLine {
                start: i as f64 * 3.0,
                end: i as f64 * 3.0 + 2.0,
                source: format!("source {i}"),
                translation: format!("câu {i}"),
            })
            .collect();
        let heard: Vec<_> = lines
            .iter()
            .map(|l| Heard {
                start: l.start,
                end: l.end,
                text: l.source.clone(),
            })
            .collect();
        write_json(&job.lines_path(), &heard).unwrap();
        write_json(&job.translation_path(), &lines).unwrap();
        let source = inspect_source(&job, &AtomicBool::new(false)).unwrap();
        begin(&job, source, context(), &AtomicBool::new(false)).unwrap();
        Fixture { root, job }
    }
    fn ticket(f: &Fixture, slot: usize) -> SpeechTicket {
        let state = status(&f.job.root).unwrap().unwrap();
        let cue = state.manifest.cues.iter().find(|c| c.slot == slot).unwrap();
        speech_ticket(&f.job.line_wav(slot), &cue.translation, "vi", "fixture")
            .unwrap()
            .unwrap()
    }
    fn pcm() -> Pcm {
        Pcm {
            rate: 22050,
            samples: vec![1, 2, 3, 4],
        }
    }

    #[test]
    fn partial_speech_receipts_survive_reopen_and_do_not_certify_legacy_wavs() {
        let f = fixture();
        write_wav(&f.job.line_wav(2), &pcm()).unwrap();
        commit_speech(&ticket(&f, 1), &f.job.line_wav(1), &pcm()).unwrap();
        let before = status(&f.job.root).unwrap().unwrap();
        assert_eq!(before.speech_ready, vec!["cue-000001"]);
        assert_eq!(before.unverified_wavs, vec!["cue-000002"]);
        assert_eq!(before.ready_regions, vec![[0.0, 2.0]]);
        let revision = before.manifest.revision;
        checkpoint(&f.job.root, None).unwrap();
        let source = inspect_source(&f.job, &AtomicBool::new(false)).unwrap();
        begin(&f.job, source, context(), &AtomicBool::new(false)).unwrap();
        let reopened = status(&f.job.root).unwrap().unwrap();
        assert_eq!(reopened.manifest.revision, revision);
        assert_eq!(reopened.speech_ready, before.speech_ready);
        assert_eq!(
            reopened
                .manifest
                .cues
                .iter()
                .map(|c| &c.id)
                .collect::<Vec<_>>(),
            before
                .manifest
                .cues
                .iter()
                .map(|c| &c.id)
                .collect::<Vec<_>>()
        );
        assert_eq!(std::fs::read(f.job.video.clone()).unwrap(), b"source bytes");
    }

    #[test]
    fn changed_words_reject_the_old_ticket_but_do_not_invalidate_another_sentence() {
        let f = fixture();
        let old = ticket(&f, 1);
        let other = ticket(&f, 2);
        let mut words: Vec<TranslatedLine> = read_json(&f.job.translation_path()).unwrap();
        words[0].translation = "lời mới".into();
        write_json(&f.job.translation_path(), &words).unwrap();
        assert!(commit_speech(&old, &f.job.line_wav(1), &pcm()).is_err());
        assert!(!f.job.line_wav(1).exists());
        commit_speech(&other, &f.job.line_wav(2), &pcm()).unwrap();
        assert_eq!(
            status(&f.job.root).unwrap().unwrap().speech_ready,
            vec!["cue-000002"]
        );
        // Returning to the old text must not resurrect an in-flight response.
        words[0].translation = "câu 0".into();
        write_json(&f.job.translation_path(), &words).unwrap();
        assert!(commit_speech(&old, &f.job.line_wav(1), &pcm()).is_err());
    }

    #[test]
    fn retiming_and_style_changes_preserve_speech_and_update_regions() {
        let f = fixture();
        let first = ticket(&f, 1);
        let mut render = crate::render::RenderSettings::default();
        render.size = 5.0;
        write_json(&f.job.root.join("render.json"), &render).unwrap();
        let words: Vec<TranslatedLine> = read_json(&f.job.translation_path()).unwrap();
        let mut timing = crate::timing::Timing::load(&f.job.root, &words).unwrap();
        timing.set(&[crate::timing::TimeChange {
            index: 0,
            start: 8.0,
            end: 9.0,
        }]);
        timing.save(&f.job.root).unwrap();
        commit_speech(&first, &f.job.line_wav(1), &pcm()).unwrap();
        commit_speech(&ticket(&f, 2), &f.job.line_wav(2), &pcm()).unwrap();
        let state = status(&f.job.root).unwrap().unwrap();
        assert_eq!(state.ready_regions, vec![[3.0, 5.0], [8.0, 9.0]]);
        assert_eq!(state.speech_ready.len(), 2);
        write_json(
            &f.job.names_path(),
            &crate::names::NameList {
                confirmed: true,
                names: vec![],
            },
        )
        .unwrap();
        assert_eq!(status(&f.job.root).unwrap().unwrap().speech_ready.len(), 2);
    }

    #[test]
    fn voice_names_and_pronunciation_changes_make_the_receipt_stale_without_deleting_files() {
        let f = fixture();
        commit_speech(&ticket(&f, 1), &f.job.line_wav(1), &pcm()).unwrap();
        let wav = std::fs::read(f.job.line_wav(1)).unwrap();
        let mut render = crate::render::RenderSettings::default();
        render.pronunciations.insert(
            "vi".into(),
            vec![crate::names::NameFix {
                from: "câu".into(),
                to: "lời".into(),
            }],
        );
        write_json(&f.job.root.join("render.json"), &render).unwrap();
        assert!(status(&f.job.root)
            .unwrap()
            .unwrap()
            .speech_ready
            .is_empty());
        assert_eq!(std::fs::read(f.job.line_wav(1)).unwrap(), wav);
        assert!(speech_ticket(&f.job.line_wav(1), "câu 0", "vi", "fixture").is_err());
        write_json(
            &f.job.names_path(),
            &serde_json::json!({"confirmed":true,"names":[]}),
        )
        .unwrap();
        assert!(status(&f.job.root)
            .unwrap()
            .unwrap()
            .speech_ready
            .is_empty());
        let mut settings: crate::pipeline::RunSettings = read_json(&f.job.settings_path()).unwrap();
        settings.voice = "other".into();
        write_json(&f.job.settings_path(), &settings).unwrap();
        assert!(speech_ticket(&f.job.line_wav(1), "lời 0", "vi", "fixture").is_err());
    }

    #[test]
    fn replacement_source_or_unsupported_manifest_fails_without_touching_intermediates() {
        let f = fixture();
        let words = std::fs::read(f.job.translation_path()).unwrap();
        std::fs::write(&f.job.video, b"other source").unwrap();
        assert!(inspect_source(&f.job, &AtomicBool::new(false)).is_err());
        assert_eq!(std::fs::read(f.job.translation_path()).unwrap(), words);
        let original = std::fs::read(f.job.root.join(FILE)).unwrap();
        let mut future: serde_json::Value = serde_json::from_slice(&original).unwrap();
        future["version"] = 100.into();
        let future = serde_json::to_vec(&future).unwrap();
        std::fs::write(f.job.root.join(FILE), &future).unwrap();
        assert!(status(&f.job.root).is_err());
        assert_eq!(std::fs::read(f.job.root.join(FILE)).unwrap(), future);
        assert_eq!(std::fs::read(f.job.translation_path()).unwrap(), words);
    }

    #[test]
    fn modified_or_truncated_wav_is_not_ready_and_status_is_read_only() {
        let f = fixture();
        commit_speech(&ticket(&f, 1), &f.job.line_wav(1), &pcm()).unwrap();
        let before = std::fs::read(f.job.root.join(FILE)).unwrap();
        let mut bytes = std::fs::read(f.job.line_wav(1)).unwrap();
        bytes.pop();
        std::fs::write(f.job.line_wav(1), &bytes).unwrap();
        assert!(status(&f.job.root)
            .unwrap()
            .unwrap()
            .speech_ready
            .is_empty());
        assert!(!record_existing(&ticket(&f, 1), &f.job.line_wav(1)).unwrap());
        assert_eq!(std::fs::read(f.job.root.join(FILE)).unwrap(), before);
    }

    #[test]
    fn a_new_run_rejects_old_responses_while_preserving_completed_receipts() {
        let f = fixture();
        let old = ticket(&f, 1);
        commit_speech(&ticket(&f, 2), &f.job.line_wav(2), &pcm()).unwrap();
        let source = inspect_source(&f.job, &AtomicBool::new(false)).unwrap();
        begin(&f.job, source, context(), &AtomicBool::new(false)).unwrap();
        assert!(commit_speech(&old, &f.job.line_wav(1), &pcm()).is_err());
        assert_eq!(
            status(&f.job.root).unwrap().unwrap().speech_ready,
            vec!["cue-000002"]
        );
    }

    #[test]
    fn split_and_undo_keep_unrelated_stable_ids_slots_and_receipts() {
        let f = fixture();
        commit_speech(&ticket(&f, 3), &f.job.line_wav(3), &pcm()).unwrap();
        let original_wav = std::fs::read(f.job.line_wav(3)).unwrap();
        let base: Vec<TranslatedLine> = read_json(&f.job.translation_path()).unwrap();
        let mut edit = crate::editing::Editing::initial(&f.job.root, &base, &base).unwrap();
        edit.restructure(
            &f.job.root,
            &crate::editing::Structure::Split {
                revision: String::new(),
                id: "cue-000001".into(),
                at: 1.0,
                left_source: "source left".into(),
                left_text: "lời trái".into(),
                right_source: "source right".into(),
                right_text: "lời phải".into(),
            },
            10.0,
        )
        .unwrap();
        edit.save(&f.job.root).unwrap();
        let state = status(&f.job.root).unwrap().unwrap();
        assert_eq!(state.manifest.cues.len(), 4);
        assert_eq!(state.manifest.cues[3].id, "cue-000003");
        assert_eq!(state.manifest.cues[3].slot, 3);
        assert_eq!(state.speech_ready, vec!["cue-000003"]);
        let left = &state.manifest.cues[0];
        let path = f
            .job
            .root
            .join("lines")
            .join(format!("{:03}.wav", left.slot));
        let new = speech_ticket(&path, &left.translation, "vi", "fixture")
            .unwrap()
            .unwrap();
        commit_speech(&new, &path, &pcm()).unwrap();
        let mut edit = crate::editing::Editing::load(&f.job.root).unwrap().unwrap();
        edit.restructure(
            &f.job.root,
            &crate::editing::Structure::Undo {
                revision: String::new(),
            },
            10.0,
        )
        .unwrap();
        edit.save(&f.job.root).unwrap();
        assert!(commit_speech(&new, &path, &pcm()).is_err());
        let state = status(&f.job.root).unwrap().unwrap();
        assert_eq!(state.manifest.cues.len(), 3);
        assert_eq!(state.speech_ready, vec!["cue-000003"]);
        assert_eq!(std::fs::read(f.job.line_wav(3)).unwrap(), original_wav);
    }

    #[test]
    fn phase_does_not_claim_an_export_and_import_keeps_every_legacy_file() {
        let f = fixture();
        std::fs::remove_file(f.job.root.join(FILE)).unwrap();
        let words = std::fs::read(f.job.translation_path()).unwrap();
        let result = f.job.root.join("previous.vi.mp4");
        std::fs::write(&result, b"previous export").unwrap();
        write_wav(&f.job.line_wav(1), &pcm()).unwrap();
        let wav = std::fs::read(f.job.line_wav(1)).unwrap();
        assert!(status(&f.job.root).unwrap().is_none());
        let source = inspect_source(&f.job, &AtomicBool::new(false)).unwrap();
        begin(&f.job, source, context(), &AtomicBool::new(false)).unwrap();
        assert_eq!(
            status(&f.job.root).unwrap().unwrap().manifest.phase,
            Phase::Preparing
        );
        checkpoint(&f.job.root, Some(Phase::Exporting)).unwrap();
        assert_eq!(
            status(&f.job.root).unwrap().unwrap().manifest.phase,
            Phase::Exporting
        );
        checkpoint(&f.job.root, Some(Phase::Idle)).unwrap();
        let state = status(&f.job.root).unwrap().unwrap();
        assert_eq!(state.manifest.phase, Phase::Idle);
        assert!(state.speech_ready.is_empty());
        assert_eq!(state.unverified_wavs, vec!["cue-000001"]);
        assert_eq!(std::fs::read(f.job.translation_path()).unwrap(), words);
        assert_eq!(std::fs::read(f.job.line_wav(1)).unwrap(), wav);
        assert_eq!(std::fs::read(result).unwrap(), b"previous export");
    }

    #[test]
    fn cached_result_does_not_bypass_the_source_fingerprint() {
        let f = fixture();
        let settings: crate::pipeline::RunSettings = read_json(&f.job.settings_path()).unwrap();
        let result = f.job.captioned();
        std::fs::write(&result, b"previous captioned export").unwrap();
        assert!(crate::pipeline::matching_result(
            &f.job.root,
            &f.job.root,
            &settings,
            false,
            false
        )
        .is_some());
        std::fs::write(&f.job.video, b"replacement source bytes").unwrap();
        assert!(crate::pipeline::matching_result(
            &f.job.root,
            &f.job.root,
            &settings,
            false,
            false
        )
        .is_none());
        assert_eq!(std::fs::read(result).unwrap(), b"previous captioned export");
    }

    #[test]
    fn corrupt_raw_speech_is_rebuilt_without_touching_other_sentences() {
        let f = fixture();
        for slot in 1..=3 {
            commit_speech(&ticket(&f, slot), &f.job.line_wav(slot), &pcm()).unwrap();
        }
        let keep = std::fs::read(f.job.line_wav(1)).unwrap();
        std::fs::write(f.job.line_wav(2), b"corrupted wav").unwrap();
        assert!(!prepare_speech_cache(&f.job).unwrap());
        assert_eq!(std::fs::read(f.job.line_wav(1)).unwrap(), keep);
        assert!(!f.job.line_wav(2).is_file());
        assert!(f.job.line_wav(3).is_file());
    }

    #[test]
    fn changed_speech_recipe_archives_the_export_without_retranslating_words() {
        let f = fixture();
        for slot in 1..=3 {
            commit_speech(&ticket(&f, slot), &f.job.line_wav(slot), &pcm()).unwrap();
        }
        let words = std::fs::read(f.job.translation_path()).unwrap();
        std::fs::write(f.job.result(), b"previous export").unwrap();
        let mut settings: crate::pipeline::RunSettings = read_json(&f.job.settings_path()).unwrap();
        settings.voice = "new voice".into();
        write_json(&f.job.settings_path(), &settings).unwrap();
        assert!(!prepare_speech_cache(&f.job).unwrap());
        assert!(!f.job.result().is_file());
        assert!(!f.job.line_wav(1).is_file());
        assert_eq!(std::fs::read(f.job.translation_path()).unwrap(), words);
        let kept = crate::job::list_kept(&f.job.root).unwrap();
        assert_eq!(std::fs::read(&kept[0].path).unwrap(), b"previous export");
    }

    #[test]
    fn missing_raw_wav_alone_does_not_invalidate_finished_media() {
        let f = fixture();
        for slot in 1..=3 {
            commit_speech(&ticket(&f, slot), &f.job.line_wav(slot), &pcm()).unwrap();
        }
        std::fs::write(f.job.result(), b"valid export").unwrap();
        std::fs::remove_file(f.job.line_wav(2)).unwrap();
        assert!(prepare_speech_cache(&f.job).unwrap());
        assert_eq!(std::fs::read(f.job.result()).unwrap(), b"valid export");
    }

    #[test]
    fn voice_fingerprint_checks_model_and_config_contents() {
        let f = fixture();
        let dir = f.root.join("fake-voice");
        std::fs::create_dir(&dir).unwrap();
        std::fs::write(dir.join("voice.onnx"), b"aaaa").unwrap();
        std::fs::write(dir.join("voice.onnx.json"), b"{}").unwrap();
        let first = hash_voice_files(&dir, "voice", &AtomicBool::new(false)).unwrap();
        std::fs::write(dir.join("voice.onnx"), b"bbbb").unwrap();
        let second = hash_voice_files(&dir, "voice", &AtomicBool::new(false)).unwrap();
        assert_ne!(first, second);
        std::fs::write(dir.join("voice.onnx.json"), b"[]").unwrap();
        assert_ne!(
            second,
            hash_voice_files(&dir, "voice", &AtomicBool::new(false)).unwrap()
        );
    }
}
