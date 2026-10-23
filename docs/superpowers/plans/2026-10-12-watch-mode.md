# Chế độ Xem dần Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Thêm chế độ Xem dần: phát video gốc trong app sau cụm câu đầu, hạ tiếng gốc lúc có giọng đọc, trong khi Whisper local vẫn chạy tiếp trên cả file.

**Architecture:** `whisper-cli` chạy một lần trên `audio.wav`. Một luồng chỉ đọc stdout/stderr (tách riêng) và đẩy cụm câu qua channel. Luồng kia dịch bằng đường dịch hiện có, đọc bằng `tts::run` vào `watch/lines/`, rồi phát sự kiện `watch-update`. Cửa sổ phát file gốc, `video.volume` 0,22 trong khoảng câu đã có wav. `translation.json` và `lines/NNN.wav` chỉ xuất hiện khi cả video xong, để `start_dub` khỏi bỏ qua nhầm.

**Tech Stack:** Tauri 2, Rust, React, whisper.cpp (`whisper-cli`, `-oj`), Piper trong WebView (`tts::run`), FFmpeg tách tiếng như bước `extract` hiện tại. Không thêm crate, không thêm dependency npm.

**Spec:** `docs/superpowers/specs/2026-10-12-watch-mode-design.md`

## Global Constraints

- Giữ Tauri. Video và audio không rời máy. Chữ lời thoại chỉ đi mạng khi người dùng đã chọn API dịch.
- Whisper local và Piper local. Không gọi API speech-to-text. Không Edge-TTS.
- Chế độ file giữ 8 bước `extract`, `transcribe`, `split`, `terms`, `translate`, `speak`, `fit`, `merge`. Không thêm id bước vào `STEPS` của màn tiến độ file.
- File trung gian cũ vẫn đọc được. Không đổi dạng `transcript.json`, `lines.json`, `translation.json`, `lines/NNN.wav`.
- Ngôn ngữ giao diện tiếng Việt. Nút mới đúng chữ **Xem dần**, **Ghép video**, **Đang chép đoạn đầu**, **Đang dịch tiếp**.
- Không đổi `productName`, identifier, tiêu đề cửa sổ.
- Không nút tải mới.
- Test Rust: `cd D:\Project\translate-video\src-tauri; cargo test --offline`.
- Test trình phát: `node --experimental-strip-types --test src/watchPlay.test.ts` tại thư mục project.
- Commit chỉ khi người dùng yêu cầu trong lúc thực thi. Folder này có thể chưa phải git repo.

## Review Focus

Năm chỗ spec bắt buộc phải có test, vì im lặng của spec không cho phép làm hỏng chế độ file hay nuốt một câu:

1. Dừng giữa chừng khi Whisper chưa thoát mã 0 mà `transcript.json` vốn chưa có: phải xóa `transcript.json`, nếu không lần **Chạy** sau sẽ bỏ qua chép lời trên file dở.
2. `translation.json` và `lines/NNN.wav` chưa được ghi khi mới xong một cụm: lần **Chạy** sau sẽ thấy wav lệch câu và bỏ qua đọc.
3. Mảnh stdout của một câu bị xen giữa bằng dòng `progress` từ stderr: câu đó vẫn phải thành một cue, không được nuốt.
4. Người dùng bấm dừng video: hết hold không được tự `play()`.
5. Kéo tay vượt `readyUntil` trong lúc còn chạy: thời gian phát quay về `readyUntil`, không phát tiếng gốc của đoạn chưa dịch.

---

## File Structure

| File | Trách nhiệm |
|---|---|
| `src-tauri/src/watch.rs` | Phân tích dòng Whisper, quyết định gửi cụm, quyết định xóa hay ghi file. Không gọi mạng. |
| `src-tauri/src/pipeline.rs` | `pub(crate)` cho `extract`, `translate_work`, `media_duration`, `read_cues`, `parse_clock`. Callback `run_tool_with` nhận biết stdout hay stderr. |
| `src-tauri/src/lib.rs` | `mod watch`, lệnh `start_watch`. |
| `src/watchPlay.ts` | Hold, volume, câu đang hiện, thời gian sau khi tua. |
| `src/watchPlay.test.ts` | Test các hàm đó. |
| `src/WatchLive.tsx` | Khung video, phụ đề, thẻ audio, nút Ghép video. |
| `src/Work.tsx` | Nút **Xem dần**, view `live`. |
| `src/App.css` | Lớp chữ phụ đề đè lên video. |
| `AGENTS.md`, `CLAUDE.md` | Một đoạn mô tả chế độ, để lần sửa sau không ghi đè file canonical sớm. |

### Task 1: Đọc dòng Whisper và cắt cụm

**Files:**
- Create: `src-tauri/src/watch.rs`
- Modify: `src-tauri/src/lib.rs` (thêm `mod watch;` cạnh `mod pipeline;`)
- Test: `src-tauri/src/watch.rs` (`#[cfg(test)]`)

**Interfaces:**
- Consumes: không
- Produces:
  - `pub(crate) struct PrintedCue { pub start: f64, pub end: f64, pub text: String }`
  - `pub(crate) struct Driver { /* private */ }`
  - `pub(crate) fn driver(duration_secs: f64) -> Driver`
  - `impl Driver { pub(crate) fn push_stdout(&mut self, chunk: &str) -> Vec<PrintedCue>; pub(crate) fn push_stderr(&mut self, chunk: &str) -> Vec<PrintedCue>; pub(crate) fn finish(&mut self) -> Vec<PrintedCue> }`
  - `PrintedCue` trả về là các câu vừa được chốt ở lần gọi đó, theo thứ tự. Câu chưa đủ dòng thì chưa trả.

- [ ] **Step 1: Viết test thất bại**

Thêm `mod watch;` vào `src-tauri/src/lib.rs`. Tạo `src-tauri/src/watch.rs` chỉ với module test dưới đây và các `use` cần để file biên dịch được đến chỗ thiếu hàm (`pub(crate) fn driver` chưa có thì test không compile: đó là FAIL đúng kỳ vọng).

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stderr_progress_between_stdout_halves_keeps_the_cue() {
        let mut driver = driver(120.0);
        assert!(driver.push_stdout("[00:00:01.000 --> 00:00:0").is_empty());
        assert!(driver.push_stderr("whisper_print_progress_callback: progress = 10%\n").is_empty());
        let got = driver.push_stdout("4.000]  Xin chào\n");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].text, "Xin chào");
        assert!((got[0].start - 1.0).abs() < 0.001);
        assert!((got[0].end - 4.0).abs() < 0.001);
    }

    #[test]
    fn four_cues_flush_even_when_whisper_time_is_still_zero() {
        let mut driver = driver(120.0);
        let block = "\
[00:00:00.000 --> 00:00:02.000]  một
[00:00:03.000 --> 00:00:05.000]  hai
[00:00:06.000 --> 00:00:08.000]  ba
[00:00:09.000 --> 00:00:11.000]  bốn
";
        let flushed = driver.push_stdout(block);
        assert_eq!(flushed.len(), 4);
        assert_eq!(flushed[3].text, "bốn");
    }

    #[test]
    fn one_cue_waits_until_whisper_is_eight_seconds_past_it() {
        let mut driver = driver(100.0);
        let held = driver.push_stdout("[00:00:01.000 --> 00:00:03.000]  một\n");
        assert!(held.is_empty());
        // 11% của 100 giây = 11 giây, vượt cuối câu (3 giây) đúng 8 giây.
        let flushed = driver.push_stderr("progress = 11%\n");
        assert_eq!(flushed.len(), 1);
        assert_eq!(flushed[0].text, "một");
    }

    #[test]
    fn span_of_twelve_seconds_flushes_before_the_fourth_cue() {
        let mut driver = driver(200.0);
        let block = "\
[00:00:00.000 --> 00:00:02.000]  một
[00:00:10.000 --> 00:00:12.000]  hai
";
        let flushed = driver.push_stdout(block);
        assert_eq!(flushed.len(), 2);
    }

    #[test]
    fn finish_flushes_the_cue_that_was_still_waiting() {
        let mut driver = driver(30.0);
        assert!(driver.push_stdout("[00:00:01.000 --> 00:00:02.000]  một\n").is_empty());
        let flushed = driver.finish();
        assert_eq!(flushed.len(), 1);
        assert!(driver.finish().is_empty());
    }
}
```

- [ ] **Step 2: Chạy test để thấy fail**

Run: `cd D:\Project\translate-video\src-tauri; cargo test --offline --lib watch::tests -- --test-threads=8`

Expected: FAIL vì `driver` chưa tồn tại.

- [ ] **Step 3: Viết implementation**

`parse_clock` đang là hàm private trong `pipeline.rs`. Đổi thành `pub(crate) fn parse_clock`. `watch.rs` gọi `crate::pipeline::parse_clock`.

```rust
use crate::pipeline::parse_clock;

pub(crate) const FLUSH_COUNT: usize = 4;
pub(crate) const FLUSH_SPAN_SECS: f64 = 12.0;
pub(crate) const FLUSH_AHEAD_SECS: f64 = 8.0;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PrintedCue {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

pub(crate) struct Driver {
    duration: f64,
    stdout: String,
    stderr: String,
    cues: Vec<PrintedCue>,
    committed: usize,
    whisper_time: f64,
}

pub(crate) fn driver(duration_secs: f64) -> Driver {
    Driver {
        duration: duration_secs,
        stdout: String::new(),
        stderr: String::new(),
        cues: Vec::new(),
        committed: 0,
        whisper_time: 0.0,
    }
}

impl Driver {
    pub(crate) fn push_stdout(&mut self, chunk: &str) -> Vec<PrintedCue> {
        self.stdout.push_str(chunk);
        self.drain_stdout();
        self.take()
    }

    pub(crate) fn push_stderr(&mut self, chunk: &str) -> Vec<PrintedCue> {
        self.stderr.push_str(chunk);
        self.drain_stderr();
        self.take()
    }

    pub(crate) fn finish(&mut self) -> Vec<PrintedCue> {
        self.drain_stdout();
        self.drain_stderr();
        let rest = self.cues[self.committed..].to_vec();
        self.committed = self.cues.len();
        rest
    }

    fn drain_stdout(&mut self) {
        let mut lines: Vec<String> = self.stdout.split(['\n', '\r']).map(str::to_string).collect();
        self.stdout = lines.pop().unwrap_or_default();
        for line in lines {
            if let Some(cue) = parse_print(&line) {
                self.cues.push(cue);
            }
        }
    }

    fn drain_stderr(&mut self) {
        let mut lines: Vec<String> = self.stderr.split(['\n', '\r']).map(str::to_string).collect();
        self.stderr = lines.pop().unwrap_or_default();
        for line in lines {
            if let Some(fraction) = progress_fraction(&line) {
                self.whisper_time = fraction * self.duration;
            }
        }
    }

    fn take(&mut self) -> Vec<PrintedCue> {
        let pending = &self.cues[self.committed..];
        if pending.is_empty() {
            return Vec::new();
        }
        let span = pending.last().unwrap().end - pending[0].start;
        let ahead = self.whisper_time - pending.last().unwrap().end;
        let ready = pending.len() >= FLUSH_COUNT || span >= FLUSH_SPAN_SECS || ahead >= FLUSH_AHEAD_SECS;
        if !ready {
            return Vec::new();
        }
        let out = pending.to_vec();
        self.committed = self.cues.len();
        out
    }
}

fn parse_print(line: &str) -> Option<PrintedCue> {
    let rest = line.trim().strip_prefix('[')?;
    let (times, text) = rest.split_once(']')?;
    let (from, to) = times.split_once("-->")?;
    let start = parse_clock(from.trim())?;
    let end = parse_clock(to.trim())?;
    let text = text.trim().to_string();
    if text.is_empty() {
        return None;
    }
    Some(PrintedCue { start, end: end.max(start + 0.05), text })
}

fn progress_fraction(line: &str) -> Option<f64> {
    let rest = line.split("progress =").nth(1)?;
    let percent: f64 = rest.trim().trim_end_matches('%').trim().parse().ok()?;
    Some((percent / 100.0).clamp(0.0, 1.0))
}
```

- [ ] **Step 4: Chạy test**

Run: `cd D:\Project\translate-video\src-tauri; cargo test --offline --lib watch::tests -- --test-threads=8`

Expected: PASS, gồm 5 test của task này.

- [ ] **Step 5: Commit**

Chỉ khi người dùng đã yêu cầu commit trong lúc thực thi, và thư mục đã là git repo.

```bash
git add src-tauri/src/watch.rs src-tauri/src/lib.rs src-tauri/src/pipeline.rs
git commit -m "feat: cắt cụm câu Whisper ngay khi in ra"
```

### Task 2: Khi nào được ghi file canonical, khi nào phải xóa

**Files:**
- Modify: `src-tauri/src/watch.rs`
- Test: cùng file

**Interfaces:**
- Consumes: không dùng `Driver`
- Produces:
  - `pub(crate) struct CancelAction { pub delete_transcript: bool, pub delete_watch_dir: bool }`
  - `pub(crate) fn cancel_action(transcript_existed_at_start: bool, whisper_exit_ok: bool) -> CancelAction`
  - `pub(crate) fn publish_allowed(line_count: usize, attempted: usize, spoken: usize) -> bool`

`publish_allowed` đúng chỉ khi `line_count > 0` và cả `attempted` lẫn `spoken` bằng `line_count`. Wav im lặng tính là đã đọc. `cancel_action` luôn `delete_watch_dir: true`. `delete_transcript` đúng chỉ khi transcript chưa có lúc bắt đầu và Whisper không thoát mã 0.

- [ ] **Step 1: Viết test thất bại**

```rust
#[test]
fn cancel_removes_a_transcript_this_run_did_not_finish() {
    let action = cancel_action(false, false);
    assert!(action.delete_transcript);
    assert!(action.delete_watch_dir);
}

#[test]
fn cancel_keeps_a_transcript_whisper_already_finished() {
    let action = cancel_action(false, true);
    assert!(!action.delete_transcript);
    assert!(action.delete_watch_dir);
}

#[test]
fn cancel_keeps_a_transcript_that_was_already_there() {
    let action = cancel_action(true, false);
    assert!(!action.delete_transcript);
}

#[test]
fn a_partial_batch_is_not_publishable() {
    assert!(!publish_allowed(10, 4, 4));
    assert!(!publish_allowed(10, 10, 4));
    assert!(!publish_allowed(0, 0, 0));
    assert!(publish_allowed(10, 10, 10));
}
```

- [ ] **Step 2: Chạy test để thấy fail**

Run: `cd D:\Project\translate-video\src-tauri; cargo test --offline --lib watch::tests::cancel_removes_a_transcript_this_run_did_not_finish -- --test-threads=8`

Expected: FAIL vì `cancel_action` chưa có.

- [ ] **Step 3: Viết implementation**

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CancelAction {
    pub delete_transcript: bool,
    pub delete_watch_dir: bool,
}

pub(crate) fn cancel_action(transcript_existed_at_start: bool, whisper_exit_ok: bool) -> CancelAction {
    CancelAction {
        delete_transcript: !transcript_existed_at_start && !whisper_exit_ok,
        delete_watch_dir: true,
    }
}

pub(crate) fn publish_allowed(line_count: usize, attempted: usize, spoken: usize) -> bool {
    line_count > 0 && attempted == line_count && spoken == line_count
}
```

- [ ] **Step 4: Chạy test**

Run: `cd D:\Project\translate-video\src-tauri; cargo test --offline --lib watch::tests -- --test-threads=8`

Expected: PASS toàn bộ test trong `watch::tests`.

- [ ] **Step 5: Commit**

Chỉ khi người dùng yêu cầu.

```bash
git add src-tauri/src/watch.rs
git commit -m "feat: chặn file lồng dở làm chế độ file bỏ bước"
```

### Task 3: Quyết định phát, hạ tiếng, và tua

**Files:**
- Create: `src/watchPlay.ts`
- Create: `src/watchPlay.test.ts`

**Interfaces:**
- Consumes: không
- Produces:
  - `export const HOLD_GAP_SECS = 1`
  - `export const DUCK_VOLUME = 0.22`
  - `export function frame(current: number, readyUntil: number, running: boolean, lines: Span[]): Frame`
  - `export function wantsPlay(hold: boolean, userPaused: boolean, started: boolean): boolean`
  - `Span = { start: number; end: number }`
  - `Frame = { hold: boolean; volume: number; line: number | null; time: number }`

`line` là chỉ số trong `lines` của câu cuối cùng thỏa `start <= time < end`. Không có câu thì `null` và `volume` là 1. `hold` đúng khi `running` và `readyUntil - time < HOLD_GAP_SECS`. Lúc hold, `volume` là 1. `time` bằng `readyUntil` khi `running` và `current > readyUntil`, còn lại bằng `current`. `wantsPlay` đúng chỉ khi đã bắt đầu, không hold, và người dùng không tự dừng.

- [ ] **Step 1: Viết test thất bại**

`src/watchPlay.test.ts`:

```typescript
import assert from "node:assert/strict";
import test from "node:test";
import { DUCK_VOLUME, frame, wantsPlay } from "./watchPlay.ts";

const lines = [
  { start: 1, end: 4 },
  { start: 6, end: 9 },
];

test("seek past the ready point snaps back while the run is going", () => {
  const got = frame(20, 12, true, lines);
  assert.equal(got.time, 12);
});

test("a line ducks and a gap between lines does not", () => {
  const inside = frame(2, 30, true, lines);
  assert.equal(inside.line, 0);
  assert.equal(inside.volume, DUCK_VOLUME);
  assert.equal(inside.hold, false);
  const gap = frame(5, 30, true, lines);
  assert.equal(gap.line, null);
  assert.equal(gap.volume, 1);
});

test("holding the lead restores the original volume", () => {
  const got = frame(11.2, 12, true, [{ start: 10, end: 12 }]);
  assert.equal(got.hold, true);
  assert.equal(got.volume, 1);
});

test("after the run finishes, seeking ahead is allowed and the tail is not held", () => {
  const got = frame(40, 12, false, lines);
  assert.equal(got.time, 40);
  assert.equal(got.hold, false);
  assert.equal(got.volume, 1);
});

test("a user pause is not cleared when the hold ends", () => {
  assert.equal(wantsPlay(false, true, true), false);
  assert.equal(wantsPlay(true, false, true), false);
  assert.equal(wantsPlay(false, false, true), true);
  assert.equal(wantsPlay(false, false, false), false);
});
```

- [ ] **Step 2: Chạy test để thấy fail**

Run: `node --experimental-strip-types --test src/watchPlay.test.ts`

Expected: FAIL vì không tìm thấy `src/watchPlay.ts`.

- [ ] **Step 3: Viết implementation**

`src/watchPlay.ts`:

```typescript
export const HOLD_GAP_SECS = 1;
export const DUCK_VOLUME = 0.22;

export type Span = { start: number; end: number };

export type Frame = {
  hold: boolean;
  volume: number;
  line: number | null;
  time: number;
};

export function frame(current: number, readyUntil: number, running: boolean, lines: Span[]): Frame {
  const time = running && current > readyUntil ? readyUntil : current;
  const hold = running && readyUntil - time < HOLD_GAP_SECS;
  let line: number | null = null;
  for (let index = 0; index < lines.length; index += 1) {
    const item = lines[index];
    if (time >= item.start && time < item.end) line = index;
  }
  const volume = !hold && line !== null ? DUCK_VOLUME : 1;
  return { hold, volume, line, time };
}

export function wantsPlay(hold: boolean, userPaused: boolean, started: boolean): boolean {
  return started && !hold && !userPaused;
}
```

- [ ] **Step 4: Chạy test**

Run: `node --experimental-strip-types --test src/watchPlay.test.ts`

Expected: PASS, 5 test.

- [ ] **Step 5: Commit**

Chỉ khi người dùng yêu cầu.

```bash
git add src/watchPlay.ts src/watchPlay.test.ts
git commit -m "feat: quyết định lúc nào hạ tiếng gốc khi xem dần"
```

### Task 4: Chạy lượt xem và phát sự kiện

**Files:**
- Modify: `src-tauri/src/pipeline.rs` (visibility, callback có cờ stderr)
- Modify: `src-tauri/src/watch.rs` (`run_watch`)
- Modify: `src-tauri/src/lib.rs` (lệnh `start_watch`)
- Test: `cargo test --offline --lib` vẫn xanh, gồm test cũ của `run_tool_with` nếu chữ ký callback đổi làm gãy test thì sửa call site, không đổi kỳ vọng của test cũ

**Interfaces:**
- Consumes: `Driver`, `cancel_action`, `publish_allowed`, `PrintedCue`, `tts::run`, `ollama::split_line`, `ollama::divide_time`, `pipeline::extract`, `pipeline::translate_work`, `pipeline::read_cues`, `pipeline::media_duration`
- Produces:
  - `pub(crate) fn run_watch(app, video, output_dir, target, cancel) -> Result<(), String>`
  - Lệnh Tauri `start_watch(video, output_dir, target) -> Result<(), String>`
  - Sự kiện `watch-update` với payload ở spec (camelCase)

Đổi callback của `run_tool_with` từ `FnMut(&str)` thành `FnMut(bool, &str)`. Tham số bool là `is_err`. Call site trong `transcribe` bỏ qua bool và vẫn gọi `whisper_fraction` như cũ. `extract`, `translate_work`, `media_duration`, `read_cues` chuyển thành `pub(crate)`.

`run_watch` làm đúng các việc sau, không thêm bước fit hay merge:

1. `Job::open`, kiểm tra giọng bằng `crate::selected_voice`. Thiếu giọng thì trả lỗi cùng câu với `pipeline::run`.
2. `extract`. Đo độ dài bằng `media_duration`.
3. `transcript_existed_at_start = file_ready(transcript)`.
4. Nếu đã đủ lines.json, translation.json và mọi `line_wav` khớp số câu: `allow_file` video gốc, emit một `watch-update` với `running: false`, `published: true`, `readyUntil` bằng cuối câu cuối, rồi return Ok. Không gọi Whisper.
5. Nếu có transcript đọc được: không gọi Whisper. Coi như Whisper đã thoát mã 0. Cắt câu đã lưu thành các cụm bằng cùng ngưỡng `FLUSH_*` (nhét từng cue vào `Driver` qua một hàm `push_cue` pub(crate) trong cùng file, rồi `finish` cho cụm cuối).
6. Nếu chưa có transcript: spawn một thread chạy whisper-cli với đúng bộ arg của `transcribe` (`-m`, `-f audio.wav`, `-t`, `-l auto`, `-oj`, `-pp`, `-of transcript`, `current_dir = job.root`). Trong `on_text`, stdout gọi `push_stdout`, stderr gọi `push_stderr`. Cụm trả về gửi qua `mpsc` bằng `send`, không dịch trong callback. Thread chính `recv` cho đến khi channel đóng, rồi `join` thread Whisper.
7. Mỗi cụm: `split_line` + `divide_time` thành `Cue`, nối vào vec câu. Dịch phần mới bằng `translate_work` với `Glossary { theme: "", terms: vec![] }`, `max_batches: None`. Wav ghi vào `job.root.join("watch").join("lines")` với tên `{index:03}.wav`, index từ 1 theo thứ tự câu đã nối. Gọi `tts::run` với các câu của cụm chưa có wav. Câu `is_speakable` sai thì ghi wav im lặng bằng cùng cách `synthesize_missing` (đưa `write_silence` lên `pub(crate)` và gọi nó).
8. Sau mỗi cụm đã có wav: `allow_file` từng wav và file video, emit `watch-update` (`running: true`, `published: false`, `readyUntil` = `end` của câu cuối đã có wav). `detail` là `Đang dịch`.
9. Hết câu và `publish_allowed`: ghi `lines.json`, `translation.json`, copy wav sang `lines/NNN.wav` (không ghi đè file đã có và dài hơn 0 byte), xóa `watch/partial.json` và `watch/lines`, ghi `watch/done.json` chứa `{"published":true}`, emit `published: true`, `running: false`.
10. `partial.json` ghi sau mỗi cụm đã dịch (vec `TranslatedLine` chưa đủ cũng được) để tắt app giữa chừng còn thấy trong folder việc. Chế độ file không đọc file này.
11. Lỗi hoặc `cancel`: áp `cancel_action`. Xóa đúng những gì struct bảo. Không ghi `lines.json` hay `translation.json`. Trả `Err("Đã dừng.")` khi bị hủy, giống pipeline.

`start_watch` copy khuôn `start_dub`: từ chối khi `running` hoặc `captioning`, đặt `cancel` về false, `spawn_blocking`, cuối cùng `finish_dub` với `Ok("")` khi xem xong (không có mp4) hoặc `Err`. Cửa sổ phân biệt bằng sự kiện `watch-update`, không bằng đường dẫn trả về.

Whisper và vòng dịch không được dùng chung một luồng. Ghi chú trong hàm: dịch trong callback đọc pipe sẽ nghẽn pipe.

- [ ] **Step 1: Viết test cho `push_cue`**

`push_cue` dùng cùng `take()` với đường stdout, để transcript có sẵn không đi một luật cắt cụm khác.

```rust
#[test]
fn seeded_cues_flush_on_the_same_four_cue_rule() {
    let mut driver = driver(80.0);
    assert!(driver.push_cue(0.0, 2.0, "một").is_empty());
    assert!(driver.push_cue(3.0, 5.0, "hai").is_empty());
    assert!(driver.push_cue(6.0, 8.0, "ba").is_empty());
    let flushed = driver.push_cue(9.0, 11.0, "bốn");
    assert_eq!(flushed.len(), 4);
}
```

- [ ] **Step 2: Chạy test để thấy fail**

Run: `cd D:\Project\translate-video\src-tauri; cargo test --offline --lib watch::tests::seeded_cues_flush_on_the_same_four_cue_rule -- --test-threads=8`

Expected: FAIL vì `push_cue` chưa có.

- [ ] **Step 3: Thêm `push_cue` và `run_watch`**

```rust
impl Driver {
    pub(crate) fn push_cue(&mut self, start: f64, end: f64, text: &str) -> Vec<PrintedCue> {
        let text = text.trim().to_string();
        if text.is_empty() {
            return Vec::new();
        }
        self.cues.push(PrintedCue { start, end: end.max(start + 0.05), text });
        self.take()
    }
}
```

Phần `run_watch` viết đủ 11 bước ở mục Interfaces. Không để hàm trống. Lệnh whisper copy nguyên khối arg từ `transcribe` trong `pipeline.rs` (khoảng dòng 688–704), gồm `current_dir(&job.root)` và tên tương đối `audio.wav`.

Sửa `run_tool_with` để callback nhận `is_err`. Trong `transcribe`, closure đổi thành `|is_err, text|` và giữ nguyên thân cũ (vẫn parse progress từ mọi dòng, vì dòng câu không chứa `progress =`).

Đăng ký `start_watch` trong `generate_handler!` ngay sau `start_dub`.

- [ ] **Step 4: Chạy test**

Run: `cd D:\Project\translate-video\src-tauri; cargo test --offline --lib -- --test-threads=8`

Expected: PASS. Test pipeline cũ vẫn xanh. Không cần chạy Whisper thật ở task này.

- [ ] **Step 5: Commit**

Chỉ khi người dùng yêu cầu.

```bash
git add src-tauri/src/watch.rs src-tauri/src/pipeline.rs src-tauri/src/lib.rs
git commit -m "feat: chạy xem dần song song với Whisper"
```

### Task 5: Nút Xem dần và khung phát

**Files:**
- Create: `src/WatchLive.tsx`
- Modify: `src/Work.tsx` (nút, view `live`, gọi `start_watch`)
- Modify: `src/App.css` (lớp `.live-caption`)

**Interfaces:**
- Consumes: `frame`, `wantsPlay`, `HOLD_GAP_SECS` từ `src/watchPlay.ts`; lệnh `start_watch`; sự kiện `watch-update`; `convertFileSrc`; `cancel_dub`
- Produces: view `live` trên màn làm việc

Payload TypeScript khớp spec:

```typescript
type WatchLine = {
  index: number;
  start: number;
  end: number;
  source: string;
  translation: string;
  wav: string;
};

type WatchUpdate = {
  video: string;
  readyUntil: number;
  running: boolean;
  published: boolean;
  detail: string;
  lines: WatchLine[];
};
```

`WatchLive` nhận `update`, `onStop`, `onExport`. Bên trong:

- `<video controls>` nguồn `convertFileSrc(update.video)`, `ref`.
- Một `<audio ref>` không có `controls`, `volume = 1`.
- Lớp `<p className="live-caption">` chỉ hiện khi `frame.line !== null`, chữ là `lines[line].translation`.
- `onTimeUpdate` và một `requestAnimationFrame` khi đang phát: đọc `currentTime`, gọi `frame`. Nếu `frame.time` lệch `currentTime` hơn 0,05 giây thì gán `currentTime = frame.time`. Gán `video.volume = frame.volume`.
- Hold: `video.pause()`. Khi `wantsPlay` chuyển từ false sang true vì hết hold, `video.play()`.
- `userPaused`: lắng `pause` và `play` từ thanh điều khiển. Bỏ qua sự kiện `pause` do chính mình gọi khi hold (dùng một ref `holding`).
- Đổi câu, hoặc `|currentTime - (audioStart)| > 0.3` so với chỗ wav đang phát: đặt `audio.src` nếu khác, `audio.currentTime = max(0, time - line.start)`, `audio.play()`. Hết câu thì `audio.pause()`.
- Chưa có câu: không phát, hiện **Đang chép đoạn đầu** thay cho `detail` nếu `readyUntil === 0`.
- `published`: nút **Ghép video** gọi `onExport`.

CSS thêm vào `src/App.css`:

```css
.live-stage { position: relative; }
.live-caption {
  position: absolute;
  left: 8%;
  right: 8%;
  bottom: 12%;
  margin: 0;
  text-align: center;
  color: #fff;
  font-size: 1.15rem;
  line-height: 1.35;
  text-shadow: 0 1px 2px #000;
  pointer-events: none;
}
.live-status { margin: 8px 0 0; }
```

`Work.tsx`:

- Thêm `"live"` vào union `View`.
- State `watchUpdate: WatchUpdate | null`.
- `useEffect` lắng `watch-update`, `setWatchUpdate`, `setView("live")`.
- Hàm `runLive` giống `run`: nếu `mode === "link"` thì `download_video` trước, rồi `invoke("start_watch", { video: source, outputDir: folder, target })`. Trước invoke, `setView("live")`, `setWatchUpdate(null)`. Lỗi thì `setError` và ở lại `live` nếu đã có update, không thì `progress`.
- Nút **Chạy** giữ nguyên. Thêm nút **Xem dần** `className="check"` cạnh nó, `disabled={!canRun}`, `onClick` gọi `runLive`.
- Nhánh `{view === "live" && watchUpdate ? <WatchLive .../> : null}`. `onStop` gọi `cancel_dub`. `onExport` gọi `runFrom(watchUpdate.video, false)` để vào đường `start_dub`.
- Dòng trạng thái dưới khung: `watchUpdate.detail`.

Không đụng view `watch` của video đã ghép.

- [ ] **Step 1: Không có test mới ở UI**

Hành vi hold, volume, tua đã khóa ở task 3. Task này chỉ nối các hàm đó vào thẻ video. Không viết test component.

- [ ] **Step 2: Viết `WatchLive.tsx` và sửa `Work.tsx`, `App.css` đúng như trên**

- [ ] **Step 3: Typecheck**

Run: `npx tsc --noEmit`

Expected: exit 0.

- [ ] **Step 4: Chạy lại test đã có**

Run: `node --experimental-strip-types --test src/watchPlay.test.ts`

Expected: PASS.

Run: `cd D:\Project\translate-video\src-tauri; cargo test --offline --lib -- --test-threads=8`

Expected: PASS.

- [ ] **Step 5: Commit**

Chỉ khi người dùng yêu cầu.

```bash
git add src/WatchLive.tsx src/watchPlay.ts src/watchPlay.test.ts src/Work.tsx src/App.css
git commit -m "feat: xem dần trong lúc video còn đang được dịch"
```

### Task 6: Ghi vào AGENTS.md để lần sau khỏi ghi file sớm

**Files:**
- Modify: `AGENTS.md`
- Modify: `CLAUDE.md`

**Interfaces:**
- Consumes: hành vi task 4 và 5
- Produces: một đoạn trong mỗi file, không đổi các luật khác

Chèn vào `AGENTS.md` sau đoạn pipeline 8 bước:

```markdown
Chế độ Xem dần (lệnh `start_watch`, sự kiện `watch-update`) phát video gốc trong cửa sổ trong lúc Whisper vẫn chạy một lần trên `audio.wav`. Rust đọc dòng in ra của whisper-cli, dịch và đọc từng cụm bằng đúng đường dịch và Piper hiện có. Tiếng gốc hạ còn 0,22 trong khoảng câu đã có wav; phụ đề và giọng đọc đè lên. Còn đang chạy mà sắp hết phần đã dịch thì video dừng, trạng thái "Đang dịch tiếp". Không thuật ngữ, không co độ dài, không ghép. `lines.json`, `translation.json` và `lines/NNN.wav` chỉ ghi khi mọi câu đã xong. Dừng giữa chừng xóa `watch/` và chỉ xóa `transcript.json` nếu file đó chưa có lúc bắt đầu và Whisper chưa thoát thành công. Nút Ghép video gọi `start_dub`; bước đã có file thì bỏ qua.
```

Trong `CLAUDE.md`, cuối mục "Việc đang làm", thêm một câu: `Đã có plan chế độ Xem dần ở docs/superpowers/plans/2026-10-12-watch-mode.md. Chưa làm thì đừng ghi translation.json trước khi cả video dịch xong.`

- [ ] **Step 1: Sửa hai file đúng đoạn trên**

- [ ] **Step 2: Đọc lại hai đoạn vừa chèn, đối chiếu spec, sửa chữ nếu lệch ngưỡng 0,22 hoặc luật xóa transcript**

- [ ] **Step 3: Commit**

Chỉ khi người dùng yêu cầu.

```bash
git add AGENTS.md CLAUDE.md
git commit -m "docs: ghi luật file của chế độ xem dần"
```

---

## Self-review

Spec coverage:

- Một lần Whisper, tách stdout/stderr, cụm 4 câu / 12 giây / sớm 8 giây: task 1.
- Không ghi canonical sớm, luật xóa transcript: task 2, thực thi ở task 4.
- Hold 1 giây, volume 0,22, tua, người dùng tự dừng: task 3, nối UI ở task 5.
- Link qua `download_video`, nút Ghép video gọi `start_dub`: task 5.
- Bỏ qua Whisper khi đã có transcript, phát ngay khi đã đủ wav: task 4 bước 4 và 5.
- Câu dịch hỏng để trống và wav im: task 4 bước 7, dùng `translate_work` sẵn có.
- AGENTS.md: task 6.

Placeholder scan: không có TBD. Task 4 mô tả đủ 11 bước và chỉ rõ khối arg Whisper phải copy từ `transcribe`; người thực thi mở `pipeline.rs` khoảng dòng 688–704 lúc làm, không bịa bộ cờ khác.

Type consistency: `PrintedCue`, `Driver::push_stdout`, `push_stderr`, `finish`, `push_cue`, `cancel_action`, `publish_allowed`, `frame`, `wantsPlay` dùng cùng tên từ task khai báo đến task sau. Payload `watch-update` trong task 4 và type `WatchUpdate` trong task 5 cùng field.

Review focus: mục 1 và 2 có test ở task 2 (`cancel_removes_a_transcript_this_run_did_not_finish`, `a_partial_batch_is_not_publishable`). Mục 3 có test `stderr_progress_between_stdout_halves_keeps_the_cue`. Mục 4 có test `a user pause is not cleared when the hold ends`. Mục 5 có test `seek past the ready point snaps back while the run is going`.
