# Nghe trước phát câu đầu Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Lúc nghe hoặc xem trước, tiếng câu đầu ra ngay khi câu đó đọc xong; lúc xuất mới đọc hết wav, viết lại câu dài và ghép.

**Architecture:** Giữ session ONNX trong Web Worker từ lúc mở màn nghe hoặc lúc `start_watch` bắt đầu, và lần nạp cùng giọng đang chạy thì chờ chung một promise. Nghe trước có phụ đề vẫn đi `speakStream`: mỗi câu một lần `session.run`, blob câu 1 phát trong lúc câu sau đang đọc. Xem trước và nghe trước không phụ đề bỏ `speak_batch` (lô `piper.exe` chỉ giao wav khi tiến trình xong) và gọi `tts::run` trong cửa sổ, mỗi lần có wav thì `emit_update`. `speak_batch`, `fit` và `merge` của `start_dub` giữ nguyên.

**Tech Stack:** Tauri 2, Rust, React, Piper ONNX trong Web Worker (`src/tts`). Không thêm crate, không tải model, không IndexedDB.

**Spec:** `docs/superpowers/specs/2026-10-23-preview-stream-design.md`

## Global Constraints

- Bốn quy tắc trong spec: nạp model một lần và giữ session; đơn vị phát là một câu và trả audio khi câu đó xong; bật đang phát trước rồi cắm blob theo thứ tự; file xuất là việc riêng, làm sau khi nghe.
- Giữ Tauri. Video và audio không rời máy. Giọng local. Không Edge-TTS.
- Không sửa `speak_batch`, `synthesize_missing`, `fit`, `retake`, `merge`, `choose_engine`, `speak_native`. Xuất (`start_dub`) vẫn đọc cả lô; có `piper.exe` thì một tiến trình.
- Xem trước và nghe trước không gọi `speak_native`. `piper.exe` ghi wav vào thư mục tạm và chỉ copy ra khi tiến trình thoát, nên câu 1 không phát được trong lúc câu sau đang đọc.
- Không đổi `LISTEN_PART_WORDS` (1.000). Không đổi cách dịch từng lô. Không gộp cả phần vào một lần `session.run` (`speakPassage` / `whole: true`).
- Tiếng Việt vẫn qua `processTextForTTS` rồi `chunkText` trước `session.run`. Ngôn ngữ khác chỉ cắt bằng `listenPieces`.
- Chữ giao diện tiếng Việt. Không đổi `productName`, identifier, tiêu đề cửa sổ.
- Không commit trừ khi người dùng yêu cầu lúc thực thi. Bước commit của mỗi task chỉ chạy khi đã được yêu cầu; nếu chưa, bỏ qua.
- Test Rust: `cd D:\Project\translate-video\src-tauri; cargo test --offline --lib -- --test-threads=8`
- Test cửa sổ: `node --experimental-strip-types --test src/tts/streamGate.test.ts src/tts/prepare.test.ts` tại thư mục project.

## Review Focus

- Bấm nghe khi lần nạp từ lúc mở chưa xong: cùng một session, không tạo worker thứ hai. Test `loadJoin` ở task 1.
- Bấm lại hoặc đổi phần khi câu cũ chưa đọc xong: chunk của lần cũ không phát. Test `acceptChunk` ở task 2.
- Câu 1 đã có wav, câu 2 chưa: máy phát chỉ nhận câu 1, `readyUntil` là `end` của câu 1. Test `ready_watch_lines` ở task 3.
- Máy có `piper.exe`: xem trước và nghe trước vẫn `SpeakEngine::Window`. Xuất vẫn `speak_batch`. Test `preview_engine` ở task 3; không sửa test `choose_engine`.
- Câu không đọc được (file im) đứng trước câu chưa có wav: hiện tới câu im, không nhảy qua lỗ trống. Test `ready_watch_lines` ở task 3.

---

### Task 1: Một lần nạp cho cùng một giọng

**Files:**
- Create: `src/tts/streamGate.ts`
- Create: `src/tts/streamGate.test.ts`
- Modify: `src/tts/engine.ts` (`load`)

**Interfaces:**
- Consumes: không
- Produces: `export function loadJoin(loaded: string, inflight: string, key: string): "ready" | "join" | "start"`

`loaded` và `inflight` rỗng nghĩa là chưa có. `key` là `` `${lang}/${voice}` ``. `ready` khi `loaded === key` và `loaded` khác rỗng. `join` khi `inflight === key` và `inflight` khác rỗng. Còn lại `start`.

- [ ] **Step 1: Viết test fail**

```ts
import assert from "node:assert/strict";
import test from "node:test";
import { loadJoin } from "./streamGate.ts";

test("a second load of the voice already in memory does nothing", () => {
  assert.equal(loadJoin("vi/nam", "", "vi/nam"), "ready");
});

test("a second load of the voice still loading joins that load", () => {
  assert.equal(loadJoin("", "vi/nam", "vi/nam"), "join");
});

test("a different voice starts a new load", () => {
  assert.equal(loadJoin("vi/nam", "", "vi/nu"), "start");
  assert.equal(loadJoin("", "", "vi/nam"), "start");
});
```

- [ ] **Step 2: Chạy test để thấy fail**

Run: `node --experimental-strip-types --test src/tts/streamGate.test.ts`
Expected: FAIL vì chưa có `streamGate.ts`

- [ ] **Step 3: Thêm `loadJoin` và dùng trong `TtsEngine.load`**

`load` giữ `loadingKey: string` và `loadingPromise: Promise<void> | null`. Gọi `loadJoin(this.voice, this.loadingKey, key)`. `ready` thì return. `join` thì return `this.loadingPromise`. `start` thì `stop()`, gán `loadingKey`, tạo promise nạp như hiện tại, và trong `finally` xóa `loadingKey` cùng `loadingPromise` chỉ khi promise đó vẫn là promise hiện tại.

- [ ] **Step 4: Chạy test để thấy pass**

Run: `node --experimental-strip-types --test src/tts/streamGate.test.ts`
Expected: PASS

- [ ] **Step 5: Commit**

Bỏ qua nếu người dùng chưa yêu cầu commit.

### Task 2: Nghe trước có phụ đề nạp lúc mở và phát câu 1

**Files:**
- Modify: `src/tts/streamGate.ts`
- Modify: `src/tts/streamGate.test.ts`
- Modify: `src/tts/prepare.test.ts`
- Modify: `src/ListenPart.tsx` (effect mở màn, `create`, callback `speakStream`)

**Interfaces:**
- Consumes: `loadJoin` từ task 1; `TtsEngine.load`, `TtsEngine.speakStream`, `listenPieces`
- Produces: `export function acceptChunk(mine: number, current: number): boolean` — true khi hai số bằng nhau

- [ ] **Step 1: Viết test fail**

Thêm vào `streamGate.test.ts`:

```ts
test("a chunk from an older click is dropped", () => {
  assert.equal(acceptChunk(1, 2), false);
  assert.equal(acceptChunk(2, 2), true);
});
```

Thêm vào `prepare.test.ts`:

```ts
test("an empty part has no sentence to speak", () => {
  assert.deepEqual(listenPieces("  \n"), []);
});
```

- [ ] **Step 2: Chạy test để thấy fail**

Run: `node --experimental-strip-types --test src/tts/streamGate.test.ts src/tts/prepare.test.ts`
Expected: FAIL ở `acceptChunk` chưa export. `listenPieces` có thể đã pass; giữ assertion.

- [ ] **Step 3: Nối màn nghe**

`acceptChunk` là `mine === current`.

Trong `ListenPart`, effect theo `[lang, voice]`: nếu `voice` khác rỗng thì `engine.load(lang, voice)` và nuốt lỗi (lần bấm sẽ hiện lỗi nếu vẫn chưa nạp được). Effect này không gọi `cancel` và không xóa audio.

`create`: tăng `gen`, `cancel`, xóa audio, `paused = false`, `setPlaying(true)`, `setMaking(true)`, rồi `load` (lần nạp đang chạy thì join), rồi `speakStream`. Callback chỉ `pushChunk` khi `acceptChunk(token, gen.current)`. Hết stream mà không có chunk và token còn hiện tại thì `setError("Không có chữ để đọc.")` và `setPlaying(false)`. Lỗi khác `"Đã dừng."` thì `setPlaying(false)`.

Không ghi wav, không ghép file tải trên màn này.

- [ ] **Step 4: Chạy test để thấy pass**

Run: `node --experimental-strip-types --test src/tts/streamGate.test.ts src/tts/prepare.test.ts`
Expected: PASS

- [ ] **Step 5: Commit**

Bỏ qua nếu người dùng chưa yêu cầu commit.

### Task 3: Xem trước phát câu đã có wav

**Files:**
- Modify: `src-tauri/src/pipeline/speak.rs` (thêm `preview_engine` và `speak_preview`; không sửa `speak_batch`)
- Modify: `src-tauri/src/pipeline/mod.rs` (re-export hai hàm đó cạnh `speak_batch`)
- Modify: `src-tauri/src/watch.rs` (`ingest`, đầu `run_watch`, test)
- Modify: `src/tts/bridge.ts` (nghe `tts-warm`)
- Modify: `AGENTS.md` (một câu ở đoạn Xem trước)

**Interfaces:**
- Consumes: `crate::listen::listen_engine`, `crate::tts::run`, `emit_update`, `load` của `TtsEngine` sau task 1
- Produces:
  - `pub(crate) fn preview_engine() -> crate::piper::SpeakEngine` trả về `listen_engine()`
  - `pub(crate) fn speak_preview(app: &AppHandle, job: &Job, lines_len: usize, batch: Vec<(usize, String, PathBuf)>, cancel: &AtomicBool, on_saved: &dyn Fn(usize)) -> Result<(), String>`
  - Sự kiện `tts-warm`, payload `{ lang: string, voice: string }`

`speak_preview` lấy `selected_voice(app, &job.target)`, gọi `tts::run` với `whole: false`, và gọi `on_saved(saved)` mỗi lần số wav đã ghi tăng. Không gọi `locate`, `choose_engine`, `speak_native`, `prepare`.

- [ ] **Step 1: Viết test fail**

Trong `watch.rs`, test:

```rust
#[test]
fn the_first_ready_line_plays_before_the_next_wav_exists() {
    let lines = vec![line(0.0, 1.2), line(1.2, 3.0)];
    let (indexes, until) = ready_watch_lines(&lines, |index| index == 1, false);
    assert_eq!(indexes, vec![1]);
    assert_eq!(until, 1.2);
}

#[test]
fn silence_shows_and_a_missing_later_line_still_stops_the_list() {
    let lines = vec![line(0.0, 1.0), line(1.0, 2.0), line(2.0, 3.5)];
    let (indexes, until) = ready_watch_lines(&lines, |index| index < 3, false);
    assert_eq!(indexes, vec![1, 2]);
    assert_eq!(until, 2.0);
}

#[test]
fn preview_speech_stays_in_the_window() {
    assert_eq!(
        crate::pipeline::preview_engine(),
        crate::piper::SpeakEngine::Window
    );
}
```

- [ ] **Step 2: Chạy test để thấy fail**

Run: `cd D:\Project\translate-video\src-tauri; cargo test --offline --lib preview_speech_stays_in_the_window -- --test-threads=8`
Expected: FAIL vì chưa có `preview_engine`. Hai test `ready_watch_lines` có thể đã pass sau khi thêm; giữ chúng.

- [ ] **Step 3: Nối xem trước**

Thêm `preview_engine` và `speak_preview` như khối Interfaces. Trong `pipeline/mod.rs`, re-export cả hai cạnh `speak_batch`. `watch.rs` gọi `crate::pipeline::speak_preview` và `crate::pipeline::preview_engine`.

`ingest`: chỗ đang gọi `speak_batch` đổi thành `speak_preview`. `on_saved` gọi `emit_update` với cùng tham số như lần emit cuối hàm (`running: true`, `published: false`, detail đang dùng, `text_only`). Lần emit sau `speak_preview` giữ nguyên.

`run_watch`: sau khi đã có `voice` và `text_only` là false, `app.emit("tts-warm", serde_json::json!({ "lang": target_code, "voice": voice }))`. Không chờ. Lỗi emit thì bỏ qua. `text_only` không emit.

`bridge.ts`: nghe `tts-warm` và `void engine.load(lang, voice)`. Không đặt `busy`. `handle` của `tts-request` vẫn `load` lại; cùng giọng thì `ready` hoặc `join`.

`AGENTS.md`, câu đang là "Rust đọc dòng in ra của whisper-cli, dịch và đọc từng cụm bằng đúng đường dịch và Piper hiện có." Đổi thành: "Rust đọc dòng in ra của whisper-cli, dịch từng cụm, rồi đọc trong cửa sổ từng câu và phát câu nào đã có wav trong lúc câu sau vẫn đang đọc. Có `piper.exe` cũng không dùng ở đường này. Lúc ghép video hoặc xuất audio mới đọc cả lô."

- [ ] **Step 4: Chạy test để thấy pass**

Run: `cd D:\Project\translate-video\src-tauri; cargo test --offline --lib -- --test-threads=8`
Expected: PASS, gồm ba test mới. `choose_engine` và test của `speak_batch` vẫn pass.

- [ ] **Step 5: Commit**

Bỏ qua nếu người dùng chưa yêu cầu commit.
