use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::job::{read_json, TranslatedLine};
use crate::timeline::Placement;

// Keep the original array and fields so older cut reports remain readable.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct FitEntry {
    line: usize,
    text: String,
    spoken: f64,
    kept: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    speed: Option<f64>,
}

impl FitEntry {
    pub(crate) fn placed(
        line: usize,
        text: &str,
        wav_secs: f64,
        place: &Placement,
    ) -> Option<Self> {
        if place.keep.is_none() && place.speed <= 1.001 {
            return None;
        }
        let spoken = wav_secs / place.speed;
        Some(Self {
            line,
            text: text.into(),
            spoken,
            kept: place.keep.unwrap_or(spoken),
            speed: Some(place.speed),
        })
    }

    pub(crate) fn is_cut(&self) -> bool {
        self.spoken - self.kept > 0.02
    }

    fn warning(&self) -> Option<FitWarning> {
        if !self.spoken.is_finite()
            || !self.kept.is_finite()
            || self.spoken <= 0.0
            || self.kept < 0.0
            || self.kept > self.spoken + 0.02
            || self
                .speed
                .is_some_and(|speed| !speed.is_finite() || speed < 1.0)
        {
            return None;
        }
        let cut_seconds = if self.is_cut() {
            self.spoken - self.kept
        } else {
            0.0
        };
        let speed = self.speed.filter(|speed| *speed > 1.001);
        (cut_seconds > 0.0 || speed.is_some()).then_some(FitWarning { cut_seconds, speed })
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct FitWarning {
    cut_seconds: f64,
    speed: Option<f64>,
}

#[derive(Serialize)]
pub(crate) struct ScriptLine {
    #[serde(flatten)]
    line: TranslatedLine,
    #[serde(skip_serializing_if = "Option::is_none")]
    fit: Option<FitWarning>,
    #[serde(skip_serializing_if = "Option::is_none")]
    missing_audio: Option<bool>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    captions: Vec<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    revision: Option<String>,
    id: String,
    sources: Vec<usize>,
    structure_undo: bool,
    structure_undo_blocked: bool,
}

pub(crate) fn annotate(result: &Path, lines: Vec<TranslatedLine>) -> Vec<ScriptLine> {
    let revision = result.parent().and_then(|root| crate::timing::revision(root).ok());
    let document = result.parent().and_then(|root| crate::editing::Editing::load(root).ok().flatten());
    // Audio follows a different timeline; only a video with burnt captions gets
    // reading warnings. No ffprobe, network, or write is needed to inspect cues.
    let has_captions = result.file_name().is_some_and(|name|
        name.to_string_lossy().to_ascii_lowercase().ends_with(".chu.mp4"));
    let mut caption_notes = if has_captions {
        crate::pipeline::review_captions(&lines).into_iter()
    } else { vec![Vec::new(); lines.len()].into_iter() };
    let mut rows: Vec<_> = lines
        .into_iter()
        .enumerate()
        .map(|(index, line)| ScriptLine {
            line,
            fit: None,
            missing_audio: None,
            captions: caption_notes.next().unwrap_or_default(),
            revision: revision.clone(),
            id: document.as_ref().and_then(|d| d.cues.get(index)).map(|c| c.id.clone()).unwrap_or_else(|| crate::editing::legacy_id(index)),
            sources: document.as_ref().and_then(|d| d.cues.get(index)).map(|c| c.sources.clone()).unwrap_or_else(|| vec![index]),
            structure_undo: document.as_ref().is_some_and(|d| d.can_undo()),
            structure_undo_blocked: document.as_ref().is_some_and(|d| d.undo_blocked()),
        })
        .collect();
    let Some(root) = result.parent() else {
        return rows;
    };
    let text_only = read_json::<crate::pipeline::RunSettings>(&root.join("settings.json"))
        .ok()
        .is_some_and(|settings| settings.text_only);
    // These are source WAVs for rebuilding, not evidence that the finished
    // video is silent. Continuous listening uses passage WAVs, not line WAVs.
    if !text_only && !crate::job::is_listen_audio(result) {
        for (index, row) in rows.iter_mut().enumerate() {
            if row.line.translation.trim().is_empty() {
                continue;
            }
            let path = crate::editing::wav_path(root, index + 1);
            let missing = match std::fs::metadata(path) {
                Ok(meta) => !meta.is_file() || meta.len() == 0,
                Err(error) => error.kind() == std::io::ErrorKind::NotFound,
            };
            row.missing_audio = missing.then_some(true);
        }
    }
    // Continuous audio uses another timeline. It must not inherit video warnings.
    if !result
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("mp4"))
    {
        return rows;
    }
    let entries: Vec<FitEntry> = read_json(&root.join("fit-report.json")).unwrap_or_default();
    for entry in entries {
        let Some(index) = entry.line.checked_sub(1) else {
            continue;
        };
        let Some(row) = rows.get_mut(index) else {
            continue;
        };
        // An old report must not mark newly edited text at the same index.
        if row.line.translation.trim() == entry.text.trim() {
            row.fit = entry.warning();
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timeline::{place_all, Slot};

    #[test]
    fn caption_review_is_read_only_and_only_attached_to_captioned_video() {
        let root = std::env::temp_dir().join(format!("caption-review-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let saved = root.join("translation.json");
        let lines = vec![TranslatedLine {
            start: 0.0, end: 0.2, source: "Original".into(),
            translation: "Câu này rất dài so với thời gian hiển thị".into(),
        }];
        crate::job::write_json(&saved, &lines).unwrap();
        let before = std::fs::read(&saved).unwrap();
        let rows = annotate(&root.join("test.chu.mp4"), lines.clone());
        assert_eq!(rows[0].captions, vec!["short_duration", "dense_text"]);
        assert_eq!(serde_json::to_value(&rows).unwrap()[0]["captions"][1], "dense_text");
        for name in ["test.mp4", "test.m4a", "test.nghe.m4a"] {
            let rows = annotate(&root.join(name), lines.clone());
            assert!(rows[0].captions.is_empty());
            assert!(serde_json::to_value(&rows).unwrap()[0].get("captions").is_none());
        }
        assert_eq!(std::fs::read(&saved).unwrap(), before);
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        std::fs::remove_file(saved).unwrap();
        std::fs::remove_dir(root).unwrap();
    }

    #[test]
    fn reports_speed_without_inventing_a_cut() {
        let slot = Slot {
            start: 0.0,
            end: 1.0,
            wav_secs: 1.04,
        };
        let place = place_all(&[slot], 1.0)[0];
        let entry = FitEntry::placed(1, "Chào", slot.wav_secs, &place).unwrap();
        let warning = entry.warning().unwrap();
        assert_eq!(warning.cut_seconds, 0.0);
        assert!((warning.speed.unwrap() - 1.04).abs() < 1e-6);
        assert!(!entry.is_cut());
    }

    #[test]
    fn reads_legacy_cuts_without_guessing_the_speed() {
        let entry: FitEntry =
            serde_json::from_str(r#"{"line":1,"text":"Chào","spoken":3.0,"kept":1.0}"#).unwrap();
        let warning = entry.warning().unwrap();
        assert_eq!(warning.cut_seconds, 2.0);
        assert_eq!(warning.speed, None);
    }

    #[test]
    fn records_real_cuts_and_omits_natural_speed() {
        let slots = [Slot {
            start: 0.0,
            end: 1.0,
            wav_secs: 3.0,
        }];
        let place = place_all(&slots, 1.0)[0];
        let entry = FitEntry::placed(1, "Chào", 3.0, &place).unwrap();
        let json = serde_json::to_string(&entry).unwrap();
        let saved: FitEntry = serde_json::from_str(&json).unwrap();
        let warning = saved.warning().unwrap();
        assert!((warning.cut_seconds - (3.0 / place.speed - 1.0)).abs() < 1e-6);
        assert_eq!(warning.speed, Some(place.speed));
        let natural = place_all(
            &[Slot {
                wav_secs: 0.8,
                ..slots[0]
            }],
            1.0,
        )[0];
        assert!(FitEntry::placed(1, "Chào", 0.8, &natural).is_none());
    }

    #[test]
    fn annotates_only_matching_video_lines_and_tolerates_bad_reports() {
        let root = std::env::temp_dir().join(format!("fit-report-review-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let report = root.join("fit-report.json");
        let lines = vec![TranslatedLine {
            start: 0.0,
            end: 1.0,
            source: "Hi".into(),
            translation: "Chào".into(),
        }];
        assert!(annotate(&root.join("test.mp4"), lines.clone())[0]
            .fit
            .is_none());
        std::fs::write(&report, r#"[{"line":0,"text":"Chào","spoken":3,"kept":1},{"line":2,"text":"Chào","spoken":3,"kept":1},{"line":1,"text":"Cũ","spoken":3,"kept":1}]"#).unwrap();
        assert!(annotate(&root.join("test.mp4"), lines.clone())[0]
            .fit
            .is_none());
        std::fs::write(&report, r#"[{"line":1,"text":"Chào","spoken":3,"kept":1}]"#).unwrap();
        assert_eq!(
            annotate(&root.join("test.mp4"), lines.clone())[0]
                .fit
                .as_ref()
                .unwrap()
                .cut_seconds,
            2.0
        );
        assert!(annotate(&root.join("test.m4a"), lines.clone())[0]
            .fit
            .is_none());
        std::fs::write(&report, "broken").unwrap();
        assert!(annotate(&root.join("test.mp4"), lines)[0].fit.is_none());
        std::fs::remove_file(report).unwrap();
        std::fs::remove_dir(root).unwrap();
    }

    #[test]
    fn missing_line_audio_is_separate_from_blank_text_and_fit_warnings() {
        let root = std::env::temp_dir().join(format!("missing-line-audio-{}", std::process::id()));
        let wavs = root.join("lines");
        std::fs::create_dir_all(&wavs).unwrap();
        let line = TranslatedLine {
            start: 0.0,
            end: 1.0,
            source: "Hi".into(),
            translation: "Chào".into(),
        };
        let lines = vec![
            line.clone(),
            TranslatedLine {
                translation: " ".into(),
                ..line.clone()
            },
            line.clone(),
            line,
        ];
        crate::wav::write_wav(
            &wavs.join("003.wav"),
            &crate::wav::Pcm {
                rate: 22050,
                samples: vec![1; 2205],
            },
        )
        .unwrap();
        std::fs::write(wavs.join("004.wav"), b"").unwrap();
        let rows = annotate(&root.join("test.mp4"), lines.clone());
        assert_eq!(rows[0].missing_audio, Some(true));
        assert_eq!(rows[1].missing_audio, None);
        assert_eq!(rows[2].missing_audio, None);
        assert_eq!(rows[3].missing_audio, Some(true));
        assert!(rows.iter().all(|row| row.fit.is_none()));
        let json = serde_json::to_value(&rows[2]).unwrap();
        assert!(json.get("missing_audio").is_none());
        assert_eq!(json["translation"], "Chào");
        assert_eq!(
            annotate(&root.join("test.m4a"), lines.clone())[0].missing_audio,
            Some(true)
        );
        assert!(annotate(&root.join("test.nghe.m4a"), lines.clone())
            .iter()
            .all(|row| row.missing_audio.is_none()));
        std::fs::write(
            root.join("settings.json"),
            r#"{"target":"vi","voice":"test","text_only":true}"#,
        )
        .unwrap();
        assert!(annotate(&root.join("test.chu.mp4"), lines)
            .iter()
            .all(|row| row.missing_audio.is_none()));
        // Annotation only reads the job; it does not synthesize or replace files.
        assert!(!wavs.join("001.wav").exists());
        assert!(!wavs.join("002.wav").exists());
        assert_eq!(std::fs::metadata(wavs.join("004.wav")).unwrap().len(), 0);
        std::fs::remove_file(wavs.join("003.wav")).unwrap();
        std::fs::remove_file(wavs.join("004.wav")).unwrap();
        std::fs::remove_file(root.join("settings.json")).unwrap();
        std::fs::remove_dir(wavs).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}
