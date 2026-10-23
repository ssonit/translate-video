# Pipeline Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Làm pipeline lồng tiếng chạy được với video dài và cho tiếng lồng không đè nhau, dựa trên những gì VideoLingo làm tốt.

**Architecture:** Thay bước `fit` (một lệnh FFmpeg khổng lồ) bằng: xếp lịch tuần tự trong Rust (`timeline.rs`) → FFmpeg xử lý từng câu → trộn PCM trong Rust (`wav.rs`). Piper đọc cả lô bằng một process. Câu đọc quá dài thì nhờ Ollama rút gọn rồi đọc lại. Dịch theo cụm 10 câu kèm ngữ cảnh trước/sau.

**Tech Stack:** Rust (Tauri 2), `serde_json`, `ureq`, FFmpeg, Piper, Ollama. Không thêm crate mới.

**Spec:** Không có spec riêng. Nguồn là phần "Điều học được từ VideoLingo" bên dưới và bản review trong hội thoại ngày 2026-10-06.

## Global Constraints

- Giữ Tauri 2. Không đề xuất Electron.
- Offline. Không gửi video/audio đi. Dịch bằng Ollama `qwen2.5:7b` tại `http://localhost:11434`; cloud chỉ khi có API key.
- Giọng đọc local (Piper). Không Edge-TTS mặc định.
- Màn chuẩn bị chỉ kiểm tra và hướng dẫn. Không thêm nút tự tải.
- Không đổi `productName`, identifier, tiêu đề cửa sổ, chữ giao diện. Không thêm id bước mới: `Work.tsx` có danh sách 8 bước cố định (`extract, transcribe, split, terms, translate, speak, fit, merge`).
- Giữ đọc được file trung gian cũ (`transcript.json`, `lines.json`, `terms.json`, `translation.json`, `lines/NNN.wav`). Chỉ được thêm file mới.
- Trả lời và chuỗi hiển thị bằng tiếng Việt.
- **Không commit** (CLAUDE.md: chỉ commit khi người dùng yêu cầu; thư mục này cũng chưa là git repo). Bỏ qua bước commit mà skill thường có.
- Chạy test: `cd D:\Project\translate-video\src-tauri; cargo test --offline`. Baseline hiện tại: 16 test pass.

## Điều học được từ VideoLingo (và áp dụng ở đâu)

| VideoLingo | Chỗ của ta hiện tại | Task |
|---|---|---|
| Dịch theo cụm 10 câu / 600 ký tự, JSON đánh số, kiểm tra đủ số dòng, thử lại 3 lần, kèm 3 câu trước + 2 câu sau (`_4_2_translate.py`, `translate_lines.py`) | Dịch từng câu, không ngữ cảnh | 6 |
| Câu quá dài so với khung giờ thì nhờ LLM rút gọn (`get_subtitle_trim_prompt`) | Không có; chỉ tăng tốc tối đa 1,4 lần rồi để đè | 5 |
| Cho phép mượn khoảng trống sau câu (`tolerance` 1,5 s), đặt câu nối tiếp nhau, cắt có fade-out và ghi log câu bị cắt (`_10_gen_audio.py`) | Mỗi câu tự co độc lập, chồng lên câu sau | 1, 3 |
| Ghép tiếng bằng cách nối đoạn, không dùng một lệnh filter dài (`_11_merge_audio.py`) | `-filter_complex` với N input, vượt giới hạn dòng lệnh Windows | 2, 3 |
| TTS: làm sạch ký tự, thử lại 3 lần, câu ≤1 ký tự thì thay bằng 0,1 s im lặng (`tts_main.py`) | Piper gọi 1 process/câu, không thử lại | 4 |

Không bê sang: lưu trung gian bằng Excel, Streamlit, phụ thuộc Python. Chưa làm (cần quyết định hoặc cần video thật): xem "Để sau" cuối file.

## File Structure

| File | Việc |
|---|---|
| `src-tauri/src/timeline.rs` (mới) | Xếp lịch câu: tốc độ, mượn khoảng trống, cắt. Hàm thuần, có test. |
| `src-tauri/src/wav.rs` (mới) | Đọc/ghi WAV PCM16 mono, trộn mẫu vào một dải. |
| `src-tauri/src/speech.rs` (mới) | Làm sạch chữ trước khi đọc, dựng input JSON cho Piper theo lô. |
| `src-tauri/src/job.rs` | Thêm `line_fit(index)`, `fit_report()`. |
| `src-tauri/src/lib.rs` | Khai báo module; tách `piper_setup()`. |
| `src-tauri/src/ollama.rs` | Thêm prompt rút gọn, dịch theo cụm. |
| `src-tauri/src/pipeline.rs` | Viết lại `fit`, `speak`, `translate`; thêm `run_tool_input`. |
| `src-tauri/Cargo.toml`, `ollama.rs` | Task 7: vệ sinh nhỏ. |

---

### Task 1: Xếp lịch tuần tự (`timeline.rs`)

**Files:**
- Create: `src-tauri/src/timeline.rs`
- Modify: `src-tauri/src/lib.rs` (thêm `mod timeline;`)

**Interfaces:**
- Produces:
  - `pub(crate) const MAX_SPEED: f64 = 1.4; pub(crate) const TOLERANCE: f64 = 1.5;`
  - `pub(crate) struct Slot { pub start: f64, pub end: f64, pub wav_secs: f64 }`
  - `pub(crate) struct Placement { pub start: f64, pub speed: f64, pub keep: Option<f64>, pub played: f64 }` (`keep = Some(giây)` nghĩa là phải cắt còn chừng đó sau khi tăng tốc)
  - `pub(crate) fn place_all(slots: &[Slot], total_secs: f64) -> Vec<Placement>`
  - `pub(crate) fn room(slots: &[Slot], index: usize, total_secs: f64) -> f64`
  - `pub(crate) fn too_long(wav_secs: f64, room_secs: f64) -> bool`

- [ ] **Step 1: Viết file với test trước (test sẽ fail vì chưa có thân hàm)**

Tạo `src-tauri/src/timeline.rs`:

```rust
pub(crate) const MAX_SPEED: f64 = 1.4;
pub(crate) const TOLERANCE: f64 = 1.5;
const MIN_ROOM: f64 = 0.05;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Slot {
    pub start: f64,
    pub end: f64,
    pub wav_secs: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Placement {
    pub start: f64,
    pub speed: f64,
    pub keep: Option<f64>,
    pub played: f64,
}

pub(crate) fn place_all(_slots: &[Slot], _total_secs: f64) -> Vec<Placement> {
    todo!()
}

pub(crate) fn room(_slots: &[Slot], _index: usize, _total_secs: f64) -> f64 {
    todo!()
}

pub(crate) fn too_long(_wav_secs: f64, _room_secs: f64) -> bool {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot(start: f64, end: f64, wav_secs: f64) -> Slot {
        Slot { start, end, wav_secs }
    }

    #[test]
    fn short_clip_keeps_speed_and_start() {
        let placed = place_all(&[slot(0.0, 2.0, 1.0)], 10.0);
        assert_eq!(placed[0].start, 0.0);
        assert_eq!(placed[0].speed, 1.0);
        assert_eq!(placed[0].keep, None);
    }

    #[test]
    fn clip_borrows_the_gap_before_speeding_up() {
        let placed = place_all(&[slot(0.0, 2.0, 2.6), slot(4.0, 6.0, 1.0)], 10.0);
        assert_eq!(placed[0].speed, 1.0);
        assert_eq!(placed[0].keep, None);
        assert_eq!(placed[1].start, 4.0);
    }

    #[test]
    fn clip_that_cannot_fit_is_sped_up_then_cut() {
        let placed = place_all(&[slot(0.0, 1.0, 3.0), slot(1.0, 5.0, 1.0)], 10.0);
        assert_eq!(placed[0].speed, MAX_SPEED);
        assert_eq!(placed[0].keep, Some(1.0));
        assert_eq!(placed[0].played, 1.0);
        assert_eq!(placed[1].start, 1.0);
    }

    #[test]
    fn clips_never_overlap() {
        let slots: Vec<Slot> = (0..20)
            .map(|i| slot(i as f64, i as f64 + 1.0, 1.0 + (i % 4) as f64))
            .collect();
        let placed = place_all(&slots, 30.0);
        for pair in placed.windows(2) {
            assert!(pair[1].start + 1e-9 >= pair[0].start + pair[0].played);
        }
    }

    #[test]
    fn room_counts_gap_up_to_tolerance() {
        let slots = [slot(0.0, 2.0, 1.0), slot(4.0, 6.0, 1.0)];
        assert!((room(&slots, 0, 10.0) - 3.5).abs() < 1e-9);
        assert!((room(&slots, 1, 10.0) - 6.0).abs() < 1e-9);
    }

    #[test]
    fn too_long_means_even_max_speed_does_not_fit() {
        assert!(too_long(2.0, 1.0));
        assert!(!too_long(1.3, 1.0));
        assert!(!too_long(f64::NAN, 1.0));
    }
}
```

Thêm `mod timeline;` vào đầu `src-tauri/src/lib.rs` (cạnh `mod job;`).

- [ ] **Step 2: Chạy, xác nhận fail**

Run: `cd D:\Project\translate-video\src-tauri; cargo test --offline timeline`
Expected: các test panic `not yet implemented`.

- [ ] **Step 3: Viết thân hàm**

Thay ba hàm `todo!()` bằng:

```rust
fn limit_end(slots: &[Slot], index: usize, total_secs: f64) -> f64 {
    let slot = slots[index];
    match slots.get(index + 1) {
        Some(next) => (slot.end + TOLERANCE).min(next.start.max(slot.end)),
        None => total_secs.max(slot.end),
    }
}

pub(crate) fn room(slots: &[Slot], index: usize, total_secs: f64) -> f64 {
    (limit_end(slots, index, total_secs) - slots[index].start.max(0.0)).max(MIN_ROOM)
}

pub(crate) fn too_long(wav_secs: f64, room_secs: f64) -> bool {
    wav_secs.is_finite() && wav_secs / MAX_SPEED > room_secs
}

pub(crate) fn place_all(slots: &[Slot], total_secs: f64) -> Vec<Placement> {
    let mut cursor = 0.0_f64;
    let mut placed = Vec::with_capacity(slots.len());
    for (index, slot) in slots.iter().enumerate() {
        let start = slot.start.max(cursor).max(0.0);
        let wav = if slot.wav_secs.is_finite() { slot.wav_secs.max(0.0) } else { 0.0 };
        let available = (limit_end(slots, index, total_secs) - start).max(MIN_ROOM);
        let speed = if wav <= available { 1.0 } else { (wav / available).min(MAX_SPEED) };
        let mut played = wav / speed;
        let mut keep = None;
        if played > available + 0.02 {
            keep = Some(available);
            played = available;
        }
        cursor = start + played;
        placed.push(Placement { start, speed, keep, played });
    }
    placed
}
```

- [ ] **Step 4: Chạy, xác nhận pass**

Run: `cd D:\Project\translate-video\src-tauri; cargo test --offline timeline`
Expected: 6 test pass.

---

### Task 2: Đọc/ghi WAV và trộn mẫu (`wav.rs`)

**Files:**
- Create: `src-tauri/src/wav.rs`
- Modify: `src-tauri/src/lib.rs` (thêm `mod wav;`)

**Interfaces:**
- Produces:
  - `pub(crate) struct Pcm { pub rate: u32, pub samples: Vec<i16> }`
  - `pub(crate) fn parse_wav(bytes: &[u8]) -> Result<Pcm, String>`
  - `pub(crate) fn encode_wav(pcm: &Pcm) -> Vec<u8>`
  - `pub(crate) fn read_wav(path: &Path) -> Result<Pcm, String>`
  - `pub(crate) fn write_wav(path: &Path, pcm: &Pcm) -> Result<(), String>` (ghi `.tmp` rồi đổi tên, để file dở không bị coi là xong)
  - `pub(crate) fn mix_at(canvas: &mut Vec<i16>, rate: u32, start_secs: f64, clip: &[i16])` (cộng bão hòa, tự nới dải)

- [ ] **Step 1: Viết test trước**

Tạo `src-tauri/src/wav.rs` chỉ với stub và test:

```rust
use std::path::Path;

pub(crate) struct Pcm {
    pub rate: u32,
    pub samples: Vec<i16>,
}

pub(crate) fn parse_wav(_bytes: &[u8]) -> Result<Pcm, String> {
    todo!()
}

pub(crate) fn encode_wav(_pcm: &Pcm) -> Vec<u8> {
    todo!()
}

pub(crate) fn read_wav(_path: &Path) -> Result<Pcm, String> {
    todo!()
}

pub(crate) fn write_wav(_path: &Path, _pcm: &Pcm) -> Result<(), String> {
    todo!()
}

pub(crate) fn mix_at(_canvas: &mut Vec<i16>, _rate: u32, _start_secs: f64, _clip: &[i16]) {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_then_parse_round_trips() {
        let pcm = Pcm { rate: 22050, samples: vec![0, 1, -1, i16::MAX, i16::MIN] };
        let back = parse_wav(&encode_wav(&pcm)).unwrap();
        assert_eq!(back.rate, 22050);
        assert_eq!(back.samples, pcm.samples);
    }

    #[test]
    fn parse_skips_extra_chunks_like_ffmpeg_list() {
        let pcm = Pcm { rate: 16000, samples: vec![5, 6, 7] };
        let mut bytes = encode_wav(&pcm);
        let tail = bytes.split_off(36);
        bytes.extend_from_slice(b"LIST");
        bytes.extend_from_slice(&4u32.to_le_bytes());
        bytes.extend_from_slice(b"abcd");
        bytes.extend_from_slice(&tail);
        assert_eq!(parse_wav(&bytes).unwrap().samples, vec![5, 6, 7]);
    }

    #[test]
    fn parse_rejects_non_wav_and_stereo() {
        assert!(parse_wav(b"not a wav file at all").is_err());
        let mut bytes = encode_wav(&Pcm { rate: 22050, samples: vec![1, 2] });
        bytes[22] = 2; // channels = 2
        assert!(parse_wav(&bytes).is_err());
    }

    #[test]
    fn mix_extends_canvas_and_saturates() {
        let mut canvas = vec![0i16; 2];
        mix_at(&mut canvas, 10, 0.1, &[i16::MAX, 5]);
        assert_eq!(canvas, vec![0, i16::MAX, 5]);
        mix_at(&mut canvas, 10, 0.1, &[10, 10]);
        assert_eq!(canvas, vec![0, i16::MAX, 15]);
    }

    #[test]
    fn write_then_read_file() {
        let dir = std::env::temp_dir().join(format!("wav-io-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.wav");
        write_wav(&path, &Pcm { rate: 22050, samples: vec![1, 2, 3] }).unwrap();
        assert_eq!(read_wav(&path).unwrap().samples, vec![1, 2, 3]);
        assert!(!dir.join("a.tmp").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
```

Thêm `mod wav;` vào `lib.rs`.

- [ ] **Step 2: Chạy, xác nhận fail**

Run: `cd D:\Project\translate-video\src-tauri; cargo test --offline wav::`
Expected: panic `not yet implemented`.

- [ ] **Step 3: Viết thân hàm**

Thay các stub (giữ nguyên `use` và struct):

```rust
pub(crate) fn parse_wav(bytes: &[u8]) -> Result<Pcm, String> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("Không phải file WAV.".into());
    }
    let mut pos = 12;
    let mut rate = None;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = u32::from_le_bytes([bytes[pos + 4], bytes[pos + 5], bytes[pos + 6], bytes[pos + 7]]) as usize;
        let body = pos + 8;
        if id == b"fmt " {
            if body + 16 > bytes.len() {
                return Err("WAV bị cụt.".into());
            }
            let tag = u16::from_le_bytes([bytes[body], bytes[body + 1]]);
            let channels = u16::from_le_bytes([bytes[body + 2], bytes[body + 3]]);
            let sample_rate = u32::from_le_bytes([bytes[body + 4], bytes[body + 5], bytes[body + 6], bytes[body + 7]]);
            let bits = u16::from_le_bytes([bytes[body + 14], bytes[body + 15]]);
            if tag != 1 || channels != 1 || bits != 16 {
                return Err("WAV phải là PCM 16-bit mono.".into());
            }
            rate = Some(sample_rate);
        } else if id == b"data" {
            let rate = rate.ok_or("WAV thiếu khối fmt.")?;
            let end = body.saturating_add(size).min(bytes.len());
            let samples = bytes[body..end]
                .chunks_exact(2)
                .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
                .collect();
            return Ok(Pcm { rate, samples });
        }
        pos = body.saturating_add(size).saturating_add(size & 1);
    }
    Err("WAV không có khối data.".into())
}

pub(crate) fn encode_wav(pcm: &Pcm) -> Vec<u8> {
    let data_len = (pcm.samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&pcm.rate.to_le_bytes());
    out.extend_from_slice(&(pcm.rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for sample in &pcm.samples {
        out.extend_from_slice(&sample.to_le_bytes());
    }
    out
}

pub(crate) fn read_wav(path: &Path) -> Result<Pcm, String> {
    let bytes = std::fs::read(path).map_err(|_| format!("Không đọc được {}.", path.display()))?;
    parse_wav(&bytes).map_err(|error| format!("{} ({}).", error.trim_end_matches('.'), path.display()))
}

pub(crate) fn write_wav(path: &Path, pcm: &Pcm) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, encode_wav(pcm)).map_err(|error| error.to_string())?;
    std::fs::rename(&tmp, path).map_err(|error| error.to_string())
}

pub(crate) fn mix_at(canvas: &mut Vec<i16>, rate: u32, start_secs: f64, clip: &[i16]) {
    let offset = (start_secs.max(0.0) * f64::from(rate)).round() as usize;
    if canvas.len() < offset + clip.len() {
        canvas.resize(offset + clip.len(), 0);
    }
    for (index, sample) in clip.iter().enumerate() {
        canvas[offset + index] = canvas[offset + index].saturating_add(*sample);
    }
}
```

- [ ] **Step 4: Chạy, xác nhận pass**

Run: `cd D:\Project\translate-video\src-tauri; cargo test --offline wav::`
Expected: 5 test pass.

---

### Task 3: Viết lại bước `fit` (từng câu qua FFmpeg, trộn trong Rust)

**Files:**
- Modify: `src-tauri/src/job.rs` (thêm 2 đường dẫn + test)
- Modify: `src-tauri/src/pipeline.rs` (xóa `MIN_SPEED`, `MAX_SPEED`, `playback_speed`, `place_line`, `mix_filter`; viết lại `fit`; sửa test)

**Interfaces:**
- Consumes: `timeline::{Slot, Placement, place_all}`, `wav::{Pcm, read_wav, write_wav, mix_at}`.
- Produces:
  - `Job::line_fit(&self, index: usize) -> PathBuf` → `lines/001.fit.wav`
  - `Job::fit_report(&self) -> PathBuf` → `fit-report.json`
  - `fn fit_filter(speed: f64, keep: Option<f64>) -> String` (private trong `pipeline.rs`)
  - File `fit-report.json`: mảng `{"line": <số câu từ 1>, "text": "...", "spoken": <giây>, "kept": <giây>}` cho câu bị cắt.

- [ ] **Step 1: Test đường dẫn mới trong `job.rs`**

Trong `job_folder_uses_video_name_and_keeps_source`, thêm sau dòng `assert_eq!(job.line_wav(12), ...)`:

```rust
        assert_eq!(job.line_fit(1), job.root.join("lines").join("001.fit.wav"));
        assert_eq!(job.fit_report(), job.root.join("fit-report.json"));
```

Chạy `cargo test --offline job::` → Expected: lỗi biên dịch (chưa có hàm).

- [ ] **Step 2: Thêm hai hàm vào `impl Job`**

Sau `line_wav`:

```rust
    pub(crate) fn line_fit(&self, index: usize) -> PathBuf {
        self.root.join("lines").join(format!("{index:03}.fit.wav"))
    }

    pub(crate) fn fit_report(&self) -> PathBuf {
        self.root.join("fit-report.json")
    }
```

Chạy `cargo test --offline job::` → Expected: pass.

- [ ] **Step 3: Viết test `fit_filter` (có chạy FFmpeg thật, bỏ qua nếu máy không có)**

Trong `mod tests` của `pipeline.rs`, **xóa** hai test `speed_stays_between_one_and_one_point_four` và `mix_speeds_up_only_long_lines` (đã thay bằng `timeline::tests`), và thêm:

```rust
    #[test]
    fn fit_filter_chain_is_plain_when_nothing_to_do() {
        assert_eq!(fit_filter(1.0, None), "anull");
        assert_eq!(fit_filter(1.25, None), "atempo=1.250");
        assert_eq!(
            fit_filter(1.4, Some(1.0)),
            "atempo=1.400,atrim=end=1.000,afade=t=out:st=0.950:d=0.050"
        );
    }

    #[test]
    fn fit_filter_cuts_after_speeding_up() {
        if crate::program_path("ffmpeg").is_none() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("fit-filter-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let tone = dir.join("tone.wav");
        let out = dir.join("out.wav");
        let made = Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i", "sine=frequency=440:duration=3"])
            .args(["-ac", "1", "-ar", "22050", "-c:a", "pcm_s16le"])
            .arg(&tone)
            .status()
            .unwrap()
            .success();
        assert!(made);
        let fitted = Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
            .arg(&tone)
            .arg("-af")
            .arg(fit_filter(1.4, Some(1.0)))
            .args(["-ac", "1", "-ar", "22050", "-c:a", "pcm_s16le"])
            .arg(&out)
            .status()
            .unwrap()
            .success();
        assert!(fitted);
        let pcm = crate::wav::read_wav(&out).unwrap();
        let secs = pcm.samples.len() as f64 / f64::from(pcm.rate);
        assert!((secs - 1.0).abs() < 0.06, "got {secs}");
        let _ = std::fs::remove_dir_all(&dir);
    }
```

Chạy `cargo test --offline pipeline::` → Expected: lỗi biên dịch (chưa có `fit_filter`).

- [ ] **Step 4: Viết lại `fit`, thêm `fit_filter`, xóa mã cũ**

Trong `pipeline.rs`:

1. Xóa `const MIN_SPEED`, `const MAX_SPEED`, hàm `playback_speed`, `place_line`, `mix_filter`.
2. Thêm vào đầu file: `use crate::timeline::{place_all, Slot};` và `use crate::wav::{mix_at, read_wav, write_wav, Pcm};`.
3. Thay toàn bộ hàm `fit` bằng:

```rust
#[derive(Serialize)]
struct Cut {
    line: usize,
    text: String,
    spoken: f64,
    kept: f64,
}

fn fit_filter(speed: f64, keep: Option<f64>) -> String {
    let mut parts = Vec::new();
    if speed > 1.001 {
        parts.push(format!("atempo={speed:.3}"));
    }
    if let Some(secs) = keep {
        let fade = secs.min(0.05);
        parts.push(format!("atrim=end={secs:.3}"));
        parts.push(format!("afade=t=out:st={:.3}:d={fade:.3}", (secs - fade).max(0.0)));
    }
    if parts.is_empty() {
        "anull".into()
    } else {
        parts.join(",")
    }
}

fn fit(app: &AppHandle, job: &Job, lines: &[TranslatedLine], cancel: &AtomicBool) -> Result<(), String> {
    if file_ready(&job.dub()) {
        report(app, "fit", "skipped", "Đã có dub.wav, bỏ qua.");
        return Ok(());
    }
    stopped(cancel)?;
    report(app, "fit", "running", "Đang xếp tiếng vào đúng chỗ.");
    let video_secs = media_duration(&job.video, cancel)?;
    let mut slots = Vec::with_capacity(lines.len());
    for (index, line) in lines.iter().enumerate() {
        let wav = job.line_wav(index + 1);
        if !file_ready(&wav) {
            return Err(format!("Thiếu tiếng của câu {}.", index + 1));
        }
        let pcm = read_wav(&wav)?;
        slots.push(Slot {
            start: line.start,
            end: line.end,
            wav_secs: pcm.samples.len() as f64 / f64::from(pcm.rate),
        });
    }
    let placements = place_all(&slots, video_secs);
    let mut canvas: Vec<i16> = Vec::new();
    let mut rate = 22050;
    let mut cuts = Vec::new();
    for (index, place) in placements.iter().enumerate() {
        stopped(cancel)?;
        report(app, "fit", "running", format!("Câu {}/{}.", index + 1, lines.len()));
        let fitted = job.line_fit(index + 1);
        let mut command = Command::new("ffmpeg");
        command
            .args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
            .arg(job.line_wav(index + 1))
            .arg("-af")
            .arg(fit_filter(place.speed, place.keep))
            .args(["-ac", "1", "-ar", "22050", "-c:a", "pcm_s16le"])
            .arg(&fitted);
        run_tool(command, Duration::from_secs(60), cancel)?;
        let pcm = read_wav(&fitted)?;
        rate = pcm.rate;
        mix_at(&mut canvas, rate, place.start, &pcm.samples);
        if let Some(kept) = place.keep {
            cuts.push(Cut {
                line: index + 1,
                text: lines[index].translation.clone(),
                spoken: slots[index].wav_secs / place.speed,
                kept,
            });
        }
    }
    let wanted = (video_secs * f64::from(rate)).ceil() as usize;
    if canvas.len() < wanted {
        canvas.resize(wanted, 0);
    }
    write_wav(&job.dub(), &Pcm { rate, samples: canvas })?;
    write_json(&job.fit_report(), &cuts)?;
    let detail = if cuts.is_empty() {
        "Đã xếp tiếng.".to_string()
    } else {
        format!("Đã xếp tiếng. {} câu bị cắt bớt, xem fit-report.json.", cuts.len())
    };
    report(app, "fit", "done", detail);
    Ok(())
}
```

`fit_filter` ra `anull` khi không có gì làm để FFmpeg vẫn chuẩn hóa về 22050 Hz mono.

- [ ] **Step 5: Chạy test**

Run: `cd D:\Project\translate-video\src-tauri; cargo test --offline`
Expected: toàn bộ pass (16 cũ − 2 test bị xóa + 6 timeline + 5 wav + 2 fit_filter + ... ; không có test nào fail, không còn cảnh báo `dead_code` cho `mix_filter`/`place_line`).

- [ ] **Step 6: Kiểm tra bằng mắt với dữ liệu giả (không cần Piper/Whisper)**

Tạo folder việc giả trong thư mục tạm: một video ngắn bằng FFmpeg (`-f lavfi -i testsrc=d=6 -f lavfi -i sine=d=6`), vài file `lines/001.wav`… bằng `sine`, kèm `audio.wav`, `transcript.json`, `lines.json`, `terms.json`, `translation.json` hợp lệ để các bước trước bị bỏ qua, rồi chạy app (`npm run tauri dev`) và bấm Lồng tiếng. Expected: bước `fit` xong, có `dub.wav` dài bằng video, `fit-report.json` tồn tại, nghe thấy các đoạn không đè nhau.

---

### Task 4: Piper đọc cả lô, có làm sạch và thử lại

> **Đã thay đổi sau khi thực thi (2026-10-07):** app không còn gọi `piper.exe`. Giọng chạy trong cửa sổ app (`src/tts`, `src-tauri/src/tts.rs`), nên `piper_setup`, `speak_batch`, `batch_input`, `run_tool_input` và đường lui đọc từng câu đã bị bỏ. `clean_for_speech`, `is_speakable` và `synthesize_missing` còn lại. Các bước Piper bên dưới giữ để tham khảo lịch sử.

**Files:**
- Create: `src-tauri/src/speech.rs`
- Modify: `src-tauri/src/lib.rs` (`mod speech;`, tách `piper_setup`, sửa `speak_vietnamese_blocking`)
- Modify: `src-tauri/src/pipeline.rs` (`run_tool_input`, `synthesize_missing`, sửa `speak`)

**Interfaces:**
- Produces:
  - `speech::clean_for_speech(text: &str) -> String`
  - `speech::is_speakable(text: &str) -> bool` (≥2 ký tự chữ/số, giống `tts_main.py` bỏ câu ≤1 ký tự)
  - `speech::batch_input(items: &[(String, PathBuf)]) -> String` (mỗi dòng một JSON `{"text":..., "output_file":...}`)
  - `crate::piper_setup() -> Result<(PathBuf /*piper*/, PathBuf /*thư mục piper*/, PathBuf /*model .onnx*/), String>`
  - `pipeline::run_tool_input(command, input: Option<String>, timeout, cancel) -> Result<String, String>`
  - `pipeline::synthesize_missing(app, job, lines: &[TranslatedLine], cancel) -> Result<(), String>`

**Chưa xác minh được trên máy này** (không có `piper.exe`): Piper có nhận `--json-input` kèm khóa `output_file` hay không. Vì vậy `synthesize_missing` luôn có đường lui: câu nào sau lô vẫn chưa có file thì đọc từng câu như cũ.

- [ ] **Step 1: Xác minh Piper (trên máy có Piper)**

Run (PowerShell, đường dẫn tự thay):
```powershell
'{"text":"xin chào các bạn","output_file":"C:\\Temp\\a.wav"}' | piper --model D:\Project\translate-video\voices\vi\banmai.onnx --json-input
Test-Path C:\Temp\a.wav
```
Expected: `True`. Nếu `False`, vẫn làm tiếp Task 4: lô sẽ không ghi gì và đường lui đọc từng câu như cũ; ghi chú lại kết quả vào README ở Step 7.

- [ ] **Step 2: Viết `speech.rs` kèm test (test fail trước)**

```rust
use std::path::PathBuf;

pub(crate) fn clean_for_speech(_text: &str) -> String {
    todo!()
}

pub(crate) fn is_speakable(_text: &str) -> bool {
    todo!()
}

pub(crate) fn batch_input(_items: &[(String, PathBuf)]) -> String {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleaning_drops_remarks_and_symbols() {
        assert_eq!(clean_for_speech("Xin chào (cười) & bạn™"), "Xin chào và bạn");
        assert_eq!(clean_for_speech("  a\nb\t*c*  "), "a b c");
        assert_eq!(clean_for_speech("[nhạc] Hết."), "Hết.");
    }

    #[test]
    fn one_letter_or_punctuation_is_not_spoken() {
        assert!(!is_speakable(""));
        assert!(!is_speakable("..."));
        assert!(!is_speakable("a."));
        assert!(!is_speakable("Ừ."));
        assert!(is_speakable("Dạ."));
        assert!(is_speakable("12"));
    }

    #[test]
    fn batch_input_is_one_json_per_line() {
        let items = vec![
            ("xin chào".to_string(), PathBuf::from(r"C:\v\lines\001.wav")),
            ("tạm biệt".to_string(), PathBuf::from(r"C:\v\lines\002.wav")),
        ];
        let text = batch_input(&items);
        let rows: Vec<&str> = text.lines().collect();
        assert_eq!(rows.len(), 2);
        let first: serde_json::Value = serde_json::from_str(rows[0]).unwrap();
        assert_eq!(first["text"], "xin chào");
        assert_eq!(first["output_file"], r"C:\v\lines\001.wav");
    }
}
```

Thêm `mod speech;` vào `lib.rs`. Run `cargo test --offline speech::` → Expected: panic `not yet implemented`.

- [ ] **Step 3: Viết thân hàm**

```rust
pub(crate) fn clean_for_speech(text: &str) -> String {
    let mut out = String::new();
    let mut depth = 0u32;
    for ch in text.chars() {
        match ch {
            '(' | '（' | '[' => depth += 1,
            ')' | '）' | ']' => depth = depth.saturating_sub(1),
            _ if depth > 0 => {}
            '&' => out.push_str(" và "),
            '®' | '™' | '©' | '*' | '#' | '_' | '~' | '`' | '"' | '“' | '”' | '\n' | '\r' | '\t' => out.push(' '),
            _ => out.push(ch),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(crate) fn is_speakable(text: &str) -> bool {
    text.chars().filter(|ch| ch.is_alphanumeric()).count() >= 2
}

pub(crate) fn batch_input(items: &[(String, PathBuf)]) -> String {
    let mut out = String::new();
    for (text, path) in items {
        let row = serde_json::json!({"text": text, "output_file": path.to_string_lossy()});
        out.push_str(&row.to_string());
        out.push('\n');
    }
    out
}
```

Run `cargo test --offline speech::` → Expected: 3 pass.

- [ ] **Step 4: Tách `piper_setup` trong `lib.rs`**

Thay phần giữa `let output = wav_in_user_folder(output_wav)?;` và `let mut command = Command::new(&piper);` của `speak_vietnamese_blocking` bằng một lời gọi, và thêm hàm mới ngay trên nó:

```rust
fn piper_setup() -> Result<(PathBuf, PathBuf, PathBuf), String> {
    let piper = program_path("piper").ok_or("Chưa thấy piper trong PATH.")?;
    let piper_dir = piper
        .parent()
        .ok_or("Đã thấy piper, chưa xác định được thư mục chứa nó.")?
        .to_path_buf();
    let (voice_dir, voice) = if let Some(dir) = bundled_voice_dir() {
        let voice = local_voice(&dir).ok_or("Folder voices/vi chưa có cặp file giọng.")?;
        (dir, voice)
    } else {
        let voice = local_voice(&piper_dir).ok_or("Chưa thấy cặp file giọng cùng thư mục với piper.exe.")?;
        (piper_dir.clone(), voice)
    };
    let model = voice_dir.join(format!("{voice}.onnx"));
    Ok((piper, piper_dir, model))
}
```

và trong `speak_vietnamese_blocking`:

```rust
    let output = wav_in_user_folder(output_wav)?;
    let (piper, piper_dir, model) = piper_setup()?;
    let mut command = Command::new(&piper);
```

Giữ nguyên thứ tự kiểm tra (chữ rỗng → đuôi `.wav` → folder → piper) để test `speak_rejects_empty_text_and_bad_wav_path` vẫn pass. Run `cargo test --offline` → Expected: pass.

- [ ] **Step 5: `run_tool_input` trong `pipeline.rs`**

Thêm `use std::io::Write;` và đổi đầu `run_tool` thành hai hàm:

```rust
fn run_tool(command: Command, timeout: Duration, cancel: &AtomicBool) -> Result<String, String> {
    run_tool_input(command, None, timeout, cancel)
}

fn run_tool_input(
    mut command: Command,
    input: Option<String>,
    timeout: Duration,
    cancel: &AtomicBool,
) -> Result<String, String> {
    let stdin = if input.is_some() { Stdio::piped() } else { Stdio::null() };
    let mut child = crate::hidden(&mut command)
        .stdin(stdin)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| error.to_string())?;
    if let Some(text) = input {
        if let Some(mut pipe) = child.stdin.take() {
            thread::spawn(move || {
                let _ = pipe.write_all(text.as_bytes());
            });
        }
    }
```

Phần còn lại của thân hàm cũ (từ `let mut stdout_pipe = child.stdout.take();`) giữ nguyên. Run `cargo test --offline` → Expected: pass.

- [ ] **Step 6: `synthesize_missing` và sửa `speak`**

Thêm `use crate::speech::{batch_input, clean_for_speech, is_speakable};` rồi thay hàm `speak` cũ bằng:

```rust
fn speak(app: &AppHandle, job: &Job, lines: &[TranslatedLine], cancel: &AtomicBool) -> Result<(), String> {
    let pending = (1..=lines.len()).filter(|index| !file_ready(&job.line_wav(*index))).count();
    if pending == 0 {
        report(app, "speak", "skipped", "Đã có tiếng từng câu, bỏ qua.");
        return Ok(());
    }
    stopped(cancel)?;
    synthesize_missing(app, job, lines, cancel)?;
    report(app, "speak", "done", "Đã đọc.");
    Ok(())
}

fn synthesize_missing(
    app: &AppHandle,
    job: &Job,
    lines: &[TranslatedLine],
    cancel: &AtomicBool,
) -> Result<(), String> {
    let mut batch: Vec<(usize, String, PathBuf)> = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let path = job.line_wav(index + 1);
        if file_ready(&path) {
            continue;
        }
        let text = clean_for_speech(&line.translation);
        if is_speakable(&text) {
            batch.push((index, text, path));
        } else {
            write_silence(&path, cancel)?;
        }
    }
    if batch.is_empty() {
        return Ok(());
    }
    report(app, "speak", "running", format!("Đang đọc {} câu.", batch.len()));
    let items: Vec<(String, PathBuf)> = batch.iter().map(|(_, text, path)| (text.clone(), path.clone())).collect();
    if let Err(error) = speak_batch(&items, cancel) {
        if error == "Đã dừng." {
            return Err(error);
        }
    }
    for (index, text, path) in &batch {
        stopped(cancel)?;
        if file_ready(path) {
            continue;
        }
        report(app, "speak", "running", format!("Đọc lại câu {}/{}.", index + 1, lines.len()));
        let mut last = String::new();
        for _ in 0..3 {
            match crate::speak_vietnamese_blocking(text, &path.to_string_lossy()) {
                Ok(_) => break,
                Err(error) => last = error,
            }
        }
        if !file_ready(path) {
            return Err(if last.is_empty() { format!("Không đọc được câu {}.", index + 1) } else { last });
        }
    }
    Ok(())
}

fn speak_batch(items: &[(String, PathBuf)], cancel: &AtomicBool) -> Result<(), String> {
    let (piper, piper_dir, model) = crate::piper_setup()?;
    let mut command = Command::new(&piper);
    command.current_dir(&piper_dir).arg("--model").arg(&model).arg("--json-input");
    let timeout = Duration::from_secs(120 + 10 * items.len() as u64);
    run_tool_input(command, Some(batch_input(items)), timeout, cancel).map(|_| ())
}
```

`write_silence` giữ như cũ (đã có trong file). Run `cargo test --offline` → Expected: pass.

- [ ] **Step 7: Ghi vào README**

Trong `README.md`, mục "Chạy"/"Hiện có", thêm một dòng: `Piper đọc cả lô câu bằng một lần chạy (--json-input); nếu bản Piper không hỗ trợ thì app đọc từng câu như trước.` Ghi thêm kết quả Step 1 (có hỗ trợ hay không).

---

### Task 5: Rút gọn câu đọc quá dài (vòng kín: đọc → đo → rút gọn → đọc lại)

**Files:**
- Modify: `src-tauri/src/ollama.rs` (thêm `shorten_prompt` + test)
- Modify: `src-tauri/src/pipeline.rs` (`speak` nhận `&mut Vec<TranslatedLine>` và `choice`; sửa `run`)

**Interfaces:**
- Consumes: `timeline::{Slot, room, too_long, MAX_SPEED}`, `synthesize_missing` (Task 4), `ollama::{complete, translated_text}`, `job::write_json`.
- Produces: `ollama::shorten_prompt(text: &str, room_secs: f64, wav_secs: f64) -> String`.

Khác VideoLingo: họ *ước lượng* thời lượng bằng đếm âm tiết trước khi đọc. Ta đã có thời lượng thật từ file wav Piper, nên dùng số đo thật, chính xác hơn và không cần bảng âm tiết tiếng Việt.

- [ ] **Step 1: Test prompt (fail trước)**

Trong `mod tests` của `ollama.rs`:

```rust
    #[test]
    fn shorten_prompt_asks_for_the_needed_ratio() {
        // 2.0 * 1.4 * 0.95 / 3.8 = 0.70
        let prompt = shorten_prompt("Một câu khá dài cần được rút gọn lại.", 2.0, 3.8);
        assert!(prompt.contains("70%"), "{prompt}");
        assert!(prompt.contains("Một câu khá dài"));
        assert!(prompt.contains("{\"text\":\"\"}"));
    }
```

Run `cargo test --offline shorten` → Expected: lỗi biên dịch (chưa có hàm).

- [ ] **Step 2: Viết `shorten_prompt` trong `ollama.rs`**

```rust
pub(crate) fn shorten_prompt(text: &str, room_secs: f64, wav_secs: f64) -> String {
    let ratio = if wav_secs > 0.0 {
        (room_secs * crate::timeline::MAX_SPEED * 0.95 / wav_secs).clamp(0.4, 0.95)
    } else {
        0.9
    };
    let percent = (ratio * 100.0).round() as u32;
    format!(
        "Rút gọn câu tiếng Việt dưới đây còn khoảng {percent}% độ dài, để đọc kịp thời gian.\n\
         Giữ ý chính và các tên riêng, thuật ngữ. Bỏ từ đệm, từ thừa. Không thêm ý mới.\n\
         Chỉ trả về JSON: {{\"text\":\"\"}}\n\
         <line>\n{text}\n</line>"
    )
}
```

Run `cargo test --offline shorten` → Expected: pass.

- [ ] **Step 3: Nối vòng kín vào `speak`**

Trong `pipeline.rs`, thêm `use crate::timeline::{room, too_long};` và `shorten_prompt` vào danh sách import từ `crate::ollama`. Thay `speak` bằng:

```rust
const MAX_SHORTEN_ROUNDS: usize = 2;

fn speak(
    app: &AppHandle,
    job: &Job,
    lines: &mut Vec<TranslatedLine>,
    choice: &crate::TranslationChoice,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let total = lines.len();
    let pending = (1..=total).filter(|index| !file_ready(&job.line_wav(*index))).count();
    if pending == 0 {
        report(app, "speak", "skipped", "Đã có tiếng từng câu, bỏ qua.");
        return Ok(());
    }
    stopped(cancel)?;
    let video_secs = media_duration(&job.video, cancel)?;
    synthesize_missing(app, job, lines, cancel)?;
    for round in 1..=MAX_SHORTEN_ROUNDS {
        let slots = read_slots(job, lines)?;
        let long: Vec<usize> = (0..total)
            .filter(|&i| !lines[i].translation.trim().is_empty() && too_long(slots[i].wav_secs, room(&slots, i, video_secs)))
            .collect();
        if long.is_empty() {
            break;
        }
        report(app, "speak", "running", format!("Rút gọn {} câu quá dài (lần {round}).", long.len()));
        let mut changed = false;
        for i in long {
            stopped(cancel)?;
            let prompt = shorten_prompt(&lines[i].translation, room(&slots, i, video_secs), slots[i].wav_secs);
            let Ok(value) = complete(choice, &prompt) else { continue };
            let Some(text) = translated_text(&value) else { continue };
            if text.chars().count() >= lines[i].translation.chars().count() {
                continue;
            }
            lines[i].translation = text;
            let _ = std::fs::remove_file(job.line_wav(i + 1));
            changed = true;
        }
        if !changed {
            break;
        }
        write_json(&job.translation_path(), lines)?;
        synthesize_missing(app, job, lines, cancel)?;
    }
    report(app, "speak", "done", "Đã đọc.");
    Ok(())
}

fn read_slots(job: &Job, lines: &[TranslatedLine]) -> Result<Vec<Slot>, String> {
    lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            let pcm = read_wav(&job.line_wav(index + 1))?;
            Ok(Slot {
                start: line.start,
                end: line.end,
                wav_secs: pcm.samples.len() as f64 / f64::from(pcm.rate),
            })
        })
        .collect()
}
```

Lỗi gọi Ollama khi rút gọn được bỏ qua có chủ ý: rút gọn là bước tùy chọn, `fit` vẫn tăng tốc/cắt như cũ.

Sửa `run`:

```rust
    let mut translated =
        translate(app, &job, &lines, &glossary, &choice, cancel).map_err(|error| fail(app, "translate", error))?;
    speak(app, &job, &mut translated, &choice, cancel).map_err(|error| fail(app, "speak", error))?;
    fit(app, &job, &translated, cancel).map_err(|error| fail(app, "fit", error))?;
```

(đổi `let translated` → `let mut translated` và dòng `speak(...)`.)

- [ ] **Step 4: Chạy test**

Run: `cd D:\Project\translate-video\src-tauri; cargo test --offline`
Expected: pass, không cảnh báo biến không dùng.

- [ ] **Step 5: Kiểm tra với dữ liệu thật (cần Ollama + Piper)**

Chạy một video ngắn có lời. Expected trong tiến độ `speak`: có dòng "Rút gọn N câu quá dài" nếu có câu dài; mở `translation.json` thấy các câu đó ngắn hơn bản `source`; `fit-report.json` ít câu hơn so với khi tắt bước rút gọn.

---

### Task 6: Dịch theo cụm có ngữ cảnh

**Files:**
- Modify: `src-tauri/src/ollama.rs` (thêm `chunk_ranges`, `batch_prompt`, `batch_translations` + test)
- Modify: `src-tauri/src/pipeline.rs` (`translate` dịch theo cụm, đường lui từng câu)

**Interfaces:**
- Produces:
  - `ollama::CHUNK_LINES: usize = 10`, `ollama::CHUNK_CHARS: usize = 600`
  - `ollama::chunk_ranges(lines: &[String], max_lines: usize, max_chars: usize) -> Vec<std::ops::Range<usize>>`
  - `ollama::batch_prompt(lines: &[&str], before: &[&str], after: &[&str], terms: &[&Term], theme: &str) -> String`
  - `ollama::batch_translations(value: &Value, count: usize) -> Option<Vec<String>>`
- Định dạng `translation.json` không đổi.

- [ ] **Step 1: Test (fail trước)**

Trong `mod tests` của `ollama.rs`:

```rust
    #[test]
    fn chunks_respect_line_and_char_limits() {
        let lines: Vec<String> = (0..25).map(|_| "0123456789".to_string()).collect();
        assert_eq!(chunk_ranges(&lines, 10, 600), vec![0..10, 10..20, 20..25]);
        assert_eq!(chunk_ranges(&lines, 100, 33), vec![0..3, 3..6, 6..9, 9..12, 12..15, 15..18, 18..21, 21..24, 24..25]);
        assert!(chunk_ranges(&[], 10, 600).is_empty());
    }

    #[test]
    fn batch_prompt_numbers_lines_and_lists_context_and_terms() {
        let term = Term { src: "CPU".into(), tgt: "CPU".into(), note: "bộ xử lý".into() };
        let prompt = batch_prompt(&["Hello", "World"], &["Before one"], &["After one"], &[&term], "Máy tính");
        assert!(prompt.contains("1. Hello"));
        assert!(prompt.contains("2. World"));
        assert!(prompt.contains("Before one"));
        assert!(prompt.contains("After one"));
        assert!(prompt.contains("\"CPU\": \"CPU\""));
        assert!(prompt.contains("đúng 2 khóa"));
    }

    #[test]
    fn batch_answer_needs_every_numbered_line() {
        let ok = json!({"1": " Xin chào ", "2": "Thế giới"});
        assert_eq!(batch_translations(&ok, 2).unwrap(), vec!["Xin chào".to_string(), "Thế giới".to_string()]);
        assert!(batch_translations(&json!({"1": "a"}), 2).is_none());
        assert!(batch_translations(&json!({"1": "a", "2": "  "}), 2).is_none());
        assert!(batch_translations(&json!(["a", "b"]), 2).is_none());
    }
```

Run `cargo test --offline ollama::` → Expected: lỗi biên dịch.

- [ ] **Step 2: Viết ba hàm trong `ollama.rs`**

```rust
pub(crate) const CHUNK_LINES: usize = 10;
pub(crate) const CHUNK_CHARS: usize = 600;

pub(crate) fn chunk_ranges(lines: &[String], max_lines: usize, max_chars: usize) -> Vec<std::ops::Range<usize>> {
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

pub(crate) fn batch_prompt(lines: &[&str], before: &[&str], after: &[&str], terms: &[&Term], theme: &str) -> String {
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
    let context = |items: &[&str]| if items.is_empty() { "(không có)".to_string() } else { items.join("\n") };
    let count = lines.len();
    format!(
        "Dịch các câu phụ đề sau sang tiếng Việt tự nhiên, đủ ngắn để đọc thành lời. Dịch từng dòng, không gộp, không bỏ dòng.\n\
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

pub(crate) fn batch_translations(value: &Value, count: usize) -> Option<Vec<String>> {
    let object = value.as_object()?;
    let mut out = Vec::with_capacity(count);
    for index in 1..=count {
        let text = object.get(&index.to_string())?.as_str()?.trim();
        if text.is_empty() {
            return None;
        }
        out.push(text.to_string());
    }
    Some(out)
}
```

Run `cargo test --offline ollama::` → Expected: pass.

- [ ] **Step 3: Dùng trong `translate` (`pipeline.rs`)**

Thêm `batch_prompt, batch_translations, chunk_ranges, CHUNK_CHARS, CHUNK_LINES` vào import từ `crate::ollama` và `use crate::job::Term;`. Trong `translate`, thay vòng `for (index, line) in lines.iter().enumerate() { ... }` (từ sau `report(app, "translate", "running", "Đang dịch.");` đến hết vòng) bằng:

```rust
    let total = lines.len();
    let texts: Vec<String> = lines.iter().map(|line| line.text.clone()).collect();
    for (index, line) in lines.iter().enumerate() {
        done[index].start = line.start;
        done[index].end = line.end;
    }
    for range in chunk_ranges(&texts, CHUNK_LINES, CHUNK_CHARS) {
        stopped(cancel)?;
        let todo: Vec<usize> = range
            .clone()
            .filter(|&i| !texts[i].trim().is_empty() && done[i].translation.trim().is_empty())
            .collect();
        if todo.is_empty() {
            continue;
        }
        report(app, "translate", "running", format!("Câu {}-{}/{}.", range.start + 1, range.end, total));
        let before: Vec<&str> = texts[range.start.saturating_sub(3)..range.start].iter().map(String::as_str).collect();
        let after: Vec<&str> = texts[range.end..(range.end + 2).min(total)].iter().map(String::as_str).collect();
        let mut matched: Vec<&Term> = Vec::new();
        for &i in &todo {
            for term in terms_for_line(&glossary.terms, &texts[i]) {
                if !matched.iter().any(|seen| seen.src == term.src) {
                    matched.push(term);
                }
            }
        }
        let batch: Vec<&str> = todo.iter().map(|&i| texts[i].as_str()).collect();
        let parsed = complete(choice, &batch_prompt(&batch, &before, &after, &matched, &glossary.theme))
            .ok()
            .and_then(|value| batch_translations(&value, batch.len()));
        match parsed {
            Some(items) => {
                for (&i, text) in todo.iter().zip(items) {
                    done[i].translation = text;
                }
            }
            None => {
                for &i in &todo {
                    stopped(cancel)?;
                    let matched = terms_for_line(&glossary.terms, &texts[i]);
                    let value =
                        complete(choice, &translate_prompt(&texts[i], &matched)).map_err(|error| error.to_string())?;
                    done[i].translation =
                        translated_text(&value).ok_or_else(|| format!("Không dịch được câu {}.", i + 1))?;
                }
            }
        }
        write_json(&job.translation_path(), &done)?;
    }
```

Giữ nguyên dòng `report(app, "translate", "done", ...)` và `Ok(done)` sau vòng. Lỗi mạng khi gọi theo cụm bị `.ok()` nuốt rồi đường lui từng câu gọi lại và báo lỗi thật.

- [ ] **Step 4: Chạy test**

Run: `cd D:\Project\translate-video\src-tauri; cargo test --offline`
Expected: pass.

- [ ] **Step 5: Kiểm tra với Ollama thật**

Chạy video có ≥ 15 câu. Expected: tiến độ hiện "Câu 1-10/N" thay vì từng câu; `translation.json` đủ câu; dừng giữa chừng rồi chạy lại thì chỉ dịch tiếp các cụm còn thiếu.

---

### Task 7: Vệ sinh nhỏ (không từ VideoLingo)

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/ollama.rs` (Gemini)

- [ ] **Step 1: Bỏ `panic = "abort"`**

Trong `[profile.release]` của `Cargo.toml`, xóa dòng `panic = "abort"`. Lý do: với `abort`, `catch_unwind` trong `start_dub` không còn tác dụng; panic sẽ giết cả app thay vì báo lỗi.

- [ ] **Step 2: Test Gemini không đưa key lên URL (fail trước)**

Trong `mod tests` của `ollama.rs`:

```rust
    #[test]
    fn gemini_key_travels_in_a_header_not_the_url() {
        assert!(!GEMINI_URL.contains("key="));
        assert!(GEMINI_URL.starts_with("https://generativelanguage.googleapis.com/"));
    }
```

Run `cargo test --offline gemini` → Expected: lỗi biên dịch (chưa có `GEMINI_URL`).

- [ ] **Step 3: Sửa `gemini_request`**

Thêm hằng số và sửa hàm (giữ nguyên tên model hiện tại; việc đổi model là quyết định riêng, xem "Để sau"):

```rust
const GEMINI_URL: &str =
    "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.0-flash:generateContent";

fn gemini_request(agent: &ureq::Agent, key: &str, prompt: &str) -> Result<String, String> {
    let payload = serde_json::json!({
        "contents": [{"parts": [{"text": prompt}]}],
        "generationConfig": {"responseMimeType": "application/json"},
    });
    let body = send(agent, "POST", GEMINI_URL, &[("x-goog-api-key", key)], &payload)
        .map_err(|error| format!("Gemini từ chối yêu cầu. {error}"))?;
    body["candidates"][0]["content"]["parts"][0]["text"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| "Gemini không trả lời.".into())
}
```

- [ ] **Step 4: Chạy test**

Run: `cd D:\Project\translate-video\src-tauri; cargo test --offline`
Expected: pass.

---

## Self-Review

1. **Phủ yêu cầu:** Lỗi 1 (giới hạn dòng lệnh) → Task 2+3. Lỗi 2 (Piper 1 process/câu) → Task 4. Lỗi 3 (`panic=abort`) → Task 7. Lỗi 4 (tiếng đè) → Task 1+3. Học từ VideoLingo: dịch theo cụm (6), rút gọn (5), mượn khoảng trống + log câu bị cắt (1, 3), làm sạch/thử lại TTS (4). Gemini key trong URL → Task 7.
2. **Placeholder:** Không còn "TBD"; mọi bước có mã. Hai chỗ dựa vào việc chạy thật được ghi rõ là bước kiểm tra tay (Task 3 Step 6, Task 4 Step 1, Task 5 Step 5, Task 6 Step 5).
3. **Nhất quán kiểu/tên:** `Slot`, `Placement`, `place_all`, `room`, `too_long`, `Pcm`, `read_wav`, `write_wav`, `mix_at`, `fit_filter`, `synthesize_missing`, `piper_setup`, `shorten_prompt`, `chunk_ranges`, `batch_prompt`, `batch_translations` dùng đồng nhất giữa các task. `speak` đổi chữ ký ở Task 5 (thêm `&mut`, `choice`), và `run` được sửa cùng task.

Điểm cần chú ý khi thực thi: Task 3 xóa mã mà Task 4/5 không dùng lại, nhưng Task 4 Step 6 gọi `write_silence` (còn nguyên trong file). Task 5 phụ thuộc Task 1 và 4; Task 6 và 7 độc lập.

## Để sau (cần quyết định của bạn hoặc cần video thật)

- **Whisper:** bật VAD (cần model Silero), chọn `large-v3-turbo`, mốc thời gian theo từ để chia câu chính xác hơn `divide_time`. Cần chạy `whisper-cli` thật để xem định dạng JSON mốc từ trước khi viết mã.
- **Giữ tiếng nền/nhạc:** VideoLingo tách bằng Demucs (Python) rồi `amix`. Ta chưa có đường offline không-Python; lựa chọn gần nhất là hạ nhỏ âm gốc rồi trộn, nhưng lời gốc vẫn lọt. Cần bạn chọn hướng.
- **Chọn giọng:** `local_voice` đang lấy giọng đầu tiên theo bảng chữ cái (`banmai`). Mục "một giọng cố định hay nhân bản" còn mở trong `AGENTS.md`.
- **Model cloud cũ** (`claude-3-5-haiku-latest`, `gpt-4o-mini`, `gemini-2.0-flash`) và **API key lưu plaintext** trong `translation.json` (dùng `keyring`): cần bạn chọn model và đồng ý thêm crate.
- **Model dịch local:** `AGENTS.md` cấm hướng dẫn model khác ngoài `qwen2.5:7b`.
- **Phụ đề cứng, song ngữ** (VideoLingo có): chưa trong phạm vi.
