//! Exclusive task ownership. Cancellation requests stop work but do not release
//! its lease; only unwinding/returning from the worker releases the slot.
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Kind {
    FullRun,
    Preview,
    Prepare,
    Edit,
    Export,
    Auxiliary,
}

#[derive(Default)]
struct State {
    next: u64,
    active: Option<(u64, Kind)>,
}

#[derive(Default)]
pub(crate) struct Coordinator {
    state: Mutex<State>,
    pub cancel: AtomicBool,
    pub running: AtomicBool,
    pub captioning: AtomicBool,
}

pub(crate) struct Lease<'a> {
    owner: &'a Coordinator,
    id: u64,
    kind: Kind,
    released: bool,
}

#[derive(Serialize)]
pub(crate) struct Status {
    id: u64,
    kind: Kind,
    stopping: bool,
}

impl Coordinator {
    pub(crate) fn acquire(&self, kind: Kind) -> Result<Lease<'_>, String> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.active.is_some()
            || self.running.load(Ordering::SeqCst)
            || self.captioning.load(Ordering::SeqCst)
        {
            return Err(if self.cancel.load(Ordering::SeqCst) {
                "Đang dừng tác vụ trước. Chờ bộ xử lý kết thúc."
            } else {
                "Đang xử lý một yêu cầu khác."
            }
            .into());
        }
        state.next = state
            .next
            .checked_add(1)
            .ok_or("Mã tác vụ vượt giới hạn.")?;
        let id = state.next;
        self.cancel.store(false, Ordering::SeqCst);
        if kind == Kind::Auxiliary {
            self.captioning.store(true, Ordering::SeqCst);
        } else {
            self.running.store(true, Ordering::SeqCst);
        }
        state.active = Some((id, kind));
        Ok(Lease {
            owner: self,
            id,
            kind,
            released: false,
        })
    }

    pub(crate) fn request_cancel(&self, revoke: impl FnOnce()) {
        self.request_cancel_if(|_| true, revoke);
    }

    pub(crate) fn request_cancel_if(
        &self,
        matches: impl FnOnce(Kind) -> bool,
        revoke: impl FnOnce(),
    ) {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.active.is_some_and(|(_, kind)| matches(kind)) {
            self.cancel.store(true, Ordering::SeqCst);
            // Prevent old revocation from touching a task acquired concurrently.
            revoke();
        }
    }

    pub(crate) fn status(&self) -> Option<Status> {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.active.map(|(id, kind)| Status {
            id,
            kind,
            stopping: self.cancel.load(Ordering::SeqCst),
        })
    }
}

impl Lease<'_> {
    pub(crate) fn release(&mut self) {
        if self.released {
            return;
        }
        let mut state = self.owner.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.active == Some((self.id, self.kind)) {
            state.active = None;
            if self.kind == Kind::Auxiliary {
                self.owner.captioning.store(false, Ordering::SeqCst);
            } else {
                self.owner.running.store(false, Ordering::SeqCst);
            }
        }
        self.released = true;
    }
}
impl Drop for Lease<'_> {
    fn drop(&mut self) {
        self.release();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancelled_task_keeps_ownership_until_its_worker_returns() {
        let tasks = Coordinator::default();
        let lease = tasks.acquire(Kind::Preview).unwrap();
        tasks.request_cancel(|| {});
        assert!(tasks.status().unwrap().stopping);
        assert!(tasks.acquire(Kind::Export).is_err());
        drop(lease);
        let _new = tasks.acquire(Kind::Export).unwrap();
        assert!(!tasks.cancel.load(Ordering::SeqCst));
    }
    #[test]
    fn stale_release_and_panic_do_not_unlock_the_next_task_or_leave_a_busy_slot() {
        let tasks = Coordinator::default();
        let mut old = tasks.acquire(Kind::Prepare).unwrap();
        old.release();
        let next = tasks.acquire(Kind::Edit).unwrap();
        old.release();
        assert!(tasks.acquire(Kind::Auxiliary).is_err());
        drop(next);
        let _ = std::panic::catch_unwind(|| {
            let _lease = tasks.acquire(Kind::FullRun).unwrap();
            panic!("worker panic");
        });
        assert!(tasks.acquire(Kind::Auxiliary).is_ok());
    }
    #[test]
    fn cancellation_revocation_finishes_before_the_next_task_is_acquired() {
        let tasks = Coordinator::default();
        let mut lease = tasks.acquire(Kind::Preview).unwrap();
        std::thread::scope(|scope| {
            let (entered, wait) = std::sync::mpsc::channel();
            let owner = &tasks;
            let cancel = scope.spawn(move || {
                owner.request_cancel(|| {
                    entered.send(()).unwrap();
                    std::thread::sleep(std::time::Duration::from_millis(30));
                })
            });
            wait.recv().unwrap();
            lease.release(); // serialized behind revocation
            let _next = tasks.acquire(Kind::Export).unwrap();
            cancel.join().unwrap();
            assert!(!tasks.cancel.load(Ordering::SeqCst));
        });
    }
}
