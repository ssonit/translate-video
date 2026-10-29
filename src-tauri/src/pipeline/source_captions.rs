// Reuse only publisher captions in the video's original language. Metadata is
// collected by the existing download, so probing captions needs no second yt-dlp run.
use std::io::Read;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use serde_json::Value;
use tauri::AppHandle;

use super::download::cues_from_subtitle;
use super::{log_line, stopped};
use crate::job::{read_json, write_json, Cue, Job};

const MAX_CAPTION_BYTES: u64 = 8 * 1024 * 1024;

fn original_track(info: &Value) -> Option<(&str, &str, &str)> {
    // `subtitles` contains publisher tracks; never use `automatic_captions`,
    // whose translated tracks and rolling fragments can silently change meaning.
    let lang = info.get("language")?.as_str()?;
    if lang.is_empty() {
        return None;
    }
    let tracks = info.get("subtitles")?.as_object()?;
    let (key, formats) = tracks.get_key_value(lang).or_else(|| {
        let base = lang.split('-').next()?;
        tracks.get_key_value(base)
    })?;
    let formats = formats.as_array()?;
    for ext in ["json3", "srt", "vtt"] {
        for format in formats {
            if format.get("ext").and_then(Value::as_str) != Some(ext) {
                continue;
            }
            let Some(url) = format.get("url").and_then(Value::as_str) else {
                continue;
            };
            if url.starts_with("https://") {
                return Some((key, ext, url));
            }
        }
    }
    None
}

fn usable(cues: &[Cue], duration: Option<f64>) -> bool {
    if cues.is_empty() {
        return false;
    }
    let mut previous_end = 0.0;
    for cue in cues {
        if !cue.start.is_finite()
            || !cue.end.is_finite()
            || cue.start < 0.0
            || cue.end <= cue.start
            || cue.start + 0.05 < previous_end
            || cue.end - cue.start > 30.0
            || cue.text.trim().is_empty()
            || cue.text.chars().count() > 600
            || cue.text.contains('<')
            || cue.text.contains('>')
            || duration.is_some_and(|length| cue.end > length + 1.0)
        {
            return false;
        }
        previous_end = cue.end;
    }
    true
}

fn save_cache(
    job: &Job,
    info: &Value,
    language: &str,
    ext: &str,
    cues: &[Cue],
    cancel: &AtomicBool,
) -> Result<bool, String> {
    stopped(cancel)?;
    if job.transcript().exists() || job.lines_path().exists() {
        return Ok(false);
    }
    write_json(
        &job.root.join("transcript-source.json"),
        &serde_json::json!({
            "kind": "publisher-subtitles", "language": language,
            "video_id": info.get("id"), "format": ext,
        }),
    )?;
    // The same Cue format is consumed by preview and export; the transcript is
    // the completion marker and is committed after its provenance.
    write_json(&job.transcript(), &cues)?;
    Ok(true)
}

pub(super) fn prepare(
    app: &AppHandle,
    video: &Path,
    output: &Path,
    cancel: &AtomicBool,
) -> Result<(), String> {
    stopped(cancel)?;
    // Older downloads have no metadata. Keep their ordinary Whisper path.
    let metadata = video.with_extension("info.json");
    let Ok(info) = read_json::<Value>(&metadata) else {
        return Ok(());
    };
    let Some((language, ext, url)) = original_track(&info) else {
        return Ok(());
    };
    // Downloading alone must not fail because a different job already uses this
    // name (for example attach_video on a listen job). The run checks collisions.
    let Ok(job) = Job::open(video, output) else {
        return Ok(());
    };
    // Never replace existing transcripts/edits, even an unreadable old file.
    if job.transcript().exists() || job.lines_path().exists() {
        return Ok(());
    }
    super::download::download_report(
        app,
        "Đang lấy phụ đề gốc của video để bỏ qua Whisper.",
        None,
    );
    let result = (|| -> Result<Vec<Cue>, String> {
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(12))
            .build();
        let mut request = agent.get(url);
        if let Some(agent) = info
            .get("http_headers")
            .and_then(|h| h.get("User-Agent"))
            .and_then(Value::as_str)
        {
            request = request.set("User-Agent", agent);
        }
        let response = request.call().map_err(|error| error.to_string())?;
        let mut text = String::new();
        response
            .into_reader()
            .take(MAX_CAPTION_BYTES + 1)
            .read_to_string(&mut text)
            .map_err(|error| error.to_string())?;
        stopped(cancel)?;
        if text.len() as u64 > MAX_CAPTION_BYTES {
            return Err("Phụ đề quá lớn.".into());
        }
        let cues = cues_from_subtitle(&text, ext)?;
        if !usable(&cues, info.get("duration").and_then(Value::as_f64)) {
            return Err("Mốc hoặc nội dung phụ đề không phù hợp; dùng Whisper.".into());
        }
        Ok(cues)
    })();
    stopped(cancel)?;
    match result {
        Ok(cues) => {
            save_cache(&job, &info, language, ext, &cues, cancel)?;
            super::download::download_report(
                app,
                &format!("Đã lấy {} câu phụ đề gốc, bỏ qua Whisper.", cues.len()),
                None,
            );
        }
        Err(error) => log_line(&job, &format!("Không dùng phụ đề gốc: {error}")),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn picks_only_original_publisher_language_and_prefers_json3() {
        let info = json!({"language":"zh-Hans", "subtitles":{
            "en":[{"ext":"json3","url":"https://example.test/translated"}],
            "zh-Hans":[{"ext":"vtt","url":"https://example.test/vtt"},
                {"ext":"json3","url":"https://example.test/original"}]
        }});
        assert_eq!(
            original_track(&info),
            Some(("zh-Hans", "json3", "https://example.test/original"))
        );
        assert!(original_track(&json!({"language":"zh", "subtitles":info["subtitles"]})).is_none());
        assert!(original_track(&json!({"subtitles":info["subtitles"]})).is_none());
    }

    #[test]
    fn automatic_or_unknown_language_never_bypasses_whisper() {
        let formats = json!([{"ext":"srt","url":"https://example.test/sub"}]);
        assert!(
            original_track(&json!({"language":"en", "automatic_captions":{"en":formats}}))
                .is_none()
        );
        assert!(original_track(&json!({"language":"zh", "subtitles":{"en":formats}})).is_none());
        assert_eq!(
            original_track(&json!({"language":"en-US", "subtitles":{"en":formats}})).map(|x| x.0),
            Some("en")
        );
    }

    #[test]
    fn bad_timelines_fall_back_but_multilingual_text_survives() {
        let cues = cues_from_subtitle("1\n00:00:00,000 --> 00:00:02,000\n你好，世界。\n\n2\n00:00:02,000 --> 00:00:04,000\nHello &amp; goodbye.\n", "srt").unwrap();
        assert!(usable(&cues, Some(4.0)));
        assert_eq!(cues[0].text, "你好，世界。");
        assert!(!usable(&cues, Some(2.0)));
        let mut bad = cues.clone();
        bad[1].start = 1.0;
        assert!(!usable(&bad, None));
        bad[1].start = f64::NAN;
        assert!(!usable(&bad, None));
        assert!(!usable(&[], None));
    }

    #[test]
    fn caption_cache_is_shared_with_asr_and_existing_work_is_preserved() {
        let root = std::env::temp_dir().join(format!(
            "caption-cache-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let job = Job {
            root: root.clone(),
            video: root.join("clip.mp4"),
            picture: root.join("clip.mp4"),
            target: "vi".into(),
        };
        let cues = vec![Cue {
            start: 0.0,
            end: 2.0,
            text: "Hello there.".into(),
        }];
        let cancel = AtomicBool::new(false);
        std::fs::write(job.audio(), b"no longer needed").unwrap();
        assert!(save_cache(&job, &json!({"id":"abc"}), "en", "srt", &cues, &cancel).unwrap());
        assert_eq!(super::super::media::settle_transcript(&job).unwrap(), cues);
        assert!(!job.audio().exists());
        std::fs::write(job.transcript(), b"unreadable existing work").unwrap();
        assert!(!save_cache(&job, &json!({}), "en", "srt", &cues, &cancel).unwrap());
        assert_eq!(
            std::fs::read(job.transcript()).unwrap(),
            b"unreadable existing work"
        );
        std::fs::remove_file(job.transcript()).unwrap();
        std::fs::write(job.lines_path(), b"edited lines").unwrap();
        assert!(!save_cache(&job, &json!({}), "en", "srt", &cues, &cancel).unwrap());
        cancel.store(true, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(
            save_cache(&job, &json!({}), "en", "srt", &cues, &cancel).unwrap_err(),
            "Đã dừng."
        );
        assert!(!job.transcript().exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}
