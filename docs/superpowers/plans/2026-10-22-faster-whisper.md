# Chép lời GPU Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Chép lời và xem trước chạy faster-whisper trên GPU (model `large-v3-turbo`) khi máy có đủ đồ, không có thì vẫn gọi whisper-cli.

**Architecture:** Một hàm Rust chọn lệnh. Đường nhanh gọi `scripts/faster_whisper.py` với đường dẫn tuyệt đối và in đúng dạng câu mà xem trước đã parse. Đường cũ giữ nguyên đối số whisper-cli. Màn chuẩn bị coi là đủ khi một trong hai đường sẵn sàng. Script không nhận tên model để khỏi tải lúc đang chép.

**Tech Stack:** Tauri 2, Rust, Python 3.11+ với `faster-whisper>=1.1.0` và CTranslate2 CUDA. Không thêm crate. Không nút tải mới.

**Spec:** `docs/superpowers/specs/2026-10-22-faster-whisper-design.md`

## Global Constraints

- Audio và video không rời máy. Script chỉ mở folder model trên đĩa, không truyền tên `large-v3-turbo` vào `WhisperModel`.
- Không thêm nút tải. Nút "Tải Whisper" và file zip `whisper-bin-x64` giữ nguyên.
- Chữ giao diện tiếng Việt. Không đổi `productName`, identifier, tiêu đề cửa sổ.
- `whisper_threads()` vẫn trả về 2. Đường whisper-cli vẫn `-t 2 -l auto -oj -pp -of transcript`, cwd là folder việc.
- Đã có `transcript.json` có câu thì `settle_transcript` bỏ qua, không đụng.
- Lệnh GPU lỗi thì trả lỗi đó. Không gọi whisper-cli trong cùng lượt.
- Không commit trừ khi người dùng yêu cầu lúc thực thi. Bước commit của mỗi task chỉ chạy khi đã được yêu cầu; nếu chưa, bỏ qua.
- Test Rust: `cd D:\Project\translate-video\src-tauri; cargo test --offline --lib -- --test-threads=8`
- Test Python: `py -3 scripts/faster_whisper_test.py` tại thư mục project. Không có `py` thì `python scripts/faster_whisper_test.py`.
- Test cửa sổ: `node --experimental-strip-types --test src/guides.test.ts` tại thư mục project.

## Review Focus

- Folder việc tên tiếng Việt: đối số `--audio` và `--out` là đường tuyệt đối, không phải `audio.wav`. Test ở Task 1.
- Không có đường nhanh nhưng có whisper-cli và ggml: lệnh cũ, có `-t` và `2`, không có `--duration`. Test ở Task 1.
- Không có cả hai đường: lỗi đúng `Chưa thấy whisper-cli.exe. Xem mục Whisper ở màn chuẩn bị.` Test ở Task 1.
- Một câu in ra đúng `[00:00:01.000 --> 00:00:04.000]  Xin chào` để `parse_print` của xem trước nhận. Test ở Task 2.
- Không có câu nào thì không tạo `transcript.json`. File rỗng làm lần sau kẹt ở "Không nghe được lời thoại." Test ở Task 2.

## File structure

- `scripts/faster_whisper.py` — đọc wav, in câu, ghi `transcript.json`. Import `faster_whisper` nằm trong hàm chạy, không ở đầu file.
- `scripts/faster_whisper_test.py` — unittest cho dạng dòng, tiến độ, file rỗng, thiếu model. Không cần GPU.
- `src-tauri/src/pipeline/media.rs` — chọn đường và dựng đối số. `transcribe` dùng hàm đó.
- `src-tauri/src/watch.rs` — `stream_whisper` dùng cùng hàm, không dựng lệnh riêng.
- `src-tauri/src/pipeline/mod.rs` — export các hàm cho `watch` và `lib`.
- `src-tauri/src/lib.rs` — `whisper_check` sẵn sàng khi đường nhanh có, hoặc khi đường cũ có.
- `src/guides.ts` — chữ hướng dẫn mục Whisper.
- `AGENTS.md` — một câu về đường GPU để lần sau không coi whisper-cli là đường duy nhất.

---

### Task 1: Chọn đường và dựng đối số

**Files:**
- Modify: `src-tauri/src/pipeline/media.rs`
- Modify: `src-tauri/src/pipeline/mod.rs` (export)
- Test: `src-tauri/src/pipeline/media.rs` (`mod tests`)

**Interfaces:**
- Consumes: `whisper_threads()`, `Job::audio`, `Job::transcript`, `Job.root`
- Produces:
  - `pub(crate) struct FastPath { pub python: PathBuf, pub python_args: Vec<String>, pub model: PathBuf }`
  - `pub(crate) enum AsrKind { Faster(FastPath), WhisperCpp { exe: PathBuf, model: PathBuf } }`
  - `pub(crate) struct AsrLaunch { pub program: PathBuf, pub args: Vec<String>, pub cwd: Option<PathBuf> }`
  - `pub(crate) fn faster_model_dir(whisper_dir: &Path) -> Option<PathBuf>` — `Some` chỉ khi `whisper_dir/large-v3-turbo/model.bin` là file. Giá trị là folder `large-v3-turbo`.
  - `pub(crate) fn assemble_fast_path(script_ok: bool, model: Option<PathBuf>, python: Option<(PathBuf, Vec<String>)>) -> Option<FastPath>` — `None` nếu một trong ba thiếu. `python_args` là phần trước `-u` (`["-3"]` hoặc rỗng).
  - `pub(crate) fn choose_asr(fast: Option<FastPath>, exe: Option<PathBuf>, ggml: Option<PathBuf>) -> Result<AsrKind, String>`
  - `pub(crate) fn asr_argv(kind: &AsrKind, root: &Path, audio: &Path, transcript: &Path, script: &Path, duration: Option<f64>) -> AsrLaunch`

- [ ] **Step 1: Write the failing test**

Trong `mod tests` của `media.rs`:

```rust
#[test]
fn vietnamese_job_uses_absolute_audio_on_the_fast_path() {
    let fast = FastPath {
        python: PathBuf::from("py"),
        python_args: vec!["-3".into()],
        model: PathBuf::from(r"D:\whisper\large-v3-turbo"),
    };
    let root = PathBuf::from(r"D:\Việc\Chép Chính Tả");
    let audio = root.join("audio.wav");
    let transcript = root.join("transcript.json");
    let script = PathBuf::from(r"D:\app\scripts\faster_whisper.py");
    let launch = asr_argv(
        &AsrKind::Faster(fast),
        &root,
        &audio,
        &transcript,
        &script,
        Some(12.5),
    );
    assert_eq!(launch.program, PathBuf::from("py"));
    assert_eq!(launch.cwd, None);
    assert_eq!(
        launch.args,
        vec![
            "-3",
            "-u",
            r"D:\app\scripts\faster_whisper.py",
            "--model",
            r"D:\whisper\large-v3-turbo",
            "--audio",
            audio.display().to_string(),
            "--out",
            transcript.display().to_string(),
            "--duration",
            "12.5",
        ]
    );
    assert!(!launch.args.iter().any(|arg| arg == "audio.wav"));
    assert!(!launch.args.iter().any(|arg| arg == "large-v3-turbo"));
}

#[test]
fn missing_fast_path_keeps_the_two_thread_whisper_cli() {
    let exe = PathBuf::from(r"D:\whisper\whisper-cli.exe");
    let ggml = PathBuf::from(r"D:\whisper\ggml-small.bin");
    let kind = choose_asr(None, Some(exe.clone()), Some(ggml.clone())).unwrap();
    let root = PathBuf::from(r"D:\job");
    let launch = asr_argv(
        &kind,
        &root,
        &root.join("audio.wav"),
        &root.join("transcript.json"),
        &PathBuf::from("unused.py"),
        Some(99.0),
    );
    assert_eq!(launch.program, exe);
    assert_eq!(launch.cwd.as_deref(), Some(root.as_path()));
    assert_eq!(
        launch.args,
        vec![
            "-m",
            ggml.display().to_string(),
            "-f",
            "audio.wav",
            "-t",
            "2",
            "-l",
            "auto",
            "-oj",
            "-pp",
            "-of",
            "transcript",
        ]
    );
}

#[test]
fn neither_engine_keeps_the_whisper_cli_error() {
    let error = choose_asr(None, None, None).unwrap_err();
    assert_eq!(
        error,
        "Chưa thấy whisper-cli.exe. Xem mục Whisper ở màn chuẩn bị."
    );
}

#[test]
fn exe_without_ggml_keeps_the_model_error() {
    let error = choose_asr(None, Some(PathBuf::from("whisper-cli.exe")), None).unwrap_err();
    assert_eq!(
        error,
        "Chưa thấy file model ggml-small.bin trong folder whisper. App không tự tải model."
    );
}

#[test]
fn fast_path_needs_script_model_bin_and_python() {
    let dir = std::env::temp_dir().join(format!("asr-model-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    assert!(faster_model_dir(&dir).is_none());
    let model = dir.join("large-v3-turbo");
    std::fs::create_dir_all(&model).unwrap();
    assert!(faster_model_dir(&dir).is_none());
    std::fs::write(model.join("model.bin"), b"x").unwrap();
    assert_eq!(faster_model_dir(&dir).as_deref(), Some(model.as_path()));
    let python = Some((PathBuf::from("py"), vec!["-3".into()]));
    assert!(assemble_fast_path(false, faster_model_dir(&dir), python.clone()).is_none());
    assert!(assemble_fast_path(true, None, python.clone()).is_none());
    assert!(assemble_fast_path(true, faster_model_dir(&dir), None).is_none());
    let fast = assemble_fast_path(true, faster_model_dir(&dir), python).unwrap();
    assert_eq!(fast.python_args, vec!["-3".to_string()]);
    assert!(matches!(choose_asr(Some(fast), None, None).unwrap(), AsrKind::Faster(_)));
    let _ = std::fs::remove_dir_all(&dir);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd D:\Project\translate-video\src-tauri; cargo test --offline --lib pipeline::media::tests::vietnamese_job_uses_absolute_audio -- --test-threads=8`

Expected: FAIL vì `asr_argv` chưa có.

- [ ] **Step 3: Implement the five functions in `media.rs`**

`choose_asr`: `fast` là `Some` thì trả `AsrKind::Faster` và bỏ qua exe. `duration` không hữu hạn hoặc `<= 0` thì không thêm `--duration`. Đường whisper-cli không nhận `--duration`. Model của đường nhanh đi sau `--model` và là đường dẫn folder, không phải chuỗi `large-v3-turbo` đứng riêng.

Export cả năm hàm cộng `FastPath`, `AsrKind`, `AsrLaunch` từ `pipeline/mod.rs`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cd D:\Project\translate-video\src-tauri; cargo test --offline --lib pipeline::media::tests:: -- --test-threads=8`

Expected: PASS, gồm `whisper_stays_on_two_threads`.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/pipeline/media.rs src-tauri/src/pipeline/mod.rs
git commit -m "feat: choose faster-whisper or whisper-cli without running either"
```

Chỉ chạy khi người dùng đã yêu cầu commit.

---

### Task 2: Script in câu và ghi transcript

**Files:**
- Create: `scripts/faster_whisper.py`
- Create: `scripts/faster_whisper_test.py`
- Test: `scripts/faster_whisper_test.py`

**Interfaces:**
- Consumes: hợp đồng trong spec (dòng stdout, dòng stderr, JSON, ba câu lỗi)
- Produces: CLI `script.py --model DIR --audio FILE --out FILE [--duration SECONDS]`. Hàm thuần để test, không import `faster_whisper` khi import module:
  - `format_cue(start: float, end: float, text: str) -> str`
  - `format_progress(fraction: float) -> str`
  - `adjust_end(start: float, end: float) -> float`
  - `cues_document(rows: list[tuple[float, float, str]]) -> list[dict]`
  - `write_cues(path: str, rows: list) -> bool` — `False` và không tạo file khi không còn câu
  - `model_kwargs() -> dict` — `{"device": "cuda", "compute_type": "float16"}`
  - `transcribe_kwargs() -> dict` — `language` là `None`, `beam_size` 5, `vad_filter` False, `word_timestamps` False
  - `reject_model(model_dir: str) -> str | None` — câu lỗi khi thiếu `model.bin`, còn không thì `None`

- [ ] **Step 1: Write the failing test**

`scripts/faster_whisper_test.py` dùng `unittest`. Import các hàm ở trên từ `faster_whisper`.

```python
def test_cue_line_matches_the_watch_parser(self):
    self.assertEqual(
        format_cue(1, 4, " Xin chào "),
        "[00:00:01.000 --> 00:00:04.000]  Xin chào",
    )

def test_short_end_is_raised_before_the_line(self):
    self.assertEqual(adjust_end(1.0, 1.0), 1.05)

def test_progress_rounds_and_clamps(self):
    self.assertEqual(format_progress(0.126), "progress = 13%\n")
    self.assertEqual(format_progress(2.0), "progress = 100%\n")

def test_empty_rows_do_not_create_a_transcript(self):
    path = self.temp()
    self.assertFalse(write_cues(path, [(0.0, 1.0, "  ")]))
    self.assertFalse(os.path.exists(path))

def test_rows_become_the_saved_cue_shape(self):
    doc = cues_document([(1.0, 1.0, " Xin chào ")])
    self.assertEqual(doc, [{"start": 1.0, "end": 1.05, "text": "Xin chào"}])

def test_missing_model_bin_is_rejected_before_load(self):
    self.assertEqual(
        reject_model(self.empty_dir()),
        "Chưa thấy model large-v3-turbo trong folder whisper.",
    )

def test_gpu_settings_stay_fixed(self):
    self.assertEqual(model_kwargs(), {"device": "cuda", "compute_type": "float16"})
    self.assertEqual(
        transcribe_kwargs(),
        {"language": None, "beam_size": 5, "vad_filter": False, "word_timestamps": False},
    )

def test_script_does_not_name_a_downloadable_model(self):
    source = pathlib.Path(__file__).with_name("faster_whisper.py").read_text(encoding="utf-8")
    self.assertNotIn("download_root", source)
    self.assertNotIn("huggingface", source.lower())
    self.assertIn("Không nghe được lời thoại.", source)
    self.assertIn("faster-whisper không chạy được trên GPU.", source)
```

- [ ] **Step 2: Run test to verify it fails**

Run: `py -3 scripts/faster_whisper_test.py`

Expected: FAIL vì module chưa có.

- [ ] **Step 3: Implement `scripts/faster_whisper.py`**

`format_cue` dùng `adjust_end` rồi in `HH:MM:SS.mmm`, hai dấu cách sau `]`. `write_cues` ghi UTF-8, `ensure_ascii=False`, bỏ câu chữ rỗng.

Hàm `main` parse đối số. `reject_model` khác `None` thì in câu đó ra stderr và thoát 1, trước mọi import `faster_whisper`. Import nằm trong hàm chạy: `WhisperModel(model_dir, **model_kwargs())` rồi `transcribe(audio, **transcribe_kwargs())`. Mỗi segment: in `format_cue` ra stdout, nếu có `--duration` thì in `format_progress(end / duration)` ra stderr, flush cả hai. Hết vòng: `write_cues`. `False` thì in `Không nghe được lời thoại.` ra stderr và thoát 1. `True` thì in `progress = 100%` nếu có duration, thoát 0. Ngoại lệ sau khi đã qua `reject_model`: in `faster-whisper không chạy được trên GPU.` ra stderr và thoát 1. Không xóa file audio.

- [ ] **Step 4: Run test to verify it passes**

Run: `py -3 scripts/faster_whisper_test.py`

Expected: PASS. Máy không cài faster-whisper vẫn pass, vì test không gọi `main`.

- [ ] **Step 5: Commit**

```bash
git add scripts/faster_whisper.py scripts/faster_whisper_test.py
git commit -m "feat: print faster-whisper cues in the watch line format"
```

Chỉ chạy khi người dùng đã yêu cầu commit.

---

### Task 3: Gắn lệnh vào chép lời và xem trước

**Files:**
- Modify: `src-tauri/src/pipeline/media.rs` (`transcribe`)
- Modify: `src-tauri/src/watch.rs` (`stream_whisper`)
- Modify: `src-tauri/src/pipeline/mod.rs` nếu Task 1 chưa export đủ cho `watch.rs`
- Test: `src-tauri/src/pipeline/media.rs`

**Interfaces:**
- Consumes: `choose_asr`, `asr_argv`, `assemble_fast_path`, `faster_model_dir`, `run_tool_with`, `read_cues`, `write_json`
- Produces: `pub(crate) fn bundled_asr_script() -> PathBuf` — `CARGO_MANIFEST_DIR/../scripts/faster_whisper.py`. `pub(crate) fn detect_fast_path() -> Option<FastPath>` — script là file, `faster_model_dir` của folder `whisper` của project có giá trị, và probe CUDA thành công. `pub(crate) fn launch_for(root, audio, transcript, duration) -> Result<AsrLaunch, String>` — `choose_asr(detect_fast_path(), whisper_exe(), whisper_model)` rồi `asr_argv`. `transcribe` và `stream_whisper` chỉ dựng `Command` từ `AsrLaunch`.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn bundled_script_path_ends_at_the_python_file() {
    let path = bundled_asr_script();
    assert_eq!(
        path.file_name().and_then(|name| name.to_str()),
        Some("faster_whisper.py")
    );
}
```

Probe nằm trong `detect_fast_path`, không gọi từ test. Thử `py` với đối số `-3` trước, không có thì `python` với `python_args` rỗng. Timeout 20 giây. Đoạn `-c` import `faster_whisper` và `ctranslate2`, thoát 0 chỉ khi `get_cuda_device_count() > 0`. Test không cần GPU.

- [ ] **Step 2: Run test to verify it fails**

Run: `cd D:\Project\translate-video\src-tauri; cargo test --offline --lib pipeline::media::tests::bundled_script_path -- --test-threads=8`

Expected: FAIL vì `bundled_asr_script` chưa có.

- [ ] **Step 3: Implement the wiring**

`detect_fast_path` làm đúng việc probe ở trên. `launch_for` không probe lần nữa. Whisper-cli lấy từ `whisper_exe()` và `whisper_model` như cũ. `whisper_model` trả `None` khi chưa có exe thì truyền `None` vào `choose_asr`, không đổi câu lỗi.

`transcribe`: giữ nguyên nhánh `settle_transcript` và nhánh file có sẵn mà rỗng. Thay khối `Command::new(whisper_exe)` bằng `launch_for`. `duration` lấy từ `media_duration` của `audio.wav`; lỗi thì `None`. `current_dir` chỉ khi `cwd` là `Some`. Phần đọc `transcript.json`, ghi lại cue, xóa `audio.wav` giữ nguyên. Không thêm nhánh chạy lại bằng whisper-cli khi lệnh trả lỗi.

`stream_whisper`: cùng `launch_for`. Giữ callback stdout/stderr vào `Driver`. Đường nhanh in cùng dạng `[...]` và `progress = N%`, nên parser cũ không đổi.

- [ ] **Step 4: Run test to verify it passes**

Run: `cd D:\Project\translate-video\src-tauri; cargo test --offline --lib pipeline::media::tests:: -- --test-threads=8`

Expected: PASS. Rồi `cargo test --offline --lib watch::tests:: -- --test-threads=8` vẫn PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/pipeline/media.rs src-tauri/src/pipeline/mod.rs src-tauri/src/watch.rs
git commit -m "feat: transcribe and preview through one ASR launch"
```

Chỉ chạy khi người dùng đã yêu cầu commit.

---

### Task 4: Màn chuẩn bị và ghi chú cho lần sau

**Files:**
- Modify: `src-tauri/src/lib.rs` (`whisper_check`)
- Modify: `src/guides.ts` (mục `id: "whisper"`)
- Modify: `AGENTS.md` (đoạn nói whisper-cli chép lời)
- Create: `src/guides.test.ts`
- Test: `src-tauri/src/lib.rs` (`mod tests`), `src/guides.test.ts`

**Interfaces:**
- Consumes: `detect_fast_path() -> Option<FastPath>` từ Task 3. Màn chuẩn bị chỉ gọi `.is_some()`, không probe thêm.
- Produces: `pub(crate) enum LegacyWhisper { MissingExe, MissingModel, Ready { model_name: String } }` và `pub(crate) fn whisper_ready_detail(fast: bool, legacy: LegacyWhisper) -> (bool, String)` trong `lib.rs`. `whisper_check` trả `id` vẫn là `"whisper"`.

- [ ] **Step 1: Write the failing test**

Rust, trong `mod tests` của `lib.rs`:

```rust
#[test]
fn gpu_path_counts_as_ready_without_whisper_cli() {
    let (ready, detail) = whisper_ready_detail(true, LegacyWhisper::MissingExe);
    assert!(ready);
    assert_eq!(
        detail,
        "Đã thấy faster-whisper trên GPU và model large-v3-turbo."
    );
}

#[test]
fn old_whisper_messages_stay_when_the_gpu_path_is_absent() {
    let (ready, detail) = whisper_ready_detail(false, LegacyWhisper::MissingExe);
    assert!(!ready);
    assert_eq!(detail, "Chưa thấy whisper-cli.exe. Làm theo các bước bên cạnh.");
    let (ready, detail) = whisper_ready_detail(
        false,
        LegacyWhisper::Ready {
            model_name: "ggml-small.bin".into(),
        },
    );
    assert!(ready);
    assert_eq!(detail, "Đã thấy whisper-cli và file model ggml-small.bin.");
}
```

`src/guides.test.ts`, `node:test`:

```ts
import assert from "node:assert/strict";
import test from "node:test";
import { GUIDES } from "./guides.ts";

test("whisper guide keeps one slot and explains the GPU model", () => {
  const whisper = GUIDES.find((guide) => guide.id === "whisper");
  assert.ok(whisper);
  assert.equal(GUIDES.filter((guide) => guide.id === "whisper").length, 1);
  const text = whisper.steps.join("\n");
  assert.match(text, /py -3 -m pip install "faster-whisper>=1\.1\.0"/);
  assert.match(text, /whisper\\large-v3-turbo/);
  assert.match(text, /App không tự tải model GPU/);
  assert.ok(whisper.links.some((link) => link.url.includes("whisper-bin-x64.zip")));
  assert.ok(
    whisper.links.some((link) =>
      link.url.includes("huggingface.co/Systran/faster-whisper-large-v3-turbo"),
    ),
  );
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd D:\Project\translate-video\src-tauri; cargo test --offline --lib tests::gpu_path_counts_as_ready -- --test-threads=8`

Expected: FAIL vì `whisper_ready_detail` chưa có.

Run: `node --experimental-strip-types --test src/guides.test.ts`

Expected: FAIL vì hướng dẫn chưa có câu pip và link Systran.

- [ ] **Step 3: Implement the check and the copy**

`whisper_check`: `detect_fast_path().is_some()` thì dùng `whisper_ready_detail(true, ...)`. False thì nhánh exe/model hiện tại, đưa vào `LegacyWhisper`. Không thêm id công cụ.

Mục Whisper trong `guides.ts`: bước đầu nói Python 3.11 trở lên, đúng lệnh pip ở test, chép file của repo Systran vào `whisper\large-v3-turbo` sao cho có `model.bin`, rồi câu `App không tự tải model GPU`. Bước sau nói bấm Kiểm tra lại; thấy GPU thì dùng đường đó, không thì dùng whisper-cli phía dưới. Giữ các bước tải whisper-cli. Thêm link nhãn `Tải tay: model GPU` tới `https://huggingface.co/Systran/faster-whisper-large-v3-turbo`. Giữ hai link tải tay cũ.

Trong `AGENTS.md`, ngay sau câu đang nói `whisper-cli.exe` và `ggml-*.bin`, thêm một câu: nếu folder `whisper` có `large-v3-turbo/model.bin` và `py -3` (hoặc `python`) import được faster-whisper với CUDA thì chép lời và xem trước gọi `scripts/faster_whisper.py`; thiếu thì vẫn whisper-cli. Không đổi các câu khác.

- [ ] **Step 4: Run test to verify it passes**

Run: `cd D:\Project\translate-video\src-tauri; cargo test --offline --lib tests::gpu_path_counts_as_ready -- --test-threads=8`

Run: `cd D:\Project\translate-video\src-tauri; cargo test --offline --lib tests::old_whisper_messages_stay -- --test-threads=8`

Run: `node --experimental-strip-types --test src/guides.test.ts`

Expected: cả ba PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/lib.rs src-tauri/src/pipeline/media.rs src/guides.ts src/guides.test.ts AGENTS.md
git commit -m "feat: treat a local GPU Whisper model as the setup check"
```

Chỉ chạy khi người dùng đã yêu cầu commit.
