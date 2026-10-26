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
}

pub(crate) fn annotate(result: &Path, lines: Vec<TranslatedLine>) -> Vec<ScriptLine> {
    let mut rows: Vec<_> = lines
        .into_iter()
        .map(|line| ScriptLine { line, fit: None })
        .collect();
    // Continuous audio uses another timeline. It must not inherit video warnings.
    if !result
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("mp4"))
    {
        return rows;
    }
    let Some(root) = result.parent() else {
        return rows;
    };
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
}
