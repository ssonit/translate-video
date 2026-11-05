//! Douyin chặn thì app nghỉ, không hỏi dồn. Hàng đợi để chính sách "tiếp tục khi lỗi" thì video sau sẽ
//! gọi Douyin ngay lập tức; không có chốt này, mỗi video bị chặn lại thêm một lần thử và Douyin chặn lâu hơn.
//!
//! fetch.py báo bị chặn bằng một dòng `[douyin-blocked] <lý do>` trên stderr. Lần chặn đầu nghỉ 15 phút,
//! mỗi lần chặn tiếp theo gấp đôi, tối đa 60 phút. Quá 6 giờ không bị chặn thì đếm lại từ đầu. Mốc nghỉ
//! ghi vào thư mục dữ liệu của app để tắt mở lại app vẫn còn. Các con số này là chọn tay, chưa đo với Douyin thật.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const BLOCK_MARK: &str = "[douyin-blocked]";
const MINUTE_MS: u64 = 60_000;
const FIRST_PAUSE_MS: u64 = 15 * MINUTE_MS;
const LONGEST_PAUSE_MS: u64 = 60 * MINUTE_MS;
const FORGET_AFTER_MS: u64 = 6 * 60 * MINUTE_MS;

// Chỗ ghi mốc nghỉ: trong thư mục dữ liệu của app.
pub(crate) fn file_in(dir: &Path) -> PathBuf {
    dir.join("douyin-pause.json")
}

#[derive(Clone, Copy, Default, Serialize, Deserialize)]
struct Record {
    until_ms: u64,
    last_ms: u64,
    strikes: u32,
}

// File thiếu hoặc hỏng nghĩa là chưa từng bị chặn. Không bao giờ để file hỏng chặn người dùng tải.
fn read(path: &Path) -> Record {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

// Lý do nằm sau dấu hiệu ở đầu dòng cuối cùng có dấu hiệu. Dấu hiệu giữa dòng (vd. trong tiêu đề) không tính.
pub(crate) fn blocked_reason(error: &str) -> Option<String> {
    error
        .lines()
        .rev()
        .find_map(|line| line.trim().strip_prefix(BLOCK_MARK))
        .map(|reason| reason.trim().to_string())
}

// Còn phải nghỉ bao nhiêu mili giây. Đồng hồ bị chỉnh lùi thì mốc nghỉ có thể nằm xa, nên chặn ở mức lâu nhất.
pub(crate) fn time_left(path: &Path, now_ms: u64) -> Option<u64> {
    let left = read(path)
        .until_ms
        .saturating_sub(now_ms)
        .min(LONGEST_PAUSE_MS);
    (left > 0).then_some(left)
}

// Ghi nhận một lần bị chặn và trả số mili giây phải nghỉ.
pub(crate) fn record_block(path: &Path, now_ms: u64) -> u64 {
    let before = read(path);
    let strikes = if before.strikes > 0 && now_ms.saturating_sub(before.last_ms) <= FORGET_AFTER_MS
    {
        before.strikes + 1
    } else {
        1
    };
    let wait = FIRST_PAUSE_MS
        .saturating_mul(1 << (strikes - 1).min(6))
        .min(LONGEST_PAUSE_MS);
    let record = Record {
        until_ms: now_ms.saturating_add(wait),
        last_ms: now_ms,
        strikes,
    };
    if let Ok(bytes) = serde_json::to_vec(&record) {
        // Ghi không được thì lần chặn này không nhớ qua lần mở app sau; vẫn báo người dùng nghỉ như thường.
        let _ = crate::atomic_file::write(path, &bytes);
    }
    wait
}

// Tải xong một video là bằng chứng Douyin không còn chặn: quên các lần chặn trước.
pub(crate) fn clear(path: &Path) {
    let _ = std::fs::remove_file(path);
}

fn minutes(ms: u64) -> u64 {
    ms.div_ceil(MINUTE_MS).max(1)
}

pub(crate) fn waiting_message(left_ms: u64) -> String {
    format!(
        "Douyin vừa chặn yêu cầu. App nghỉ thêm khoảng {} phút rồi mới hỏi lại, để không bị chặn lâu hơn.",
        minutes(left_ms)
    )
}

fn reason_or_default(reason: &str) -> &str {
    match reason.trim() {
        "" => "Douyin từ chối yêu cầu.",
        text => text,
    }
}

pub(crate) fn blocked_message(reason: &str, wait_ms: u64) -> String {
    format!(
        "{} App nghỉ {} phút rồi mới hỏi Douyin lại, để không bị chặn lâu hơn.",
        reason_or_default(reason),
        minutes(wait_ms)
    )
}

// Chạy một lần hỏi Douyin qua chốt nghỉ: đang nghỉ thì không chạy gì cả (không mở Edge, không ra mạng);
// bị chặn thì ghi mốc nghỉ; chạy xong thì quên các lần chặn trước. Lỗi khác (đã dừng, video đã xóa) để nguyên.
pub(crate) fn guarded(
    path: Option<&Path>,
    now_ms: u64,
    run: impl FnOnce() -> Result<String, String>,
) -> Result<String, String> {
    if let Some(left) = path.and_then(|path| time_left(path, now_ms)) {
        return Err(waiting_message(left));
    }
    match run() {
        Ok(output) => {
            if let Some(path) = path {
                clear(path);
            }
            Ok(output)
        }
        Err(error) => match (blocked_reason(&error), path) {
            (Some(reason), Some(path)) => Err(blocked_message(&reason, record_block(path, now_ms))),
            // Không có chỗ nhớ mốc nghỉ thì đừng hứa là app sẽ nghỉ.
            (Some(reason), None) => Err(reason_or_default(&reason).to_string()),
            (None, _) => Err(error),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    const MINUTE: u64 = 60_000;
    const HOUR: u64 = 60 * MINUTE;
    const START: u64 = 1_800_000_000_000;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("douyin-pause-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_mark_fetch_py_prints_is_recognised_with_its_reason() {
        assert_eq!(
            blocked_reason("[douyin-blocked] Douyin từ chối yêu cầu (HTTP 403)."),
            Some("Douyin từ chối yêu cầu (HTTP 403).".into())
        );
        assert_eq!(
            blocked_reason(
                "DLP downloading 1 2 NA NA 0\n[douyin-blocked] Douyin trả về trang trống.\n"
            ),
            Some("Douyin trả về trang trống.".into())
        );
    }

    #[test]
    fn other_errors_are_not_a_block() {
        assert_eq!(blocked_reason("Douyin không trả địa chỉ video."), None);
        assert_eq!(
            blocked_reason("Tiêu đề nói về [douyin-blocked] giữa dòng"),
            None
        );
        assert_eq!(blocked_reason(""), None);
    }

    #[test]
    fn a_mark_with_no_reason_still_counts() {
        assert_eq!(blocked_reason("[douyin-blocked]"), Some(String::new()));
    }

    #[test]
    fn nothing_is_paused_before_any_block() {
        let dir = scratch("fresh");
        assert_eq!(time_left(&dir.join("douyin-pause.json"), START), None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_first_block_pauses_for_fifteen_minutes_and_the_pause_counts_down() {
        let dir = scratch("first");
        let file = dir.join("douyin-pause.json");
        assert_eq!(record_block(&file, START), 15 * MINUTE);
        assert_eq!(time_left(&file, START), Some(15 * MINUTE));
        assert_eq!(time_left(&file, START + 10 * MINUTE), Some(5 * MINUTE));
        assert_eq!(time_left(&file, START + 15 * MINUTE), None);
        assert_eq!(time_left(&file, START + 16 * MINUTE), None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn each_new_block_doubles_the_pause_up_to_an_hour() {
        let dir = scratch("doubling");
        let file = dir.join("douyin-pause.json");
        let mut now = START;
        let mut waits = Vec::new();
        for _ in 0..5 {
            let wait = record_block(&file, now);
            waits.push(wait / MINUTE);
            now += wait;
        }
        assert_eq!(waits, [15, 30, 60, 60, 60]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_block_long_after_the_last_one_starts_again_from_fifteen_minutes() {
        let dir = scratch("forget");
        let file = dir.join("douyin-pause.json");
        record_block(&file, START);
        record_block(&file, START + 20 * MINUTE);
        assert_eq!(
            record_block(&file, START + 20 * MINUTE + 7 * HOUR),
            15 * MINUTE
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_finished_download_lifts_the_pause_and_forgets_earlier_blocks() {
        let dir = scratch("clear");
        let file = dir.join("douyin-pause.json");
        record_block(&file, START);
        record_block(&file, START + 20 * MINUTE);
        clear(&file);
        assert_eq!(time_left(&file, START + 21 * MINUTE), None);
        assert_eq!(record_block(&file, START + 22 * MINUTE), 15 * MINUTE);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_clock_set_back_never_leaves_the_pause_longer_than_an_hour() {
        let dir = scratch("clock");
        let file = dir.join("douyin-pause.json");
        record_block(&file, START);
        record_block(&file, START + 20 * MINUTE);
        record_block(&file, START + 90 * MINUTE);
        let left = time_left(&file, START - 5 * HOUR).unwrap();
        assert!(left <= HOUR, "{left}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_damaged_file_means_no_pause_and_is_replaced_by_the_next_block() {
        let dir = scratch("damaged");
        let file = dir.join("douyin-pause.json");
        fs::write(&file, "{ không phải json").unwrap();
        assert_eq!(time_left(&file, START), None);
        assert_eq!(record_block(&file, START), 15 * MINUTE);
        assert_eq!(time_left(&file, START + MINUTE), Some(14 * MINUTE));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_pause_survives_a_restart_because_it_lives_in_the_file() {
        let dir = scratch("restart");
        let file = dir.join("data").join("douyin-pause.json");
        record_block(&file, START);
        let raw = fs::read_to_string(&file).unwrap();
        assert!(raw.contains("until_ms"), "{raw}");
        assert_eq!(time_left(&file, START + MINUTE), Some(14 * MINUTE));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn messages_tell_the_wait_in_whole_minutes_rounded_up() {
        let waiting = waiting_message(14 * MINUTE + 1);
        assert!(waiting.contains("15 phút"), "{waiting}");
        assert!(waiting_message(1).contains("1 phút"));
        let blocked = blocked_message("Douyin từ chối yêu cầu (HTTP 403).", 30 * MINUTE);
        assert!(
            blocked.starts_with("Douyin từ chối yêu cầu (HTTP 403)."),
            "{blocked}"
        );
        assert!(blocked.contains("30 phút"), "{blocked}");
        assert!(blocked_message("", 15 * MINUTE).starts_with("Douyin từ chối yêu cầu."));
    }

    #[test]
    fn nothing_runs_while_the_pause_lasts() {
        let dir = scratch("gate");
        let file = dir.join("douyin-pause.json");
        record_block(&file, START);
        let mut ran = false;
        let result = guarded(Some(&file), START + MINUTE, || {
            ran = true;
            Ok("xong".into())
        });
        assert!(!ran, "vẫn chạy fetch.py trong lúc nghỉ");
        let message = result.unwrap_err();
        assert!(message.contains("14 phút"), "{message}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_block_starts_a_pause_that_stops_the_next_run() {
        let dir = scratch("block-then-gate");
        let file = dir.join("douyin-pause.json");
        let result = guarded(Some(&file), START, || {
            Err("[douyin-blocked] Douyin từ chối yêu cầu (HTTP 429).".into())
        });
        let message = result.unwrap_err();
        assert!(
            message.starts_with("Douyin từ chối yêu cầu (HTTP 429)."),
            "{message}"
        );
        assert!(message.contains("15 phút"), "{message}");
        let mut ran = false;
        let _ = guarded(Some(&file), START + MINUTE, || {
            ran = true;
            Ok(String::new())
        });
        assert!(!ran);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_good_run_after_the_pause_forgets_the_blocks() {
        let dir = scratch("good-run");
        let file = dir.join("douyin-pause.json");
        record_block(&file, START);
        record_block(&file, START + 20 * MINUTE);
        let after = START + 20 * MINUTE + 31 * MINUTE;
        assert_eq!(
            guarded(Some(&file), after, || Ok("đường dẫn".into())),
            Ok("đường dẫn".into())
        );
        assert!(!file.exists());
        assert_eq!(record_block(&file, after + MINUTE), 15 * MINUTE);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn errors_that_are_not_a_block_pass_through_and_start_no_pause() {
        let dir = scratch("other-errors");
        let file = dir.join("douyin-pause.json");
        for error in [
            "Đã dừng.",
            "Douyin không trả địa chỉ video. Video có thể đã bị xóa hoặc ẩn.",
        ] {
            assert_eq!(
                guarded(Some(&file), START, || Err(error.into())),
                Err(error.into())
            );
        }
        assert!(!file.exists());
        assert_eq!(time_left(&file, START), None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn without_a_place_to_keep_the_pause_a_block_is_only_reported() {
        let result = guarded(None, START, || {
            Err("[douyin-blocked] Douyin trả về trang trống.".into())
        });
        assert_eq!(result, Err("Douyin trả về trang trống.".into()));
        assert_eq!(
            guarded(None, START, || Ok("xong".into())),
            Ok("xong".into())
        );
    }

    #[test]
    fn the_mark_here_is_the_one_fetch_py_prints() {
        let script = include_str!("../douyin/fetch.py");
        assert!(
            script.contains(&format!("BLOCK_MARK = \"{BLOCK_MARK}\"")),
            "fetch.py và douyin_pause.rs phải dùng cùng một dấu hiệu"
        );
    }

    #[test]
    fn the_pause_is_kept_in_douyin_pause_json_inside_the_given_folder() {
        assert_eq!(
            file_in(Path::new("dữ liệu")),
            Path::new("dữ liệu").join("douyin-pause.json")
        );
    }
}
