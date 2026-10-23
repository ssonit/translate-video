use std::thread;

// Một việc chạy trên luồng riêng. Được mượn dữ liệu của người gọi, nên không cần 'static.
pub(crate) type Job<'a, T> = Box<dyn FnOnce() -> T + Send + 'a>;

// Chạy mọi việc cùng lúc, trả kết quả đúng thứ tự đã đưa vào. Việc nào panic thì lấy
// `fallback(vị trí)` thay, để một mục hỏng không làm mất cả bảng.
pub(crate) fn in_order<'a, T: Send>(jobs: Vec<Job<'a, T>>, fallback: impl Fn(usize) -> T) -> Vec<T> {
    thread::scope(|scope| {
        let handles: Vec<_> = jobs.into_iter().map(|job| scope.spawn(job)).collect();
        handles
            .into_iter()
            .enumerate()
            .map(|(index, handle)| handle.join().unwrap_or_else(|_| fallback(index)))
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn job(wait_ms: u64, value: &str) -> Job<'_, String> {
        Box::new(move || {
            thread::sleep(Duration::from_millis(wait_ms));
            value.to_string()
        })
    }

    #[test]
    fn results_keep_the_order_of_the_jobs_even_when_the_first_one_is_slowest() {
        let out = in_order(vec![job(120, "a"), job(10, "b"), job(60, "c")], |_| "x".into());
        assert_eq!(out, ["a", "b", "c"]);
    }

    #[test]
    fn jobs_run_at_the_same_time() {
        let started = Instant::now();
        let out = in_order(vec![job(300, "a"), job(300, "b"), job(300, "c")], |_| "x".into());
        assert_eq!(out.len(), 3);
        // Chạy nối đuôi thì mất 900 ms. Chừa nhiều chỗ cho máy chậm.
        assert!(
            started.elapsed() < Duration::from_millis(800),
            "took {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn a_job_that_panics_gets_its_fallback_and_the_others_still_report() {
        let broken: Job<String> = Box::new(|| panic!("boom"));
        let out = in_order(vec![job(0, "a"), broken, job(0, "c")], |index| {
            format!("fallback {index}")
        });
        assert_eq!(out, ["a", "fallback 1", "c"]);
    }

    #[test]
    fn jobs_can_borrow_from_the_caller() {
        let words = vec![String::from("one"), String::from("three")];
        let jobs: Vec<Job<usize>> = words
            .iter()
            .map(|word| -> Job<usize> { Box::new(move || word.len()) })
            .collect();
        assert_eq!(in_order(jobs, |_| 0), [3, 5]);
    }
}
