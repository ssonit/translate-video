use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::job::{Glossary, Term, MAX_LINE_CHARS};
use crate::TranslationChoice;

pub(crate) const LOCAL_MODEL: &str = "qwen3.5:9b-q4_K_M";

pub(crate) fn is_translate_gemma(model: &str) -> bool {
    model
        .trim()
        .to_ascii_lowercase()
        .starts_with("translategemma")
}
// 127.0.0.1, not "localhost": on this machine localhost takes about two seconds to connect.
pub(crate) const LOCAL_URL: &str = "http://127.0.0.1:11434/api/chat";

const BREAK_MARKS: &[char] = &[
    ',', ';', ':', '，', '、', '；', '：', '。', '！', '？', '!', '?',
];

#[derive(Debug)]
pub(crate) enum CompleteError {
    Transport(String),
    BadJson,
    Stopped,
}

impl std::fmt::Display for CompleteError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transport(message) => formatter.write_str(message),
            Self::Stopped => formatter.write_str("Đã dừng."),
            Self::BadJson => formatter.write_str("Model không trả JSON dùng được."),
        }
    }
}

pub(crate) fn parts_needed(text: &str) -> usize {
    let count = text.trim().chars().count();
    if count == 0 {
        1
    } else {
        (count + MAX_LINE_CHARS - 1) / MAX_LINE_CHARS
    }
}

/// Cắt theo dấu câu trên máy. Không gọi model: mỗi câu dài là một lượt chờ, mà chỗ cắt cơ học đủ để dịch.
pub(crate) fn split_line(text: &str) -> Vec<String> {
    let cleaned = text.trim();
    if cleaned.is_empty() {
        return Vec::new();
    }
    let needed = parts_needed(cleaned).min(8);
    if needed <= 1 {
        return vec![cleaned.to_string()];
    }
    shorten(&mechanical_split(cleaned, needed))
}

pub(crate) fn divide_time(start: f64, end: f64, parts: &[String]) -> Vec<crate::job::Cue> {
    if parts.is_empty() {
        return Vec::new();
    }
    let span = if end > start {
        end - start
    } else {
        0.3 * parts.len() as f64
    };
    let weights: Vec<usize> = parts
        .iter()
        .map(|part| part.chars().count().max(1))
        .collect();
    let total = weights.iter().sum::<usize>() as f64;
    let mut cursor = start.max(0.0);
    let mut cues = Vec::with_capacity(parts.len());
    for (index, part) in parts.iter().enumerate() {
        let width = if index + 1 == parts.len() {
            (start.max(0.0) + span - cursor).max(0.05)
        } else {
            (span * (weights[index] as f64 / total)).max(0.05)
        };
        let next = cursor + width;
        cues.push(crate::job::Cue {
            start: cursor,
            end: next,
            text: part.clone(),
        });
        cursor = next;
    }
    cues
}

pub(crate) fn glossary_from(value: &Value) -> Result<Glossary, CompleteError> {
    let Some(terms) = value.get("terms").and_then(Value::as_array) else {
        return Err(CompleteError::BadJson);
    };
    let mut parsed = Vec::new();
    for term in terms.iter().take(15) {
        let src = term
            .get("src")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
        if src.is_empty() {
            continue;
        }
        parsed.push(Term {
            src,
            tgt: term
                .get("tgt")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string(),
            note: term
                .get("note")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string(),
        });
    }
    Ok(Glossary {
        theme: value
            .get("theme")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string(),
        terms: parsed,
    })
}

pub(crate) fn terms_for_line<'a>(terms: &'a [Term], line: &str) -> Vec<&'a Term> {
    let lower = line.to_lowercase();
    terms
        .iter()
        .filter(|term| !term.src.is_empty() && lower.contains(&term.src.to_lowercase()))
        .collect()
}

const VIETNAMESE_ONLY: &str = "ăâđêôơưạảấầẩẫậắằẳẵặẹẻẽếềểễệỉịọỏốồổỗộớờởỡợụủứừửữựỳỵỷỹ";

/// True when `text` is clearly not in the target language. Only checks what a script or letters can tell
/// apart (Vietnamese, Chinese, Japanese, Korean, Russian, Thai); other targets always pass.
/// Short texts pass, since names and one-word lines prove nothing.
pub(crate) fn wrong_language(text: &str, target_code: &str) -> bool {
    if stray_script(text, target_code) {
        return true;
    }
    if text.chars().filter(|ch| ch.is_alphabetic()).count() < 12 {
        return false;
    }
    let lower = text.to_lowercase();
    let any = |test: &dyn Fn(char) -> bool| lower.chars().any(test);
    let han = |ch: char| ('\u{4e00}'..='\u{9fff}').contains(&ch);
    match target_code {
        "vi" => !any(&|ch| VIETNAMESE_ONLY.contains(ch)),
        "zh" => !any(&han),
        "ja" => !any(&|ch| han(ch) || ('\u{3040}'..='\u{30ff}').contains(&ch)),
        "ko" => !any(&|ch| ('\u{ac00}'..='\u{d7af}').contains(&ch)),
        "ru" => !any(&|ch| ('\u{0400}'..='\u{04ff}').contains(&ch)),
        "th" => !any(&|ch| ('\u{0e00}'..='\u{0e7f}').contains(&ch)),
        _ => false,
    }
}

/// True when `text` holds letters of a script the target never uses, even a single word. Small models
/// (qwen2.5 especially) slip Chinese or Korean words into a Vietnamese line, and those would otherwise pass.
fn is_stray(ch: char, target_code: &str) -> bool {
    let (han, kana, hangul, cyrillic, thai) = match target_code {
        "zh" => (true, false, false, false, false),
        "ja" => (true, true, false, false, false),
        "ko" => (false, false, true, false, false),
        "ru" => (false, false, false, true, false),
        "th" => (false, false, false, false, true),
        _ => (false, false, false, false, false),
    };
    (('\u{4e00}'..='\u{9fff}').contains(&ch) && !han)
        || (('\u{3040}'..='\u{30ff}').contains(&ch) && !kana)
        || (('\u{ac00}'..='\u{d7af}').contains(&ch) && !hangul)
        || (('\u{0400}'..='\u{04ff}').contains(&ch) && !cyrillic)
        || (('\u{0e00}'..='\u{0e7f}').contains(&ch) && !thai)
}

fn stray_script(text: &str, target_code: &str) -> bool {
    text.chars().any(|ch| is_stray(ch, target_code))
}

/// Drops the stray-script letters from a translation, as a last resort when the model keeps slipping
/// a word of another language into an otherwise good line.
pub(crate) fn strip_stray_script(text: &str, target_code: &str) -> String {
    let kept: String = text
        .chars()
        .filter(|ch| !is_stray(*ch, target_code))
        .collect();
    kept.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(crate) fn translated_text(value: &Value) -> Option<String> {
    let from_field = value
        .get("text")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string);
    from_field.or_else(|| {
        value
            .as_str()
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string)
    })
}

pub(crate) fn summary_prompt(source: &str, target: &str) -> String {
    format!(
        "Bạn là người dịch video. Đọc đoạn lời thoại sau.\n\
         1. Tóm tắt chủ đề bằng hai câu {target}, ghi vào theme.\n\
         2. Lấy tối đa 15 thuật ngữ hoặc tên riêng. src là chữ gốc, tgt là cách viết {target}, note là nghĩa ngắn.\n\
         Chỉ trả về JSON:\n\
         {{\"theme\":\"\",\"terms\":[{{\"src\":\"\",\"tgt\":\"\",\"note\":\"\"}}]}}\n\
         <text>\n{source}\n</text>"
    )
}

/// Short prompt in English for a line the first pass got wrong. A long Vietnamese prompt makes qwen2.5:7b
/// answer in Chinese or Korean about half the time; this form came out right on 16 of 17 lines in a test.
pub(crate) fn retry_prompt(line: &str, terms: &[&Term], target: &str) -> String {
    let notes: String = terms
        .iter()
        .map(|term| format!("\nKeep \"{}\" as \"{}\".", term.src, term.tgt))
        .collect();
    format!(
        "Translate this subtitle line into natural, short {target} that is easy to read aloud. \
         Output JSON {{\"text\":\"...\"}} with {target} words only: no Chinese, Korean or English words.{notes}\n\
         Subtitle: {line}"
    )
}

/// Một lượt dịch trên máy. 20 câu đã ngắt (khoảng 75 ký tự) vẫn nằm dưới context 4096.
pub(crate) const CHUNK_LINES: usize = 20;
pub(crate) const CHUNK_CHARS: usize = 1600;

/// Một lượt dịch qua API. Context rộng, nên gom dày để lời dẫn và bảng thuật ngữ khỏi gửi lại nhiều lần.
pub(crate) const ONLINE_CHUNK_LINES: usize = 40;
pub(crate) const ONLINE_CHUNK_CHARS: usize = 4000;

pub(crate) fn chunk_ranges(
    lines: &[String],
    max_lines: usize,
    max_chars: usize,
) -> Vec<std::ops::Range<usize>> {
    let mut ranges = Vec::new();
    let mut start = 0;
    let mut chars = 0;
    for (index, line) in lines.iter().enumerate() {
        let len = line.chars().count() + 1;
        if index > start && (index - start >= max_lines || chars + len > max_chars) {
            ranges.push(start..index);
            start = index;
            chars = 0;
        }
        chars += len;
    }
    if start < lines.len() {
        ranges.push(start..lines.len());
    }
    ranges
}

pub(crate) fn batch_prompt(
    lines: &[&str],
    before: &[&str],
    after: &[&str],
    terms: &[&Term],
    theme: &str,
    target: &str,
) -> String {
    let notes = if terms.is_empty() {
        "Không có thuật ngữ bắt buộc.".to_string()
    } else {
        terms
            .iter()
            .map(|term| format!("\"{}\": \"{}\". Nghĩa: {}", term.src, term.tgt, term.note))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let numbered = lines
        .iter()
        .enumerate()
        .map(|(index, line)| format!("{}. {line}", index + 1))
        .collect::<Vec<_>>()
        .join("\n");
    let context = |items: &[&str]| {
        if items.is_empty() {
            "(không có)".to_string()
        } else {
            items.join("\n")
        }
    };
    let count = lines.len();
    format!(
        "Dịch các câu phụ đề sau (ngôn ngữ gốc tự nhận) sang {target} tự nhiên, đủ ngắn để đọc thành lời. Dịch từng dòng, không gộp, không bỏ dòng.\n\
         BẮT BUỘC: bản dịch chỉ viết bằng {target}, tuyệt đối không dùng ngôn ngữ khác (kể cả tiếng Tây Ban Nha hay tiếng Anh). MANDATORY: output ONLY in {target}.\n\
         Chủ đề video: {theme}\n\
         Thuật ngữ cần giữ:\n{notes}\n\
         Ngữ cảnh trước (chỉ để hiểu, không dịch):\n{}\n\
         Ngữ cảnh sau (chỉ để hiểu, không dịch):\n{}\n\
         Chỉ trả về JSON dạng {{\"1\":\"...\",\"2\":\"...\"}} với đúng {count} khóa.\n\
         <lines>\n{numbered}\n</lines>",
        context(before),
        context(after)
    )
}

/// One slot per line. A missing or blank key is None; the lines that did arrive stay usable.
pub(crate) fn batch_translations(value: &Value, count: usize) -> Vec<Option<String>> {
    let object = value.as_object();
    (1..=count)
        .map(|index| {
            object
                .and_then(|map| map.get(&index.to_string()))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .map(str::to_string)
        })
        .collect()
}

/// Phần trăm độ dài chữ cần giữ để câu đọc vừa chỗ ở tốc độ bình thường.
pub(crate) fn shorten_percent(room_secs: f64, wav_secs: f64) -> u32 {
    let ratio = if wav_secs > 0.0 {
        (room_secs * 0.95 / wav_secs).clamp(0.4, 0.95)
    } else {
        0.9
    };
    (ratio * 100.0).round() as u32
}

/// Một lượt rút nhiều câu. Mỗi câu kèm phần trăm riêng vì chỗ trống trên video khác nhau.
pub(crate) fn shorten_batch_prompt(items: &[(u32, &str)], target: &str) -> String {
    let numbered = items
        .iter()
        .enumerate()
        .map(|(index, (percent, text))| format!("{}. ({percent}%) {text}", index + 1))
        .collect::<Vec<_>>()
        .join("\n");
    let count = items.len();
    format!(
        "Viết lại từng câu {target} dưới đây thành câu nói ngắn hơn, còn khoảng phần trăm ghi cạnh số thứ tự, để đọc vừa chỗ ở tốc độ bình thường.\n\
         Giữ ý chính, tên riêng và thuật ngữ. Câu vẫn tròn và nghe tự nhiên. Không thêm ý mới. Không gộp, không bỏ câu.\n\
         Chỉ trả về JSON dạng {{\"1\":\"...\",\"2\":\"...\"}} với đúng {count} khóa.\n\
         <lines>\n{numbered}\n</lines>"
    )
}

/// Câu model bỏ trống thì để None, các câu khác trong cùng lượt vẫn dùng được.
pub(crate) fn shorten_batch_texts(value: &Value, count: usize) -> Vec<Option<String>> {
    let object = value.as_object();
    (1..=count)
        .map(|index| {
            object
                .and_then(|map| map.get(&index.to_string()))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .map(str::to_string)
        })
        .collect()
}

pub(crate) fn complete(
    choice: &TranslationChoice,
    prompt: &str,
    cancel: &AtomicBool,
) -> Result<Value, CompleteError> {
    complete_timed(choice, prompt, cancel, &mut TranslateTrace::quiet())
}

/// Dịch một đoạn dài thành văn để nghe, không bắt JSON từng câu.
pub(crate) fn complete_plain(
    choice: &TranslationChoice,
    prompt: &str,
    cancel: &AtomicBool,
) -> Result<String, String> {
    let (text, _) =
        request_cancelable(choice, prompt, cancel, false).map_err(|error| error.to_string())?;
    let text = unwrap_prose(&text);
    if text.is_empty() {
        Err("Bản dịch trống.".into())
    } else {
        Ok(text)
    }
}

pub(crate) fn prose_prompt(chunk: &str, target: &str, prev: &str) -> String {
    let prev = prev.trim();
    let context = if prev.is_empty() {
        String::new()
    } else {
        format!(
            "Đoạn liền trước, chỉ để giữ cách gọi, không dịch lại:\n{}\n\n",
            tail_chars(prev, 450)
        )
    };
    format!(
        "Bạn dịch lời video sang {target} để đọc thành tiếng.\n\
         Lời lấy từ phụ đề, có thể thiếu dấu câu. Hãy thêm dấu câu và viết câu vừa đọc.\n\
         Viết {target} đời thường. Đủ ý, không tóm tắt, không chú thích, không lời dẫn.\n\
         Chỉ trả về bản dịch.\n\
         {context}<text>\n{chunk}\n</text>"
    )
}

fn tail_chars(text: &str, limit: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= limit {
        text.to_string()
    } else {
        chars[chars.len() - limit..].iter().collect()
    }
}

fn unwrap_prose(raw: &str) -> String {
    let mut text = raw.trim();
    if let Some(rest) = text.strip_prefix("```") {
        text = rest
            .trim_start_matches(|ch: char| ch != '\n')
            .trim_start_matches('\n')
            .trim_end_matches("```")
            .trim();
    }
    if let Ok(value) = serde_json::from_str::<Value>(text) {
        if let Some(inner) = translated_text(&value) {
            return inner;
        }
    }
    text.trim().to_string()
}

/// Same as `complete`, and every HTTP call is stored on `trace` (batch index and attempt come from `TranslateTrace::begin`).
pub(crate) fn complete_timed(
    choice: &TranslationChoice,
    prompt: &str,
    cancel: &AtomicBool,
    trace: &mut TranslateTrace,
) -> Result<Value, CompleteError> {
    let started = Instant::now();
    match request_cancelable(choice, prompt, cancel, true) {
        Ok((text, timings)) => {
            trace.record(&timings, started.elapsed());
            if let Some(value) = parse_json_value(&text) {
                return Ok(value);
            }
        }
        Err(error) => {
            trace.record(&LocalTimings::default(), started.elapsed());
            return Err(error);
        }
    }
    let retry = format!("{prompt}\n\nChỉ trả về một JSON object, không thêm chữ khác.");
    let started = Instant::now();
    match request_cancelable(choice, &retry, cancel, true) {
        Ok((text, timings)) => {
            trace.record(&timings, started.elapsed());
            parse_json_value(&text).ok_or(CompleteError::BadJson)
        }
        Err(error) => {
            trace.record(&LocalTimings::default(), started.elapsed());
            Err(error)
        }
    }
}

// The blocking HTTP exchange stays owned by the task. Cancellation stops retries
// and discards the response, but the slot remains occupied until the exchange ends.
fn request_cancelable(
    choice: &TranslationChoice,
    prompt: &str,
    cancel: &AtomicBool,
    json: bool,
) -> Result<(String, LocalTimings), CompleteError> {
    owned_request(cancel, || request(choice, prompt, json, cancel))
}

fn owned_request<T>(cancel: &AtomicBool, work: impl FnOnce() -> Result<T, String>) -> Result<T, CompleteError> {
    if cancel.load(Ordering::SeqCst) { return Err(CompleteError::Stopped); }
    let result = work();
    if cancel.load(Ordering::SeqCst) { return Err(CompleteError::Stopped); }
    result.map_err(CompleteError::Transport)
}

fn wait_cancel(cancel: &AtomicBool, duration: Duration) -> Result<(), String> {
    let until = Instant::now() + duration;
    loop {
        if cancel.load(Ordering::SeqCst) { return Err("Đã dừng.".into()); }
        let remaining = until.saturating_duration_since(Instant::now());
        if remaining.is_zero() { return Ok(()); }
        std::thread::sleep(remaining.min(Duration::from_millis(50)));
    }
}

// Một agent cho cả phiên: ureq giữ kết nối TLS. Dựng agent mỗi lô thì bắt tay lại từ đầu.
fn shared_agent() -> &'static ureq::Agent {
    static AGENT: std::sync::OnceLock<ureq::Agent> = std::sync::OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_secs(180))
            .build()
    })
}

fn request(
    choice: &TranslationChoice,
    prompt: &str,
    json: bool,
    cancel: &AtomicBool,
) -> Result<(String, LocalTimings), String> {
    let agent = shared_agent();
    match choice.provider.as_str() {
        "local" => ollama_request(&agent, model_or(&choice.model, LOCAL_MODEL), prompt, json, cancel),
        "openai" => openai_request(
            &agent,
            &choice.api_key,
            model_or(&choice.model, OPENAI_MODEL),
            prompt,
            json,
            cancel,
        )
        .map(|text| (text, LocalTimings::default())),
        "claude" => claude_request(
            &agent,
            &choice.api_key,
            model_or(&choice.model, CLAUDE_MODEL),
            prompt,
            // Lô JSON vài chục câu. Trần thấp cắt giữa object, mất khóa rồi phải gọi lại.
            PROSE_MAX_TOKENS,
            cancel,
        )
        .map(|text| (text, LocalTimings::default())),
        "gemini" => gemini_request(
            &agent,
            &choice.api_key,
            model_or(&choice.model, GEMINI_MODEL),
            prompt,
            json,
            cancel,
        )
        .map(|text| (text, LocalTimings::default())),
        "deepseek" => deepseek_request(
            &agent,
            &choice.api_key,
            model_or(&choice.model, DEEPSEEK_MODEL),
            prompt,
            json,
            cancel,
        )
        .map(|text| (text, LocalTimings::default())),
        "deepseek-local" => {
            deepseek_local_request(model_or(&choice.model, DEEPSEEK_LOCAL_MODEL), prompt, json, cancel)
                .map(|text| (text, LocalTimings::default()))
        }
        _ => Err("Không rõ dịch vụ.".into()),
    }
}

// Model người dùng chọn ở màn lồng tiếng; để trống thì dùng model mặc định của dịch vụ.
fn model_or<'a>(chosen: &'a str, default: &'a str) -> &'a str {
    let chosen = chosen.trim();
    if chosen.is_empty() {
        default
    } else {
        chosen
    }
}

pub(crate) fn model_identity(provider: &str, chosen: &str) -> String {
    model_or(chosen, match provider {
        "local" => LOCAL_MODEL, "openai" => OPENAI_MODEL, "claude" => CLAUDE_MODEL,
        "gemini" => GEMINI_MODEL, "deepseek" => DEEPSEEK_MODEL,
        "deepseek-local" => DEEPSEEK_LOCAL_MODEL, _ => "",
    }).into()
}

pub(crate) const OPENAI_MODEL: &str = "gpt-4o-mini";
pub(crate) const CLAUDE_MODEL: &str = "claude-haiku-4-5-20251001";
pub(crate) const GEMINI_MODEL: &str = "gemini-3.8-flash";
pub(crate) const DEEPSEEK_MODEL: &str = "deepseek-flash";
/// Model của server Deepseek-API trên máy. Tên khác API trả phí, hai bên không dùng chung.
pub(crate) const DEEPSEEK_LOCAL_MODEL: &str = "deepseek-chat";

/// Đoạn nghe khoảng 1.000 từ. Trần này cho bản dịch cả đoạn, mọi dịch vụ, khỏi cắt giữa chừng.
const PROSE_MAX_TOKENS: u32 = 8192;

/// Qwen3.5 nghĩ hàng trăm token trước khi dịch nếu không tắt. Context lớn cũng làm chậm, trong khi mỗi cụm chỉ khoảng 600 chữ.
const OLLAMA_NUM_CTX: u32 = 4096;

fn ollama_payload(model: &str, prompt: &str, json: bool) -> Value {
    let mut payload = serde_json::json!({
        "model": model,
        "stream": false,
        "think": false,
        "keep_alive": "30m",
        "options": { "num_ctx": if json { OLLAMA_NUM_CTX } else { 8192 } },
        "messages": [{"role": "user", "content": prompt}],
    });
    if json {
        payload["format"] = serde_json::json!("json");
    } else {
        payload["options"]["num_predict"] = serde_json::json!(PROSE_MAX_TOKENS);
    }
    payload
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct LocalTimings {
    load_ns: u64,
    prompt_ns: u64,
    prompt_count: u64,
    eval_ns: u64,
    eval_count: u64,
    total_ns: u64,
}

fn nanos(body: &Value, name: &str) -> u64 {
    body.get(name).and_then(Value::as_u64).unwrap_or(0)
}

fn local_timings(body: &Value) -> LocalTimings {
    LocalTimings {
        load_ns: nanos(body, "load_duration"),
        prompt_ns: nanos(body, "prompt_eval_duration"),
        prompt_count: nanos(body, "prompt_eval_count"),
        eval_ns: nanos(body, "eval_duration"),
        eval_count: nanos(body, "eval_count"),
        total_ns: nanos(body, "total_duration"),
    }
}

fn ollama_request(
    agent: &ureq::Agent,
    model: &str,
    prompt: &str,
    json: bool,
    cancel: &AtomicBool,
) -> Result<(String, LocalTimings), String> {
    let payload = ollama_payload(model, prompt, json);
    let body = send(agent, "POST", LOCAL_URL, &[], &payload, cancel)
        .map_err(|error| format!("Không gọi được Ollama tại http://127.0.0.1:11434. {error}"))?;
    let text = body
        .get("message")
        .and_then(|message| message.get("content"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| "Ollama không trả lời.".to_string())?;
    Ok((text, local_timings(&body)))
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LocalHit {
    pub batch: u32,
    pub attempt: u32,
    pub load_ns: u64,
    pub prompt_ns: u64,
    pub prompt_count: u64,
    pub eval_ns: u64,
    pub eval_count: u64,
    pub total_ns: u64,
    pub wall: Duration,
}

pub(crate) struct TranslateTrace {
    hits: Vec<LocalHit>,
    batch: u32,
    next_attempt: u32,
    echo: bool,
}

impl TranslateTrace {
    pub(crate) fn logging() -> Self {
        Self {
            hits: Vec::new(),
            batch: 0,
            next_attempt: 1,
            echo: true,
        }
    }

    fn quiet() -> Self {
        Self {
            hits: Vec::new(),
            batch: 0,
            next_attempt: 1,
            echo: false,
        }
    }

    /// Next HTTP calls belong to this batch. Attempt numbers start again at 1. Batch 0 is the warm-up.
    pub(crate) fn begin(&mut self, batch: u32) {
        self.batch = batch;
        self.next_attempt = 1;
    }

    /// A trace for one request running beside others. Hits stay here until `absorb`.
    pub(crate) fn detached(&self, batch: u32) -> Self {
        Self {
            hits: Vec::new(),
            batch,
            next_attempt: 1,
            echo: self.echo,
        }
    }

    pub(crate) fn absorb(&mut self, other: TranslateTrace) {
        self.hits.extend(other.hits);
    }

    fn record(&mut self, timings: &LocalTimings, wall: Duration) {
        let hit = LocalHit {
            batch: self.batch,
            attempt: self.next_attempt,
            load_ns: timings.load_ns,
            prompt_ns: timings.prompt_ns,
            prompt_count: timings.prompt_count,
            eval_ns: timings.eval_ns,
            eval_count: timings.eval_count,
            total_ns: timings.total_ns,
            wall,
        };
        self.next_attempt += 1;
        if self.echo {
            eprintln!("{}", format_hit(&hit));
        }
        self.hits.push(hit);
    }

    pub(crate) fn summary_line(&self) -> String {
        format_summary(&self.hits)
    }
}

fn seconds(ns: u64) -> f64 {
    ns as f64 / 1e9
}

fn tokens_per_sec(count: u64, ns: u64) -> Option<f64> {
    if ns == 0 {
        None
    } else {
        Some(count as f64 / seconds(ns))
    }
}

pub(crate) fn format_hit(hit: &LocalHit) -> String {
    let rate = tokens_per_sec(hit.eval_count, hit.eval_ns)
        .map(|rate| format!("{rate:.0} tok/s"))
        .unwrap_or_else(|| "không có tok/s".to_string());
    format!(
        "[timing] dịch lượt {} lần {}: load {:.2}s, prompt_eval {:.2}s ({}), eval {:.2}s ({}, {rate}), total {:.2}s, wall {:.2}s",
        hit.batch,
        hit.attempt,
        seconds(hit.load_ns),
        seconds(hit.prompt_ns),
        hit.prompt_count,
        seconds(hit.eval_ns),
        hit.eval_count,
        seconds(hit.total_ns),
        hit.wall.as_secs_f64(),
    )
}

pub(crate) fn format_summary(hits: &[LocalHit]) -> String {
    let warm_load: u64 = hits
        .iter()
        .filter(|hit| hit.batch == 0)
        .map(|hit| hit.load_ns)
        .sum();
    let work: Vec<&LocalHit> = hits.iter().filter(|hit| hit.batch >= 1).collect();
    let batches = work
        .iter()
        .map(|hit| hit.batch)
        .collect::<BTreeSet<_>>()
        .len();
    let requests = work.len();
    let extra = requests.saturating_sub(batches);
    let load: u64 = work.iter().map(|hit| hit.load_ns).sum();
    let eval: u64 = work.iter().map(|hit| hit.eval_ns).sum();
    let total: u64 = work.iter().map(|hit| hit.total_ns).sum();
    let wall: f64 = work.iter().map(|hit| hit.wall.as_secs_f64()).sum();
    let mut rates: Vec<f64> = work
        .iter()
        .filter_map(|hit| tokens_per_sec(hit.eval_count, hit.eval_ns))
        .collect();
    rates.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    let spread = match (rates.first(), rates.last()) {
        (Some(low), Some(high)) => format!("{low:.0}–{high:.0}"),
        _ => "không có".to_string(),
    };
    format!(
        "[timing] dịch tổng: {batches} lượt, {requests} request, {extra} lần gọi thêm, warm load {:.2}s, load {:.2}s, eval {:.2}s, total {:.2}s, wall {:.2}s, tok/s {spread}",
        seconds(warm_load),
        seconds(load),
        seconds(eval),
        seconds(total),
        wall,
    )
}

const DEEPSEEK_URL: &str = "https://api.deepseek.com/chat/completions";

fn deepseek_payload(model: &str, prompt: &str, json: bool) -> Value {
    let mut payload = serde_json::json!({
        "model": model,
        "messages": [{"role": "user", "content": prompt}],
        // Mặc định của DeepSeek là suy nghĩ trước khi trả lời, tốn token và chậm. Dịch câu thì tắt.
        "thinking": {"type": "disabled"},
        "max_tokens": PROSE_MAX_TOKENS,
    });
    if json {
        payload["response_format"] = serde_json::json!({"type": "json_object"});
    }
    payload
}

fn deepseek_request(
    agent: &ureq::Agent,
    key: &str,
    model: &str,
    prompt: &str,
    json: bool,
    cancel: &AtomicBool,
) -> Result<String, String> {
    let payload = deepseek_payload(model, prompt, json);
    let body = send(
        agent,
        "POST",
        DEEPSEEK_URL,
        &[("Authorization", &format!("Bearer {key}"))],
        &payload,
        cancel,
    )
    .map_err(|error| format!("DeepSeek từ chối yêu cầu. {error}"))?;
    body["choices"][0]["message"]["content"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| "DeepSeek không trả lời.".into())
}

/// DeepSeek local: server Deepseek-API trên máy, dùng tài khoản chat.deepseek.com, không cần API key.
/// Khác API trả phí ở URL, tên model và tham số, nên có đường gọi riêng.
pub(crate) const DEEPSEEK_LOCAL_URL: &str = "http://127.0.0.1:8000/v1/chat/completions";
pub(crate) const DEEPSEEK_LOCAL_HEALTH_URL: &str = "http://127.0.0.1:8000/healthz";

fn deepseek_local_payload(model: &str, prompt: &str, _json: bool) -> Value {
    // Server bỏ max_tokens và response_format. thinking là bool. Prompt đã yêu cầu JSON.
    serde_json::json!({
        "model": model,
        "messages": [{"role": "user", "content": prompt}],
        "stream": false,
        "thinking": false,
        "search": false,
    })
}

const DEEPSEEK_LOCAL_TIMEOUT: Duration = Duration::from_secs(300);

fn deepseek_local_agent() -> &'static ureq::Agent {
    static AGENT: std::sync::OnceLock<ureq::Agent> = std::sync::OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::AgentBuilder::new()
            .timeout(DEEPSEEK_LOCAL_TIMEOUT)
            .build()
    })
}

pub(crate) fn deepseek_local_up() -> bool {
    deepseek_local_health(std::time::Duration::from_secs(2))
}

pub(crate) fn deepseek_local_up_fast() -> bool {
    deepseek_local_health(std::time::Duration::from_millis(800))
}

fn deepseek_local_health(timeout: std::time::Duration) -> bool {
    ureq::AgentBuilder::new()
        .timeout(timeout)
        .build()
        .get(DEEPSEEK_LOCAL_HEALTH_URL)
        .call()
        .is_ok()
}

pub(crate) const DEEPSEEK_LOCAL_DOWN: &str =
    "Chưa bật DeepSeek local. Bấm Bật DeepSeek ở ô Dịch bằng hoặc ở Đồ nghề, rồi chạy lại.";

/// True khi lượt này sẽ gọi server DeepSeek local. Không dịch thì không cần server.
pub(crate) fn needs_deepseek_local(choice: &TranslationChoice, skip_translate: bool) -> bool {
    choice.provider == "deepseek-local" && !skip_translate
}

/// Hỏi server trước khi tách tiếng và chép lời. Không hỏi thì lỗi chỉ hiện sau khi Whisper chạy xong.
pub(crate) fn require_deepseek_local(
    choice: &TranslationChoice,
    skip_translate: bool,
) -> Result<(), String> {
    if needs_deepseek_local(choice, skip_translate) && !deepseek_local_up() {
        return Err(DEEPSEEK_LOCAL_DOWN.into());
    }
    Ok(())
}

fn deepseek_local_request(model: &str, prompt: &str, json: bool, cancel: &AtomicBool) -> Result<String, String> {
    static ACTIVE: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _request = loop {
        if cancel.load(Ordering::SeqCst) { return Err("Đã dừng.".into()); }
        match ACTIVE.try_lock() {
            Ok(guard) => break guard,
            Err(std::sync::TryLockError::Poisoned(error)) => break error.into_inner(),
            Err(std::sync::TryLockError::WouldBlock) => wait_cancel(cancel, Duration::from_millis(50))?,
        }
    };
    let payload = deepseek_local_payload(model, prompt, json);
    let text = serde_json::to_string(&payload).map_err(|error| error.to_string())?;
    let waits = [1u64, 2, 4];
    let mut attempt = 0u8;
    let response = loop {
        if cancel.load(Ordering::SeqCst) { return Err("Đã dừng.".into()); }
        match deepseek_local_agent()
            .post(DEEPSEEK_LOCAL_URL)
            .set("Content-Type", "application/json")
            .send_string(&text)
        {
            Ok(response) => break response,
            Err(ureq::Error::Status(code, response)) => {
                let retry_after = response
                    .header("retry-after")
                    .and_then(|value| value.trim().parse::<u64>().ok());
                let body = response.into_string().unwrap_or_default();
                if code == 404 {
                    return Err(format!(
                        "Model {model} không có trên Deepseek-API. Dùng deepseek-chat hoặc deepseek-expert."
                    ));
                }
                if code == 503 && login_required(&body) {
                    return Err(
                        "Chưa đăng nhập DeepSeek. Trong thư mục Deepseek-API chạy: python -m deepseek.auth"
                            .into(),
                    );
                }
                if code == 429 {
                    let wait = retry_after
                        .or_else(|| waits.get(attempt as usize).copied())
                        .unwrap_or(4)
                        .min(60);
                    attempt += 1;
                    if attempt > 3 {
                        return Err(
                            "DeepSeek trên máy đang giới hạn số lần gọi. Chờ một phút rồi chạy lại."
                                .into(),
                        );
                    }
                    wait_cancel(cancel, Duration::from_secs(wait))?;
                    continue;
                }
                return Err(format!(
                    "DeepSeek trên máy {code}: {}",
                    deepseek_local_error_text(&body)
                ));
            }
            Err(ureq::Error::Transport(error)) => {
                return Err(deepseek_local_transport_message(
                    error.kind(),
                    io_kind(&error),
                ));
            }
        }
    };
    let body = response.into_string().map_err(|error| error.to_string())?;
    let parsed: Value =
        serde_json::from_str(&body).map_err(|_| "Phản hồi không phải JSON.".to_string())?;
    parsed["choices"][0]["message"]["content"]
        .as_str()
        .map(strip_think)
        .filter(|text| !text.is_empty())
        .ok_or_else(|| "DeepSeek trên máy không trả lời.".into())
}

fn io_kind(error: &ureq::Transport) -> Option<std::io::ErrorKind> {
    use std::error::Error;
    error
        .source()
        .and_then(|source| source.downcast_ref::<std::io::Error>())
        .map(|io| io.kind())
}

/// Từ chối kết nối nghĩa là server chưa nghe. Hết giờ đọc thì server vẫn đang chạy,
/// thường là cửa sổ đăng nhập hoặc deepseek-expert chưa trả lời.
fn deepseek_local_transport_message(
    kind: ureq::ErrorKind,
    io_kind: Option<std::io::ErrorKind>,
) -> String {
    if kind == ureq::ErrorKind::Io && io_kind == Some(std::io::ErrorKind::TimedOut) {
        let minutes = DEEPSEEK_LOCAL_TIMEOUT.as_secs() / 60;
        return format!(
            "Hết {minutes} phút mà DeepSeek chưa trả lời. Nếu cửa sổ đăng nhập đang mở thì đăng nhập xong rồi chạy lại."
        );
    }
    if kind == ureq::ErrorKind::ConnectionFailed
        || io_kind == Some(std::io::ErrorKind::ConnectionRefused)
    {
        return DEEPSEEK_LOCAL_DOWN.into();
    }
    if matches!(
        io_kind,
        Some(std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted)
    ) {
        return "Mất kết nối tới DeepSeek lúc đang gọi. Chạy lại.".into();
    }
    "Không gọi được DeepSeek trên máy.".into()
}

fn login_required(body: &str) -> bool {
    let lowered = body.to_ascii_lowercase();
    lowered.contains("login_required")
        || lowered.contains("no deepseek session")
        || lowered.contains("log in first")
}

fn deepseek_local_error_text(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("error")
                .and_then(|error| error.get("message"))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .filter(|text| !text.trim().is_empty())
        .unwrap_or_else(|| brief(body))
}

fn strip_think(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("<think>") {
        out.push_str(&rest[..start]);
        rest = &rest[start + "<think>".len()..];
        if let Some(end) = rest.find("</think>") {
            rest = &rest[end + "</think>".len()..];
        } else {
            rest = "";
            break;
        }
    }
    out.push_str(rest);
    out.trim().to_string()
}

fn openai_payload(model: &str, prompt: &str, json: bool) -> Value {
    let mut payload = serde_json::json!({
        "model": model,
        "messages": [{"role": "user", "content": prompt}],
    });
    if json {
        payload["response_format"] = serde_json::json!({"type": "json_object"});
    } else {
        payload["max_completion_tokens"] = serde_json::json!(PROSE_MAX_TOKENS);
    }
    payload
}

fn openai_request(
    agent: &ureq::Agent,
    key: &str,
    model: &str,
    prompt: &str,
    json: bool,
    cancel: &AtomicBool,
) -> Result<String, String> {
    let payload = openai_payload(model, prompt, json);
    let body = send(
        agent,
        "POST",
        "https://api.openai.com/v1/chat/completions",
        &[("Authorization", &format!("Bearer {key}"))],
        &payload,
        cancel,
    )
    .map_err(|error| format!("ChatGPT từ chối yêu cầu. {error}"))?;
    body["choices"][0]["message"]["content"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| "ChatGPT không trả lời.".into())
}

fn claude_payload(model: &str, prompt: &str, max_tokens: u32) -> Value {
    serde_json::json!({
        "model": model,
        "max_tokens": max_tokens,
        "messages": [{"role": "user", "content": prompt}],
    })
}

fn claude_request(
    agent: &ureq::Agent,
    key: &str,
    model: &str,
    prompt: &str,
    max_tokens: u32,
    cancel: &AtomicBool,
) -> Result<String, String> {
    let payload = claude_payload(model, prompt, max_tokens);
    let body = send(
        agent,
        "POST",
        "https://api.anthropic.com/v1/messages",
        &[("x-api-key", key), ("anthropic-version", "2023-06-01")],
        &payload,
        cancel,
    )
    .map_err(|error| format!("Claude từ chối yêu cầu. {error}"))?;
    body["content"][0]["text"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| "Claude không trả lời.".into())
}

const GEMINI_URL: &str = "https://generativelanguage.googleapis.com/v1beta/models/";

fn gemini_thinking(model: &str) -> Option<Value> {
    let name = model.to_ascii_lowercase();
    // 2.5 dùng ngân sách token. Flash tắt hẳn. Pro không xuống dưới 128.
    if name.contains("2.5") {
        let budget = if name.contains("pro") { 128 } else { 0 };
        return Some(serde_json::json!({ "thinkingBudget": budget }));
    }
    // Gemini 3 trở đi nghĩ ở mức medium hoặc high nếu không nói gì. Dịch câu không cần vậy.
    // 3.8 Flash và các bản Pro từ chối "minimal".
    if name.contains("gemini-3") || name.contains("gemini-4") || name.contains("argon") {
        let level = if name.contains("3.8") || name.contains("pro") || name.contains("argon") {
            "low"
        } else {
            "minimal"
        };
        return Some(serde_json::json!({ "thinkingLevel": level }));
    }
    None
}

fn gemini_payload(model: &str, prompt: &str, json: bool) -> Value {
    let mut payload = if json {
        serde_json::json!({
            "contents": [{"parts": [{"text": prompt}]}],
            "generationConfig": {"responseMimeType": "application/json"},
        })
    } else {
        serde_json::json!({
            "contents": [{"parts": [{"text": prompt}]}],
            "generationConfig": {"maxOutputTokens": PROSE_MAX_TOKENS},
        })
    };
    if let Some(thinking) = gemini_thinking(model) {
        payload["generationConfig"]["thinkingConfig"] = thinking;
    }
    payload
}

fn gemini_answer(body: &Value) -> Option<String> {
    let parts = body
        .get("candidates")?
        .get(0)?
        .get("content")?
        .get("parts")?
        .as_array()?;
    parts.iter().find_map(|part| {
        if part.get("thought").and_then(Value::as_bool) == Some(true) {
            None
        } else {
            part.get("text").and_then(Value::as_str).map(str::to_string)
        }
    })
}

fn gemini_request(
    agent: &ureq::Agent,
    key: &str,
    model: &str,
    prompt: &str,
    json: bool,
    cancel: &AtomicBool,
) -> Result<String, String> {
    let payload = gemini_payload(model, prompt, json);
    let body = send(
        agent,
        "POST",
        &format!("{GEMINI_URL}{model}:generateContent"),
        &[("x-goog-api-key", key)],
        &payload,
        cancel,
    )
    .map_err(|error| format!("Gemini từ chối yêu cầu. {error}"))?;
    gemini_answer(&body).ok_or_else(|| "Gemini không trả lời.".into())
}

const RETRY_WAITS_SECS: [u64; 3] = [3, 8, 20];

fn is_retryable(code: u16) -> bool {
    matches!(code, 429 | 500 | 502 | 503 | 504)
}

fn send(
    agent: &ureq::Agent,
    _method: &str,
    url: &str,
    headers: &[(&str, &str)],
    payload: &Value,
    cancel: &AtomicBool,
) -> Result<Value, String> {
    let text = serde_json::to_string(payload).map_err(|error| error.to_string())?;
    // Máy chủ AI hay quá tải tạm thời (503, 429...). Thử lại vài lần, chờ lâu dần.
    let mut waits = RETRY_WAITS_SECS.iter();
    let response = loop {
        if cancel.load(Ordering::SeqCst) { return Err("Đã dừng.".into()); }
        let mut request = agent.post(url).set("Content-Type", "application/json");
        for (name, value) in headers {
            request = request.set(name, value);
        }
        match request.send_string(&text) {
            Ok(response) => break response,
            Err(ureq::Error::Status(code, _)) if is_retryable(code) => match waits.next() {
                Some(secs) => wait_cancel(cancel, Duration::from_secs(*secs))?,
                None => return Err(format!("{code} Dịch vụ đang quá tải, đã tự thử lại nhưng vẫn lỗi. Chờ vài phút rồi chạy lại.")),
            },
            Err(error) => return Err(http_error(error)),
        }
    };
    let body = response.into_string().map_err(|error| error.to_string())?;
    serde_json::from_str(&body).map_err(|_| "Phản hồi không phải JSON.".into())
}

fn http_error(error: ureq::Error) -> String {
    match error {
        ureq::Error::Status(code, response) => {
            let body = response.into_string().unwrap_or_default();
            format!("{code} {}", brief(&body))
        }
        ureq::Error::Transport(_) => "Không kết nối được.".into(),
    }
}

fn brief(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= 240 {
        trimmed.to_string()
    } else {
        trimmed.chars().take(240).collect()
    }
}

pub(crate) fn parse_json_value(text: &str) -> Option<Value> {
    let text = text.trim().trim_start_matches('\u{feff}').trim();
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if end < start {
        return None;
    }
    serde_json::from_str(&text[start..=end]).ok()
}

fn shorten(parts: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for part in parts {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }
        if parts_needed(trimmed) > 1
            && mechanical_split(trimmed, parts_needed(trimmed).min(8)).len() > 1
        {
            out.extend(mechanical_split(trimmed, parts_needed(trimmed).min(8)));
        } else {
            out.push(trimmed.to_string());
        }
    }
    out.into_iter()
        .map(|part| part.trim().to_string())
        .filter(|part| !part.is_empty())
        .collect()
}

pub(crate) fn mechanical_split(sentence: &str, num_parts: usize) -> Vec<String> {
    let mut rest = sentence.trim().to_string();
    if rest.is_empty() || num_parts <= 1 {
        return if rest.is_empty() {
            Vec::new()
        } else {
            vec![rest]
        };
    }
    let mut lines = Vec::new();
    for parts_left in (2..=num_parts).rev() {
        let chars: Vec<char> = rest.chars().collect();
        let len = chars.len();
        if len < 2 {
            break;
        }
        let target = len / parts_left;
        let window = (len / (parts_left * 3)).max(1);
        let from = target.saturating_sub(window).max(1);
        let to = (target + window).min(len - 1);
        let mut cuts: Vec<usize> = (from..=to)
            .filter(|&index| BREAK_MARKS.contains(&chars[index - 1]))
            .collect();
        if cuts.is_empty() {
            cuts = (from..=to)
                .filter(|&index| chars[index].is_whitespace())
                .collect();
        }
        if cuts.is_empty() {
            if chars.iter().any(|ch| ch.is_whitespace()) {
                match nearest_space(&chars, target) {
                    Some(index) => cuts.push(index),
                    None => break,
                }
            } else {
                cuts.push(target.clamp(1, len - 1));
            }
        }
        let cut = cuts
            .into_iter()
            .min_by_key(|index| index.abs_diff(target))
            .unwrap_or(target);
        let left: String = chars[..cut].iter().collect();
        let right: String = chars[cut..].iter().collect();
        let left = left.trim();
        if !left.is_empty() {
            lines.push(left.to_string());
        }
        rest = right.trim().to_string();
        if rest.is_empty() {
            break;
        }
    }
    if !rest.is_empty() {
        lines.push(rest);
    }
    lines
}

fn nearest_space(chars: &[char], target: usize) -> Option<usize> {
    chars
        .iter()
        .enumerate()
        .filter(|(index, ch)| *index > 0 && ch.is_whitespace())
        .min_by_key(|(index, _)| index.abs_diff(target))
        .map(|(index, _)| index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancelling_an_owned_request_does_not_detach_it_or_unlock_another_job() {
        let tasks = crate::coordinator::Coordinator::default();
        let (entered, started) = std::sync::mpsc::channel();
        let (finish, wait) = std::sync::mpsc::channel();
        std::thread::scope(|scope| {
            let owner = &tasks;
            let worker = scope.spawn(move || {
                let _lease = owner.acquire(crate::coordinator::Kind::Preview).unwrap();
                let result = owned_request(&owner.cancel, || {
                    entered.send(()).unwrap();
                    wait.recv().unwrap();
                    Ok("response that must be discarded")
                });
                assert!(matches!(result, Err(CompleteError::Stopped)));
            });
            started.recv().unwrap();
            tasks.request_cancel(|| {});
            assert!(tasks.acquire(crate::coordinator::Kind::Export).is_err());
            finish.send(()).unwrap();
            worker.join().unwrap();
            assert!(tasks.acquire(crate::coordinator::Kind::Export).is_ok());
        });
    }

    #[test]
    fn cancellation_stops_http_retry_backoff_without_sending_another_request() {
        use std::io::{BufRead, Read, Write};
        let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/test", server.local_addr().unwrap());
        let cancel = AtomicBool::new(false);
        std::thread::scope(|scope| {
            scope.spawn(|| {
                let (stream, _) = server.accept().unwrap();
                stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
                let mut reader = std::io::BufReader::new(stream);
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" { break; }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = value.trim().parse::<usize>().unwrap();
                    }
                }
                reader.read_exact(&mut vec![0; length]).unwrap();
                cancel.store(true, Ordering::SeqCst);
                reader.get_mut().write_all(b"HTTP/1.1 429 Too Many Requests\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
            });
            let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(2)).build();
            let started = Instant::now();
            let result = send(&agent, "POST", &url, &[], &serde_json::json!({}), &cancel);
            assert_eq!(result.unwrap_err(), "Đã dừng.");
            assert!(started.elapsed() < Duration::from_secs(2));
        });
    }
    use serde_json::json;

    #[test]
    fn json_object_is_taken_from_around_prose() {
        let value = parse_json_value("```json\nĐây là {\"text\": \"xin chào\"}\n```").unwrap();
        assert_eq!(translated_text(&value).as_deref(), Some("xin chào"));
    }

    #[test]
    fn long_line_is_cut_on_punctuation_without_a_model() {
        let long = format!("Xin chào, {}", "đây là lời ".repeat(8));
        assert!(parts_needed(&long) > 1);
        let parts = split_line(&long);
        assert!(parts.len() > 1);
        assert!(parts
            .iter()
            .all(|part| !part.ends_with(' ') && part.chars().count() <= MAX_LINE_CHARS + 20));
        assert!(parts.iter().any(|part| part.contains("Xin chào")));
    }

    #[test]
    fn short_line_stays_whole() {
        assert_eq!(split_line("Xin chào."), vec!["Xin chào.".to_string()]);
        assert!(split_line("   ").is_empty());
    }

    #[test]
    fn time_follows_character_count() {
        let cues = divide_time(0.0, 10.0, &["aa".into(), "bbbbbbbb".into()]);
        assert_eq!(cues.len(), 2);
        assert!((cues[0].end - 2.0).abs() < 0.001);
        assert!((cues[1].end - 10.0).abs() < 0.001);
    }

    #[test]
    fn terms_match_ignoring_case() {
        let glossary = glossary_from(&json!({
            "theme": "Máy",
            "terms": [
                {"src": "CPU", "tgt": "CPU", "note": "bộ xử lý"},
                {"src": "", "tgt": "bỏ"},
                {"src": "RAM", "tgt": "RAM", "note": "bộ nhớ"}
            ]
        }))
        .unwrap();
        let matched = terms_for_line(&glossary.terms, "Đổi cpu ngay");
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].src, "CPU");
    }

    #[test]
    fn chunks_respect_line_and_char_limits() {
        let lines: Vec<String> = (0..25).map(|_| "0123456789".to_string()).collect();
        assert_eq!(chunk_ranges(&lines, 10, 600), vec![0..10, 10..20, 20..25]);
        assert_eq!(
            chunk_ranges(&lines, 100, 33),
            vec![
                0..3,
                3..6,
                6..9,
                9..12,
                12..15,
                15..18,
                18..21,
                21..24,
                24..25
            ]
        );
        assert!(chunk_ranges(&[], 10, 600).is_empty());
    }

    #[test]
    fn translate_batches_hold_about_twenty_lines() {
        assert_eq!(CHUNK_LINES, 20);
        assert_eq!(CHUNK_CHARS, 1600);
        let lines: Vec<String> = (0..21).map(|_| "x".repeat(75)).collect();
        assert_eq!(
            chunk_ranges(&lines, CHUNK_LINES, CHUNK_CHARS),
            vec![0..20, 20..21]
        );
    }

    #[test]
    fn online_batches_hold_about_forty_lines() {
        assert_eq!(ONLINE_CHUNK_LINES, 40);
        assert_eq!(ONLINE_CHUNK_CHARS, 4000);
        let lines: Vec<String> = (0..80).map(|_| "x".to_string()).collect();
        assert_eq!(chunk_ranges(&lines, CHUNK_LINES, CHUNK_CHARS).len(), 4);
        assert_eq!(
            chunk_ranges(&lines, ONLINE_CHUNK_LINES, ONLINE_CHUNK_CHARS).len(),
            2
        );
    }

    #[test]
    fn batch_prompt_numbers_lines_and_lists_context_and_terms() {
        let term = Term {
            src: "CPU".into(),
            tgt: "CPU".into(),
            note: "bộ xử lý".into(),
        };
        let prompt = batch_prompt(
            &["Hello", "World"],
            &["Before one"],
            &["After one"],
            &[&term],
            "Máy tính",
            "tiếng Việt",
        );
        assert!(prompt.contains("1. Hello"));
        assert!(prompt.contains("2. World"));
        assert!(prompt.contains("Before one"));
        assert!(prompt.contains("After one"));
        assert!(prompt.contains("\"CPU\": \"CPU\""));
        assert!(prompt.contains("đúng 2 khóa"));
    }

    #[test]
    fn wrong_language_spots_other_languages() {
        assert!(wrong_language("Hablamos sobre rutinas diarias.", "vi"));
        assert!(wrong_language("We usually meet friends at the cafe.", "vi"));
        assert!(!wrong_language(
            "Tôi thường thức dậy sớm, khoảng 7 giờ sáng.",
            "vi"
        ));
        assert!(!wrong_language("Ok", "vi"));
        assert!(wrong_language("Thường chỉ uống một tách咖啡.", "vi"));
        assert!(wrong_language(
            "Sau bữa tối, tôi dẫn chó đi dạo到公园",
            "vi"
        ));
        assert!(!wrong_language("我们通常在咖啡馆见朋友。", "ja"));
        assert_eq!(
            strip_stray_script("Tôi uống một tách咖啡 mỗi sáng", "vi"),
            "Tôi uống một tách mỗi sáng"
        );
        assert!(wrong_language("We usually meet friends at the cafe.", "zh"));
        assert!(!wrong_language("我们通常在咖啡馆见朋友。", "zh"));
        assert!(wrong_language("Hablamos sobre rutinas diarias.", "ru"));
        assert!(!wrong_language("Hablamos sobre rutinas diarias.", "es"));
    }

    #[test]
    fn batch_answer_keeps_lines_that_arrived() {
        let ok = json!({"1": " Xin chào ", "2": "Thế giới"});
        assert_eq!(
            batch_translations(&ok, 2),
            vec![Some("Xin chào".to_string()), Some("Thế giới".to_string())]
        );
        assert_eq!(
            batch_translations(&json!({"1": "a"}), 2),
            vec![Some("a".to_string()), None]
        );
        assert_eq!(
            batch_translations(&json!({"1": "a", "2": "  "}), 2),
            vec![Some("a".to_string()), None]
        );
        assert_eq!(batch_translations(&json!(["a", "b"]), 2), vec![None, None]);
    }

    #[test]
    fn shorten_percent_matches_the_room_left() {
        // 2.0 * 0.95 / 3.8 = 0.50
        assert_eq!(shorten_percent(2.0, 3.8), 50);
        assert_eq!(shorten_percent(1.0, 0.0), 90);
    }

    #[test]
    fn shorten_batch_keeps_each_line_when_one_is_missing() {
        let prompt = shorten_batch_prompt(
            &[(70, "Câu dài thứ nhất."), (40, "Câu dài thứ hai.")],
            "tiếng Việt",
        );
        assert!(prompt.contains("1. (70%) Câu dài thứ nhất."));
        assert!(prompt.contains("2. (40%) Câu dài thứ hai."));
        assert!(prompt.contains("đúng 2 khóa"));
        assert!(prompt.contains("câu nói"));
        assert!(prompt.contains("tốc độ bình thường"));
        assert!(!prompt.contains("từ đệm"));
        let texts = shorten_batch_texts(&json!({"1": " Câu ngắn. ", "2": "  "}), 2);
        assert_eq!(texts[0].as_deref(), Some("Câu ngắn."));
        assert!(texts[1].is_none());
        assert_eq!(
            shorten_batch_texts(&json!("không phải object"), 2),
            vec![None, None]
        );
    }

    #[test]
    fn gemini_answer_skips_a_thought_part() {
        let body = json!({
            "candidates": [{
                "content": {
                    "parts": [
                        {"thought": true, "text": "suy nghĩ"},
                        {"text": "{\"1\":\"Xin chào\"}"}
                    ]
                }
            }]
        });
        assert_eq!(
            gemini_answer(&body).as_deref(),
            Some("{\"1\":\"Xin chào\"}")
        );
        assert!(gemini_answer(&json!({})).is_none());
    }

    #[test]
    fn gemini_key_travels_in_a_header_not_the_url() {
        assert!(!GEMINI_URL.contains("key="));
        assert!(GEMINI_URL.starts_with("https://generativelanguage.googleapis.com/"));
    }

    #[test]
    fn deepseek_asks_for_json_and_skips_thinking() {
        let payload = deepseek_payload("deepseek-flash", "dịch câu này", true);
        assert_eq!(payload["model"], "deepseek-flash");
        assert_eq!(payload["response_format"]["type"], "json_object");
        assert_eq!(payload["thinking"]["type"], "disabled");
        assert_eq!(payload["messages"][0]["content"], "dịch câu này");
        assert!(!DEEPSEEK_URL.contains("key="));
        assert!(DEEPSEEK_URL.starts_with("https://api.deepseek.com/"));
        assert_eq!(DEEPSEEK_MODEL, "deepseek-flash");
    }

    #[test]
    fn deepseek_local_talks_to_the_local_server_and_skips_thinking() {
        let payload = deepseek_local_payload("deepseek-chat", "dịch câu này", true);
        assert_eq!(payload["model"], "deepseek-chat");
        assert!(payload.get("response_format").is_none());
        assert!(payload.get("max_tokens").is_none());
        assert_eq!(payload["thinking"], false);
        assert_eq!(payload["search"], false);
        assert_eq!(payload["stream"], false);
        assert_eq!(payload["messages"][0]["content"], "dịch câu này");
        assert_eq!(
            DEEPSEEK_LOCAL_URL,
            "http://127.0.0.1:8000/v1/chat/completions"
        );
        assert_eq!(DEEPSEEK_LOCAL_HEALTH_URL, "http://127.0.0.1:8000/healthz");
        assert_eq!(DEEPSEEK_LOCAL_MODEL, "deepseek-chat");
        // Hai đường DeepSeek không dùng chung URL hay tên model.
        assert_ne!(DEEPSEEK_URL, DEEPSEEK_LOCAL_URL);
        assert_ne!(DEEPSEEK_MODEL, DEEPSEEK_LOCAL_MODEL);
        assert!(DEEPSEEK_LOCAL_URL.starts_with("http://127.0.0.1:"));
        assert_eq!(strip_think("  <think>suy</think>Xin chào  "), "Xin chào");
        let down = deepseek_local_transport_message(
            ureq::ErrorKind::ConnectionFailed,
            Some(std::io::ErrorKind::ConnectionRefused),
        );
        assert!(down.contains("Bật DeepSeek"));
        assert_eq!(down, DEEPSEEK_LOCAL_DOWN);
        let timed_out = deepseek_local_transport_message(
            ureq::ErrorKind::Io,
            Some(std::io::ErrorKind::TimedOut),
        );
        assert!(timed_out.contains("5 phút"));
        assert!(timed_out.contains("đăng nhập"));
        assert!(!timed_out.contains("Chưa chạy"));
        let reset = deepseek_local_transport_message(
            ureq::ErrorKind::Io,
            Some(std::io::ErrorKind::ConnectionReset),
        );
        assert!(reset.contains("Mất kết nối"));
    }

    #[test]
    fn only_a_translating_deepseek_local_run_needs_the_server() {
        let choice = |provider: &str| TranslationChoice {
            provider: provider.into(),
            api_key: String::new(),
            model: String::new(),
            saved: Default::default(),
        };
        assert!(needs_deepseek_local(&choice("deepseek-local"), false));
        // Không dịch, chỉ đổi giọng: không gọi AI nên không cần server.
        assert!(!needs_deepseek_local(&choice("deepseek-local"), true));
        for other in ["local", "deepseek", "gemini", "openai", "claude"] {
            assert!(!needs_deepseek_local(&choice(other), false), "{other}");
        }
        // Các trường hợp này không hỏi server, nên kết quả không phụ thuộc cổng 8000 trên máy chạy test.
        assert!(require_deepseek_local(&choice("deepseek"), false).is_ok());
        assert!(require_deepseek_local(&choice("deepseek-local"), true).is_ok());
        assert!(DEEPSEEK_LOCAL_DOWN.contains("Bật DeepSeek"));
        assert!(DEEPSEEK_LOCAL_DOWN.contains("Đồ nghề"));
    }

    #[test]
    fn local_endpoint_stays_on_this_machine() {
        assert!(LOCAL_URL.starts_with("http://127.0.0.1:11434"));
        assert_eq!(LOCAL_MODEL, "qwen3.5:9b-q4_K_M");
    }

    #[test]
    fn ollama_payload_skips_thinking_and_keeps_context_small() {
        let payload = ollama_payload("qwen3.5:9b-q4_K_M", "dịch câu này", true);
        assert_eq!(payload["think"], false);
        assert_eq!(payload["keep_alive"], "30m");
        assert_eq!(payload["options"]["num_ctx"], 4096);
        assert_eq!(payload["stream"], false);
        assert_eq!(payload["format"], "json");
        assert_eq!(payload["messages"][0]["content"], "dịch câu này");
    }

    #[test]
    fn a_listen_part_asks_every_model_for_prose_not_json() {
        let gemini = gemini_payload(GEMINI_MODEL, "đoạn", false);
        assert!(gemini["generationConfig"].get("responseMimeType").is_none());
        assert_eq!(
            gemini["generationConfig"]["maxOutputTokens"],
            PROSE_MAX_TOKENS
        );
        assert_eq!(
            gemini["generationConfig"]["thinkingConfig"]["thinkingLevel"],
            "low"
        );
        let gemini_json = gemini_payload(GEMINI_MODEL, "đoạn", true);
        assert_eq!(
            gemini_json["generationConfig"]["responseMimeType"],
            "application/json"
        );
        assert!(gemini_json["generationConfig"]
            .get("maxOutputTokens")
            .is_none());
        assert_eq!(
            gemini_json["generationConfig"]["thinkingConfig"]["thinkingLevel"],
            "low"
        );
        assert_eq!(
            gemini_payload("gemini-2.5-flash", "đoạn", true)["generationConfig"]["thinkingConfig"]
                ["thinkingBudget"],
            0
        );
        assert_eq!(
            gemini_payload("gemini-2.5-pro", "đoạn", true)["generationConfig"]["thinkingConfig"]
                ["thinkingBudget"],
            128
        );
        assert_eq!(
            gemini_payload("gemini-3-flash-preview", "đoạn", true)["generationConfig"]
                ["thinkingConfig"]["thinkingLevel"],
            "minimal"
        );
        assert!(
            gemini_payload("gemini-2.0-flash", "đoạn", true)["generationConfig"]
                .get("thinkingConfig")
                .is_none()
        );

        let openai = openai_payload("gpt-4o-mini", "đoạn", false);
        assert!(openai.get("response_format").is_none());
        assert_eq!(openai["max_completion_tokens"], PROSE_MAX_TOKENS);
        assert!(openai_payload("gpt-4o-mini", "đoạn", true)
            .get("max_completion_tokens")
            .is_none());

        let claude = claude_payload(CLAUDE_MODEL, "đoạn", PROSE_MAX_TOKENS);
        assert_eq!(claude["max_tokens"], PROSE_MAX_TOKENS);
        assert!(claude.get("response_format").is_none());

        let deepseek = deepseek_payload("deepseek-flash", "đoạn", false);
        assert!(deepseek.get("response_format").is_none());
        assert_eq!(deepseek["thinking"]["type"], "disabled");
        assert_eq!(deepseek["max_tokens"], PROSE_MAX_TOKENS);
        let deepseek_json = deepseek_payload("deepseek-flash", "đoạn", true);
        assert_eq!(deepseek_json["max_tokens"], PROSE_MAX_TOKENS);
        assert_eq!(deepseek_json["response_format"]["type"], "json_object");

        let deepseek_local = deepseek_local_payload("deepseek-chat", "đoạn", false);
        assert!(deepseek_local.get("response_format").is_none());
        assert_eq!(deepseek_local["thinking"], false);
        assert_eq!(deepseek_local["search"], false);
        let deepseek_local_json = deepseek_local_payload("deepseek-chat", "đoạn", true);
        assert!(deepseek_local_json.get("response_format").is_none());
        assert!(deepseek_local_json.get("max_tokens").is_none());

        let local = ollama_payload(LOCAL_MODEL, "đoạn", false);
        assert!(local.get("format").is_none());
        assert_eq!(local["think"], false);
        assert_eq!(local["options"]["num_ctx"], 8192);
        assert_eq!(local["options"]["num_predict"], PROSE_MAX_TOKENS);
        assert!(ollama_payload(LOCAL_MODEL, "đoạn", true)["options"]
            .get("num_predict")
            .is_none());
    }

    #[test]
    fn prose_prompt_names_the_language_and_keeps_a_short_tail() {
        let prompt = prose_prompt("Hello there.", "tiếng Việt", &"à".repeat(500));
        assert!(prompt.contains("tiếng Việt"));
        assert!(prompt.contains("<text>\nHello there.\n</text>"));
        assert!(prompt.contains("không dịch lại"));
        assert!(prompt.contains(&"à".repeat(450)));
        assert!(!prompt.contains(&"à".repeat(451)));
        assert!(!prose_prompt("Hi.", "tiếng Việt", "  ").contains("không dịch lại"));
    }

    #[test]
    fn unwrap_prose_reads_a_fenced_json_text_field() {
        assert_eq!(
            unwrap_prose("```json\n{\"text\":\" Xin chào. \"}\n```"),
            "Xin chào."
        );
        assert_eq!(unwrap_prose("  Xin chào.  "), "Xin chào.");
    }

    fn hit(
        batch: u32,
        attempt: u32,
        load_ns: u64,
        eval_ns: u64,
        eval_count: u64,
        total_ns: u64,
        wall_ms: u64,
    ) -> LocalHit {
        LocalHit {
            batch,
            attempt,
            load_ns,
            prompt_ns: 0,
            prompt_count: 0,
            eval_ns,
            eval_count,
            total_ns,
            wall: Duration::from_millis(wall_ms),
        }
    }

    #[test]
    fn local_timings_read_durations_in_nanoseconds() {
        let body = serde_json::json!({
            "load_duration": 11_000_000_000u64,
            "prompt_eval_count": 471,
            "prompt_eval_duration": 640_000_000u64,
            "eval_count": 227,
            "eval_duration": 6_830_000_000u64,
            "total_duration": 18_400_000_000u64
        });
        let timings = local_timings(&body);
        assert_eq!(timings.load_ns, 11_000_000_000);
        assert_eq!(timings.prompt_count, 471);
        assert_eq!(timings.prompt_ns, 640_000_000);
        assert_eq!(timings.eval_count, 227);
        assert_eq!(timings.eval_ns, 6_830_000_000);
        assert_eq!(timings.total_ns, 18_400_000_000);
        assert_eq!(local_timings(&serde_json::json!({})).eval_count, 0);
    }

    #[test]
    fn translate_summary_counts_extra_calls_and_tok_spread() {
        let hits = vec![
            hit(0, 1, 10_000_000_000, 100_000_000, 5, 10_100_000_000, 12_000),
            hit(1, 1, 100_000_000, 5_000_000_000, 150, 5_200_000_000, 7_000),
            hit(1, 2, 0, 4_000_000_000, 100, 4_100_000_000, 4_500),
            hit(2, 1, 0, 2_000_000_000, 80, 2_100_000_000, 2_200),
        ];
        let line = format_summary(&hits);
        assert!(line.contains("2 lượt"), "{line}");
        assert!(line.contains("3 request"), "{line}");
        assert!(line.contains("1 lần gọi thêm"), "{line}");
        assert!(line.contains("warm load 10.00s"), "{line}");
        assert!(line.contains("tok/s 25–40"), "{line}");
        let one = format_hit(&hits[1]);
        assert!(one.contains("lượt 1 lần 1"), "{one}");
        assert!(one.contains("eval 5.00s (150, 30 tok/s)"), "{one}");
        assert!(one.contains("wall 7.00s"), "{one}");
    }
}
