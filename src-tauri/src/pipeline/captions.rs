use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;
use unicode_segmentation::UnicodeSegmentation;

use super::media::{parse_clock, run_tool};

static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

pub(super) struct PreparedCaptions(pub PathBuf);

impl Drop for PreparedCaptions {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

// Only the burn overlay is reformatted. Saved SRT, translations and speech caches stay intact.
pub(super) fn prepare(
    video: &Path,
    srt: &Path,
    cancel: &AtomicBool,
) -> Result<PreparedCaptions, String> {
    let (width, height) = probe_dimensions(video, cancel)?;
    prepare_for(
        srt,
        width,
        height,
        &crate::render::RenderSettings::read(srt.parent().unwrap_or(Path::new(".")))?,
    )
}

pub(super) fn probe_dimensions(video: &Path, cancel: &AtomicBool) -> Result<(u32, u32), String> {
    let mut command = Command::new("ffprobe");
    command
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height,sample_aspect_ratio:stream_tags=rotate:stream_side_data=rotation",
            "-of",
            "json",
        ])
        .arg(video);
    let probe = run_tool(command, Duration::from_secs(20), cancel)?;
    dimensions(&probe)
}

pub(super) fn prepare_for(
    srt: &Path,
    width: u32,
    height: u32,
    settings: &crate::render::RenderSettings,
) -> Result<PreparedCaptions, String> {
    let text = std::fs::read_to_string(srt).map_err(|e| e.to_string())?;
    let overlay = render_styled(&text, width, height, settings)?;
    let dir = srt.parent().ok_or("Chưa thấy folder phụ đề.")?;
    let name = format!(
        "caption-layout-{}-{}.ass",
        std::process::id(),
        NEXT_FILE.fetch_add(1, Ordering::Relaxed)
    );
    let path = dir.join(name);
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|e| e.to_string())?;
    let prepared = PreparedCaptions(path);
    file.write_all(overlay.as_bytes())
        .map_err(|e| e.to_string())?;
    Ok(prepared)
}

fn dimensions(probe: &str) -> Result<(u32, u32), String> {
    let json: serde_json::Value = serde_json::from_str(probe).map_err(|e| e.to_string())?;
    let stream = &json["streams"][0];
    let width = stream["width"].as_u64().unwrap_or(0) as u32;
    let height = stream["height"].as_u64().unwrap_or(0) as u32;
    if width == 0 || height == 0 {
        return Err("Không đọc được kích thước video để đặt phụ đề.".into());
    }
    let sar = stream["sample_aspect_ratio"].as_str().unwrap_or("1:1");
    let ratio = sar
        .split_once(':')
        .and_then(|(n, d)| {
            let n = n.parse::<f64>().ok()?;
            let d = d.parse::<f64>().ok()?;
            (n > 0.0 && d > 0.0).then_some(n / d)
        })
        .unwrap_or(1.0);
    let width = (width as f64 * ratio).round().max(1.0) as u32;
    let rotation = stream["side_data_list"]
        .as_array()
        .and_then(|items| items.iter().find_map(|item| item["rotation"].as_f64()))
        .or_else(|| {
            stream["tags"]["rotate"]
                .as_str()
                .and_then(|r| r.parse().ok())
        })
        .unwrap_or(0.0);
    if (rotation.round() as i64).rem_euclid(180) == 90 {
        Ok((height, width))
    } else {
        Ok((width, height))
    }
}

fn units(ch: char) -> f64 {
    match ch {
        '\u{0300}'..='\u{036f}' => 0.0,
        ' '..='~' if "ilI.,:;'!|".contains(ch) => 0.35,
        ' '..='~' if "MW@%".contains(ch) => 1.0,
        '\u{2e80}'..='\u{9fff}'
        | '\u{ac00}'..='\u{d7af}'
        | '\u{f900}'..='\u{faff}'
        | '\u{ff00}'..='\u{ffef}'
        | '\u{1f000}'..='\u{1ffff}' => 1.1,
        _ => 0.72,
    }
}

fn wrap(text: &str, limit: f64) -> Vec<String> {
    // Every row is an exact contiguous slice. Whitespace belongs to a row too;
    // do not insert spaces inside long words, URLs or unspaced writing systems.
    let clusters: Vec<_> = text.grapheme_indices(true).collect();
    let mut rows = Vec::new();
    let mut first = 0;
    while first < clusters.len() {
        let mut last = first;
        let mut used = 0.0;
        let mut boundary = None;
        let mut punctuation = None;
        while last < clusters.len() {
            let cluster = clusters[last].1;
            let next = cluster_width(cluster);
            if last > first && used + next > limit && !cluster.chars().all(char::is_whitespace) {
                break;
            }
            used += next;
            last += 1;
            if cluster.chars().all(char::is_whitespace) {
                boundary = Some(last);
            }
            if cluster.chars().any(|c| ",;:!?。！？，；：".contains(c)) && used >= limit * 0.6
            {
                punctuation = Some(last);
            }
        }
        if last < clusters.len() {
            // Prefer punctuation near the end, then a whole-word boundary.
            last = punctuation.or(boundary).unwrap_or(last);
        }
        let from = clusters[first].0;
        let to = clusters.get(last).map(|c| c.0).unwrap_or(text.len());
        rows.push(text[from..to].to_string());
        first = last;
    }
    rows
}

fn cluster_width(cluster: &str) -> f64 {
    // An emoji family/flag occupies one glyph group, not one cell per codepoint.
    if cluster.contains('\u{200d}')
        || cluster
            .chars()
            .any(|c| ('\u{1f000}'..='\u{1ffff}').contains(&c))
    {
        return 1.1;
    }
    cluster.chars().map(units).sum()
}

fn escape(text: &str) -> String {
    let mut safe = String::new();
    for ch in text.chars() {
        match ch {
            // A zero-width WORD JOINER stops literal \N/\n/\h becoming ASS
            // controls. libass understands escaped braces as literal glyphs.
            '\\' => safe.push_str("\\\u{2060}"),
            '{' => safe.push_str("\\{"),
            '}' => safe.push_str("\\}"),
            _ => safe.push(ch),
        }
    }
    safe
}

#[derive(Debug)]
struct CaptionSegment {
    start_cs: u64,
    end_cs: u64,
    rows: Vec<String>,
}

fn layout(text: &str, start: f64, end: f64, limit: f64) -> Result<Vec<CaptionSegment>, String> {
    if !start.is_finite()
        || !end.is_finite()
        || start < 0.0
        || end <= start
        || end > 3_600_000_000.0
    {
        return Err("mốc bắt đầu/kết thúc không hợp lệ".into());
    }
    // Only whitespace is normalized, before splitting and before ASS escaping.
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return Ok(Vec::new());
    }
    if normalized.chars().any(|c| c.is_control()) {
        return Err("chữ chứa ký tự điều khiển không thể ghi an toàn".into());
    }
    let rows = wrap(&normalized, limit);
    if rows.concat() != normalized {
        return Err("chia phụ đề không giữ đủ nội dung".into());
    }
    // ASS uses centiseconds: round inward so display never borrows time from
    // the adjacent cue. Epsilon only cancels binary floating-point noise.
    let start_cs = (start * 100.0 - 1e-7).ceil() as u64;
    let end_cs = (end * 100.0 + 1e-7).floor() as u64;
    let groups: Vec<_> = rows.chunks(2).collect();
    let count = groups.len() as u64;
    if end_cs.saturating_sub(start_cs) < count {
        return Err("thời gian quá ngắn để hiện đủ chữ; hãy rút gọn lời hoặc chỉnh mốc câu".into());
    }
    let weights: Vec<_> = groups
        .iter()
        .map(|rows| {
            rows.iter()
                .map(|s| {
                    s.graphemes(true)
                        .filter(|g| !g.chars().all(char::is_whitespace))
                        .count()
                })
                .sum::<usize>()
                .max(1) as u64
        })
        .collect();
    let total: u64 = weights.iter().sum();
    let spare = end_cs - start_cs - count;
    let mut consumed = 0u64;
    let mut from = start_cs;
    let mut segments = Vec::new();
    for (index, rows) in groups.into_iter().enumerate() {
        consumed += weights[index];
        // Reserve at least one ASS centisecond for EVERY segment. Proportional
        // rounding alone could produce zero-length events and silently drop text.
        let to = start_cs
            + index as u64
            + 1
            + ((spare as u128 * consumed as u128) / total as u128) as u64;
        segments.push(CaptionSegment {
            start_cs: from,
            end_cs: to,
            rows: rows.to_vec(),
        });
        from = to;
    }
    Ok(segments)
}

// Advisory checks use source cue durations, not a claim of exact word alignment
// or font measurement. Read-only, shared by old and newly exported video jobs.
pub(crate) fn review(lines: &[crate::job::TranslatedLine]) -> Vec<Vec<&'static str>> {
    let mut warnings = vec![Vec::new(); lines.len()];
    let mut ordered = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if !line.start.is_finite()
            || !line.end.is_finite()
            || line.start < 0.0
            || line.end <= line.start
            || line.end > 3_600_000_000.0
        {
            warnings[index].push("invalid_timing");
            continue;
        }
        if line.translation.trim().is_empty() {
            continue;
        }
        ordered.push(index);
        let chars = line
            .translation
            .graphemes(true)
            .filter(|g| !g.chars().all(char::is_whitespace))
            .count();
        let duration = line.end - line.start;
        if chars > 0 && duration < 0.65 {
            warnings[index].push("short_duration");
        }
        if chars as f64 / duration > 25.0 {
            warnings[index].push("dense_text");
        }
    }
    // Inspect actual intervals, even when a legacy file isn't sorted. Preserve
    // original indexes so review navigation and retake still target the same row.
    ordered.sort_by(|a, b| lines[*a].start.total_cmp(&lines[*b].start));
    let mut latest: Option<(usize, f64)> = None;
    for index in ordered {
        let line = &lines[index];
        if let Some((previous, end)) = latest {
            if line.start < end - 0.005 {
                if !warnings[previous].contains(&"overlap") {
                    warnings[previous].insert(0, "overlap");
                }
                if !warnings[index].contains(&"overlap") {
                    warnings[index].insert(0, "overlap");
                }
            }
        }
        if latest.is_none_or(|(_, end)| line.end > end) {
            latest = Some((index, line.end));
        }
    }
    warnings
}

fn stamp(cs: u64) -> String {
    format!(
        "{}:{:02}:{:02}.{:02}",
        cs / 360_000,
        cs / 6000 % 60,
        cs / 100 % 60,
        cs % 100
    )
}

fn caption_clock(text: &str) -> Option<f64> {
    let raw = text.trim();
    let hms = raw.split([',', '.']).next()?;
    let fields: Vec<_> = hms.split(':').collect();
    if fields.len() != 3
        || fields
            .iter()
            .any(|s| s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    if fields[1].parse::<u32>().ok()? >= 60 || fields[2].parse::<u32>().ok()? >= 60 {
        return None;
    }
    if let Some((_, fraction)) = raw.split_once([',', '.']) {
        if fraction.is_empty()
            || fraction.len() > 3
            || !fraction.bytes().all(|b| b.is_ascii_digit())
        {
            return None;
        }
    }
    parse_clock(raw)
}

#[cfg(test)]
fn render(srt: &str, width: u32, height: u32) -> Result<String, String> {
    render_styled(srt, width, height, &Default::default())
}

fn render_styled(
    srt: &str,
    width: u32,
    height: u32,
    settings: &crate::render::RenderSettings,
) -> Result<String, String> {
    settings.validate()?;
    if width == 0 || height == 0 {
        return Err("Khung phụ đề phải có kích thước lớn hơn 0.".into());
    }
    let font = (width.min(height) as f64 * settings.size / 100.0).max(1.0);
    let margin_x = (width as f64 * 0.07).round() as u32;
    let margin_y = (height as f64 * if height > width { 0.10 } else { 0.065 }).round() as u32;
    let outline = settings.outline * font / 45.36;
    let ass_color = |raw: &str| format!("&H00{}{}{}", &raw[5..7], &raw[3..5], &raw[1..3]);
    let primary = ass_color(&settings.color);
    let edge = ass_color(&settings.outline_color);
    let font_name = &settings.font;
    let border = if settings.background { 3 } else { 1 };
    let limit =
        (width as f64 * 2.0 * (settings.caption_x.min(100.0 - settings.caption_x) / 100.0 - 0.07))
            .max(font)
            / font;
    let mut ass = format!("[Script Info]\nScriptType: v4.00+\nPlayResX: {width}\nPlayResY: {height}\nWrapStyle: 2\nScaledBorderAndShadow: yes\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,{font_name},{font:.2},{primary},{primary},{edge},&H80000000,0,0,0,0,100,100,0,0,{border},{outline:.2},0,2,{margin_x},{margin_x},{margin_y},1\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n");
    let normalized = srt.replace("\r\n", "\n").replace('\r', "\n");
    let mut count = 0;
    for (block_index, block) in normalized
        .split("\n\n")
        .filter(|b| !b.trim().is_empty())
        .enumerate()
    {
        let invalid =
            |reason: &str| format!("Phụ đề {}: {reason}. Chưa ghi lại video.", block_index + 1);
        let lines: Vec<_> = block.lines().collect();
        let Some(index) = lines.iter().position(|line| line.contains("-->")) else {
            return Err(invalid("không đọc được dòng thời gian"));
        };
        let Some((start, end)) = lines[index].split_once("-->") else {
            return Err(invalid("không đọc được dòng thời gian"));
        };
        let (Some(start), Some(end)) = (caption_clock(start), caption_clock(end)) else {
            return Err(invalid("không đọc được mốc thời gian"));
        };
        if lines[index + 1..].iter().any(|line| line.contains("-->")) {
            return Err(invalid("thiếu dòng trống giữa hai câu phụ đề"));
        }
        let segments = layout(&lines[index + 1..].join(" "), start, end, limit)
            .map_err(|reason| invalid(&reason))?;
        for segment in segments {
            ass.push_str(&format!(
                "Dialogue: 0,{},{},Default,,0,0,0,,{}\n",
                stamp(segment.start_cs),
                stamp(segment.end_cs),
                format!(
                    "{{\\an2\\pos({:.1},{:.1})}}{}",
                    width as f64 * settings.caption_x / 100.0,
                    height as f64 * settings.caption_y / 100.0,
                    segment
                        .rows
                        .iter()
                        .map(|row| escape(row.trim()))
                        .collect::<Vec<_>>()
                        .join("\\N")
                )
            ));
            count += 1;
        }
    }
    if count == 0 {
        return Err("Chưa có câu phụ đề hợp lệ để ghi lên video.".into());
    }
    Ok(ass)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_style_uses_export_coordinates_and_safe_ass_colors() {
        let style = crate::render::RenderSettings {
            font: "Tahoma".into(),
            size: 5.0,
            color: "#ffcc00".into(),
            outline_color: "#123456".into(),
            background: true,
            caption_x: 40.0,
            caption_y: 72.0,
            ..Default::default()
        };
        let ass = render_styled(
            "1\n00:00:00,000 --> 00:00:03,000\nXin chào\n",
            1080,
            1920,
            &style,
        )
        .unwrap();
        assert!(ass.contains("Tahoma,54.00,&H0000ccff"));
        assert!(ass.contains("&H00563412"));
        assert!(ass.contains("\\pos(432.0,1382.4)"));
        assert!(ass.contains(",100,100,0,0,3,"));
    }

    #[test]
    fn dimensions_follow_rotation_and_pixel_aspect() {
        assert_eq!(
            dimensions(
                r#"{"streams":[{"width":1920,"height":1080,"side_data_list":[{"rotation":-90}]}]}"#
            )
            .unwrap(),
            (1080, 1920)
        );
        assert_eq!(
            dimensions(r#"{"streams":[{"width":720,"height":576,"sample_aspect_ratio":"16:15"}]}"#)
                .unwrap(),
            (768, 576)
        );
        assert!(dimensions(r#"{"streams":[]}"#).is_err());
    }

    #[test]
    fn long_captions_keep_all_words_and_stay_inside_original_time() {
        let text = "Đây là câu phụ đề rất dài để kiểm tra video dọc và giữ đầy đủ tất cả những từ đã dịch trong câu gốc.";
        let ass = render(
            &format!("1\n00:00:02,000 --> 00:00:10,000\n{text}\n"),
            1080,
            1920,
        )
        .unwrap();
        let events: Vec<_> = ass
            .lines()
            .filter(|line| line.starts_with("Dialogue:"))
            .collect();
        assert!(events.len() > 1);
        assert!(events[0].starts_with("Dialogue: 0,0:00:02.00,"));
        assert!(events.last().unwrap().contains(",0:00:10.00,"));
        let mut recovered = Vec::new();
        for event in events {
            let text = event.splitn(10, ',').nth(9).unwrap();
            let text = text.split_once('}').map(|(_, text)| text).unwrap_or(text);
            assert!(text.matches("\\N").count() <= 1);
            recovered.push(text.replace("\\N", " "));
        }
        assert_eq!(recovered.join(" "), text);
    }

    #[test]
    fn square_and_portrait_use_short_edge_for_font_and_explicit_canvas() {
        for (w, h) in [(1920, 1080), (1080, 1920), (1080, 1080)] {
            let ass = render("1\n00:00:00,000 --> 00:00:03,000\nXin chào\n", w, h).unwrap();
            assert!(ass.contains(&format!("PlayResX: {w}\nPlayResY: {h}")));
            assert!(ass.contains("Arial,45.36,"));
        }
        let rows = wrap("汉字字幕没有空格也必须换行", 5.0);
        assert!(rows
            .iter()
            .all(|row| row.chars().map(units).sum::<f64>() <= 5.0));
        assert_eq!(rows.concat(), "汉字字幕没有空格也必须换行");
        assert_eq!(escape(r"{\pos(0,0)}"), "\\{\\\u{2060}pos(0,0)\\}");
    }

    #[test]
    fn unicode_rows_are_exact_slices_and_never_split_graphemes() {
        for text in [
            "Tie\u{302}\u{301}ng Vie\u{323}\u{302}t có dấu tổ hợp 👨‍👩‍👧‍👦 🇻🇳 👍🏽",
            "汉字字幕没有空格也必须完整。下一句话。",
            "日本語の字幕です。韓国語 한국어 자막입니다",
            "Toronto Niagara 1.250.000đ 28/10/2026 https://example.com/verylongpath",
            "Một          câu\n có\t khoảng trắng",
        ] {
            let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
            for limit in [1.0, 5.0, 18.0, 40.0] {
                let segments = layout(text, 2.0, 120.0, limit).unwrap();
                let recovered: String = segments.iter().flat_map(|s| &s.rows).cloned().collect();
                assert_eq!(recovered, normalized);
                let mut offset = 0;
                let boundaries: Vec<_> = normalized
                    .grapheme_indices(true)
                    .map(|(i, _)| i)
                    .chain(std::iter::once(normalized.len()))
                    .collect();
                for row in segments.iter().flat_map(|s| &s.rows) {
                    offset += row.len();
                    assert!(boundaries.contains(&offset), "split grapheme at {offset}");
                    assert!(!row.trim().is_empty());
                }
            }
        }
    }

    #[test]
    fn every_segment_gets_time_or_export_fails_instead_of_dropping_text() {
        let text = "Một câu dài có rất nhiều chữ cần giữ nguyên khi chia đoạn.";
        let count = layout(text, 0.0, 10.0, 2.0).unwrap().len();
        let segments = layout(text, 0.0, count as f64 / 100.0, 2.0).unwrap();
        assert_eq!(segments.len(), count);
        assert!(segments.iter().all(|s| s.end_cs == s.start_cs + 1));
        assert!(segments.windows(2).all(|w| w[0].end_cs == w[1].start_cs));
        assert!(layout(text, 0.0, (count - 1) as f64 / 100.0, 2.0).is_err());
        let precise = layout("Chào", 1.001, 1.999, 10.0).unwrap();
        assert_eq!((precise[0].start_cs, precise[0].end_cs), (101, 199));
        assert!(layout("Chào", 1.001, 1.009, 10.0).is_err());
        for (start, end) in [
            (f64::NAN, 1.0),
            (0.0, f64::INFINITY),
            (-1.0, 2.0),
            (2.0, 1.0),
            (1.0, 1.0),
        ] {
            assert!(layout("Chào", start, end, 10.0).is_err());
        }
    }

    #[test]
    fn malformed_blocks_are_reported_even_if_another_cue_is_valid() {
        let valid = "1\n00:00:00,000 --> 00:00:01,000\nChào\n\n";
        for bad in [
            "2\n00:00:02,000 --> 00:00:01,000\nSai mốc",
            "2\n-00:00:02,000 --> 00:00:03,000\nSai mốc",
            "2\n00:61:00,000 --> 02:00:00,000\nSai phút",
            "2\nKhông có mốc\nKhông được bỏ qua câu này",
            "2\n00:00:02,abc --> 00:00:03,000\nSai phần lẻ",
        ] {
            let error = render(&format!("{valid}{bad}"), 1080, 1920).unwrap_err();
            assert!(error.contains("Phụ đề 2:"), "{error}");
        }
    }

    #[test]
    fn overlapping_parent_cues_are_kept_without_moving_or_deleting_them() {
        let ass = render("1\n00:00:01,000 --> 00:00:03,000\nCâu một\n\n2\n00:00:02,000 --> 00:00:04,000\nCâu hai", 1080, 1920).unwrap();
        assert!(ass.contains("0:00:01.00,0:00:03.00"));
        assert!(ass.contains("0:00:02.00,0:00:04.00"));
        assert!(ass.contains("Câu một") && ass.contains("Câu hai"));
    }

    #[test]
    fn literal_ass_controls_are_escaped_after_layout_without_changing_source() {
        let text = r"C:\New\name {\pos(0,0)} https://example.com/a";
        let segments = layout(text, 0.0, 4.0, 200.0).unwrap();
        assert_eq!(segments[0].rows.concat(), text);
        let safe = escape(text);
        assert!(!safe.contains(r"\N"));
        assert!(!safe.contains(r"\n"));
        assert!(!safe.contains(r"{\pos"));
        assert!(safe.contains("\\{"));
        assert!(!safe.contains('｛') && !safe.contains('＼'));
    }

    #[test]
    fn review_marks_both_overlapping_cues_and_uses_graphemes_for_reading_load() {
        let make = |start, end, text: &str| crate::job::TranslatedLine {
            start,
            end,
            source: "Original".into(),
            translation: text.into(),
        };
        let notes = review(&[
            make(0.0, 4.0, "Chào"),
            make(1.0, 1.2, "Chữ dày và không đủ thời gian đọc"),
            make(2.0, 3.0, "Tie\u{302}\u{301}ng"),
            make(-1.0, 2.0, "Lỗi"),
            make(5.0, 5.2, ""),
        ]);
        assert_eq!(notes[0], vec!["overlap"]);
        assert_eq!(notes[1], vec!["overlap", "short_duration", "dense_text"]);
        assert_eq!(notes[2], vec!["overlap"]);
        assert_eq!(notes[3], vec!["invalid_timing"]);
        assert!(notes[4].is_empty());
        let unsorted = review(&[make(10.0, 12.0, "Chào"), make(0.0, 2.0, "Chào")]);
        assert!(unsorted.iter().all(Vec::is_empty));
    }
}
