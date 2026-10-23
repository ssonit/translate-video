# Nghe liền mạch theo đoạn Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Nút **Nghe liền mạch** đọc mỗi đoạn hội thoại bằng một lần chạy Piper, để dấu câu tự thành nhịp nghỉ, thay vì ghép `lines/NNN.wav`.

**Architecture:** Gom các câu trong `translation.json` thành đoạn. Câu cách nhau dưới 0,65 giây nằm cùng đoạn, tối đa 4 câu hoặc 180 ký tự. Mỗi đoạn là một lần `session.run` trong cửa sổ app, kể cả khi máy có `piper.exe`, vì `piper.exe` vẫn tách tại `.!?` rồi chèn im. Đoạn chỉ tách vì trần độ dài thì trộn mép 20 ms, không chèn số 0. Đoạn bắt đầu sau một khoảng ngắt của video thì chèn nghỉ 0,45 giây. Wav từng câu dùng để ghép video giữ nguyên.

**Tech Stack:** Tauri 2, Rust, React, Piper ONNX trong Web Worker (`src/tts/piper.ts`). Không thêm crate, không tải model.

**Spec:** Hội thoại 2026-10-18. Ghép wav từng câu luôn để khoảng ngưng: 120 ms im cố ý, mép mỗi file bị kéo về 0, và mỗi câu là một lần chạy model nên mất nối âm. Đọc cả video một lần thì model hỏng giọng và mất file từng câu. Cách làm: đọc theo đoạn, một lần chạy cho mỗi đoạn.

## Global Constraints

- Giữ Tauri. Video và audio không rời máy. Giọng local. Không Edge-TTS.
- Không đổi `lines/NNN.wav`, bước `fit` (xếp theo hình), `fit_story` (Chỉ lấy audio), hay `join_story`. Những đường đó vẫn đọc và ghép từng câu.
- Không dùng `piper.exe` / `speak_native` / `tts::prepare` cho nút này. `piper.exe` tách câu tại `.!?` và chèn im giữa các lần chạy; một dòng JSON không phải một lần chạy model.
- Trần đoạn: 4 câu, 180 ký tự Unicode (`chars().count()`), kể cả dấu cách nối. Một câu dài hơn 180 ký tự vẫn là một đoạn, không cắt giữa câu.
- Ngắt đoạn khi `start` của câu này trừ `end` của câu chữ trước đó ≥ `STORY_PARAGRAPH_GAP` (0,65). Nghỉ lúc ghép là `STORY_REST` (0,45 giây). Trộn mép khi tách vì trần là `STORY_FADE` (0,02 giây).
- Không bịa dấu câu. Nối câu bằng một dấu cách. Câu dịch rỗng hoặc chỉ có trắng thì bỏ qua.
- File video, phụ đề theo hình, cờ `fit-mode.txt`, và `lines/NNN.wav` không đổi. Vẫn xóa `dub.wav` sau khi ghép được mạch nghe, như `assemble_listen` hiện tại.
- Chữ giao diện tiếng Việt. Không đổi `productName`, identifier, tiêu đề cửa sổ.
- Không commit trừ khi người dùng yêu cầu lúc thực thi. Bước commit của mỗi task chỉ chạy khi đã được yêu cầu; nếu chưa, bỏ qua.
- Test Rust: `cd D:\Project\translate-video\src-tauri; cargo test --offline --lib -- --test-threads=8`
- Test cửa sổ: `node --experimental-strip-types --test src/tts/piper.test.ts` tại thư mục project.

## Review Focus

- Hai câu cách nhau 0,4 giây, mỗi câu đã có dấu chấm, vẫn một đoạn. Đây là ca đang nghe thấy nhịp thở 120 ms.
- Khoảng ngắt video ≥ 0,65 giây vẫn nghỉ 0,45 giây giữa hai đoạn, dù hai đoạn đều ngắn.
- Câu thứ năm, hoặc câu làm vượt 180 ký tự, sang đoạn mới với `rest_before = false`, và chỗ nối không có một dải mẫu 0.
- Một câu dài hơn 180 ký tự không bị cắt, không bị bỏ.
- Bấm lại cùng giọng, cùng chữ, wav đoạn còn nguyên: không đọc lại. Đổi giọng hoặc đổi một câu: đọc lại từ đầu. Dừng giữa chừng giữ wav đoạn đã xong và không xóa video hay `lines/NNN.wav`.

---

### Task 1: Gom câu thành đoạn

**Files:**
- Create: `src-tauri/src/listen.rs`
- Modify: `src-tauri/src/lib.rs` (thêm `mod listen;` cạnh `mod job;`)
- Modify: `src-tauri/src/wav.rs` (`STORY_PARAGRAPH_GAP` thành `pub(crate)`)
- Test: `src-tauri/src/listen.rs` module `tests`

**Interfaces:**
- Consumes: `crate::job::TranslatedLine`, `crate::wav::STORY_PARAGRAPH_GAP`
- Produces:
  - `pub(crate) const PASSAGE_MAX_LINES: usize = 4`
  - `pub(crate) const PASSAGE_MAX_CHARS: usize = 180`
  - `pub(crate) struct Passage { pub text: String, pub rest_before: bool }` với `Clone, Debug, PartialEq, Serialize, Deserialize`
  - `pub(crate) fn group_passages(lines: &[TranslatedLine]) -> Vec<Passage>`

Quy tắc trong `group_passages`, theo thứ tự với mỗi câu còn chữ sau `trim`:

1. Đo `gap = start - end` của câu chữ liền trước. Âm thì coi là 0. Câu chữ đầu tiên không có gap.
2. `gap >= STORY_PARAGRAPH_GAP` thì mở đoạn mới, `rest_before = true`.
3. Không thì nếu đoạn hiện đã có 4 câu, hoặc `chữ hiện tại + ' ' + câu mới` vượt 180 ký tự, mở đoạn mới, `rest_before = false`.
4. Không thì nối vào đoạn hiện tại bằng một dấu cách.
5. Câu đầu của cả danh sách: `rest_before = false`. Không thêm dấu chấm.

- [ ] **Step 1: Viết test fail**

```rust
#[test]
fn close_sentences_with_periods_stay_one_passage() {
    let lines = [
        line(0.0, 1.0, "Một."),
        line(1.4, 2.4, "Hai."),
    ];
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

fn line(start: f64, end: f64, translation: &str) -> TranslatedLine {
    TranslatedLine {
        start,
        end,
        source: String::new(),
        translation: translation.to_string(),
    }
}
```

- [ ] **Step 2: Chạy test, xác nhận fail**

Run: `cd D:\Project\translate-video\src-tauri; cargo test --offline --lib listen:: -- --test-threads=8`

Expected: FAIL vì thiếu module hoặc thiếu `group_passages`.

- [ ] **Step 3: Hiện thực `group_passages` trong `src-tauri/src/listen.rs`**

Đúng năm quy tắc ở Interfaces. `STORY_PARAGRAPH_GAP` lấy từ `wav`, không chép số 0,65 lần nữa.

- [ ] **Step 4: Chạy test, xác nhận pass**

Run: `cd D:\Project\translate-video\src-tauri; cargo test --offline --lib listen:: -- --test-threads=8`

Expected: PASS

- [ ] **Step 5: Commit** khi người dùng đã yêu cầu commit

```bash
git add src-tauri/src/listen.rs src-tauri/src/lib.rs src-tauri/src/wav.rs
git commit -m "feat: gom câu nghe liền mạch thành đoạn"
```

---

### Task 2: Ghép wav đoạn

**Files:**
- Modify: `src-tauri/src/wav.rs` (`STORY_FADE` thành `pub(crate)`; thêm hàm ghép đoạn)
- Test: `src-tauri/src/wav.rs` module `tests`

**Interfaces:**
- Consumes: `STORY_REST`, `STORY_FADE`, `trim_silence`, `fade_edges`
- Produces:
  - `pub(crate) struct PassageClip { pub rest_before: bool, pub samples: Vec<i16> }`
  - `pub(crate) fn join_passages(rate: u32, clips: &[PassageClip]) -> Vec<i16>`
  - `pub(crate) fn passage_spans(rate: u32, clips: &[PassageClip]) -> Vec<StorySpan>`

`rest_before == false` (và không phải clip có tiếng đầu tiên): trộn chồng `STORY_FADE` giây, không chèn mẫu 0. Mẫu trong vùng trộn là trung bình tuyến tính của đuôi clip trước và đầu clip sau. `rest_before == true`: chèn `STORY_REST` giây mẫu 0, rồi `fade_edges` trên clip mới, không trộn chồng. Clip rỗng sau `trim_silence` thì bỏ. Nếu clip bỏ đó có `rest_before` và phía trước đã có tiếng, clip có tiếng kế tiếp nghỉ 0,45 giây. Clip im ở đầu file thì bỏ, không chèn im đầu. `passage_spans` bám đúng vị trí mẫu đã ghép, kể cả chỗ chồng.

- [ ] **Step 1: Viết test fail**

```rust
#[test]
fn a_cap_split_joins_without_a_hole() {
    let rate = 1000;
    let clips = [
        PassageClip { rest_before: false, samples: vec![1000; 1000] },
        PassageClip { rest_before: false, samples: vec![3000; 1000] },
    ];
    let audio = join_passages(rate, &clips);
    assert_eq!(audio.len(), 1980);
    assert!(audio.iter().all(|sample| *sample > 0));
    let spans = passage_spans(rate, &clips);
    let overlap = spans[0].end - spans[1].start;
    assert!((overlap - 0.02).abs() < 0.001, "overlap {overlap}");
}

#[test]
fn a_source_pause_inserts_the_paragraph_rest() {
    let rate = 1000;
    let clips = [
        PassageClip { rest_before: false, samples: vec![8000; 1000] },
        PassageClip { rest_before: true, samples: vec![8000; 1000] },
    ];
    let spans = passage_spans(rate, &clips);
    let rest = spans[1].start - spans[0].end;
    assert!((rest - STORY_REST).abs() < 0.001, "rest {rest}");
    assert_eq!(join_passages(rate, &clips).len(), 2450);
}

#[test]
fn a_silent_rest_is_kept_on_the_next_audible_clip() {
    let rate = 1000;
    let clips = [
        PassageClip { rest_before: false, samples: vec![8000; 1000] },
        PassageClip { rest_before: true, samples: vec![0; 400] },
        PassageClip { rest_before: false, samples: vec![8000; 1000] },
    ];
    let spans = passage_spans(rate, &clips);
    assert_eq!(spans.len(), 2);
    let rest = spans[1].start - spans[0].end;
    assert!((rest - STORY_REST).abs() < 0.001, "rest {rest}");
}

#[test]
fn a_silent_cap_split_does_not_open_a_hole() {
    let rate = 1000;
    let clips = [
        PassageClip { rest_before: false, samples: vec![1000; 1000] },
        PassageClip { rest_before: false, samples: vec![0; 400] },
        PassageClip { rest_before: false, samples: vec![3000; 1000] },
    ];
    let audio = join_passages(rate, &clips);
    assert_eq!(audio.len(), 1980);
    assert!(audio.iter().all(|sample| *sample > 0));
}
```

- [ ] **Step 2: Chạy test, xác nhận fail**

Run: `cd D:\Project\translate-video\src-tauri; cargo test --offline --lib wav::tests::a_cap_split -- --test-threads=8`

Expected: FAIL vì thiếu `join_passages`.

- [ ] **Step 3: Hiện thực `join_passages` và `passage_spans` trong `src-tauri/src/wav.rs`**

Không sửa `join_story`. Áp dụng quy tắc clip im ở Interfaces: nghỉ của clip im được chuyển sang clip có tiếng phía sau, chỉ khi phía trước đã có tiếng.

- [ ] **Step 4: Chạy test, xác nhận pass**

Run: `cd D:\Project\translate-video\src-tauri; cargo test --offline --lib wav:: -- --test-threads=8`

Expected: PASS, gồm các test `story_join_*` cũ.

- [ ] **Step 5: Commit** khi người dùng đã yêu cầu commit

```bash
git add src-tauri/src/wav.rs
git commit -m "feat: ghép đoạn nghe liền mạch không chèn im giữa câu"
```

---

### Task 3: Một lần chạy model cho cả đoạn

**Files:**
- Modify: `src/tts/piper.ts` (`mergePhonemes`, thêm `speakPassage`)
- Modify: `src/tts/prepare.ts` (thêm `plainPassage`)
- Modify: `src/tts/tts.worker.ts`, `src/tts/engine.ts`, `src/tts/bridge.ts`
- Modify: `src-tauri/src/tts.rs` (`Ask.whole`, tham số `whole` của `run`)
- Modify: `src-tauri/src/pipeline.rs` (lời gọi `tts::run` hiện tại truyền `false`)
- Modify: `tsconfig.json` (exclude file test, cùng chỗ với `src/watchPlay.test.ts`)
- Create: `src/tts/piper.test.ts`
- Test: `src/tts/piper.test.ts`

**Interfaces:**
- Consumes: `speakChunk`, `processTextForTTS`, `toPcm16`
- Produces:
  - `mergePhonemes` nhận thêm `.!?` làm dấu ngăn giữa các vế, cùng thứ tự xuất hiện với `,;:`
  - `export function plainPassage(raw: string): string` — cùng bước dọn như `plainChunks`, trả một chuỗi, không tách câu
  - `Piper.speakPassage(raw: string, lang: string): Promise<Pcm16>` — `vi` thì `processTextForTTS` một lần; ngôn ngữ khác thì `plainPassage`. Rồi đúng một `speakChunk`. Không gọi `chunkText`, không gọi `plainChunks`
  - `TtsEngine.speakPassage(text: string, lang: string): Promise<Pcm16>` gửi `{ type: "speak", whole: true, ... }`
  - Worker: `message.whole` thì `piper.speakPassage`, không thì `piper.speak` như cũ
  - `tts::run(..., whole: bool, ...)` ghi `whole` vào `Ask`. `speak_batch` truyền `false`
  - Bridge: `request.whole` thì `engine.speakPassage`, không thì `engine.speak`

- [ ] **Step 1: Viết test fail**

`src/tts/piper.test.ts`:

```ts
import test from "node:test";
import assert from "node:assert/strict";
import { mergePhonemes } from "./piper.ts";

test("a passage keeps the period inside one phoneme string", () => {
  assert.equal(mergePhonemes("Một. Hai.", ["mot", "hai"]), "mot. hai");
});

test("a comma between clauses stays a comma", () => {
  assert.equal(mergePhonemes("Một, hai", ["mot", "hai"]), "mot, hai");
});

test("a single clause gains no extra mark", () => {
  assert.equal(mergePhonemes("Một.", ["mot"]), "mot");
});
```

Thêm `src/tts/piper.test.ts` vào mảng `exclude` của `tsconfig.json`.

- [ ] **Step 2: Chạy test, xác nhận fail**

Run: `node --experimental-strip-types --test src/tts/piper.test.ts`

Expected: FAIL ở test dấu chấm (`mot, hai` thay vì `mot. hai`, vì regex hiện chỉ có `,;:`).

- [ ] **Step 3: Sửa `mergePhonemes` và thêm đường `speakPassage`**

Regex dấu ngăn đổi thành `/[,;:.!?]/g`. `speak()` giữ nguyên: vẫn tách câu rồi nhiều `speakChunk`. Chỉ `speakPassage` chạy một lần. `Speak` message trong worker thêm `whole?: boolean`.

- [ ] **Step 4: Chạy test và kiểu**

Run: `node --experimental-strip-types --test src/tts/piper.test.ts`

Expected: PASS, 3 test.

Run: `npx tsc --noEmit`

Expected: exit 0.

- [ ] **Step 5: Commit** khi người dùng đã yêu cầu commit

```bash
git add src/tts/piper.ts src/tts/piper.test.ts src/tts/prepare.ts src/tts/tts.worker.ts src/tts/engine.ts src/tts/bridge.ts src-tauri/src/tts.rs src-tauri/src/pipeline.rs tsconfig.json
git commit -m "feat: đọc một đoạn nghe liền mạch bằng một lần chạy giọng"
```

---

### Task 4: Nhớ đoạn đã đọc

**Files:**
- Modify: `src-tauri/src/listen.rs`
- Test: `src-tauri/src/listen.rs` module `tests`

**Interfaces:**
- Consumes: `Passage` từ Task 1
- Produces:
  - `#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)] pub(crate) struct ListenPlan { pub voice: String, pub passages: Vec<Passage> }`
  - `pub(crate) enum SpeakSet { All, Missing(Vec<usize>) }` — chỉ số 0-based trong `passages`
  - `pub(crate) fn speak_indexes(saved: &ListenPlan, voice: &str, passages: &[Passage], wav_ready: &[bool]) -> SpeakSet`

`voice` khác hoặc `passages` khác (`PartialEq`) thì `All`. Trùng thì `Missing` gồm những chỉ số `wav_ready` không phải `true`, kể cả chỉ số nằm ngoài độ dài `wav_ready`.

- [ ] **Step 1: Viết test fail**

```rust
#[test]
fn the_same_voice_and_text_speaks_only_missing_wavs() {
    let passages = vec![Passage { text: "Một.".into(), rest_before: false }];
    let saved = ListenPlan { voice: "banmai".into(), passages: passages.clone() };
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
    let passages = vec![Passage { text: "Một.".into(), rest_before: false }];
    let saved = ListenPlan { voice: "banmai".into(), passages: passages.clone() };
    assert_eq!(speak_indexes(&saved, "thientam", &passages, &[true]), SpeakSet::All);
    let changed = vec![Passage { text: "Hai.".into(), rest_before: false }];
    assert_eq!(speak_indexes(&saved, "banmai", &changed, &[true]), SpeakSet::All);
}
```

- [ ] **Step 2: Chạy test, xác nhận fail**

Run: `cd D:\Project\translate-video\src-tauri; cargo test --offline --lib listen::tests::the_same_voice -- --test-threads=8`

Expected: FAIL vì thiếu `speak_indexes`.

- [ ] **Step 3: Hiện thực `ListenPlan` và `speak_indexes`**

Không đọc file trong hàm này. Người gọi lo wav và manifest.

- [ ] **Step 4: Chạy test, xác nhận pass**

Run: `cd D:\Project\translate-video\src-tauri; cargo test --offline --lib listen:: -- --test-threads=8`

Expected: PASS

- [ ] **Step 5: Commit** khi người dùng đã yêu cầu commit

```bash
git add src-tauri/src/listen.rs
git commit -m "feat: bỏ qua đoạn nghe liền mạch đã đọc"
```

---

### Task 5: Nối vào nút Nghe liền mạch

**Files:**
- Modify: `src-tauri/src/listen.rs` (thêm `listen_engine`)
- Modify: `src-tauri/src/pipeline.rs` (`join_listen`, `assemble_listen`, `script_for`, test `join_listen_keeps_the_video_and_drops_the_picture_gaps`)
- Modify: `src/Work.tsx` (chữ trạng thái nút, khoảng dòng 2683–2686)
- Test: test hiện có trong `src-tauri/src/pipeline.rs`

**Interfaces:**
- Consumes: `group_passages`, `ListenPlan`, `speak_indexes`, `SpeakSet`, `join_passages`, `passage_spans`, `tts::run(..., whole: true, ...)`, `listen_engine`
- Produces:
  - `pub(crate) fn listen_engine() -> crate::piper::SpeakEngine` luôn trả `SpeakEngine::Window`
  - Manifest `listen/manifest.json` trong folder việc, wav `listen/001.wav` (chỉ số đoạn + 1, ba chữ số)
  - `script_for` với file `*.nghe.m4a`: mốc lấy từ wav đoạn + `passage_spans`. Không có manifest hoặc thiếu wav thì trả lời dịch trong `translation.json`, không gọi `story_captions`

`join_listen` làm đúng thứ tự:

1. `group_passages`. Rỗng thì lỗi `"Chưa có lời đã dịch."`.
2. Đọc manifest nếu có. `speak_indexes`. `All` thì xóa mọi wav trong `listen/`, không xóa `lines/`.
3. Ghi manifest của giọng hiện tại và các đoạn mới, trước khi đọc, để lần bấm sau đọc tiếp chỗ thiếu.
4. `match listen_engine()`: nhánh `Window` gọi `tts::run` với `whole: true`, text là `passage.text`, file là `listen/NNN.wav`, chỉ với các chỉ số cần đọc. Nhánh `Native` trả lỗi `"Nghe liền mạch đọc cả đoạn trong cửa sổ app."` và không gọi `speak_native`.
5. Ghép bằng `join_passages`, ghi `_listen.wav`, ghi SRT cạnh `*.nghe.m4a` (một cue một đoạn, chữ là `passage.text`), xóa `dub.wav`. Không đụng video, phụ đề hình, `fit-mode.txt`, `lines/NNN.wav`.
6. `encode_spoken` như hiện tại, phát `listen-progress`. Xong thì xóa `_listen.wav`.
7. Lỗi hoặc dừng: không ghi đè `*.nghe.m4a` nếu encode chưa xong, không xóa video, không xóa `lines/`. Wav đoạn đã hợp lệ để lại.

Tiến độ: đang đọc thì `(done as f64) / (tổng đoạn as f64 + 1.0)`. Encode `fraction` thì `tổng / (tổng + 1) + fraction / (tổng + 1)`.

`Work.tsx`: khi `joining`, chữ là `Đang đọc liền mạch…`, có phần trăm thì `Đang đọc liền mạch… ${phần trăm}%`. Nút lúc rảnh vẫn là `Nghe liền mạch`.

- [ ] **Step 1: Sửa test cũ cho đúng mạch mới**

Trong `join_listen_keeps_the_video_and_drops_the_picture_gaps`: hai câu cách 0,4 giây không còn là hai file câu ghép bằng nhịp thở. Dựng hai wav đoạn trong `listen/` (mỗi cái 1000 mẫu, rate 1000, biên độ 8000) và manifest hai đoạn, đoạn hai `rest_before = true`. Gọi hàm ghép đoạn (hàm mà `join_listen` dùng sau khi đã có wav), không gọi TTS.

Giữ các assert: byte video, nội dung SRT hình, `fit-mode` vẫn `sync`, `lines/001.wav` còn, `dub.wav` hết. Đổi assert độ dài và mốc: SRT nghe chứa `00:00:01,450`, không chứa `00:00:01,120`.

Thêm test thuần:

```rust
#[test]
fn listen_speaks_in_the_window_even_when_piper_exe_exists() {
    assert_eq!(crate::listen::listen_engine(), crate::piper::SpeakEngine::Window);
}
```

- [ ] **Step 2: Chạy test, xác nhận fail**

Run: `cd D:\Project\translate-video\src-tauri; cargo test --offline --lib pipeline::tests::join_listen_keeps -- --test-threads=8`

Expected: FAIL vì còn ghép theo nhịp thở 120 ms, hoặc chưa có đường đi từ wav đoạn.

- [ ] **Step 3: Nối `join_listen` và `script_for`, sửa chữ nút**

Đúng thứ tự bảy bước ở trên. `assemble_listen` cũ, nếu không còn ai gọi, xóa. `story_captions` vẫn để cho `fit_story`.

- [ ] **Step 4: Chạy test**

Run: `cd D:\Project\translate-video\src-tauri; cargo test --offline --lib listen:: wav:: pipeline::tests::join_listen -- --test-threads=8`

Expected: PASS.

Run: `npx tsc --noEmit` tại thư mục project.

Expected: exit 0.

- [ ] **Step 5: Commit** khi người dùng đã yêu cầu commit

```bash
git add src-tauri/src/pipeline.rs src-tauri/src/listen.rs src/Work.tsx
git commit -m "feat: nghe liền mạch đọc theo đoạn thay vì ghép từng câu"
```
