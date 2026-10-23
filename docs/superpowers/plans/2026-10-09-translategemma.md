# Nhánh dịch TranslateGemma

> Đã chốt lại ngày 2026-10-09 sau khi thử model trên máy: giữ nguyên JSON lô của Qwen và Gemma 3. Không làm prompt từng câu, không lưu mã Whisper. Việc còn lại là bỏ bước thuật ngữ khi model là TranslateGemma, vì lượt thử trả `{"theme":"","terms":[]}`. Các task phía dưới không còn được thực hiện.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Khi model local có tên bắt đầu bằng `translategemma`, dịch từng câu bằng đúng mẫu prompt của Ollama, với ngôn ngữ gốc Whisper nhận được và ngôn ngữ đích người dùng đã chọn.

**Architecture:** Qwen và Gemma 3 giữ nguyên lô JSON 20 câu. TranslateGemma đi nhánh riêng trong `ollama.rs` và `pipeline.rs`: một câu một request, không `format: json`, không bước thuật ngữ, không nhờ model rút câu. Mã ngôn ngữ gốc lấy từ `result.language` của Whisper và được lưu vào `transcript.json` trước khi file đó bị ghi thành danh sách câu. Không thêm ô chọn ngôn ngữ gốc.

**Tech Stack:** Rust (Tauri 2), Ollama `http://127.0.0.1:11434/api/chat`, whisper.cpp `-l auto`. Không thêm crate. Không sửa giao diện.

**Spec:** Hội thoại 2026-10-09. Không có file spec riêng. Yêu cầu đã chốt nằm ở Global Constraints.

## Global Constraints

- Giữ Tauri 2. Video và audio không rời máy.
- Không thêm ô chọn ngôn ngữ gốc. Whisper tự nhận (`-l auto` đã có trong `transcribe`).
- Prompt TranslateGemma điền cặp mã thật. Nguồn là mã Whisper ghi. Đích là mã người dùng chọn ở màn lồng tiếng (`clean_target`). Không gắn sẵn `English (en)` sang `Vietnamese (vi)`.
- Chữ Latin mà không còn mã thì dừng và báo thiếu ngôn ngữ gốc. Không gán tiếng Anh.
- Qwen `qwen3.5:9b-q4_K_M` và Gemma 3 `gemma3:12b` giữ payload hiện tại: `format: json`, `think: false`, `num_ctx` 4096, lô khoảng 20 câu.
- Tám bước giữ nguyên: `extract`, `transcribe`, `split`, `terms`, `translate`, `speak`, `fit`, `merge`. Không thêm id bước. Không đổi `productName`, identifier, tiêu đề, chữ trên giao diện.
- File trung gian cũ (mảng câu `{start, end, text}`) vẫn đọc được.
- Bước fit giữ `MAX_SPEED` 1,4. Nhánh này không gọi model để rút câu.
- Trả lời và chuỗi hiển thị bằng tiếng Việt.
- Không commit trừ khi người dùng yêu cầu trong lúc thực thi.
- Test: `cd D:\Project\translate-video\src-tauri; cargo test --offline`.

## File Structure

| File | Việc |
|---|---|
| `src-tauri/src/ollama.rs` | Bảng tên ngôn ngữ, `gemma_prompt`, `is_translate_gemma`, payload không `format` khi là TranslateGemma, `complete_plain`. |
| `src-tauri/src/pipeline.rs` | Đọc và lưu mã Whisper, nhận bộ chữ khi file cũ mất mã, nhánh dịch từng câu, bỏ thuật ngữ, bỏ rút câu. |

`src/guides.ts` và `SUGGESTED_LOCAL_MODELS` đã có `translategemma:4b`. Không sửa.

---

### Task 1: Prompt theo cặp ngôn ngữ thật

**Files:**
- Modify: `src-tauri/src/ollama.rs` (thêm sau `LOCAL_MODEL`, và test trong `mod tests` cuối file)
- Test: `src-tauri/src/ollama.rs` (`mod tests`)

**Interfaces:**
- Consumes: không
- Produces:
  - `pub(crate) fn is_translate_gemma(model: &str) -> bool`
  - `pub(crate) fn language_name(code: &str) -> Option<&'static str>`
  - `pub(crate) fn gemma_prompt(source_code: &str, target_code: &str, text: &str) -> Result<String, String>`

- [ ] **Step 1: Write the failing test**

Thêm vào `mod tests` của `ollama.rs`:

```rust
#[test]
fn gemma_prompt_uses_the_detected_pair_not_a_fixed_english_to_vietnamese() {
    let spanish = gemma_prompt("es", "fr", "¿Cómo estás?").unwrap();
    assert!(spanish.contains("You are a professional Spanish (es) to French (fr) translator."));
    assert!(spanish.contains("Please translate the following Spanish text into French:"));
    assert!(spanish.ends_with("\n\n\n¿Cómo estás?"));
    assert!(!spanish.contains("English (en)"));
    assert!(!spanish.contains("Vietnamese"));

    let japanese = gemma_prompt("ja", "en", "こんにちは").unwrap();
    assert!(japanese.contains("Japanese (ja) to English (en)"));
    assert!(japanese.ends_with("\n\n\nこんにちは"));

    assert!(gemma_prompt("xx", "vi", "hola").is_err());
    assert!(gemma_prompt("es", "xx", "hola").is_err());
    assert_eq!(language_name("VI"), Some("Vietnamese"));
    assert_eq!(language_name("zh"), Some("Chinese"));
    assert!(language_name("auto").is_none());
    assert!(is_translate_gemma("translategemma:4b"));
    assert!(is_translate_gemma("TranslateGemma:12b"));
    assert!(!is_translate_gemma("gemma3:12b"));
    assert!(!is_translate_gemma("qwen3.5:9b-q4_K_M"));
    assert!(!is_translate_gemma(""));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd D:\Project\translate-video\src-tauri; cargo test --offline gemma_prompt_uses_the_detected_pair -- --nocapture`

Expected: FAIL, `gemma_prompt` không tồn tại.

- [ ] **Step 3: Write minimal implementation**

Trong `ollama.rs`, sau `LOCAL_MODEL`:

```rust
pub(crate) fn is_translate_gemma(model: &str) -> bool {
    model.trim().to_ascii_lowercase().starts_with("translategemma")
}

/// Tên tiếng Anh trong mẫu prompt của TranslateGemma. Mã là mã Whisper / ISO 639-1.
pub(crate) fn language_name(code: &str) -> Option<&'static str> {
    let code = code.trim().to_ascii_lowercase();
    let code = code.split(['-', '_']).next().unwrap_or("");
    LANGUAGE_NAMES.iter().find(|(known, _)| *known == code).map(|(_, name)| *name)
}

const LANGUAGE_NAMES: &[(&str, &str)] = &[
    ("en", "English"),
    ("zh", "Chinese"),
    ("de", "German"),
    ("es", "Spanish"),
    ("ru", "Russian"),
    ("ko", "Korean"),
    ("fr", "French"),
    ("ja", "Japanese"),
    ("pt", "Portuguese"),
    ("tr", "Turkish"),
    ("pl", "Polish"),
    ("ca", "Catalan"),
    ("nl", "Dutch"),
    ("ar", "Arabic"),
    ("sv", "Swedish"),
    ("it", "Italian"),
    ("id", "Indonesian"),
    ("hi", "Hindi"),
    ("fi", "Finnish"),
    ("vi", "Vietnamese"),
    ("he", "Hebrew"),
    ("uk", "Ukrainian"),
    ("el", "Greek"),
    ("ms", "Malay"),
    ("cs", "Czech"),
    ("ro", "Romanian"),
    ("da", "Danish"),
    ("hu", "Hungarian"),
    ("ta", "Tamil"),
    ("no", "Norwegian"),
    ("th", "Thai"),
    ("ur", "Urdu"),
    ("hr", "Croatian"),
    ("bg", "Bulgarian"),
    ("lt", "Lithuanian"),
    ("la", "Latin"),
    ("mi", "Maori"),
    ("ml", "Malayalam"),
    ("cy", "Welsh"),
    ("sk", "Slovak"),
    ("te", "Telugu"),
    ("fa", "Persian"),
    ("lv", "Latvian"),
    ("bn", "Bengali"),
    ("sr", "Serbian"),
    ("az", "Azerbaijani"),
    ("sl", "Slovenian"),
    ("kn", "Kannada"),
    ("et", "Estonian"),
    ("mk", "Macedonian"),
    ("br", "Breton"),
    ("eu", "Basque"),
    ("is", "Icelandic"),
    ("hy", "Armenian"),
    ("ne", "Nepali"),
    ("mn", "Mongolian"),
    ("bs", "Bosnian"),
    ("kk", "Kazakh"),
    ("sq", "Albanian"),
    ("sw", "Swahili"),
    ("gl", "Galician"),
    ("mr", "Marathi"),
    ("pa", "Punjabi"),
    ("si", "Sinhala"),
    ("km", "Khmer"),
    ("sn", "Shona"),
    ("yo", "Yoruba"),
    ("so", "Somali"),
    ("af", "Afrikaans"),
    ("oc", "Occitan"),
    ("ka", "Georgian"),
    ("be", "Belarusian"),
    ("tg", "Tajik"),
    ("sd", "Sindhi"),
    ("gu", "Gujarati"),
    ("am", "Amharic"),
    ("yi", "Yiddish"),
    ("lo", "Lao"),
    ("uz", "Uzbek"),
    ("fo", "Faroese"),
    ("ht", "Haitian Creole"),
    ("ps", "Pashto"),
    ("tk", "Turkmen"),
    ("nn", "Nynorsk"),
    ("mt", "Maltese"),
    ("sa", "Sanskrit"),
    ("lb", "Luxembourgish"),
    ("my", "Myanmar"),
    ("bo", "Tibetan"),
    ("tl", "Tagalog"),
    ("mg", "Malagasy"),
    ("as", "Assamese"),
    ("tt", "Tatar"),
    ("haw", "Hawaiian"),
    ("ln", "Lingala"),
    ("ha", "Hausa"),
    ("ba", "Bashkir"),
    ("jw", "Javanese"),
    ("su", "Sundanese"),
];

/// Mẫu một tin nhắn của Ollama cho TranslateGemma. Hai dòng trống đứng ngay trước câu gốc.
pub(crate) fn gemma_prompt(source_code: &str, target_code: &str, text: &str) -> Result<String, String> {
    let source_code = source_code.trim().to_ascii_lowercase();
    let source_code = source_code.split(['-', '_']).next().unwrap_or("");
    let target_code = target_code.trim().to_ascii_lowercase();
    let target_code = target_code.split(['-', '_']).next().unwrap_or("");
    let source = language_name(source_code).ok_or_else(|| format!("Chưa có tên cho ngôn ngữ gốc {source_code}."))?;
    let target = language_name(target_code).ok_or_else(|| format!("Chưa có tên cho ngôn ngữ đích {target_code}."))?;
    Ok(format!(
        "You are a professional {source} ({source_code}) to {target} ({target_code}) translator. Your goal is to accurately convey the meaning and nuances of the original {source} text while adhering to {target} grammar, vocabulary, and cultural sensitivities.\n\
         Produce only the {target} translation, without any additional explanations or commentary. Please translate the following {source} text into {target}:\n\
         \n\
         \n\
         {text}"
    ))
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd D:\Project\translate-video\src-tauri; cargo test --offline gemma_prompt_uses_the_detected_pair -- --nocapture`

Expected: PASS

- [ ] **Step 5: Không commit**

---

### Task 2: Request trả chữ, không ép JSON

**Files:**
- Modify: `src-tauri/src/ollama.rs` (`ollama_payload` khoảng dòng 417, thêm `complete_plain` cạnh `complete_timed`)
- Test: `src-tauri/src/ollama.rs` (`mod tests`)

**Interfaces:**
- Consumes: `is_translate_gemma` từ task 1
- Produces:
  - `ollama_payload` bỏ khóa `format` khi model là TranslateGemma. Model khác vẫn `"format": "json"`.
  - `pub(crate) fn complete_plain(choice: &TranslationChoice, prompt: &str, cancel: &AtomicBool, trace: &mut TranslateTrace) -> Result<String, CompleteError>`

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn translategemma_payload_does_not_force_json() {
    let gemma = ollama_payload("translategemma:4b", "dịch câu này");
    assert!(gemma.get("format").is_none());
    assert_eq!(gemma["think"], false);
    assert_eq!(gemma["keep_alive"], "30m");
    assert_eq!(gemma["options"]["num_ctx"], 4096);
    assert_eq!(gemma["messages"][0]["content"], "dịch câu này");

    let qwen = ollama_payload("qwen3.5:9b-q4_K_M", "dịch câu này");
    assert_eq!(qwen["format"], "json");
}
```

Test `ollama_payload_skips_thinking_and_keeps_context_small` hiện có phải vẫn PASS. Không sửa assertion `format == json` của test đó.

- [ ] **Step 2: Run test to verify it fails**

Run: `cd D:\Project\translate-video\src-tauri; cargo test --offline translategemma_payload_does_not_force_json -- --nocapture`

Expected: FAIL vì payload TranslateGemma vẫn có `format`.

- [ ] **Step 3: Write minimal implementation**

Đổi `ollama_payload` thành:

```rust
fn ollama_payload(model: &str, prompt: &str) -> Value {
    let mut payload = serde_json::json!({
        "model": model,
        "stream": false,
        "think": false,
        "keep_alive": "30m",
        "options": { "num_ctx": OLLAMA_NUM_CTX },
        "messages": [{"role": "user", "content": prompt}],
    });
    if !is_translate_gemma(model) {
        payload["format"] = serde_json::json!("json");
    }
    payload
}
```

Thêm `complete_plain` ngay sau `complete_timed`. Hàm này gọi `request_cancelable`, ghi `trace.record`, trả `text.trim()`. Không gọi `parse_json_value`. Không nối câu "Chỉ trả về một JSON object".

```rust
pub(crate) fn complete_plain(
    choice: &TranslationChoice,
    prompt: &str,
    cancel: &AtomicBool,
    trace: &mut TranslateTrace,
) -> Result<String, CompleteError> {
    let started = Instant::now();
    match request_cancelable(choice, prompt, cancel) {
        Ok((text, timings)) => {
            trace.record(&timings, started.elapsed());
            let text = text.trim().to_string();
            if text.is_empty() {
                Err(CompleteError::BadJson)
            } else {
                Ok(text)
            }
        }
        Err(error) => {
            trace.record(&LocalTimings::default(), started.elapsed());
            Err(error)
        }
    }
}
```

`CompleteError::BadJson` đang hiện "Model không trả JSON dùng được." Với nhánh này câu đó sai nghĩa. Đổi `Display` của `BadJson` thành "Model không trả lời dùng được." Test nào so đúng câu cũ thì cập nhật theo câu mới. Không thêm variant mới.

- [ ] **Step 4: Run test to verify it passes**

Run: `cd D:\Project\translate-video\src-tauri; cargo test --offline ollama_payload -- --nocapture`

Expected: PASS cả `translategemma_payload_does_not_force_json` và `ollama_payload_skips_thinking_and_keeps_context_small`.

- [ ] **Step 5: Không commit**

---

### Task 3: Giữ mã ngôn ngữ Whisper

**Files:**
- Modify: `src-tauri/src/pipeline.rs` (`cues_from_value` khoảng dòng 1399, `transcribe` khoảng dòng 671, test `whisper_json_and_saved_cues_keep_times`)
- Test: `src-tauri/src/pipeline.rs` (`mod tests`)

**Interfaces:**
- Consumes: `language_name` từ task 1
- Produces:
  - `fn spoken_language(value: &Value) -> Option<String>` — mã chữ thường, hai hoặc ba ký tự, có trong `language_name`
  - `fn script_language(text: &str) -> Option<&'static str>` — `ja`, `zh`, `ko`, `ru`, `th`, hoặc `None`
  - `fn source_language(value: &Value, lines: &[Cue]) -> Result<String, String>`
  - `cues_from_value` đọc thêm khóa `cues`
  - `transcribe` sau khi Whisper ghi file: đọc mã, rồi ghi `{"language":"<mã>","cues":[...]}`

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn source_language_comes_from_whisper_and_not_from_latin_text() {
    let whisper = json!({
        "result": {"language": "es"},
        "transcription": [{
            "timestamps": {"from": "00:00:01,000", "to": "00:00:02,000"},
            "text": " Hola"
        }]
    });
    let cues = cues_from_value(&whisper).unwrap();
    assert_eq!(spoken_language(&whisper).as_deref(), Some("es"));
    assert_eq!(source_language(&whisper, &cues).unwrap(), "es");

    let saved = json!({
        "language": "ja",
        "cues": [{"start": 1.0, "end": 2.0, "text": "こんにちは"}]
    });
    let cues = cues_from_value(&saved).unwrap();
    assert_eq!(cues[0].text, "こんにちは");
    assert_eq!(source_language(&saved, &cues).unwrap(), "ja");

    let old = json!([{"start": 0.0, "end": 1.0, "text": "Hola, ¿cómo estás?"}]);
    let cues = cues_from_value(&old).unwrap();
    assert!(source_language(&old, &cues).is_err());

    assert_eq!(script_language("こんにちは、東京"), Some("ja"));
    assert_eq!(script_language("我们通常在咖啡馆见朋友"), Some("zh"));
    assert_eq!(script_language("안녕하세요"), Some("ko"));
    assert_eq!(script_language("Привет"), Some("ru"));
    assert_eq!(script_language("สวัสดี"), Some("th"));
    assert!(script_language("Chúng tôi thường gặp bạn").is_none());
    assert!(script_language("Hello there").is_none());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd D:\Project\translate-video\src-tauri; cargo test --offline source_language_comes_from_whisper -- --nocapture`

Expected: FAIL, `spoken_language` không tồn tại. `cues_from_value` với khóa `cues` có thể trả `None`.

- [ ] **Step 3: Write minimal implementation**

Trong `cues_from_value`, trước nhánh `transcription`:

```rust
if let Some(items) = value.get("cues").and_then(Value::as_array) {
    return Some(items.iter().filter_map(cue_from_saved).collect());
}
```

Thêm cạnh `cues_from_value`:

```rust
fn spoken_language(value: &Value) -> Option<String> {
    let raw = value
        .get("language")
        .and_then(Value::as_str)
        .or_else(|| value.pointer("/result/language").and_then(Value::as_str))?;
    let code = raw.trim().to_ascii_lowercase();
    let code = code.split(['-', '_']).next().unwrap_or("");
    if code.is_empty() || code == "auto" || crate::ollama::language_name(code).is_none() {
        return None;
    }
    Some(code.to_string())
}

/// Bộ chữ không lẫn với chữ Latin. Chữ Hán không kèm kana thì là zh. Chữ Latin trả None.
fn script_language(text: &str) -> Option<&'static str> {
    let mut han = 0usize;
    let mut kana = 0usize;
    let mut hangul = 0usize;
    let mut cyrillic = 0usize;
    let mut thai = 0usize;
    for ch in text.chars() {
        if ('\u{3040}'..='\u{30ff}').contains(&ch) {
            kana += 1;
        } else if ('\u{4e00}'..='\u{9fff}').contains(&ch) {
            han += 1;
        } else if ('\u{ac00}'..='\u{d7af}').contains(&ch) {
            hangul += 1;
        } else if ('\u{0400}'..='\u{04ff}').contains(&ch) {
            cyrillic += 1;
        } else if ('\u{0e00}'..='\u{0e7f}').contains(&ch) {
            thai += 1;
        }
    }
    let ranked = [(kana + han, if kana > 0 { "ja" } else { "" }), (hangul, "ko"), (cyrillic, "ru"), (thai, "th"), (han, "zh")];
    ranked
        .into_iter()
        .filter(|(count, code)| *count > 0 && !code.is_empty())
        .max_by_key(|(count, _)| *count)
        .map(|(_, code)| code)
}

fn source_language(value: &Value, lines: &[Cue]) -> Result<String, String> {
    if let Some(code) = spoken_language(value) {
        return Ok(code);
    }
    let joined = lines.iter().map(|line| line.text.as_str()).collect::<Vec<_>>().join(" ");
    if let Some(code) = script_language(&joined) {
        return Ok(code.to_string());
    }
    Err("Không còn mã ngôn ngữ gốc trong bản chép lời. Chạy lại từ bước chép lời.".into())
}
```

`ranked` như trên sẽ chọn `ja` khi có kana vì `kana + han` lớn hơn `han`. Khi chỉ có Hán, mục `ja` bị loại vì `code` rỗng, còn `zh` được chọn. Khi hòa số chữ, `max_by_key` lấy phần tử sau. Với lời thoại một ngôn ngữ thì một bộ chữ áp đảo.

Trong `transcribe`, nhánh file đã có giữ nguyên (`read_cues` rồi return). Nhánh vừa chạy Whisper, thay đoạn đọc rồi `write_json(&job.transcript(), &cues)` bằng:

```rust
let raw: Value = read_json(&job.transcript())?;
let cues = cues_from_value(&raw).ok_or_else(|| "Không đọc được transcript.json.".to_string())?;
if cues.is_empty() {
    return Err("Không nghe được lời thoại.".into());
}
let language = spoken_language(&raw).or_else(|| script_language(&cues.iter().map(|cue| cue.text.as_str()).collect::<Vec<_>>().join(" ")).map(str::to_string));
let Some(language) = language else {
    return Err("Whisper không ghi mã ngôn ngữ gốc. Chạy lại bước chép lời.".into());
};
write_json(&job.transcript(), &json!({"language": language, "cues": cues}))?;
```

Đổi dòng import đầu `pipeline.rs` từ `use serde_json::Value;` thành `use serde_json::{json, Value};`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cd D:\Project\translate-video\src-tauri; cargo test --offline source_language_comes_from_whisper whisper_json_and_saved_cues_keep_times -- --nocapture`

Expected: PASS. Test cũ `whisper_json_and_saved_cues_keep_times` vẫn đọc được mảng `transcription`.

- [ ] **Step 5: Không commit**

---

### Task 4: Dịch từng câu, bỏ thuật ngữ và bỏ rút câu

**Files:**
- Modify: `src-tauri/src/pipeline.rs` (`terms` khoảng dòng 760, `translate` khoảng dòng 813, `translate_work` khoảng dòng 886, `speak` khoảng dòng 1011)
- Modify: `src-tauri/src/pipeline.rs` phần `use crate::ollama::...` để thêm `complete_plain`, `gemma_prompt`, `is_translate_gemma`

**Interfaces:**
- Consumes: `is_translate_gemma`, `gemma_prompt`, `complete_plain`, `source_language`, `model_or` (hàm private trong `ollama.rs` — nếu `translate_gemma` cần model đã chọn thì so `is_translate_gemma` trên `choice.model` sau khi áp default: nếu `choice.model.trim()` rỗng thì model là `LOCAL_MODEL`, không phải TranslateGemma)
- Produces: không có hàm mới cho file khác. Nhánh nằm trong `translate`, `terms`, `speak`.

Hành vi:

- `terms`: nếu `is_translate_gemma` với model đang chọn (rỗng nghĩa là `LOCAL_MODEL`), `report(..., "skipped", "Model này chỉ dịch từng câu, không lấy thuật ngữ.")` và trả glossary rỗng. Không gọi `summary_prompt`. Công tắc thuật ngữ bật cũng bỏ.
- `translate`: đọc raw JSON của `job.transcript()` bằng `read_json`. Gọi `source_language`. Lỗi thì trả lỗi đó, bước dịch dừng. Nếu model là TranslateGemma thì gọi `translate_gemma` thay cho `translate_work`.
- `translate_gemma`: cùng phần chuẩn bị `done` như `translate` (file cũ, xóa câu sai ngôn ngữ). Không gửi warm-up `Trả về JSON {"ok":true}`. Với mỗi câu còn trống, tối đa 4 lần `complete_plain` cùng một `gemma_prompt(source_code, target_code, &texts[i])`. Câu đạt `!wrong_language` thì ghi. Hết 4 lần thì `strip_stray_script` như nhánh `again` hiện tại; vẫn hỏng thì đẩy số câu vào `skipped`. `progress` dạng `Câu {i}/{total}.`. `save` sau mỗi câu. `trace.begin((i + 1) as u32)` trước mỗi câu.
- `speak`: nếu model là TranslateGemma, gọi `synthesize_missing` rồi `report` done và return. Không vào vòng `MAX_SHORTEN_ROUNDS`. `fit` không đổi.

- [ ] **Step 1: Write the failing test**

Test thuần, không gọi Ollama. Kiểm tra điều kiện rẽ nhánh và prompt của một câu:

```rust
#[test]
fn translategemma_line_prompt_follows_whisper_code_and_the_chosen_target() {
    let prompt = crate::ollama::gemma_prompt("es", "vi", "¿Cómo estás?").unwrap();
    assert!(prompt.contains("Spanish (es) to Vietnamese (vi)"));
    assert!(prompt.ends_with("\n\n\n¿Cómo estás?"));
    assert!(crate::ollama::is_translate_gemma("translategemma:4b"));
    assert!(!crate::ollama::is_translate_gemma(crate::ollama::LOCAL_MODEL));
}
```

Test này PASS ngay sau task 1. Nó khóa cặp `es` → `vi` để task 4 không viết prompt khác. Phần rẽ nhánh kiểm tra bằng `cargo test` toàn bộ sau khi nối dây, cộng đọc lại `terms` / `speak` / `translate` xem ba chỗ đều gọi `is_translate_gemma`.

- [ ] **Step 2: Run test to verify it fails**

Nếu task 1 đã xong, test prompt PASS. Chưa nối `terms` thì chưa có gì để fail ở hàm đó. Đừng thêm test gọi HTTP.

- [ ] **Step 3: Write minimal implementation**

Thêm helper trong `pipeline.rs`:

```rust
fn chosen_local_model(choice: &crate::TranslationChoice) -> String {
    let model = choice.model.trim();
    if choice.provider == "local" && model.is_empty() {
        crate::ollama::LOCAL_MODEL.to_string()
    } else {
        model.to_string()
    }
}

fn uses_translate_gemma(choice: &crate::TranslationChoice) -> bool {
    choice.provider == "local" && crate::ollama::is_translate_gemma(&chosen_local_model(choice))
}
```

Đầu `terms`, trước `if !use_terms`:

```rust
if uses_translate_gemma(choice) {
    report(app, "terms", "skipped", "Model này chỉ dịch từng câu, không lấy thuật ngữ.");
    return Ok(Glossary { theme: String::new(), terms: Vec::new() });
}
```

Trong `translate`, sau `report(..., "running", "Đang dịch.")` và `let mut trace = TranslateTrace::logging();`, rẽ trước `translate_work`:

```rust
let outcome = if uses_translate_gemma(choice) {
    let raw: Value = read_json(&job.transcript()).unwrap_or(Value::Null);
    let source_code = source_language(&raw, lines)?;
    translate_gemma(
        lines,
        done,
        choice,
        &source_code,
        target_code,
        cancel,
        &mut trace,
        &mut |detail, done_n, total| report_progress(app, "translate", detail, done_n, total),
        &mut |done| write_json(&job.translation_path(), done),
    )
} else {
    translate_work(
        lines,
        done,
        glossary,
        choice,
        target,
        target_code,
        cancel,
        None,
        &mut trace,
        &mut |detail, done_n, total| report_progress(app, "translate", detail, done_n, total),
        &mut |done| write_json(&job.translation_path(), done),
    )
};
eprintln!("{}", trace.summary_line());
let (done, skipped) = outcome?;
```

Phần `if skipped.is_empty()` phía dưới giữ nguyên, dùng biến `target` (tên tiếng Việt như `"tiếng Việt"`).

Thêm `translate_gemma` cạnh `translate_work`:

```rust
fn translate_gemma(
    lines: &[Cue],
    mut done: Vec<TranslatedLine>,
    choice: &crate::TranslationChoice,
    source_code: &str,
    target_code: &str,
    cancel: &AtomicBool,
    trace: &mut TranslateTrace,
    progress: &mut dyn FnMut(String, f64, f64),
    save: &mut dyn FnMut(&Vec<TranslatedLine>) -> Result<(), String>,
) -> Result<(Vec<TranslatedLine>, Vec<usize>), String> {
    let mut skipped: Vec<usize> = Vec::new();
    let total = lines.len();
    let texts: Vec<String> = lines.iter().map(|line| line.text.clone()).collect();
    for i in 0..total {
        if texts[i].trim().is_empty() || !done[i].translation.trim().is_empty() {
            continue;
        }
        stopped(cancel)?;
        progress(format!("Câu {}/{total}.", i + 1), i as f64, total as f64);
        let prompt = gemma_prompt(source_code, target_code, &texts[i])?;
        trace.begin((i + 1) as u32);
        let mut rejected: Vec<String> = Vec::new();
        let mut last_error = String::new();
        let mut accepted = false;
        for _ in 0..4 {
            stopped(cancel)?;
            match complete_plain(choice, &prompt, cancel, trace) {
                Ok(text) if !wrong_language(&text, target_code) => {
                    done[i].translation = text;
                    accepted = true;
                    break;
                }
                Ok(text) => rejected.push(text),
                Err(CompleteError::Stopped) => return Err(CompleteError::Stopped.to_string()),
                Err(error) => last_error = error.to_string(),
            }
        }
        if accepted {
            save(&done)?;
            continue;
        }
        if rejected.is_empty() {
            return Err(last_error);
        }
        let cleaned = rejected
            .iter()
            .rev()
            .map(|text| strip_stray_script(text, target_code))
            .find(|text| text.chars().filter(|ch| ch.is_alphabetic()).count() >= 3 && !wrong_language(text, target_code));
        match cleaned {
            Some(text) => done[i].translation = text,
            None => skipped.push(i + 1),
        }
        save(&done)?;
    }
    Ok((done, skipped))
}
```

`target_code` đến từ `clean_target` nên luôn có trong bảng tên (`vi` … `pt`). `gemma_prompt` vẫn trả `Err` nếu mã lạ; lỗi đó dừng bước dịch.

Trong `speak`, ngay sau lần `synthesize_missing` đầu tiên (dòng đang gọi trước vòng `MAX_SHORTEN_ROUNDS`):

```rust
synthesize_missing(app, job, lines, cancel)?;
if uses_translate_gemma(choice) {
    report(app, "speak", "done", "Đã đọc.");
    return Ok(());
}
```

Vòng rút câu phía dưới không chạy với TranslateGemma. `fit` không sửa.

- [ ] **Step 4: Run test to verify it passes**

Run: `cd D:\Project\translate-video\src-tauri; cargo test --offline`

Expected: PASS toàn bộ. `cargo test` không gọi Ollama.

- [ ] **Step 5: Không commit**

## Self-review

- Prompt không gắn Anh → Việt: task 1 test `es` → `fr` và `ja` → `en`. Task 4 test `es` → `vi` chỉ khi đích là `vi`.
- Không ô chọn ngôn ngữ: không có task UI.
- Mã Whisper được lưu: task 3. File mảng cũ vẫn đọc câu. Chữ Latin không thành `en`.
- Không JSON, dịch từng câu, bỏ thuật ngữ, bỏ rút câu: task 2 và 4. Fit không nằm trong diff.
- Qwen giữ `format: json`: test cũ cộng test task 2.
