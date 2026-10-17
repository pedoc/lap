//! Fair, cancellable inference lane shared by all image-analysis capabilities.
//! Nested calls on the same worker may borrow the lane; callers must not move a lease
//! to another thread and then reacquire it there. Lease IDs prevent stale drops.
use super::types::Task;
use std::{
    collections::VecDeque,
    sync::{Arc, Condvar, LazyLock, Mutex},
    thread::ThreadId,
    time::Duration,
};
#[derive(Clone, Copy)]
struct Owner {
    token: u64,
    thread: ThreadId,
    task: Task,
    depth: usize,
}
#[derive(Default)]
struct State {
    next: u64,
    owner: Option<Owner>,
    waiting: VecDeque<(u64, Task)>,
}
#[derive(Default)]
struct Lane {
    state: Mutex<State>,
    changed: Condvar,
}
static LANE: LazyLock<Arc<Lane>> = LazyLock::new(|| Arc::new(Lane::default()));
pub struct Lease {
    lane: Arc<Lane>,
    token: u64,
    // Reentrancy is worker-local: never keep a lease across an async thread hop.
    _worker_bound: std::marker::PhantomData<std::rc::Rc<()>>,
}
impl Lease {
    fn new(lane: Arc<Lane>, token: u64) -> Self {
        Self {
            lane,
            token,
            _worker_bound: std::marker::PhantomData,
        }
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        if let Ok(mut state) = self.lane.state.lock() {
            if let Some(owner) = state
                .owner
                .as_mut()
                .filter(|owner| owner.token == self.token)
            {
                owner.depth = owner.depth.saturating_sub(1);
                if owner.depth == 0 {
                    state.owner = None;
                    self.lane.changed.notify_all();
                }
            }
        }
    }
}
struct Waiter {
    lane: Arc<Lane>,
    token: u64,
}
impl Drop for Waiter {
    fn drop(&mut self) {
        if let Ok(mut state) = self.lane.state.lock() {
            state.waiting.retain(|(id, _)| *id != self.token);
            self.lane.changed.notify_all();
        }
    }
}
impl Lane {
    fn nested(self: &Arc<Self>, state: &mut State) -> Option<Lease> {
        let owner = state
            .owner
            .as_mut()
            .filter(|owner| owner.thread == std::thread::current().id())?;
        owner.depth += 1;
        Some(Lease::new(self.clone(), owner.token))
    }
    fn try_claim(self: &Arc<Self>, task: Task) -> Result<Lease, String> {
        let mut state = self.state.lock().map_err(|e| e.to_string())?;
        if let Some(lease) = self.nested(&mut state) {
            return Ok(lease);
        }
        if state.owner.is_some() || !state.waiting.is_empty() {
            return Err("Another image-analysis inference is running or queued".into());
        }
        state.next = state.next.wrapping_add(1);
        let token = state.next;
        state.owner = Some(Owner {
            token,
            thread: std::thread::current().id(),
            task,
            depth: 1,
        });
        Ok(Lease::new(self.clone(), token))
    }
    fn claim(
        self: &Arc<Self>,
        task: Task,
        check: impl Fn() -> Result<(), String>,
    ) -> Result<Lease, String> {
        check()?;
        let token = {
            let mut state = self.state.lock().map_err(|e| e.to_string())?;
            if let Some(lease) = self.nested(&mut state) {
                return Ok(lease);
            }
            state.next = state.next.wrapping_add(1);
            let token = state.next;
            state.waiting.push_back((token, task));
            token
        };
        let _waiter = Waiter {
            lane: self.clone(),
            token,
        };
        loop {
            check()?;
            let mut state = self.state.lock().map_err(|e| e.to_string())?;
            if state.owner.is_none() && state.waiting.front().is_some_and(|(id, _)| *id == token) {
                state.waiting.pop_front();
                state.owner = Some(Owner {
                    token,
                    thread: std::thread::current().id(),
                    task,
                    depth: 1,
                });
                return Ok(Lease::new(self.clone(), token));
            }
            drop(
                self.changed
                    .wait_timeout(state, Duration::from_millis(50))
                    .map_err(|e| e.to_string())?,
            );
        }
    }
}
pub fn try_claim(task: Task) -> Result<Lease, String> {
    LANE.try_claim(task)
}
pub fn claim(task: Task, check: impl Fn() -> Result<(), String>) -> Result<Lease, String> {
    LANE.claim(task, check)
}
pub fn busy() -> bool {
    LANE.state
        .lock()
        .map(|s| s.owner.is_some() || !s.waiting.is_empty())
        .unwrap_or(true)
}
pub fn running(task: Task) -> bool {
    LANE.state
        .lock()
        .map(|s| s.owner.is_some_and(|o| o.task == task))
        .unwrap_or(true)
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    #[test]
    fn nested_calls_hold_the_lane_until_the_last_lease_drops() {
        let lane = Arc::new(Lane::default());
        let first = lane.try_claim(Task::Semantic).unwrap();
        let nested = lane.try_claim(Task::Face).unwrap();
        drop(first);
        let other = lane.clone();
        assert!(
            std::thread::spawn(move || other.try_claim(Task::Detection).is_err())
                .join()
                .unwrap()
        );
        drop(nested);
        assert!(lane.try_claim(Task::Detection).is_ok());
    }
    #[test]
    fn waiters_are_fifo_and_try_claim_cannot_jump_the_queue() {
        let lane = Arc::new(Lane::default());
        let held = lane.try_claim(Task::Ocr).unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let mut workers = Vec::new();
        for (index, task) in [(1, Task::Semantic), (2, Task::Detection)] {
            let worker_lane = lane.clone();
            let tx = tx.clone();
            workers.push(std::thread::spawn(move || {
                let _lease = worker_lane.claim(task, || Ok(())).unwrap();
                tx.send(index).unwrap();
            }));
            while lane.state.lock().unwrap().waiting.len() < index {
                std::thread::yield_now();
            }
        }
        let other = lane.clone();
        assert!(
            std::thread::spawn(move || other.try_claim(Task::Face).is_err())
                .join()
                .unwrap()
        );
        drop(held);
        assert_eq!(rx.recv_timeout(Duration::from_secs(2)).unwrap(), 1);
        assert_eq!(rx.recv_timeout(Duration::from_secs(2)).unwrap(), 2);
        workers.into_iter().for_each(|w| w.join().unwrap());
    }
    #[test]
    fn cancelled_waiters_do_not_block_later_work() {
        let lane = Arc::new(Lane::default());
        let held = lane.try_claim(Task::Detection).unwrap();
        let cancelled = Arc::new(AtomicBool::new(false));
        let other = lane.clone();
        let flag = cancelled.clone();
        let worker = std::thread::spawn(move || {
            other
                .claim(Task::Semantic, || {
                    if flag.load(Ordering::SeqCst) {
                        Err("cancelled".into())
                    } else {
                        Ok(())
                    }
                })
                .is_err()
        });
        while lane.state.lock().unwrap().waiting.is_empty() {
            std::thread::yield_now();
        }
        cancelled.store(true, Ordering::SeqCst);
        assert!(worker.join().unwrap());
        drop(held);
        assert!(lane.try_claim(Task::Face).is_ok());
    }
    #[test]
    fn unwinding_releases_an_owned_lane() {
        let lane = Arc::new(Lane::default());
        let other = lane.clone();
        assert!(
            std::thread::spawn(move || {
                let _lease = other.try_claim(Task::Semantic).unwrap();
                panic!("test panic")
            })
            .join()
            .is_err()
        );
        assert!(lane.try_claim(Task::Detection).is_ok());
    }
}
