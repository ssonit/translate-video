//! Khoảng nghỉ tối thiểu giữa hai lần hỏi Douyin liền nhau. Hàng đợi chạy việc này sang việc khác không chờ
//! gì, nên nhiều link ngắn liền nhau là nhiều "khách mới" từ cùng một IP trong vài chục giây.
//!
//! Mỗi lần hỏi Douyin xong thì lần sau phải chờ đủ 30 đến 60 giây (chọn ngẫu nhiên mỗi lần) kể từ lúc đó.
//! Việc trước chạy lâu hơn thế (dịch, đọc giọng, ghép) thì không phải chờ gì. Chỉ nhớ trong lúc app mở.
//! Các con số là chọn tay, chưa đo với Douyin thật.

use std::hash::BuildHasher;
use std::sync::{Mutex, PoisonError};

const SHORTEST_GAP_MS: u64 = 30_000;
const LONGEST_GAP_MS: u64 = 60_000;
// Chờ từng đoạn ngắn để nút Dừng có tác dụng ngay và dòng đếm ngược nhảy đúng từng giây.
const STEP_MS: u64 = 250;

// Lúc lần hỏi Douyin gần nhất kết thúc (mili giây từ 1970).
static LAST_ASKED_MS: Mutex<Option<u64>> = Mutex::new(None);

// Độ dài khoảng nghỉ của một lần, từ 30 đến 60 giây, chọn theo `seed`.
fn gap_ms(seed: u64) -> u64 {
    SHORTEST_GAP_MS + seed % (LONGEST_GAP_MS - SHORTEST_GAP_MS + 1)
}

// Số ngẫu nhiên lấy từ bộ băm có khóa ngẫu nhiên của hệ điều hành, khỏi thêm thư viện chỉ để chọn một con số.
fn random_gap_ms() -> u64 {
    gap_ms(std::collections::hash_map::RandomState::new().hash_one(crate::now_ms()))
}

// Còn phải chờ bao nhiêu mili giây nữa mới được hỏi Douyin. Đồng hồ bị chỉnh lùi thì coi như vừa hỏi xong,
// nên chờ tối đa một khoảng nghỉ chứ không chờ theo độ lùi.
fn wait_left(last_asked: Option<u64>, now_ms: u64, gap: u64) -> u64 {
    match last_asked {
        None => 0,
        Some(last) => gap.saturating_sub(now_ms.saturating_sub(last)),
    }
}

fn wait_with(
    state: &Mutex<Option<u64>>,
    gap: u64,
    now: impl Fn() -> u64,
    sleep: impl Fn(u64),
    mut report: impl FnMut(u64),
    stop: impl Fn() -> Result<(), String>,
) -> Result<(), String> {
    let mut shown = 0;
    loop {
        // Bấm Dừng rồi thì không hỏi Douyin nữa, kể cả khi không phải chờ gì.
        stop()?;
        // Khóa hỏng vì một luồng khác lỡ panic thì vẫn dùng giá trị cũ, không để chặn việc tải.
        let last = *state.lock().unwrap_or_else(PoisonError::into_inner);
        let left = wait_left(last, now(), gap);
        if left == 0 {
            return Ok(());
        }
        let seconds = left.div_ceil(1000);
        if seconds != shown {
            report(seconds);
            shown = seconds;
        }
        sleep(left.min(STEP_MS));
    }
}

fn note_with(state: &Mutex<Option<u64>>, now_ms: u64) {
    *state.lock().unwrap_or_else(PoisonError::into_inner) = Some(now_ms);
}

// Chờ cho đủ khoảng nghỉ kể từ lần hỏi Douyin trước. `report` nhận số giây còn lại mỗi khi số đó đổi;
// `stop` báo lỗi thì thôi chờ ngay.
pub(crate) fn wait_turn(
    report: impl FnMut(u64),
    stop: impl Fn() -> Result<(), String>,
) -> Result<(), String> {
    wait_with(
        &LAST_ASKED_MS,
        random_gap_ms(),
        crate::now_ms,
        |ms| std::thread::sleep(std::time::Duration::from_millis(ms)),
        report,
        stop,
    )
}

// Ghi lại là vừa hỏi Douyin xong, kể cả khi lần đó lỗi hay bị dừng giữa chừng: mạng đã bị gọi rồi.
pub(crate) fn note_asked() {
    note_with(&LAST_ASKED_MS, crate::now_ms());
}

// Dòng hiện ở bước tải trong lúc chờ.
pub(crate) fn waiting_line(seconds: u64) -> String {
    format!("Chờ {seconds} giây rồi mới hỏi Douyin, để không bị chặn.")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::sync::Arc;

    const GAP: u64 = 45_000;
    const T: u64 = 1_800_000_000_000;

    // Đồng hồ giả: ngủ bao nhiêu thì đồng hồ chạy bấy nhiêu, test không chờ thật.
    struct Clock {
        now: Cell<u64>,
        slept: Cell<u64>,
    }

    impl Clock {
        fn at(now: u64) -> Self {
            Clock {
                now: Cell::new(now),
                slept: Cell::new(0),
            }
        }

        fn read(&self) -> u64 {
            self.now.get()
        }

        fn sleep(&self, ms: u64) {
            self.now.set(self.now.get() + ms);
            self.slept.set(self.slept.get() + ms);
        }
    }

    fn wait(
        last: Option<u64>,
        clock: &Clock,
        reports: &RefCell<Vec<u64>>,
        stop: impl Fn() -> Result<(), String>,
    ) -> Result<(), String> {
        let state = Mutex::new(last);
        wait_with(
            &state,
            GAP,
            || clock.read(),
            |ms| clock.sleep(ms),
            |seconds| reports.borrow_mut().push(seconds),
            stop,
        )
    }

    #[test]
    fn the_gap_is_between_thirty_and_sixty_seconds_and_follows_the_seed() {
        assert_eq!(gap_ms(0), 30_000);
        assert_eq!(gap_ms(30_000), 60_000);
        assert_eq!(gap_ms(30_001), 30_000);
        assert_eq!(gap_ms(12_345), 42_345);
        for seed in (0..2_000_000u64)
            .step_by(37)
            .chain([u64::MAX - 1, u64::MAX])
        {
            let gap = gap_ms(seed);
            assert!((30_000..=60_000).contains(&gap), "seed {seed} gave {gap}");
        }
    }

    #[test]
    fn the_gap_used_for_real_stays_in_range_and_is_not_always_the_same() {
        let gaps: Vec<u64> = (0..200).map(|_| random_gap_ms()).collect();
        assert!(gaps.iter().all(|gap| (30_000..=60_000).contains(gap)));
        assert!(
            gaps.iter().any(|gap| *gap != gaps[0]),
            "luôn ra cùng một số"
        );
    }

    #[test]
    fn nothing_to_wait_for_before_the_first_ask() {
        assert_eq!(wait_left(None, T, GAP), 0);
    }

    #[test]
    fn the_wait_counts_down_from_the_last_ask() {
        assert_eq!(wait_left(Some(T), T, GAP), 45_000);
        assert_eq!(wait_left(Some(T), T + 10_000, GAP), 35_000);
        assert_eq!(wait_left(Some(T), T + 44_999, GAP), 1);
        assert_eq!(wait_left(Some(T), T + 45_000, GAP), 0);
        assert_eq!(wait_left(Some(T), T + 3_600_000, GAP), 0);
    }

    #[test]
    fn a_clock_set_back_never_makes_the_wait_longer_than_one_gap() {
        assert_eq!(wait_left(Some(T), T - 3_600_000, GAP), 45_000);
        assert_eq!(wait_left(Some(T), 0, GAP), 45_000);
    }

    #[test]
    fn waiting_sleeps_out_the_gap_and_reports_each_second_once() {
        let clock = Clock::at(T + 10_000);
        let reports = RefCell::new(Vec::new());
        assert_eq!(wait(Some(T), &clock, &reports, || Ok(())), Ok(()));
        assert_eq!(clock.slept.get(), 35_000);
        let counting_down: Vec<u64> = (1..=35).rev().collect();
        assert_eq!(*reports.borrow(), counting_down);
    }

    #[test]
    fn no_waiting_when_the_gap_is_over_or_there_was_no_earlier_ask() {
        for last in [None, Some(T)] {
            let clock = Clock::at(T + 60_000);
            let reports = RefCell::new(Vec::new());
            assert_eq!(wait(last, &clock, &reports, || Ok(())), Ok(()));
            assert_eq!(clock.slept.get(), 0);
            assert!(reports.borrow().is_empty());
        }
    }

    #[test]
    fn a_stop_ends_the_wait_without_sleeping_on() {
        let clock = Clock::at(T);
        let reports = RefCell::new(Vec::new());
        let polls = Cell::new(0);
        let stop = || {
            polls.set(polls.get() + 1);
            if polls.get() > 3 {
                Err("Đã dừng.".to_string())
            } else {
                Ok(())
            }
        };
        assert_eq!(
            wait(Some(T), &clock, &reports, stop),
            Err("Đã dừng.".to_string())
        );
        assert_eq!(clock.slept.get(), 3 * 250);
    }

    #[test]
    fn a_stop_already_pressed_blocks_the_ask_even_when_there_is_nothing_to_wait_for() {
        let clock = Clock::at(T);
        let reports = RefCell::new(Vec::new());
        assert_eq!(
            wait(None, &clock, &reports, || Err("Đã dừng.".to_string())),
            Err("Đã dừng.".to_string())
        );
        assert_eq!(clock.slept.get(), 0);
    }

    #[test]
    fn an_ask_noted_now_makes_the_next_one_wait_a_whole_gap() {
        let state = Mutex::new(None);
        note_with(&state, T);
        assert_eq!(*state.lock().unwrap(), Some(T));
        let clock = Clock::at(T);
        let result = wait_with(
            &state,
            GAP,
            || clock.read(),
            |ms| clock.sleep(ms),
            |_| {},
            || Ok(()),
        );
        assert_eq!(result, Ok(()));
        assert_eq!(clock.slept.get(), GAP);
    }

    #[test]
    fn a_later_ask_replaces_the_earlier_mark() {
        let state = Mutex::new(Some(T));
        note_with(&state, T + 100_000);
        assert_eq!(*state.lock().unwrap(), Some(T + 100_000));
    }

    #[test]
    fn a_mark_poisoned_by_a_panic_does_not_stop_downloads() {
        let state = Arc::new(Mutex::new(Some(T)));
        let holder = state.clone();
        let _ = std::thread::spawn(move || {
            let _guard = holder.lock().unwrap();
            panic!("cố ý làm hỏng khóa để thử");
        })
        .join();
        assert!(state.is_poisoned());
        note_with(&state, T + 1);
        let clock = Clock::at(T + 1);
        let result = wait_with(
            &state,
            GAP,
            || clock.read(),
            |ms| clock.sleep(ms),
            |_| {},
            || Ok(()),
        );
        assert_eq!(result, Ok(()));
        assert_eq!(clock.slept.get(), GAP);
    }

    // Hai hàm app dùng thật (đồng hồ thật, ngủ thật, một mốc chung): ghi nhận một lần hỏi rồi chờ lượt thì
    // phải thấy ngay mình còn chờ 30 đến 60 giây. Dừng sau vài nhịp để test không chờ hết cả khoảng nghỉ.
    // Chỉ test này được đụng vào mốc chung, nên không giẫm lên test khác.
    #[test]
    fn the_functions_the_app_uses_share_one_mark_and_really_sleep() {
        note_asked();
        let polls = Cell::new(0);
        let first_report = Cell::new(0);
        let started = std::time::Instant::now();
        let result = wait_turn(
            |seconds| {
                if first_report.get() == 0 {
                    first_report.set(seconds);
                }
            },
            || {
                polls.set(polls.get() + 1);
                if polls.get() > 2 {
                    Err("Đã dừng.".to_string())
                } else {
                    Ok(())
                }
            },
        );
        let waited = started.elapsed();
        *LAST_ASKED_MS.lock().unwrap() = None;
        assert_eq!(result, Err("Đã dừng.".to_string()));
        assert!(
            (30..=60).contains(&first_report.get()),
            "báo {} giây",
            first_report.get()
        );
        assert!(
            waited >= std::time::Duration::from_millis(400),
            "{waited:?}"
        );
        assert!(waited < std::time::Duration::from_secs(10), "{waited:?}");
    }

    #[test]
    fn the_line_shown_while_waiting_names_the_seconds_left() {
        let line = waiting_line(42);
        assert!(line.contains("42 giây"), "{line}");
        assert!(line.contains("Douyin"), "{line}");
    }
}
