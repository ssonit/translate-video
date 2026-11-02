// Small ASR fragments need a shared sentence-sized window before translation.
// Only new lines.json files use this plan; saved work and edits stay unchanged.
// A long clock span does not cut a phrase that has nowhere to break: slicing
// 那这个是 in half is what turned 个是 into the line "là". One- and two-character
// CJK scraps may share a slightly longer window so 也 and 也许 stay one line.
use crate::job::{Cue, MAX_LINE_CHARS};
use crate::timeline::{room, Slot};

const GROUP_SECONDS: f64 = 8.0;
const GROUP_GAP: f64 = 0.45;
const SHORT_FRAGMENT_SECONDS: f64 = 12.0;
const SHORT_FRAGMENT_CHARS: usize = 2;
const MIN_CHARS_PER_TIME_PART: usize = 8;

fn cjk_char(ch: char) -> bool {
    matches!(ch as u32, 0x3400..=0x9fff | 0x3040..=0x30ff | 0xac00..=0xd7af)
}

fn text_limit(text: &str) -> usize {
    // The 75-character subtitle budget fragments spaced speech before it is
    // translated. Keep a bounded spoken phrase; subtitle layout is independent.
    if text.split_whitespace().count() >= 4 && !text.chars().any(cjk_char) {
        220
    } else {
        MAX_LINE_CHARS
    }
}

pub(super) fn parts(cue: &Cue) -> Vec<String> {
    let text = cue.text.trim();
    if text.is_empty() {
        return Vec::new();
    }
    let by_text = text.chars().count().div_ceil(text_limit(text)).max(1);
    let by_time = time_parts(cue, text);
    let count = if by_time > by_text && !crate::ollama::has_interior_break(text) {
        by_text
    } else {
        by_text.max(by_time)
    };
    crate::ollama::mechanical_split(text, count.clamp(1, 64))
}

fn time_parts(cue: &Cue, text: &str) -> usize {
    if !valid(cue) {
        return 1;
    }
    let by_clock = ((cue.end - cue.start) / GROUP_SECONDS).ceil() as usize;
    let by_length = text.chars().count() / MIN_CHARS_PER_TIME_PART;
    by_clock.min(by_length.max(1)).max(1)
}

fn sentence_end(text: &str) -> bool {
    text.trim_end_matches(|ch: char| {
        ch.is_whitespace() || matches!(ch, '"' | '\'' | '”' | '’' | '」' | '』')
    })
    .ends_with(['.', '!', '?', '。', '！', '？', '…'])
}

fn valid(cue: &Cue) -> bool {
    cue.start.is_finite() && cue.end.is_finite() && cue.start >= 0.0 && cue.end > cue.start
}

fn meaningful_chars(text: &str) -> usize {
    text.chars().filter(|ch| !ch.is_whitespace()).count()
}

fn short_cjk_fragment(text: &str) -> bool {
    let count = meaningful_chars(text);
    (1..=SHORT_FRAGMENT_CHARS).contains(&count)
        && text.chars().any(cjk_char)
        && !sentence_end(text)
}

fn same_window(previous: &Cue, cue: &Cue) -> bool {
    let span = cue.end - previous.start;
    span <= GROUP_SECONDS
        || (span <= SHORT_FRAGMENT_SECONDS
            && short_cjk_fragment(&previous.text)
            && short_cjk_fragment(&cue.text))
}

pub(super) fn group_fragments(cues: &[Cue]) -> Vec<Cue> {
    let mut groups: Vec<Cue> = Vec::new();
    for cue in cues {
        if let Some(previous) = groups.last_mut() {
            let gap = cue.start - previous.end;
            if valid(previous)
                && valid(cue)
                && (-0.02..=GROUP_GAP).contains(&gap)
                && same_window(previous, cue)
                && previous.text.chars().count() + cue.text.chars().count() + 1
                    <= text_limit(&format!("{} {}", previous.text, cue.text))
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

    #[test]
    fn spaced_speech_keeps_money_and_context_in_a_single_window() {
        let input = vec![cue(0.0, 2.0, "my business in the past twelve months has done just"),
            cue(2.0, 4.0, "under $5 million of top line revenue and it is because"),
            cue(4.0, 6.0, "everyone is able to work faster using these systems.")];
        let grouped = group_fragments(&input);
        assert_eq!(grouped.len(), 1);
        assert_eq!(parts(&grouped[0]), vec![input.iter().map(|c| c.text.as_str()).collect::<Vec<_>>().join(" ")]);
        assert_eq!((grouped[0].start, grouped[0].end), (0.0, 6.0));
    }

    #[test]
    fn long_phrases_remain_bounded_and_keep_every_word() {
        let text = "names numbers and negations must stay intact ".repeat(12);
        let c = cue(0.0, 25.0, &text);
        let pieces = parts(&c);
        assert!(pieces.len() >= 4);
        assert_eq!(pieces.join(" ").split_whitespace().collect::<Vec<_>>(), text.split_whitespace().collect::<Vec<_>>());
        let chinese = cue(0.0, 4.0, &"中".repeat(100));
        assert_eq!(parts(&chinese).len(), 2);
    }

    fn planned(cues: &[Cue]) -> Vec<String> {
        group_fragments(cues)
            .iter()
            .flat_map(|cue| parts(cue))
            .collect()
    }

    #[test]
    fn short_chinese_phrases_stay_whole_when_the_clock_span_is_long() {
        assert_eq!(parts(&cue(10.0, 20.23, "那这个是")), vec!["那这个是"]);
        assert_eq!(parts(&cue(41.19, 52.11, "怎么脸红了")), vec!["怎么脸红了"]);
        let scraps = group_fragments(&[
            cue(60.26, 62.14, "也"),
            cue(62.14, 68.5, "也许"),
        ]);
        assert_eq!(scraps, vec![cue(60.26, 68.5, "也 也许")]);
        assert_eq!(parts(&scraps[0]), vec!["也 也许"]);
        let question = vec![cue(53.59, 60.26, "还红吗"), cue(60.26, 62.14, "也")];
        assert_eq!(group_fragments(&question), question);
    }

    #[test]
    fn pearl_clip_does_not_emit_function_word_scraps() {
        let transcript = vec![
            cue(0.72, 1.76, "珍珠小姐"),
            cue(1.76, 4.2, "虽然不知道为什么你会变成这样"),
            cue(4.2, 6.64, "但是这样也好可爱哦"),
            cue(6.64, 10.0, "你在做什么呢"),
            cue(10.0, 20.23, "那这个是"),
            cue(20.23, 22.18, "我做的绘画助手"),
            cue(22.18, 23.49, "它叫猪头"),
            cue(23.49, 28.97, "怪这么可爱"),
            cue(28.97, 31.04, "它是照着我的样子"),
            cue(31.04, 35.18, "模仿看到的动作"),
            cue(35.18, 39.21, "笑了"),
            cue(39.21, 41.19, "模仿"),
            cue(41.19, 52.11, "怎么脸红了"),
            cue(52.11, 53.59, "你一直看着"),
            cue(53.59, 60.26, "还红吗"),
            cue(60.26, 62.14, "也"),
            cue(62.14, 68.5, "也许"),
        ];
        assert_eq!(
            planned(&transcript),
            vec![
                "珍珠小姐 虽然不知道为什么你会变成这样 但是这样也好可爱哦",
                "你在做什么呢",
                "那这个是",
                "我做的绘画助手 它叫猪头",
                "怪这么可爱 它是照着我的样子",
                "模仿看到的动作",
                "笑了 模仿",
                "怎么脸红了",
                "你一直看着",
                "还红吗",
                "也 也许",
            ]
        );
    }

    #[test]
    fn a_long_chinese_line_still_splits_where_punctuation_allows() {
        let text = "福姐，现在怎么办呢这件事已经说了很久，后面还有一句完整的话。";
        let pieces = parts(&cue(0.0, 24.0, text));
        assert!(pieces.len() >= 2, "{pieces:?}");
        let kept: String = pieces.iter().map(|part| part.as_str()).collect();
        assert_eq!(kept, text);
    }

    #[test]
    fn a_long_phrase_with_nowhere_to_break_stays_whole_however_long_the_clock_runs() {
        // Long enough that the character cap alone allows two parts, so only the
        // "nothing to break on" check keeps it in one piece.
        let bare = "这是一个没有任何标点的很长的中文短语啊";
        assert!(bare.chars().count() / MIN_CHARS_PER_TIME_PART >= 2);
        assert_eq!(parts(&cue(0.0, 30.0, bare)), vec![bare]);
        // A mark that only closes the phrase is not a place to cut it in two.
        let closed = format!("{bare}。");
        assert_eq!(parts(&cue(0.0, 30.0, &closed)), vec![closed.as_str()]);
    }
}
