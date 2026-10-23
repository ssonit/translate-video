# Đọc giọng bằng piper.exe Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Khi máy đã có `piper.exe` kèm `espeak-ng-data`, đọc cả lô câu bằng một tiến trình (model nạp một lần). Thiếu hoặc bản không nhận `--json-input` thì giữ đường đọc trong cửa sổ app.

**Architecture:** Cửa sổ app vẫn chuẩn hóa chữ (số, ngày, từ viết tắt) rồi trả các đoạn đã sạch. Rust gom mỗi câu thành một dòng JSON và đưa vào stdin của một `piper.exe`. Piper ghi thẳng `lines/NNN.wav`. Không mở một process cho từng câu. Đường ONNX trong Web Worker (`src/tts/piper.ts`) giữ nguyên làm đường lui.

**Tech Stack:** Tauri 2, Rust, React, Piper bản Windows C++ (`piper.exe`), file giọng `.onnx` sẵn trong `voices/`. Không thêm crate. Không `onnxruntime` native.

**Spec:** Hội thoại 2026-10-12. Đọc giọng trong app là Piper WASM một luồng (`ort.env.wasm.numThreads = 1` trong `src/tts/piper.ts`), đo khoảng 1,1 giây/câu (752 câu mất 13 phút 23 giây). Piper native trên CPU nhiều luồng, giữ một tiến trình, là đường nhanh hơn. Mở `piper.exe` mới cho từng câu thì chậm hơn đường hiện tại. Chưa đo trên máy này. Bộ chuẩn hóa tiếng Việt nằm trong `src/tts/vendor` và phải chạy trước khi chữ tới Piper.

## Global Constraints

- Giữ Tauri 2. Video và audio không rời máy. Giọng local. Không Edge-TTS. Không `piper.exe` làm bắt buộc: thiếu thì app vẫn đọc trong cửa sổ.
- Không nút tự tải Piper, không tải zip, không đối chiếu SHA trong task này. Người dùng tự giải nén. App chỉ kiểm tra file đã có.
- Không đổi `productName`, identifier, tiêu đề cửa sổ. Không thêm id bước pipeline. Tám bước giữ nguyên.
- File trung gian cũ vẫn đọc được. WAV Piper phải là PCM 16-bit mono thì `wav::parse_wav` đọc được.
- Trả lời và chữ trên giao diện bằng tiếng Việt.
- Không commit trừ khi người dùng yêu cầu trong lúc thực thi. Bước commit ở mỗi task chỉ chạy khi đã được yêu cầu; nếu chưa, bỏ qua bước đó.
- Test Rust: `cd D:\Project\translate-video\src-tauri; cargo test --offline --lib piper:: -- --test-threads=8`
- Ngoài phạm vi plan này: co độ dài bằng pool FFmpeg, Whisper CUDA, chồng đọc với dịch. Những việc đó không đụng trong các task dưới.

## Review Focus

- Có `piper.exe` nhưng thiếu thư mục `espeak-ng-data` cạnh nó: không chạy, đọc trong cửa sổ.
- `--help` không có `--json-input`: không chạy, và không được mở một process cho từng câu.
- Người dùng bấm dừng: giết process, không đọc lại bằng cửa sổ.
- Câu sau khi chuẩn hóa không còn chữ để đọc: ghi wav im lặng, không đưa câu đó vào stdin.
- Thử giọng và lồng tiếng chọn cùng một đường, để câu mẫu nghe cùng kiểu với video.

---

### Task 1: Tìm piper và đọc cờ từ `--help`

**Files:**
- Create: `src-tauri/src/piper.rs`
- Modify: `src-tauri/src/lib.rs` (thêm `mod piper;` cạnh các `mod` khác)
- Test: `src-tauri/src/piper.rs` module `tests`

**Interfaces:**
- Consumes: không
- Produces:
  - `pub(crate) struct PiperCaps { pub json_input: bool, pub noise_scale: Option<&'static str>, pub length_scale: Option<&'static str>, pub noise_w: Option<&'static str> }`
  - `pub(crate) fn caps_from_help(help: &str) -> Option<PiperCaps>` — `None` khi không có `--json-input`
  - `pub(crate) fn piper_exe_in(dir: &Path) -> Option<PathBuf>` — `Some` chỉ khi `piper.exe` là file và `espeak-ng-data` là thư mục trong `dir`
  - `pub(crate) enum SpeakEngine { Native, Window }`
  - `pub(crate) fn choose_engine(exe: Option<PathBuf>, caps: Option<PiperCaps>) -> SpeakEngine`

- [ ] **Step 1: Viết test fail**

Thêm vào cuối `src-tauri/src/piper.rs` (file mới, các hàm chưa có nên test chưa biên dịch được cho đến step 3; step 2 chỉ cần xác nhận `cargo test` báo thiếu module hoặc thiếu hàm):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn help_without_json_input_is_not_native() {
        assert!(caps_from_help("usage: piper --model FILE").is_none());
        assert_eq!(choose_engine(Some(PathBuf::from("piper.exe")), None), SpeakEngine::Window);
    }

    #[test]
    fn help_with_json_input_keeps_scale_flags_it_actually_prints() {
        let caps = caps_from_help("usage:\n  --json-input\n  --noise-scale\n  --length_scale\n").unwrap();
        assert!(caps.json_input);
        assert_eq!(caps.noise_scale, Some("--noise-scale"));
        assert_eq!(caps.length_scale, Some("--length_scale"));
        assert_eq!(caps.noise_w, None);
        assert_eq!(
            choose_engine(Some(PathBuf::from("piper.exe")), Some(caps)),
            SpeakEngine::Native
        );
    }

    #[test]
    fn exe_without_espeak_data_is_ignored() {
        let dir = std::env::temp_dir().join(format!("piper-home-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("piper.exe"), b"nope").unwrap();
        assert!(piper_exe_in(&dir).is_none());
        fs::create_dir_all(dir.join("espeak-ng-data")).unwrap();
        assert_eq!(piper_exe_in(&dir), Some(dir.join("piper.exe")));
        let _ = fs::remove_dir_all(&dir);
    }
}
```

- [ ] **Step 2: Chạy test, xác nhận fail**

Run: `cd D:\Project\translate-video\src-tauri; cargo test --offline --lib piper:: -- --test-threads=8`

Expected: FAIL vì chưa có module `piper` hoặc chưa có hàm.

- [ ] **Step 3: Viết `piper.rs` và khai báo module**

Trong `src-tauri/src/lib.rs`, thêm `mod piper;` cạnh `mod tts;`.

```rust
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PiperCaps {
    pub json_input: bool,
    pub noise_scale: Option<&'static str>,
    pub length_scale: Option<&'static str>,
    pub noise_w: Option<&'static str>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SpeakEngine {
    Native,
    Window,
}

fn flag<'a>(help: &'a str, dashed: &'static str, underscored: &'static str) -> Option<&'static str> {
    if help.contains(dashed) {
        Some(dashed)
    } else if help.contains(underscored) {
        Some(underscored)
    } else {
        None
    }
}

pub(crate) fn caps_from_help(help: &str) -> Option<PiperCaps> {
    if !help.contains("--json-input") {
        return None;
    }
    Some(PiperCaps {
        json_input: true,
        noise_scale: flag(help, "--noise-scale", "--noise_scale"),
        length_scale: flag(help, "--length-scale", "--length_scale"),
        noise_w: flag(help, "--noise-w", "--noise_w"),
    })
}

pub(crate) fn piper_exe_in(dir: &Path) -> Option<PathBuf> {
    let exe = dir.join("piper.exe");
    let data = dir.join("espeak-ng-data");
    (exe.is_file() && data.is_dir()).then_some(exe)
}

pub(crate) const ALLOW_NATIVE: bool = true;

pub(crate) fn choose_engine(exe: Option<PathBuf>, caps: Option<PiperCaps>) -> SpeakEngine {
    if !ALLOW_NATIVE {
        return SpeakEngine::Window;
    }
    match (exe, caps) {
        (Some(_), Some(caps)) if caps.json_input => SpeakEngine::Native,
        _ => SpeakEngine::Window,
    }
}
```

- [ ] **Step 4: Test pass**

Run: `cd D:\Project\translate-video\src-tauri; cargo test --offline --lib piper:: -- --test-threads=8`

Expected: PASS.

- [ ] **Step 5: Commit** (chỉ khi người dùng đã yêu cầu)

```bash
git add src-tauri/src/piper.rs src-tauri/src/lib.rs
git commit -m "feat: nhận piper.exe chỉ khi có espeak-ng-data và --json-input"
```

---

### Task 2: Ghép câu và dựng stdin JSON

**Files:**
- Modify: `src-tauri/src/piper.rs`
- Test: cùng module `tests`

**Interfaces:**
- Consumes: `PiperCaps` từ task 1
- Produces:
  - `pub(crate) fn join_chunks(chunks: &[String]) -> String`
  - `pub(crate) struct Scales { pub noise: f64, pub length: f64, pub noise_w: f64 }`
  - `pub(crate) fn scales_from_config(text: &str) -> Scales` — mặc định `0.667`, `1.0`, `0.8` khi thiếu hoặc JSON hỏng
  - `pub(crate) struct SpeechLine { pub text: String, pub output: PathBuf }`
  - `pub(crate) fn json_batch(items: &[SpeechLine]) -> String` — mỗi dòng một JSON, kết thúc bằng newline, không có dòng chữ rỗng

- [ ] **Step 1: Viết test fail**

```rust
#[test]
fn chunks_join_with_one_space_and_blank_chunks_drop() {
    assert_eq!(join_chunks(&["Xin chào.".into(), "  ".into(), "Hôm nay.".into()]), "Xin chào. Hôm nay.");
    assert_eq!(join_chunks(&["  ".into()]), "");
}

#[test]
fn scales_fall_back_when_the_config_has_none() {
    assert_eq!(scales_from_config("{}").noise, 0.667);
    assert_eq!(scales_from_config("nope").length, 1.0);
    let scales = scales_from_config(r#"{"inference":{"noise_scale":0.5,"length_scale":1.2,"noise_w":0.4}}"#);
    assert_eq!((scales.noise, scales.length, scales.noise_w), (0.5, 1.2, 0.4));
}

#[test]
fn json_batch_is_one_object_per_line_and_skips_empty_text() {
    let items = vec![
        SpeechLine { text: "Xin chào".into(), output: PathBuf::from(r"D:\job\lines\001.wav") },
        SpeechLine { text: "  ".into(), output: PathBuf::from(r"D:\job\lines\002.wav") },
    ];
    let batch = json_batch(&items);
    let lines: Vec<&str> = batch.lines().collect();
    assert_eq!(lines.len(), 1);
    let row: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(row["text"], "Xin chào");
    assert_eq!(row["output_file"], r"D:\job\lines\001.wav");
}
```

- [ ] **Step 2: Chạy test, xác nhận fail**

Run: `cd D:\Project\translate-video\src-tauri; cargo test --offline --lib piper::json_batch -- --test-threads=8`

Expected: FAIL, chưa có `json_batch`.

- [ ] **Step 3: Thêm hàm**

```rust
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Scales {
    pub noise: f64,
    pub length: f64,
    pub noise_w: f64,
}

pub(crate) fn join_chunks(chunks: &[String]) -> String {
    chunks.iter().map(|chunk| chunk.trim()).filter(|chunk| !chunk.is_empty()).collect::<Vec<_>>().join(" ")
}

pub(crate) fn scales_from_config(text: &str) -> Scales {
    let value: Value = serde_json::from_str(text).unwrap_or(Value::Null);
    let inference = value.get("inference");
    let number = |name: &str, default: f64| {
        inference.and_then(|block| block.get(name)).and_then(Value::as_f64).unwrap_or(default)
    };
    Scales { noise: number("noise_scale", 0.667), length: number("length_scale", 1.0), noise_w: number("noise_w", 0.8) }
}

pub(crate) struct SpeechLine {
    pub text: String,
    pub output: PathBuf,
}

pub(crate) fn json_batch(items: &[SpeechLine]) -> String {
    let mut out = String::new();
    for item in items {
        let text = item.text.trim();
        if text.is_empty() {
            continue;
        }
        let row = serde_json::json!({
            "text": text,
            "output_file": item.output.to_string_lossy(),
        });
        out.push_str(&row.to_string());
        out.push('\n');
    }
    out
}
```

`SpeechLine` cần `pub` các field như trên để test trong module con đọc được. Thêm `use serde_json::Value;` nếu file chưa có.

- [ ] **Step 4: Test pass**

Run: `cd D:\Project\translate-video\src-tauri; cargo test --offline --lib piper:: -- --test-threads=8`

Expected: PASS.

- [ ] **Step 5: Commit** (chỉ khi người dùng đã yêu cầu)

```bash
git add src-tauri/src/piper.rs
git commit -m "feat: dựng một lô JSON cho piper.exe"
```

---

### Task 3: Một tiến trình đọc cả lô, dừng thì giết, lỗi thì giữ wav đọc được

**Files:**
- Modify: `src-tauri/src/piper.rs`
- Test: cùng module `tests`

**Interfaces:**
- Consumes: `json_batch`, `Scales`, `PiperCaps`, `SpeechLine`, `crate::hidden`, `crate::wav::parse_wav`
- Produces:
  - `pub(crate) fn scale_args(caps: &PiperCaps, scales: Scales) -> Vec<String>` — chỉ thêm cờ mà `--help` có
  - `pub(crate) fn keep_valid_wavs(paths: &[PathBuf]) -> Vec<PathBuf>` — xóa file không phải PCM 16-bit mono, trả về path còn lại
  - `pub(crate) fn speak_native(exe: &Path, model: &Path, caps: &PiperCaps, scales: Scales, items: &[SpeechLine], cancel: &AtomicBool, progress: &dyn Fn(usize, usize)) -> Result<(), String>`

`speak_native` làm đúng các việc sau, không việc khác:

- `current_dir` là thư mục chứa `piper.exe` (để thấy `espeak-ng-data`).
- Đối số: `--model` + đường dẫn model, rồi `scale_args`, rồi `--json-input`. Không `--output_file` riêng vì mỗi dòng JSON đã có `output_file`.
- Ghi `json_batch(items)` vào stdin ở thread phụ, flush, rồi thả stdin để Piper gặp EOF.
- Vòng chờ giống `run_tool_with` trong `pipeline.rs`: mỗi 80 ms nếu `cancel` thì `child.kill()`, `child.wait()`, trả `Err("Đã dừng.")`. Hết `Duration::from_secs(60 + 2 * items.len() as u64)` thì giết và trả `Err("Lệnh chạy quá lâu.")`.
- Mỗi vòng, `progress` nhận số file trong `items` mà `parse_wav` đọc được.
- Process thoát mã khác 0, hoặc còn câu chữ không rỗng mà wav không đọc được: gọi `keep_valid_wavs` trên các `output` của `items`, rồi trả lỗi stderr (cắt còn một dòng, tối đa 400 ký tự). Wav hợp lệ giữ lại. Không spawn process thứ hai.

- [ ] **Step 1: Viết test fail cho phần thuần**

`speak_native` cần process thật, không gọi trong unit test. Test phần thuần:

```rust
#[test]
fn scale_args_skip_flags_the_help_did_not_list() {
    let caps = caps_from_help("--json-input\n--length-scale\n").unwrap();
    let scales = Scales { noise: 0.5, length: 1.25, noise_w: 0.4 };
    assert_eq!(scale_args(&caps, scales), vec!["--length-scale".to_string(), "1.250".to_string()]);
}

#[test]
fn keep_valid_wavs_deletes_a_file_that_is_not_pcm() {
    let dir = std::env::temp_dir().join(format!("piper-wav-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let good = dir.join("good.wav");
    let bad = dir.join("bad.wav");
    let pcm = crate::wav::Pcm { rate: 22050, samples: vec![0, 100, -100] };
    fs::write(&good, crate::wav::encode_wav(&pcm)).unwrap();
    fs::write(&bad, b"not a wav").unwrap();
    let kept = keep_valid_wavs(&[good.clone(), bad.clone()]);
    assert_eq!(kept, vec![good]);
    assert!(!bad.exists());
    let _ = fs::remove_dir_all(&dir);
}
```

- [ ] **Step 2: Chạy, xác nhận fail**

Run: `cd D:\Project\translate-video\src-tauri; cargo test --offline --lib piper::keep_valid_wavs -- --test-threads=8`

Expected: FAIL, chưa có hàm.

- [ ] **Step 3: Cài `scale_args`, `keep_valid_wavs`, `speak_native`**

`scale_args` đẩy từng cờ `Some` trong `caps` kèm số định dạng `{:.3}`.

`keep_valid_wavs` đọc từng path. `parse_wav` thành công thì giữ. File không đọc được thì `std::fs::remove_file` (bỏ qua lỗi xóa) và không đưa vào vec trả về. File không tồn tại thì bỏ qua, không coi là lỗi.

`speak_native` dùng `crate::hidden`, `Stdio::piped()` cho stdin/stdout/stderr, `spawn_reader` thì copy cục bộ một hàm đọc pipe giống `pipeline.rs` (đừng `pub` hàm của pipeline chỉ để dùng chung). Sau khi child thoát, nếu còn câu có `text.trim()` không rỗng mà `parse_wav` lỗi, đó là thất bại dù exit code là 0.

- [ ] **Step 4: Test pass**

Run: `cd D:\Project\translate-video\src-tauri; cargo test --offline --lib piper:: -- --test-threads=8`

Expected: PASS. Chưa cần `piper.exe` trên máy.

- [ ] **Step 5: Commit** (chỉ khi người dùng đã yêu cầu)

```bash
git add src-tauri/src/piper.rs
git commit -m "feat: một tiến trình piper đọc cả lô câu"
```

---

### Task 4: Cửa sổ chuẩn hóa chữ, không nạp ONNX

**Files:**
- Create: `src/tts/prepare.ts`
- Modify: `src/tts/piper.ts` (bỏ `plainChunks` khỏi file này, import từ `prepare.ts`)
- Modify: `src/tts/bridge.ts` (nghe thêm `tts-clean`)
- Modify: `src-tauri/src/tts.rs` (lệnh `tts_prepared` và `prepare`)

**Interfaces:**
- Consumes: `processTextForTTS`, `chunkText` từ `src/tts/vendor/text-cleaner.js`. `plainChunks` chuyển sang `prepare.ts` với đúng thân hàm hiện ở `piper.ts` dòng 63–74.
- Produces:
  - `export async function prepareLine(text: string, lang: string): Promise<string[]>`
  - Sự kiện Rust `tts-clean`: `{ lang: string, items: { index: number, text: string }[] }`
  - Lệnh `tts_prepared(lines: Vec<PreparedLine>)` với `PreparedLine { index: usize, chunks: Vec<String> }`
  - `pub(crate) fn prepare(app: &AppHandle, lang: &str, items: &[(usize, String)], cancel: &AtomicBool) -> Result<Vec<(usize, Vec<String>)>, String>`

`prepareLine`: `lang === "vi"` thì `(await chunkText(await processTextForTTS(text))).filter(chunk => chunk.trim() !== "")`. Ngôn ngữ khác thì `plainChunks(text)`. Không gọi `Piper.load`, không `session.run`.

`useTtsBridge` nghe `tts-clean`. Không đặt cờ `busy` của đường đọc wav (hai việc này không chạy cùng lúc: Rust chỉ `prepare` xong rồi mới `speak_native`). Lần lượt `prepareLine`, rồi `invoke("tts_prepared", { lines })`. Lỗi thì `invoke("tts_prepared", { lines: [], error })`.

`prepare` trong Rust giống vòng chờ của `tts::run`: timeout câu đầu 180 giây, emit `tts-clean`, nhận đủ mọi `index` đã gửi thì trả. `cancel` trả `Err("Đã dừng.")`. Thiếu index hoặc có `error` thì trả lỗi đó. Không ghi file wav.

- [ ] **Step 1: Chuyển `plainChunks` và thêm `prepareLine`**

`piper.ts` xóa hàm `plainChunks` và import nó từ `./prepare`. `speak()` vẫn gọi `plainChunks` như cũ khi `lang !== "vi"`.

Trong `prepare.ts`, giữ nguyên comment và thân `plainChunks`. Thêm `prepareLine` như phần Interfaces.

- [ ] **Step 2: Rust nhận danh sách đoạn**

Trong `tts.rs`, struct pending thêm `prepared: Option<mpsc::Sender<Result<Vec<(usize, Vec<String>)>, String>>>`. Lệnh:

```rust
#[derive(serde::Deserialize)]
struct PreparedLine {
    index: usize,
    chunks: Vec<String>,
}

#[tauri::command]
pub(crate) fn tts_prepared(bridge: State<'_, Bridge>, lines: Vec<PreparedLine>, error: Option<String>) {
    let sender = bridge.lock().prepared.clone();
    if let Some(sender) = sender {
        let _ = sender.send(match error {
            Some(message) => Err(message),
            None => Ok(lines.into_iter().map(|line| (line.index, line.chunks)).collect()),
        });
    }
}
```

Đăng ký lệnh trong `lib.rs` chỗ các lệnh `tts_finish` / `save_line_wav`.

`prepare` emit `tts-clean` với payload `{ lang, items: [{ index, text }] }`. Nếu `prepared` đang có sender thì trả `Err("Đang chuẩn hóa một yêu cầu khác.")`. Hết chờ thì xóa sender.

- [ ] **Step 3: Bridge gọi `prepareLine`**

```ts
const clean = listen<{ lang: string; items: { index: number; text: string }[] }>("tts-clean", (event) => {
  void (async () => {
    try {
      const lines = [];
      for (const item of event.payload.items) {
        lines.push({ index: item.index, chunks: await prepareLine(item.text, event.payload.lang) });
      }
      await invoke("tts_prepared", { lines });
    } catch (caught) {
      await invoke("tts_prepared", { lines: [], error: errorText(caught) });
    }
  })();
});
```

Thêm `clean` vào hàm dọn `useEffect` giống `request`.

- [ ] **Step 4: Rust test vẫn pass, và `tsc` không lỗi ở file vừa sửa**

Run: `cd D:\Project\translate-video\src-tauri; cargo test --offline --lib tts:: -- --test-threads=8`

Run: `cd D:\Project\translate-video; npx tsc --noEmit`

Expected: cả hai pass. Không có runner unit test cho TS; `prepareLine` được kiểm tra ở task 6 bằng nút Thử giọng.

- [ ] **Step 5: Commit** (chỉ khi người dùng đã yêu cầu)

```bash
git add src/tts/prepare.ts src/tts/piper.ts src/tts/bridge.ts src-tauri/src/tts.rs src-tauri/src/lib.rs
git commit -m "feat: chuẩn hóa chữ đọc trước khi đưa sang piper"
```

---

### Task 5: Lồng tiếng và Thử giọng đi cùng một đường

**Files:**
- Modify: `src-tauri/src/piper.rs` (thêm `locate` và `read_caps`)
- Modify: `src-tauri/src/pipeline.rs` hàm `synthesize_missing` (khoảng dòng 1119)
- Modify: `src-tauri/src/tts.rs` hàm `preview_voice`

**Interfaces:**
- Consumes: `choose_engine`, `piper_exe_in`, `caps_from_help`, `scales_from_config`, `join_chunks`, `speak_native`, `tts::prepare`, `tts::run`
- Produces:
  - `pub(crate) fn locate() -> Option<PathBuf>` — thử `PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../piper")` bằng `piper_exe_in`, không có thì `crate::program_path("piper")` và `piper_exe_in` trên thư mục cha của file đó
  - `pub(crate) fn read_caps(exe: &Path) -> Option<PiperCaps>` — chạy `exe` với `--help`, timeout 8 giây, `current_dir` là thư mục chứa exe, ghép stdout và stderr vào `caps_from_help`. Lỗi spawn hoặc timeout thì `None`
  - `pub(crate) fn model_path(voice_dir: &Path, name: &str) -> PathBuf` — `voice_dir.join(format!("{name}.onnx"))`
  - `pub(crate) enum LinePlan { Silence, Speak(String) }`
  - `pub(crate) fn plan_line(chunks: &[String]) -> LinePlan` — `join_chunks` rỗng thì `Silence`, còn lại `Speak`
  - `pub(crate) enum AfterNative { Stop, KeepGoing }`
  - `pub(crate) fn after_native(error: &str) -> AfterNative` — đúng chuỗi `"Đã dừng."` thì `Stop`, mọi lỗi khác `KeepGoing`
  - `pub(crate) const ALLOW_NATIVE: bool = true` — `choose_engine` trả `Window` ngay khi hằng này là `false`

Trong `synthesize_missing`, sau khi đã có `batch` và `voice`, gọi `locate` và `read_caps` mỗi thứ một lần:

```rust
let exe = crate::piper::locate();
let caps = exe.as_ref().and_then(|path| crate::piper::read_caps(path));
if crate::piper::choose_engine(exe.clone(), caps) == crate::piper::SpeakEngine::Native {
    let exe = exe.expect("choose_engine Native chỉ khi có exe");
    let caps = caps.expect("choose_engine Native chỉ khi có cờ");
    let voice_dir = crate::bundled_voice_dir(&job.target).ok_or("Chưa thấy folder giọng.")?;
    let model = crate::piper::model_path(&voice_dir, &voice);
    let scales = crate::piper::scales_from_config(
        &std::fs::read_to_string(voice_dir.join(format!("{voice}.onnx.json"))).unwrap_or_default(),
    );
    let raw: Vec<(usize, String)> = batch.iter().map(|(index, text, _)| (*index, text.clone())).collect();
    let prepared = crate::tts::prepare(app, &job.target, &raw, cancel)?;
    let mut native_items = Vec::new();
    for (index, _, path) in &batch {
        let chunks = prepared
            .iter()
            .find(|(found, _)| found == index)
            .map(|(_, chunks)| chunks.as_slice())
            .unwrap_or(&[]);
        match crate::piper::plan_line(chunks) {
            crate::piper::LinePlan::Silence => write_silence(path, cancel)?,
            crate::piper::LinePlan::Speak(text) => {
                native_items.push(crate::piper::SpeechLine { text, output: path.clone() });
            }
        }
    }
    report(app, "speak", "running", format!("Đang đọc {} câu bằng Piper trên máy.", native_items.len()));
    if let Err(error) = crate::piper::speak_native(&exe, &model, &caps, scales, &native_items, cancel, &|done, _| {
        report_progress(
            app,
            "speak",
            format!("Đã đọc {done}/{} câu.", native_items.len()),
            (already + done) as f64,
            lines.len() as f64,
        );
    }) {
        if crate::piper::after_native(&error) == crate::piper::AfterNative::Stop {
            return Err(error);
        }
        log_line(job, &format!("piper.exe lỗi, đọc phần còn lại trong app: {error}"));
    }
}
```

Sau khối đó, dựng lại `batch` từ những câu `!file_ready(path)` rồi gọi `crate::tts::run` như hiện tại nếu batch còn câu. Batch rỗng thì return `Ok(())` trước `tts::run`. `after_native` trả `Stop` thì không chạy tới `tts::run`.

`preview_voice` dùng cùng thứ tự: `locate` + `read_caps` + `choose_engine`. Native thì `prepare` một câu `preview_text(code)`, `join_chunks`, `speak_native` ghi vào file tạm, rồi đọc bytes trả về. `Window` thì giữ `tts::run` như cũ. Lỗi native khác `"Đã dừng."` thì xóa file tạm nếu `parse_wav` thất bại và đọc bằng `tts::run`.

Gọi `read_caps` một lần mỗi lượt nói, không cache qua lần lồng sau (Piper có thể bị gỡ giữa chừng).

- [ ] **Step 1: Test đường lui khi không có exe**

Trong `piper.rs`:

```rust
#[test]
fn locate_on_an_empty_temp_tree_is_none() {
    let missing = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../piper-does-not-exist");
    assert!(piper_exe_in(&missing).is_none());
}

#[test]
fn a_blank_line_is_silence_and_cancel_does_not_continue() {
    assert_eq!(plan_line(&["  ".into()]), LinePlan::Silence);
    assert_eq!(plan_line(&["Xin chào.".into()]), LinePlan::Speak("Xin chào.".into()));
    assert_eq!(after_native("Đã dừng."), AfterNative::Stop);
    assert_eq!(after_native("Lệnh chạy quá lâu."), AfterNative::KeepGoing);
}

#[test]
fn native_stays_off_when_the_measure_gate_is_closed() {
    let caps = caps_from_help("--json-input").unwrap();
    let before = choose_engine(Some(PathBuf::from("piper.exe")), Some(caps));
    assert_eq!(before, if ALLOW_NATIVE { SpeakEngine::Native } else { SpeakEngine::Window });
}
```

`locate()` không đổi thư mục project. Test này chỉ ghim `piper_exe_in` với đường chưa có. Không tạo folder `piper/` trong repo.

- [ ] **Step 2: Thêm ba hàm rồi nối `synthesize_missing` và `preview_voice`**

`ALLOW_NATIVE` đã có từ task 1. Thêm:

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LinePlan {
    Silence,
    Speak(String),
}

pub(crate) fn plan_line(chunks: &[String]) -> LinePlan {
    let spoken = join_chunks(chunks);
    if spoken.is_empty() { LinePlan::Silence } else { LinePlan::Speak(spoken) }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AfterNative {
    Stop,
    KeepGoing,
}

pub(crate) fn after_native(error: &str) -> AfterNative {
    if error == "Đã dừng." { AfterNative::Stop } else { AfterNative::KeepGoing }
}
```

Nối `synthesize_missing` và `preview_voice` như khối ở phần Interfaces. Đừng xóa `tts::run`. Đường cửa sổ vẫn chạy khi `choose_engine` trả `Window`, hoặc khi `after_native` trả `KeepGoing` và còn câu chưa có wav đọc được.

- [ ] **Step 3: Test Rust pass**

Run: `cd D:\Project\translate-video\src-tauri; cargo test --offline --lib piper:: pipeline::tts -- --test-threads=8`

Expected: PASS. Lệnh lọc có thể không khớp tên module `pipeline::tts`; nếu vậy chạy `cargo test --offline --lib piper::` và `cargo test --offline --lib pipeline::`.

- [ ] **Step 4: Commit** (chỉ khi người dùng đã yêu cầu)

```bash
git add src-tauri/src/piper.rs src-tauri/src/pipeline.rs src-tauri/src/tts.rs
git commit -m "feat: lồng tiếng dùng piper.exe khi máy đã có"
```

---

### Task 6: Màn chuẩn bị báo tùy chọn, không chặn

**Files:**
- Modify: `src/guides.ts` (`ToolId`, `OPTIONAL_TOOLS`, mục `voice`, thêm guide `piper`)
- Modify: `src-tauri/src/lib.rs` (`run_checks`, thêm `piper_check`)
- Modify: `README.md` câu "không cần cài piper.exe"
- Modify: `AGENTS.md` đoạn đọc giọng, một câu về đường piper.exe

**Interfaces:**
- Consumes: `crate::piper::locate` và `crate::piper::read_caps`
- Produces: `ToolCheck` id `"piper"`, `ready` khi `choose_engine` là `Native`. Thiếu thì `ready: false` và detail `"Chưa có piper.exe. App vẫn đọc giọng trong cửa sổ."`. Có exe nhưng không có `--json-input` thì detail `"Bản piper này không đọc được cả lô. App vẫn đọc trong cửa sổ."`.

Guide `piper`, các bước đúng ba dòng:

```ts
{
  id: "piper",
  name: "Piper",
  role: "Đọc giọng nhanh hơn, trên máy",
  steps: [
    "Tải piper_windows_amd64.zip ở trang Releases. Giải nén cả thư mục, để piper.exe và espeak-ng-data cạnh nhau.",
    "Chép cả thư mục đó thành folder piper cạnh folder voices của project này. Không cần thêm vào Path.",
    "Bấm Kiểm tra lại. Thiếu thì app vẫn đọc giọng trong cửa sổ.",
  ],
  links: [{ label: "Mở trang Piper", url: "https://github.com/rhasspy/piper/releases" }],
}
```

`OPTIONAL_TOOLS` thêm `"piper"`. `run_checks` đẩy check này vào vec, không làm mục khác thành chưa sẵn sàng.

Sửa câu trong guide `voice`: `"Có piper.exe trong folder piper thì app đọc bằng chương trình đó. Không có thì app vẫn tự đọc các file giọng này."`

README và AGENTS.md: thay câu "không cần cài piper.exe" bằng cùng ý đó. Không thêm nút tải.

- [ ] **Step 1: Sửa guide, check, và hai câu tài liệu**

- [ ] **Step 2: `tsc` và test check**

Run: `cd D:\Project\translate-video; npx tsc --noEmit`

Run: `cd D:\Project\translate-video\src-tauri; cargo test --offline --lib -- --test-threads=8`

Expected: PASS. Nếu có test ghim danh sách id công cụ, thêm `"piper"` vào kỳ vọng của test đó chứ không xóa test.

- [ ] **Step 3: Commit** (chỉ khi người dùng đã yêu cầu)

```bash
git add src/guides.ts src-tauri/src/lib.rs README.md AGENTS.md
git commit -m "docs: Piper trên máy là tùy chọn, thiếu thì vẫn đọc trong cửa sổ"
```

---

### Task 7: Đo 20 câu, giữ đường native chỉ khi nhanh hơn

Không viết thêm tính năng. Cần `piper.exe` và `espeak-ng-data` trong `D:\Project\translate-video\piper\`. Chưa có bộ đó thì dừng task này, để code đường lui, và ghi vào cuối plan: "Chưa đo, máy chưa có piper.exe".

- [ ] **Step 1: Một câu Thử giọng tiếng Việt**

Mở app, bấm Thử giọng. Câu mẫu phải phát được. File tạm là WAV PCM 16-bit mono. Lần bấm này được phép chậm ở giây đầu vì process mới nạp model.

- [ ] **Step 2: 20 câu của một video đã có bản dịch**

Xóa `lines/001.wav` đến `lines/020.wav` của một job đã có `translation.json`. Chạy lại. `timing.log` phải có dòng `Đang đọc` bằng Piper trên máy, hoặc nếu lỗi thì dòng `piper.exe lỗi, đọc phần còn lại trong app`.

Ghi số:

- giây cho 20 câu bằng piper.exe: ________
- giây cho cùng 20 câu khi đổi tên folder `piper` đi (đường cửa sổ): ________

Đường cửa sổ đã đo khoảng 1,1 giây/câu, nên 20 câu khoảng 22 giây. Native đáng giữ khi 20 câu xong dưới 11 giây (nhanh hơn khoảng hai lần).

- [ ] **Step 3: Cửa thua thì tắt đường native**

Nếu piper.exe không dưới 11 giây, hoặc wav không phải PCM mono, hoặc chữ số trong câu mẫu bị đọc sai so với Thử giọng đường cửa sổ: đặt `ALLOW_NATIVE` thành `false` và ghi số đo vào mục này. Test `native_stays_off_when_the_measure_gate_is_closed` vẫn đúng vì nó đọc hằng đó. Đừng giữ đường native khi nó chậm hơn hoặc đọc khác bộ chuẩn hóa.

Nếu nhanh hơn và câu có số (ví dụ ngày 25 tháng 12) nghe cùng kiểu với đường cửa sổ, để `choose_engine` như task 1.

Chưa đo, máy chưa có piper.exe.

## Self-review

- Một process, `--json-input`, không spawn từng câu: task 1 và task 3.
- Thiếu exe hoặc thiếu `espeak-ng-data` hoặc thiếu cờ: `choose_engine` trả `Window`, task 5 vẫn gọi `tts::run`.
- Chuẩn hóa tiếng Việt trước Piper: task 4, `prepareLine` gọi `processTextForTTS`.
- Dừng không đọc lại: task 5, lỗi `"Đã dừng."` return ngay.
- Wav hỏng bị xóa, wav đọc được giữ: `keep_valid_wavs` trong task 3, task 5 chỉ đưa câu còn thiếu sang cửa sổ.
- Thử giọng cùng đường: task 5 `preview_voice`.
- Không nút tải: task 6 chỉ hướng dẫn giải nén.
- Đo rồi mới giữ mặc định native: task 7.
- Co độ dài, Whisper CUDA, chồng bước: nêu trong Global Constraints, không có task.
