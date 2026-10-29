// Small ASR fragments need a shared sentence-sized window before translation.
// Only new lines.json files use this plan; saved work and edits stay unchanged.
use crate::job::{Cue, MAX_LINE_CHARS};
use crate::timeline::{room, Slot};

const GROUP_SECONDS: f64 = 8.0;
const GROUP_GAP: f64 = 0.45;

fn sentence_end(text: &str) -> bool {
    text.trim_end_matches(|ch: char| {
        ch.is_whitespace() || matches!(ch, '"' | '\'' | '”' | '’' | '」' | '』')
    })
    .ends_with(['.', '!', '?', '。', '！', '？', '…'])
}

fn valid(cue: &Cue) -> bool {
    cue.start.is_finite() && cue.end.is_finite() && cue.start >= 0.0 && cue.end > cue.start
}

pub(super) fn group_fragments(cues: &[Cue]) -> Vec<Cue> {
    let mut groups: Vec<Cue> = Vec::new();
    for cue in cues {
        if let Some(previous) = groups.last_mut() {
            let gap = cue.start - previous.end;
            if valid(previous)
                && valid(cue)
                && (-0.02..=GROUP_GAP).contains(&gap)
                && cue.end - previous.start <= GROUP_SECONDS
                && previous.text.chars().count() + cue.text.chars().count() + 1 <= MAX_LINE_CHARS
                && !sentence_end(&previous.text)
                && !previous.text.trim().is_empty()
                && !cue.text.trim().is_empty()
            {
                previous.text.push(' ');
                previous.text.push_str(&cue.text);
                previous.end = cue.end.max(previous.end);
                continue;
            }
        }
        groups.push(cue.clone());
    }
    groups
}

pub(super) fn windows(lines: &[Cue]) -> Vec<f64> {
    let slots: Vec<Slot> = lines
        .iter()
        .map(|line| Slot {
            start: line.start,
            end: line.end,
            wav_secs: 0.0,
        })
        .collect();
    let end = lines.last().map(|line| line.end).unwrap_or(0.0);
    (0..lines.len())
        .map(|index| room(&slots, index, end))
        .collect()
}

// A soft budget, not a reason to drop a name, number or the rest of a sentence.
// Vietnamese syllables usually have spaces; other languages get seconds only.
pub(super) fn timed_prompt(
    mut prompt: String,
    indices: &[usize],
    windows: Option<&[f64]>,
    target_code: &str,
) -> String {
    let Some(windows) = windows else {
        return prompt;
    };
    prompt.push_str("\n<speech-windows>\nBản dịch dùng để lồng tiếng theo mốc. Viết gọn ngay từ đầu, bỏ từ đệm không cần thiết; không thêm lời giải thích. Giữ ý, tên riêng, số liệu và phủ định. Không bỏ dòng hoặc cắt ngang câu chỉ để vừa giới hạn.\n");
    for (number, &index) in indices.iter().enumerate() {
        let Some(&seconds) = windows.get(index) else {
            continue;
        };
        if !seconds.is_finite() || seconds <= 0.0 {
            continue;
        }
        let syllables = if target_code == "vi" {
            format!(
                ", khoảng {} tiếng (âm tiết) tiếng Việt",
                (seconds * 3.2).floor().max(1.0) as usize
            )
        } else {
            String::new()
        };
        prompt.push_str(&format!(
            "{}. Có {:.2} giây để đọc{syllables}.\n",
            number + 1,
            seconds
        ));
    }
    prompt.push_str("</speech-windows>");
    prompt
}

#[cfg(test)]
mod tests {
    use super::*;
    fn cue(start: f64, end: f64, text: &str) -> Cue {
        Cue {
            start,
            end,
            text: text.into(),
        }
    }

    #[test]
    fn packs_adjacent_fragments_without_losing_words_or_original_times() {
        let cues = vec![
            cue(0.2, 0.8, "这里"),
            cue(0.8, 1.5, "选择模型"),
            cue(1.6, 2.1, "然后运行。"),
        ];
        let packed = group_fragments(&cues);
        assert_eq!(packed, vec![cue(0.2, 2.1, "这里 选择模型 然后运行。")]);
        assert_eq!(cues[0].text, "这里");
    }

    #[test]
    fn keeps_sentence_pause_overlap_and_size_boundaries() {
        let cues = vec![
            cue(0.0, 1.0, "Hello."),
            cue(1.0, 2.0, "Next"),
            cue(3.0, 4.0, "after pause"),
            cue(3.5, 4.5, "overlap"),
        ];
        assert_eq!(group_fragments(&cues), cues);
        let cues = vec![cue(0.0, 5.0, "first"), cue(5.0, 9.0, "second")];
        assert_eq!(group_fragments(&cues), cues);
        let cues = vec![cue(0.0, 1.0, &"a".repeat(74)), cue(1.0, 2.0, "bc")];
        assert_eq!(group_fragments(&cues), cues);
    }

    #[test]
    fn timed_prompt_matches_batch_indices_and_does_not_constrain_listening() {
        let lines = vec![cue(0.0, 2.0, "a"), cue(3.0, 4.0, "b"), cue(4.0, 5.0, "c")];
        let budget = windows(&lines);
        assert_eq!(budget, vec![3.0, 1.0, 1.0]);
        let prompt = timed_prompt("translate".into(), &[2, 0], Some(&budget), "vi");
        assert!(prompt.contains("1. Có 1.00 giây"));
        assert!(prompt.contains("2. Có 3.00 giây"));
        assert!(prompt.contains("khoảng 9 tiếng"));
        assert!(!timed_prompt("translate".into(), &[0], Some(&budget), "en").contains("âm tiết"));
        assert_eq!(
            timed_prompt("translate".into(), &[0], None, "vi"),
            "translate"
        );
    }
}
