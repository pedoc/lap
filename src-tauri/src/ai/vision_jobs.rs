//! Cancellable image-level inference lanes, independent for each recognition task.
//! A lease stays on its worker; nested adapter calls borrow that worker's same-task lease.
use super::types::Task;
use std::{
    collections::VecDeque,
    sync::{
        Arc, Condvar, LazyLock, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread::ThreadId,
    time::Duration,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Priority {
    #[default]
    Normal,
    Interactive,
}
impl Priority {
    pub fn key(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Interactive => "interactive",
        }
    }
}

struct Owner {
    token: u64,
    thread: ThreadId,
    task: Task,
    job_id: Option<String>,
    depth: usize,
}
struct Waiting {
    token: u64,
    priority: Priority,
    job_id: Option<String>,
}
struct Request {
    token: u64,
    job_id: String,
    priority: Priority,
}
#[derive(Default)]
struct State {
    next: u64,
    owner: Option<Owner>,
    waiting: VecDeque<Waiting>,
    requests: VecDeque<Request>,
}
impl State {
    fn active_or_next_request(&self) -> Option<String> {
        if let Some(owner) = &self.owner {
            return Some(owner.job_id.clone().unwrap_or_default());
        }
        self.requests
            .iter()
            .find(|request| request.priority == Priority::Interactive)
            .or_else(|| self.requests.front())
            .map(|request| request.job_id.clone())
            .or_else(|| {
                self.first_waiter().and_then(|token| {
                    self.waiting
                        .iter()
                        .find(|waiter| waiter.token == token)
                        .map(|waiter| waiter.job_id.clone().unwrap_or_default())
                })
            })
    }
    fn first_waiter(&self) -> Option<u64> {
        if let Some(request) = self
            .requests
            .iter()
            .find(|request| request.priority == Priority::Interactive)
        {
            return self
                .waiting
                .iter()
                .find(|waiter| waiter.job_id.as_deref() == Some(request.job_id.as_str()))
                .map(|waiter| waiter.token);
        }
        self.waiting
            .iter()
            .find(|waiter| waiter.priority == Priority::Interactive)
            .or_else(|| self.waiting.front())
            .map(|waiter| waiter.token)
    }
}
#[derive(Default)]
struct Lane {
    state: Mutex<State>,
    changed: Condvar,
}
struct Lanes([Arc<Lane>; 4]);
impl Default for Lanes {
    fn default() -> Self {
        Self(std::array::from_fn(|_| Arc::new(Lane::default())))
    }
}
impl Lanes {
    fn lane(&self, task: Task) -> &Arc<Lane> {
        &self.0[match task {
            Task::Semantic => 0,
            Task::Face => 1,
            Task::Ocr => 2,
            Task::Detection => 3,
        }]
    }
    fn busy(&self) -> bool {
        self.0.iter().any(|lane| {
            lane.state
                .lock()
                .map(|state| {
                    state.owner.is_some() || !state.waiting.is_empty() || !state.requests.is_empty()
                })
                .unwrap_or(true)
        })
    }
    fn try_exclusive(&self) -> Result<ExclusiveLease, String> {
        ensure_admission()?;
        if self.busy() {
            return Err(
                "Recognition is running or queued; wait before changing its runtime".into(),
            );
        }
        let mut leases = Vec::new();
        for task in [Task::Semantic, Task::Face, Task::Ocr, Task::Detection] {
            leases.push(self.lane(task).try_claim(task)?);
        }
        Ok(ExclusiveLease { _leases: leases })
    }
}
static LANES: LazyLock<Lanes> = LazyLock::new(Lanes::default);
static ADMISSION_PAUSES: LazyLock<Arc<AtomicUsize>> =
    LazyLock::new(|| Arc::new(AtomicUsize::new(0)));

pub struct AdmissionPause(Arc<AtomicUsize>);
pub fn pause_admission() -> AdmissionPause {
    pause_counter(ADMISSION_PAUSES.clone())
}
fn pause_counter(counter: Arc<AtomicUsize>) -> AdmissionPause {
    counter.fetch_add(1, Ordering::AcqRel);
    AdmissionPause(counter)
}
impl Drop for AdmissionPause {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}
pub fn ensure_admission() -> Result<(), String> {
    ensure_counter(&ADMISSION_PAUSES)
}
fn ensure_counter(counter: &AtomicUsize) -> Result<(), String> {
    if counter.load(Ordering::Acquire) != 0 {
        Err("Switching libraries; retry recognition after the switch".into())
    } else {
        Ok(())
    }
}

pub struct RequestRegistration {
    lane: Arc<Lane>,
    token: u64,
    active: AtomicBool,
}
impl RequestRegistration {
    pub fn release(&self) {
        if self.active.swap(false, Ordering::AcqRel) {
            if let Ok(mut state) = self.lane.state.lock() {
                state.requests.retain(|request| request.token != self.token);
                self.lane.changed.notify_all();
            }
        }
    }
}
impl Drop for RequestRegistration {
    fn drop(&mut self) {
        self.release();
    }
}
pub fn register_request(
    task: Task,
    priority: Priority,
    job_id: &str,
) -> Result<RequestRegistration, String> {
    LANES.lane(task).register(priority, job_id)
}

pub struct Lease {
    lane: Arc<Lane>,
    token: u64,
    _worker_bound: std::marker::PhantomData<std::rc::Rc<()>>,
}
pub struct ExclusiveLease {
    _leases: Vec<Lease>,
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
            state.waiting.retain(|waiter| waiter.token != self.token);
            self.lane.changed.notify_all();
        }
    }
}
impl Lane {
    fn register(
        self: &Arc<Self>,
        priority: Priority,
        job_id: &str,
    ) -> Result<RequestRegistration, String> {
        ensure_admission()?;
        let mut state = self.state.lock().map_err(|error| error.to_string())?;
        ensure_admission()?;
        if job_id.is_empty()
            || job_id.len() > 256
            || state
                .requests
                .iter()
                .any(|request| request.job_id == job_id)
        {
            return Err("Recognition request ID is invalid or already queued".into());
        }
        if state.requests.len() >= 64 {
            return Err("Too many queued recognition requests of this type".into());
        }
        state.next = state.next.wrapping_add(1);
        let token = state.next;
        state.requests.push_back(Request {
            token,
            job_id: job_id.into(),
            priority,
        });
        self.changed.notify_all();
        Ok(RequestRegistration {
            lane: self.clone(),
            token,
            active: AtomicBool::new(true),
        })
    }
    fn nested(
        self: &Arc<Self>,
        state: &mut State,
        task: Task,
        job_id: Option<&str>,
    ) -> Result<Option<Lease>, String> {
        let Some(owner) = state
            .owner
            .as_mut()
            .filter(|owner| owner.thread == std::thread::current().id())
        else {
            return Ok(None);
        };
        if owner.task != task || job_id.is_some_and(|id| owner.job_id.as_deref() != Some(id)) {
            return Err("Cannot nest a different recognition request in an owned lane".into());
        }
        owner.depth = owner
            .depth
            .checked_add(1)
            .ok_or("Recognition nesting limit exceeded")?;
        Ok(Some(Lease::new(self.clone(), owner.token)))
    }
    fn try_claim(self: &Arc<Self>, task: Task) -> Result<Lease, String> {
        let mut state = self.state.lock().map_err(|error| error.to_string())?;
        if let Some(lease) = self.nested(&mut state, task, None)? {
            return Ok(lease);
        }
        ensure_admission()?;
        if state.owner.is_some()
            || !state.waiting.is_empty()
            || state
                .requests
                .iter()
                .any(|request| request.priority == Priority::Interactive)
        {
            return Err("Another recognition request of this type is running or queued".into());
        }
        state.next = state.next.wrapping_add(1);
        let token = state.next;
        state.owner = Some(Owner {
            token,
            thread: std::thread::current().id(),
            task,
            job_id: None,
            depth: 1,
        });
        Ok(Lease::new(self.clone(), token))
    }
    fn claim(
        self: &Arc<Self>,
        task: Task,
        priority: Priority,
        job_id: Option<&str>,
        check: impl Fn() -> Result<(), String>,
    ) -> Result<Lease, String> {
        check()?;
        let token = {
            let mut state = self.state.lock().map_err(|error| error.to_string())?;
            if let Some(lease) = self.nested(&mut state, task, job_id)? {
                return Ok(lease);
            }
            ensure_admission()?;
            state.next = state.next.wrapping_add(1);
            let token = state.next;
            state.waiting.push_back(Waiting {
                token,
                priority,
                job_id: job_id.map(str::to_owned),
            });
            token
        };
        let _waiter = Waiter {
            lane: self.clone(),
            token,
        };
        loop {
            ensure_admission()?;
            check()?;
            let mut state = self.state.lock().map_err(|error| error.to_string())?;
            if state.owner.is_none() && state.first_waiter() == Some(token) {
                state.waiting.retain(|waiter| waiter.token != token);
                state.owner = Some(Owner {
                    token,
                    thread: std::thread::current().id(),
                    task,
                    job_id: job_id.map(str::to_owned),
                    depth: 1,
                });
                return Ok(Lease::new(self.clone(), token));
            }
            drop(
                self.changed
                    .wait_timeout(state, Duration::from_millis(50))
                    .map_err(|error| error.to_string())?,
            );
        }
    }
}

pub fn claim(task: Task, check: impl Fn() -> Result<(), String>) -> Result<Lease, String> {
    claim_priority(task, Priority::Normal, check)
}
pub fn claim_priority(
    task: Task,
    priority: Priority,
    check: impl Fn() -> Result<(), String>,
) -> Result<Lease, String> {
    LANES.lane(task).claim(task, priority, None, check)
}
pub fn claim_job(
    task: Task,
    priority: Priority,
    job_id: &str,
    check: impl Fn() -> Result<(), String>,
) -> Result<Lease, String> {
    LANES.lane(task).claim(task, priority, Some(job_id), check)
}
pub fn try_claim_exclusive() -> Result<ExclusiveLease, String> {
    LANES.try_exclusive()
}
pub fn busy() -> bool {
    ADMISSION_PAUSES.load(Ordering::Acquire) != 0 || LANES.busy()
}
pub fn running(task: Task) -> bool {
    LANES
        .lane(task)
        .state
        .lock()
        .map(|state| state.owner.is_some())
        .unwrap_or(true)
}
pub fn active_or_next_request(task: Task) -> Option<String> {
    LANES
        .lane(task)
        .state
        .lock()
        .ok()
        .and_then(|state| state.active_or_next_request())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    };
    use std::time::Instant;

    fn wait_for(lane: &Lane, count: usize) {
        let until = Instant::now() + Duration::from_secs(5);
        while lane.state.lock().unwrap().waiting.len() < count {
            assert!(Instant::now() < until, "worker did not join the lane");
            std::thread::yield_now();
        }
    }
    #[test]
    fn different_recognition_types_can_enter_while_face_is_running() {
        let lanes = Arc::new(Lanes::default());
        let _face = lanes.lane(Task::Face).try_claim(Task::Face).unwrap();
        let mut workers = Vec::new();
        for task in [Task::Ocr, Task::Detection] {
            let lanes = lanes.clone();
            let (entered, received) = mpsc::channel();
            let (release, released) = mpsc::channel();
            let worker = std::thread::spawn(move || {
                let _lease = lanes
                    .lane(task)
                    .claim(task, Priority::Normal, None, || Ok(()))
                    .unwrap();
                entered.send(()).unwrap();
                let _ = released.recv();
            });
            received.recv_timeout(Duration::from_secs(5)).unwrap();
            workers.push((worker, release));
        }
        let other = lanes.clone();
        assert!(
            std::thread::spawn(move || other.lane(Task::Face).try_claim(Task::Face).is_err())
                .join()
                .unwrap()
        );
        for (worker, release) in workers {
            release.send(()).unwrap();
            worker.join().unwrap();
        }
    }
    #[test]
    fn interactive_requests_run_before_background_and_each_priority_is_fifo() {
        let lane = Arc::new(Lane::default());
        let held = lane.try_claim(Task::Ocr).unwrap();
        let (sent, received) = mpsc::channel();
        let mut workers = Vec::new();
        for (index, (id, priority)) in [
            ("background-1", Priority::Normal),
            ("selected-1", Priority::Interactive),
            ("selected-2", Priority::Interactive),
            ("background-2", Priority::Normal),
        ]
        .into_iter()
        .enumerate()
        {
            let lane = lane.clone();
            let sender = sent.clone();
            let worker_lane = lane.clone();
            workers.push(std::thread::spawn(move || {
                let _lease = worker_lane
                    .claim(Task::Ocr, priority, Some(id), || Ok(()))
                    .unwrap();
                sender.send(id).unwrap();
            }));
            wait_for(&lane, index + 1);
        }
        let other = lane.clone();
        assert!(
            std::thread::spawn(move || other.try_claim(Task::Ocr).is_err())
                .join()
                .unwrap()
        );
        drop(held);
        for id in ["selected-1", "selected-2", "background-1", "background-2"] {
            assert_eq!(received.recv_timeout(Duration::from_secs(5)).unwrap(), id);
        }
        for worker in workers {
            worker.join().unwrap();
        }
    }
    #[test]
    fn nested_adapter_calls_keep_job_identity_and_ownership_until_the_last_drop() {
        let lane = Arc::new(Lane::default());
        let first = lane
            .claim(Task::Face, Priority::Interactive, Some("selected"), || {
                Ok(())
            })
            .unwrap();
        let nested = lane
            .claim(Task::Face, Priority::Normal, None, || Ok(()))
            .unwrap();
        assert_eq!(
            lane.state
                .lock()
                .unwrap()
                .owner
                .as_ref()
                .unwrap()
                .job_id
                .as_deref(),
            Some("selected")
        );
        assert!(
            lane.claim(Task::Face, Priority::Normal, Some("another"), || Ok(()))
                .is_err()
        );
        drop(first);
        let other = lane.clone();
        assert!(
            std::thread::spawn(move || other.try_claim(Task::Face).is_err())
                .join()
                .unwrap()
        );
        drop(nested);
        assert!(lane.try_claim(Task::Face).is_ok());
    }
    #[test]
    fn cancelled_priority_waiters_are_removed_without_blocking_later_work() {
        let lane = Arc::new(Lane::default());
        let held = lane.try_claim(Task::Detection).unwrap();
        let cancelled = Arc::new(AtomicBool::new(false));
        let other = lane.clone();
        let flag = cancelled.clone();
        let worker = std::thread::spawn(move || {
            other
                .claim(
                    Task::Detection,
                    Priority::Interactive,
                    Some("cancelled"),
                    || {
                        if flag.load(Ordering::SeqCst) {
                            Err("cancelled".into())
                        } else {
                            Ok(())
                        }
                    },
                )
                .is_err()
        });
        wait_for(&lane, 1);
        cancelled.store(true, Ordering::SeqCst);
        assert!(worker.join().unwrap());
        assert!(lane.state.lock().unwrap().waiting.is_empty());
        drop(held);
        assert!(lane.try_claim(Task::Detection).is_ok());
    }
    #[test]
    fn unwinding_releases_a_lane_without_affecting_other_types() {
        let lanes = Arc::new(Lanes::default());
        let other = lanes.clone();
        assert!(
            std::thread::spawn(move || {
                let _lease = other.lane(Task::Face).try_claim(Task::Face).unwrap();
                panic!("test panic");
            })
            .join()
            .is_err()
        );
        assert!(lanes.lane(Task::Face).try_claim(Task::Face).is_ok());
        assert!(lanes.lane(Task::Ocr).try_claim(Task::Ocr).is_ok());
    }
    #[test]
    fn runtime_changes_still_require_all_lanes_to_be_idle() {
        let lanes = Arc::new(Lanes::default());
        let held = lanes
            .lane(Task::Detection)
            .try_claim(Task::Detection)
            .unwrap();
        assert!(lanes.try_exclusive().is_err());
        drop(held);
        let registered = lanes
            .lane(Task::Ocr)
            .register(Priority::Normal, "preparing")
            .unwrap();
        assert!(lanes.try_exclusive().is_err());
        drop(registered);
        let exclusive = lanes.try_exclusive().unwrap();
        let other = lanes.clone();
        assert!(
            std::thread::spawn(move || other.lane(Task::Ocr).try_claim(Task::Ocr).is_err())
                .join()
                .unwrap()
        );
        drop(exclusive);
        assert!(!lanes.busy());
    }
    #[test]
    fn interactive_demand_stays_ahead_between_loading_and_each_image() {
        let lane = Arc::new(Lane::default());
        let request = lane.register(Priority::Interactive, "selected").unwrap();
        let loading = lane
            .claim(Task::Face, Priority::Interactive, Some("selected"), || {
                Ok(())
            })
            .unwrap();
        let other = lane.clone();
        let (sent, received) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let _lease = other
                .claim(Task::Face, Priority::Normal, Some("background"), || Ok(()))
                .unwrap();
            sent.send(()).unwrap();
        });
        wait_for(&lane, 1);
        drop(loading);
        assert!(
            lane.state.lock().unwrap().first_waiter().is_none(),
            "background must wait while selected images are being prepared"
        );
        let image = lane
            .claim(Task::Face, Priority::Interactive, Some("selected"), || {
                Ok(())
            })
            .unwrap();
        drop(image);
        assert!(
            lane.state.lock().unwrap().first_waiter().is_none(),
            "background must not take the gap before the next selected image"
        );
        request.release();
        request.release();
        received.recv_timeout(Duration::from_secs(5)).unwrap();
        worker.join().unwrap();
    }
    #[test]
    fn registered_interactive_requests_preserve_request_fifo_and_release_on_drop() {
        let lane = Arc::new(Lane::default());
        let first = lane.register(Priority::Interactive, "first").unwrap();
        let second = lane.register(Priority::Interactive, "second").unwrap();
        let other = lane.clone();
        let (sent, received) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let _lease = other
                .claim(Task::Ocr, Priority::Interactive, Some("second"), || Ok(()))
                .unwrap();
            sent.send(()).unwrap();
        });
        wait_for(&lane, 1);
        assert!(lane.state.lock().unwrap().first_waiter().is_none());
        let current = lane
            .claim(Task::Ocr, Priority::Interactive, Some("first"), || Ok(()))
            .unwrap();
        drop(current);
        drop(first);
        received.recv_timeout(Duration::from_secs(5)).unwrap();
        worker.join().unwrap();
        drop(second);
        assert!(lane.state.lock().unwrap().requests.is_empty());
    }
    #[test]
    fn library_switch_admission_is_nested_and_reopens_when_guards_drop() {
        let counter = Arc::new(AtomicUsize::new(0));
        let first = pause_counter(counter.clone());
        let second = pause_counter(counter.clone());
        assert!(ensure_counter(&counter).is_err());
        drop(first);
        assert!(ensure_counter(&counter).is_err());
        drop(second);
        assert!(ensure_counter(&counter).is_ok());
    }
    #[test]
    fn progress_identity_includes_preparing_requests_and_anonymous_owners() {
        let lane = Arc::new(Lane::default());
        let held = lane.try_claim(Task::Ocr).unwrap();
        let normal = lane.register(Priority::Normal, "background").unwrap();
        let selected = lane
            .register(Priority::Interactive, "roi:selected")
            .unwrap();
        assert_eq!(
            lane.state
                .lock()
                .unwrap()
                .active_or_next_request()
                .as_deref(),
            Some("")
        );
        drop(held);
        assert_eq!(
            lane.state
                .lock()
                .unwrap()
                .active_or_next_request()
                .as_deref(),
            Some("roi:selected")
        );
        drop(selected);
        assert_eq!(
            lane.state
                .lock()
                .unwrap()
                .active_or_next_request()
                .as_deref(),
            Some("background")
        );
        drop(normal);
        assert!(
            lane.state
                .lock()
                .unwrap()
                .active_or_next_request()
                .is_none()
        );
    }
}
