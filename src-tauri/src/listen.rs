use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::job::{Cue, TranslatedLine};
use crate::wav::STORY_PARAGRAPH_GAP;

/// Một lần gọi model đang chọn, cùng cỡ đoạn Local-TTS dịch trước khi cho nghe.
pub(crate) const LISTEN_PART_WORDS: usize = 1000;

pub(crate) fn listen_engine() -> crate::piper::SpeakEngine {
    crate::piper::SpeakEngine::Window
}

pub(crate) const PASSAGE_MAX_LINES: usize = 4;
pub(crate) const PASSAGE_MAX_CHARS: usize = 180;
pub(crate) const PASSAGE_RENDER: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Passage {
    pub text: String,
    pub rest_before: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct ListenPlan {
    pub voice: String,
    pub passages: Vec<Passage>,
    // Bản đọc cũ cắt cụt từ cuối. Đổi số này thì đọc lại.
    #[serde(default)]
    pub render: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum SpeakSet {
    All,
    Missing(Vec<usize>),
}

pub(crate) fn speak_indexes(
    saved: &ListenPlan,
    voice: &str,
    passages: &[Passage],
    wav_ready: &[bool],
) -> SpeakSet {
    if saved.voice != voice || saved.passages != passages || saved.render != PASSAGE_RENDER {
        return SpeakSet::All;
    }
    SpeakSet::Missing(
        (0..passages.len())
            .filter(|index| wav_ready.get(*index) != Some(&true))
            .collect(),
    )
}

pub(crate) fn group_passages(lines: &[TranslatedLine]) -> Vec<Passage> {
    let mut passages: Vec<Passage> = Vec::new();
    let mut previous_end: Option<f64> = None;
    let mut count = 0usize;
    for line in lines {
        let text = line.translation.trim();
        if text.is_empty() {
            continue;
        }
        let gap = previous_end.map(|end| (line.start - end).max(0.0));
        let rest = gap.is_some_and(|gap| gap >= STORY_PARAGRAPH_GAP);
        let overflow = passages.last().is_some_and(|current| {
            count >= PASSAGE_MAX_LINES
                || current.text.chars().count() + 1 + text.chars().count() > PASSAGE_MAX_CHARS
        });
        if passages.is_empty() || rest || overflow {
            passages.push(Passage {
                text: text.to_string(),
                rest_before: rest,
            });
            count = 1;
        } else if let Some(current) = passages.last_mut() {
            current.text.push(' ');
            current.text.push_str(text);
            count += 1;
        }
        previous_end = Some(line.end);
    }
    passages
}

// Gom phụ đề thành từng phần để dịch một lần rồi nghe, không ghi wav.
pub(crate) fn pack_listen_parts(cues: &[Cue]) -> Vec<String> {
    let mut parts = Vec::new();
    let mut buf: Vec<&str> = Vec::new();
    let mut count = 0usize;
    for cue in cues {
        let text = cue.text.trim();
        if text.is_empty() {
            continue;
        }
        let words = text.split_whitespace().count();
        if count > 0 && count + words > LISTEN_PART_WORDS {
            parts.push(buf.join(" "));
            buf.clear();
            count = 0;
        }
        buf.push(text);
        count += words;
    }
    if !buf.is_empty() {
        parts.push(buf.join(" "));
    }
    parts
}

fn part_paths(dir: &Path, index: usize) -> (PathBuf, PathBuf) {
    let stem = format!("{index:04}");
    (
        dir.join(format!("{stem}.src")),
        dir.join(format!("{stem}.txt")),
    )
}

pub(crate) fn cached_part(dir: &Path, index: usize, source: &str) -> Option<String> {
    let (src, out) = part_paths(dir, index);
    let saved = std::fs::read_to_string(src).ok()?;
    if saved != source {
        return None;
    }
    let text = std::fs::read_to_string(out).ok()?;
    let text = text.trim();
    if text.is_empty() {
        None
    } else {
        Some(text.to_string())
    }
}

pub(crate) fn store_part(dir: &Path, index: usize, source: &str, text: &str) -> Result<(), String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("Bản dịch trống.".into());
    }
    std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    let (src, out) = part_paths(dir, index);
    std::fs::write(src, source).map_err(|error| error.to_string())?;
    std::fs::write(out, text).map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job::TranslatedLine;

    #[test]
    fn close_sentences_with_periods_stay_one_passage() {
        let lines = [line(0.0, 1.0, "Một."), line(1.4, 2.4, "Hai.")];
        let passages = group_passages(&lines);
        assert_eq!(passages.len(), 1);
        assert_eq!(passages[0].text, "Một. Hai.");
        assert!(!passages[0].rest_before);
    }

    #[test]
    fn a_source_pause_starts_a_rested_passage() {
        let lines = [line(0.0, 1.0, "Một."), line(1.7, 2.5, "Hai.")];
        let passages = group_passages(&lines);
        assert_eq!(passages.len(), 2);
        assert!(passages[1].rest_before);
        assert_eq!(passages[1].text, "Hai.");
    }

    #[test]
    fn the_fifth_short_line_splits_without_a_rest() {
        let lines: Vec<_> = (0..5)
            .map(|i| line(i as f64, i as f64 + 0.4, "Ờ."))
            .collect();
        let passages = group_passages(&lines);
        assert_eq!(passages.len(), 2);
        assert_eq!(passages[0].text, "Ờ. Ờ. Ờ. Ờ.");
        assert!(!passages[1].rest_before);
        assert_eq!(passages[1].text, "Ờ.");
    }

    #[test]
    fn a_line_past_the_char_cap_stays_whole() {
        let long = "à".repeat(181);
        let passages = group_passages(&[line(0.0, 1.0, &long)]);
        assert_eq!(passages.len(), 1);
        assert_eq!(passages[0].text.chars().count(), 181);
    }

    #[test]
    fn overflowing_chars_split_without_a_rest() {
        let a = "à".repeat(100);
        let b = "à".repeat(100);
        let passages = group_passages(&[line(0.0, 0.4, &a), line(0.5, 0.9, &b)]);
        assert_eq!(passages.len(), 2);
        assert!(!passages[1].rest_before);
    }

    #[test]
    fn no_spoken_lines_make_no_passages() {
        assert!(group_passages(&[line(0.0, 1.0, "  ")]).is_empty());
    }

    #[test]
    fn blank_lines_drop_and_the_gap_uses_the_kept_neighbors() {
        let lines = [
            line(0.0, 1.0, "Một."),
            line(1.0, 1.2, "  "),
            line(1.3, 2.0, "Hai."),
        ];
        let passages = group_passages(&lines);
        assert_eq!(passages.len(), 1);
        assert_eq!(passages[0].text, "Một. Hai.");
    }

    #[test]
    fn the_same_voice_and_text_speaks_only_missing_wavs() {
        let passages = vec![Passage {
            text: "Một.".into(),
            rest_before: false,
        }];
        let saved = ListenPlan {
            voice: "banmai".into(),
            passages: passages.clone(),
            render: PASSAGE_RENDER,
        };
        assert_eq!(
            speak_indexes(&saved, "banmai", &passages, &[true]),
            SpeakSet::Missing(vec![])
        );
        assert_eq!(
            speak_indexes(&saved, "banmai", &passages, &[false]),
            SpeakSet::Missing(vec![0])
        );
    }

    #[test]
    fn a_new_voice_or_a_changed_line_speaks_every_passage() {
        let passages = vec![Passage {
            text: "Một.".into(),
            rest_before: false,
        }];
        let saved = ListenPlan {
            voice: "banmai".into(),
            passages: passages.clone(),
            render: PASSAGE_RENDER,
        };
        assert_eq!(
            speak_indexes(&saved, "thientam", &passages, &[true]),
            SpeakSet::All
        );
        let changed = vec![Passage {
            text: "Hai.".into(),
            rest_before: false,
        }];
        assert_eq!(
            speak_indexes(&saved, "banmai", &changed, &[true]),
            SpeakSet::All
        );
    }

    #[test]
    fn an_older_render_speaks_every_passage() {
        let passages = vec![Passage {
            text: "Một.".into(),
            rest_before: false,
        }];
        let saved = ListenPlan {
            voice: "banmai".into(),
            passages: passages.clone(),
            render: 0,
        };
        assert_eq!(
            speak_indexes(&saved, "banmai", &passages, &[true]),
            SpeakSet::All
        );
    }

    #[test]
    fn short_cues_stay_one_listen_part() {
        let parts = pack_listen_parts(&[cue("Một hai."), cue("Ba bốn.")]);
        assert_eq!(parts, vec!["Một hai. Ba bốn.".to_string()]);
    }

    #[test]
    fn the_cue_that_crosses_one_thousand_words_starts_a_part() {
        let parts = pack_listen_parts(&[cue(&words(600)), cue(&words(600))]);
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].split_whitespace().count(), 600);
        assert_eq!(parts[1].split_whitespace().count(), 600);
    }

    #[test]
    fn a_cue_past_the_word_cap_stays_one_part() {
        let parts = pack_listen_parts(&[cue(&words(1001))]);
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].split_whitespace().count(), 1001);
    }

    #[test]
    fn blank_cues_drop_out_of_listen_parts() {
        assert!(pack_listen_parts(&[cue("  "), cue("")]).is_empty());
    }

    #[test]
    fn a_cached_part_is_kept_only_for_the_same_source() {
        let dir = std::env::temp_dir().join(format!("preview-parts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        store_part(&dir, 1, "nguồn", "dịch").unwrap();
        assert_eq!(cached_part(&dir, 1, "nguồn").as_deref(), Some("dịch"));
        assert!(cached_part(&dir, 1, "khác").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn cue(text: &str) -> Cue {
        Cue {
            start: 0.0,
            end: 1.0,
            text: text.to_string(),
        }
    }

    fn words(count: usize) -> String {
        vec!["một"; count].join(" ")
    }

    fn line(start: f64, end: f64, translation: &str) -> TranslatedLine {
        TranslatedLine {
            start,
            end,
            source: String::new(),
            translation: translation.to_string(),
        }
    }
}
