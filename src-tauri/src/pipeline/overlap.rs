// One bounded speech consumer overlaps translation. No detached threads survive
// a failure/cancel, and the original error is retained when the sibling stops.
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, TrySendError};
use std::thread;
use std::time::Duration;

use crate::job::{file_ready, Job, TranslatedLine};
use crate::speech::{clean_for_speech, is_speakable};
use std::path::PathBuf;

#[derive(Debug, PartialEq)]
pub(super) struct Failure {
    pub step: &'static str,
    pub message: String,
}

pub(super) fn run<T, B: Send>(
    cancel: &AtomicBool,
    produce: impl FnOnce(&mut dyn FnMut(B) -> Result<(), String>) -> Result<T, String>,
    mut consume: impl FnMut(B) -> Result<(), String> + Send,
    abort: impl Fn() + Sync,
) -> Result<T, Failure> {
    thread::scope(|scope| {
        let (sender, receiver) = sync_channel::<B>(2);
        let abort = &abort;
        let worker = scope.spawn(move || -> Result<(), String> {
            for batch in receiver {
                super::stopped(cancel)?;
                if let Err(error) = consume(batch) {
                    cancel.store(true, Ordering::SeqCst);
                    abort();
                    return Err(error);
                }
            }
            Ok(())
        });
        let produced = produce(&mut |mut batch| loop {
            super::stopped(cancel)?;
            match sender.try_send(batch) {
                Ok(()) => return Ok(()),
                Err(TrySendError::Full(value)) => {
                    batch = value;
                    thread::sleep(Duration::from_millis(20));
                }
                Err(TrySendError::Disconnected(_)) => return Err("Bộ đọc giọng đã dừng.".into()),
            }
        });
        if produced.is_err() {
            cancel.store(true, Ordering::SeqCst);
            abort();
        }
        drop(sender);
        let spoken = worker
            .join()
            .unwrap_or_else(|_| Err("Bộ đọc giọng bị lỗi.".into()));
        match (produced, spoken) {
            (Err(message), Err(speech))
                if message == "Đã dừng." || message == "Bộ đọc giọng đã dừng." =>
            {
                Err(Failure {
                    step: "speak",
                    message: speech,
                })
            }
            (Err(message), _) => Err(Failure {
                step: "translate",
                message,
            }),
            (_, Err(message)) => Err(Failure {
                step: "speak",
                message,
            }),
            (Ok(value), Ok(())) => Ok(value),
        }
    })
}

// Blank translations are unresolved, not silent yet. Only the final speak pass
// creates silence for sentences that still failed after every retry.
pub(super) struct ReadyLines {
    sent: Vec<bool>,
}

impl ReadyLines {
    pub fn new(total: usize) -> Self {
        Self {
            sent: vec![false; total],
        }
    }

    pub fn take(&mut self, job: &Job, lines: &[TranslatedLine]) -> Vec<(usize, String, PathBuf)> {
        lines
            .iter()
            .enumerate()
            .filter_map(|(index, line)| {
                if self.sent[index] || line.translation.trim().is_empty() {
                    return None;
                }
                let path = job.line_wav(index + 1);
                if file_ready(&path) {
                    return None;
                }
                let text = clean_for_speech(&line.translation);
                if !is_speakable(&text) {
                    return None;
                }
                self.sent[index] = true;
                Some((index, text, path))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{mpsc, Mutex};

    #[test]
    fn speech_starts_before_translation_finishes_and_order_is_preserved() {
        let cancel = AtomicBool::new(false);
        let (started, wait) = mpsc::channel();
        let seen = Mutex::new(Vec::new());
        let result = run(
            &cancel,
            |send| {
                send(1)?;
                wait.recv_timeout(Duration::from_secs(2))
                    .map_err(|e| e.to_string())?;
                send(2)?;
                Ok(42)
            },
            |batch| {
                seen.lock().unwrap().push(batch);
                if batch == 1 {
                    started.send(()).unwrap();
                }
                Ok(())
            },
            || {},
        );
        assert_eq!(result, Ok(42));
        assert_eq!(*seen.lock().unwrap(), vec![1, 2]);
    }

    #[test]
    fn speech_failure_unblocks_a_full_queue_and_keeps_the_error() {
        let cancel = AtomicBool::new(false);
        let result = run(
            &cancel,
            |send| {
                for number in 0..100 {
                    send(number)?;
                }
                Ok(())
            },
            |_| Err("Không đọc được giọng.".into()),
            || {},
        );
        assert_eq!(
            result,
            Err(Failure {
                step: "speak",
                message: "Không đọc được giọng.".into()
            })
        );
    }

    #[test]
    fn translation_error_stops_and_joins_the_active_speaker() {
        let cancel = AtomicBool::new(false);
        let (started, wait) = mpsc::channel();
        let joined = AtomicBool::new(false);
        let result = run(
            &cancel,
            |send| {
                send(1)?;
                wait.recv_timeout(Duration::from_secs(2)).unwrap();
                Err::<(), _>("Dịch vụ dịch lỗi.".into())
            },
            |_| {
                started.send(()).unwrap();
                while !cancel.load(Ordering::SeqCst) {
                    thread::yield_now();
                }
                joined.store(true, Ordering::SeqCst);
                Err("Đã dừng.".into())
            },
            || {},
        );
        assert!(joined.load(Ordering::SeqCst));
        assert_eq!(
            result,
            Err(Failure {
                step: "translate",
                message: "Dịch vụ dịch lỗi.".into()
            })
        );
    }

    #[test]
    fn unresolved_lines_wait_and_retry_results_keep_the_original_index() {
        let job = Job {
            root: std::env::temp_dir().join("overlap-no-wavs"),
            video: PathBuf::new(),
            picture: PathBuf::new(),
            target: "vi".into(),
        };
        let mut lines = vec![
            TranslatedLine {
                start: 0.0,
                end: 1.0,
                source: "hello".into(),
                translation: String::new()
            };
            3
        ];
        lines[2].translation = "Xin chào".into();
        let mut ready = ReadyLines::new(3);
        assert_eq!(
            ready
                .take(&job, &lines)
                .iter()
                .map(|x| x.0)
                .collect::<Vec<_>>(),
            vec![2]
        );
        assert!(ready.take(&job, &lines).is_empty());
        lines[0].translation = "Tạm biệt".into();
        assert_eq!(ready.take(&job, &lines)[0].0, 0);
    }
}
