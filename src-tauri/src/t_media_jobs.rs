//! Window/library-scoped background media operations. Public progress never contains private paths.
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    io::{Read, Write},
    sync::{
        Arc, LazyLock, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tauri::Emitter;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Inspect,
    Move,
    Restore,
    Purge,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum Item {
    Inspection,
    File { file_id: i64, file_path: String },
    Group { group_id: String },
}
impl Item {
    pub fn group(id: &str) -> Self {
        Self::Group {
            group_id: id.into(),
        }
    }
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileInput {
    pub file_id: i64,
    pub file_path: String,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub library_id: String,
    pub job_id: String,
    pub action: Action,
    #[serde(default)]
    pub files: Vec<FileInput>,
    #[serde(default)]
    pub group_ids: Vec<String>,
    #[serde(default)]
    pub from_hidden: bool,
    #[serde(default)]
    pub options: crate::t_media_restore::RestoreOptions,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub library_id: String,
    pub job_id: String,
    pub action: Action,
    pub stage: String,
    pub revision: u64,
    pub created_at: i64,
    pub retry_job_id: Option<String>,
    pub total: usize,
    pub completed: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub skipped: usize,
    pub remaining: usize,
    pub index: usize,
    pub member: usize,
    pub members: usize,
    pub bytes: u64,
    pub total_bytes: u64,
    pub cancel_requested: bool,
    pub cancelled: bool,
    pub finished: bool,
}
#[derive(Debug, Clone, Serialize)]
pub struct Failure {
    pub item: Item,
    pub error: String,
}
struct State {
    summary: Summary,
    remaining: Vec<Item>,
    failures: Vec<Failure>,
    failed_keys: HashSet<String>,
    current: Option<Item>,
    notified: Instant,
}
pub(crate) struct Control {
    cancelled: Arc<AtomicBool>,
    state: Mutex<State>,
    check_library: bool,
    changed_at: Mutex<Instant>,
    #[cfg(test)]
    observe_every_update: bool,
    update: Box<dyn Fn(&Summary) + Send + Sync>,
    changed: Box<dyn Fn() + Send + Sync>,
}
impl Control {
    fn new(
        request: &Request,
        items: Vec<Item>,
        check_library: bool,
        update: impl Fn(&Summary) + Send + Sync + 'static,
        changed: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            check_library,
            changed_at: Mutex::new(Instant::now() - Duration::from_secs(1)),
            #[cfg(test)]
            observe_every_update: false,
            update: Box::new(update),
            changed: Box::new(changed),
            state: Mutex::new(State {
                summary: Summary {
                    library_id: request.library_id.clone(),
                    job_id: request.job_id.clone(),
                    action: request.action,
                    stage: "queued".into(),
                    revision: 0,
                    created_at: chrono::Utc::now().timestamp_millis(),
                    retry_job_id: None,
                    total: items.len(),
                    completed: 0,
                    succeeded: 0,
                    failed: 0,
                    skipped: 0,
                    remaining: items.len(),
                    index: 0,
                    member: 0,
                    members: 0,
                    bytes: 0,
                    total_bytes: 0,
                    cancel_requested: false,
                    cancelled: false,
                    finished: false,
                },
                remaining: items,
                failures: Vec::new(),
                failed_keys: HashSet::new(),
                current: None,
                notified: Instant::now() - Duration::from_secs(1),
            }),
        }
    }
    fn mutate(&self, force: bool, edit: impl FnOnce(&mut State)) {
        #[cfg(test)]
        let force = force || self.observe_every_update;
        let snapshot = if let Ok(mut state) = self.state.lock() {
            edit(&mut state);
            state.summary.revision += 1;
            state.summary.remaining = state.remaining.len();
            if force || state.notified.elapsed() >= Duration::from_millis(150) {
                state.notified = Instant::now();
                Some(state.summary.clone())
            } else {
                None
            }
        } else {
            None
        };
        if let Some(snapshot) = snapshot {
            (self.update)(&snapshot);
        }
    }
    fn check(&self) -> Result<(), String> {
        if self.check_library {
            let library = self.snapshot()?.library_id;
            match crate::t_config::current_library_id() {
                Ok(current) if current == library => {}
                _ => self.cancelled.store(true, Ordering::Release),
            }
        }
        if self.cancelled.load(Ordering::Acquire) {
            Err(
                "Media operation cancelled at a safe boundary; unfinished files can be retried"
                    .into(),
            )
        } else {
            Ok(())
        }
    }
    fn notify_changed(&self, force: bool) {
        let notify = self
            .changed_at
            .lock()
            .map(|mut last| {
                if force || last.elapsed() >= Duration::from_millis(200) {
                    *last = Instant::now();
                    true
                } else {
                    false
                }
            })
            .unwrap_or(false);
        if notify {
            (self.changed)();
        }
    }
    fn snapshot(&self) -> Result<Summary, String> {
        Ok(self
            .state
            .lock()
            .map_err(|e| e.to_string())?
            .summary
            .clone())
    }
    fn select(&self, item: Item) {
        self.mutate(false, |s| {
            s.summary.index = s
                .remaining
                .iter()
                .position(|i| i == &item)
                .map(|i| i + 1)
                .unwrap_or(s.summary.completed + 1);
            s.current = Some(item);
            s.summary.member = 0;
            s.summary.members = 0;
            s.summary.bytes = 0;
            s.summary.total_bytes = 0;
            s.summary.stage = "planning".into();
        });
    }
    fn replace(&self, old: &Item, new: Item) {
        self.mutate(false, |s| {
            if let Some(item) = s.remaining.iter_mut().find(|item| *item == old) {
                *item = new.clone();
            }
            s.current = Some(new);
        });
    }
    fn skip(&self, item: &Item) {
        self.mutate(false, |s| {
            s.remaining.retain(|i| i != item);
            s.summary.completed += 1;
            s.summary.skipped += 1;
        });
    }
    fn complete(&self) {
        self.mutate(false, |s| {
            if let Some(item) = &s.current {
                s.remaining.retain(|i| i != item);
            }
            s.summary.completed += 1;
            s.summary.succeeded += 1;
        });
        self.notify_changed(false);
    }
    fn fail(&self, error: String) {
        if self.cancelled.load(Ordering::Acquire) {
            return;
        }
        self.mutate(false, |s| {
            if s.current
                .as_ref()
                .is_none_or(|item| !s.remaining.contains(item))
            {
                s.current = s.remaining.first().cloned();
            }
            if let Some(item) = &s.current {
                let key = match item {
                    Item::Inspection => "inspect".into(),
                    Item::File { file_id, .. } => format!("f:{file_id}"),
                    Item::Group { group_id } => format!("g:{group_id}"),
                };
                if s.failed_keys.insert(key) {
                    if s.failures.len() < 200 {
                        s.failures.push(Failure {
                            item: item.clone(),
                            error: error.chars().take(4096).collect(),
                        });
                    }
                    s.summary.failed += 1;
                    s.summary.completed += 1;
                }
            }
        });
    }
    fn finish(&self) {
        self.mutate(true, |s| {
            s.summary.finished = true;
            s.summary.cancelled = self.cancelled.load(Ordering::Acquire) && !s.remaining.is_empty();
            s.summary.stage = if s.summary.cancelled {
                "cancelled"
            } else if s.summary.failed > 0 {
                "failed"
            } else {
                "completed"
            }
            .into();
        });
        self.notify_changed(true);
    }
}
thread_local! {static CURRENT:RefCell<Option<Arc<Control>>>=const {RefCell::new(None)};}
struct RestoreContext(Option<Arc<Control>>);
impl Drop for RestoreContext {
    fn drop(&mut self) {
        CURRENT.with(|slot| *slot.borrow_mut() = self.0.take());
    }
}
pub(crate) fn with_control<T>(control: Arc<Control>, run: impl FnOnce() -> T) -> T {
    let old = CURRENT.with(|slot| slot.replace(Some(control)));
    let _restore = RestoreContext(old);
    run()
}
fn current() -> Option<Arc<Control>> {
    CURRENT.with(|slot| slot.borrow().clone())
}
pub(crate) fn check() -> Result<(), String> {
    if let Some(c) = current() {
        c.check()
    } else {
        Ok(())
    }
}
pub(crate) fn stage(name: &str) {
    if let Some(c) = current() {
        c.mutate(false, |s| s.summary.stage = name.into());
    }
}
pub(crate) fn member(index: usize, total: usize) {
    if let Some(c) = current() {
        c.mutate(false, |s| {
            s.summary.member = index;
            s.summary.members = total;
        });
    }
}
pub(crate) fn bytes(name: &str, done: u64, total: u64) {
    if let Some(c) = current() {
        c.mutate(false, |s| {
            s.summary.stage = name.into();
            s.summary.bytes = done;
            s.summary.total_bytes = total;
        });
    }
}
pub(crate) fn select(item: Item) {
    if let Some(c) = current() {
        c.select(item);
    }
}
pub(crate) fn admitted(id: &str) {
    if let Some(control) = current() {
        let old = control
            .state
            .lock()
            .ok()
            .and_then(|state| state.current.clone());
        if let Some(old) = old {
            control.replace(&old, Item::group(id));
            control.notify_changed(false);
        }
    }
}
pub(crate) fn skipped(item: &Item) {
    if let Some(c) = current() {
        c.skip(item);
    }
}
pub(crate) fn complete() {
    if let Some(c) = current() {
        c.complete();
    }
}
pub(crate) fn failed(error: String) {
    if let Some(c) = current() {
        c.fail(error);
    }
}
pub(crate) fn copy(
    input: &mut impl Read,
    output: &mut impl Write,
    total: u64,
) -> Result<u64, String> {
    let mut done = 0;
    let mut buf = [0u8; 64 * 1024];
    loop {
        check()?;
        let n = input.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        output.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        done += n as u64;
        bytes("copying", done, total);
    }
    check()?;
    Ok(done)
}
struct Job {
    request: Request,
    owner: String,
    control: Arc<Control>,
}
static JOBS: LazyLock<Mutex<HashMap<String, Arc<Job>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
pub(crate) fn assert_library_idle(library: &str) -> Result<(), String> {
    let jobs = JOBS.lock().map_err(|e| e.to_string())?;
    if jobs.values().any(|job| {
        job.request.library_id == library && job.control.snapshot().is_ok_and(|s| !s.finished)
    }) {
        return Err("Stop media operations before removing their library/database".into());
    }
    Ok(())
}
fn lookup(library: &str, id: &str, owner: &str) -> Result<Arc<Job>, String> {
    JOBS.lock()
        .map_err(|e| e.to_string())?
        .get(id)
        .filter(|j| j.request.library_id == library && j.owner == owner)
        .cloned()
        .ok_or("Media job is unavailable for this window/library".into())
}
fn validate(request: &Request) -> Result<Vec<Item>, String> {
    uuid::Uuid::parse_str(&request.job_id).map_err(|_| "Invalid media job ID")?;
    let mut items = Vec::new();
    let mut seen = HashMap::new();
    match request.action {
        Action::Inspect => {
            if !request.files.is_empty() || !request.group_ids.is_empty() || request.from_hidden {
                return Err(
                    "Recovery inspection accepts no original files or group selection".into(),
                );
            }
            items.push(Item::Inspection);
        }
        Action::Move => {
            if !request.group_ids.is_empty() {
                // Resuming persisted moves is a private operation, even if the originals were public.
                if !request.from_hidden || !request.files.is_empty() {
                    return Err("Interrupted groups require private admission and no original-file selection".into());
                }
                let mut groups = HashSet::new();
                for id in &request.group_ids {
                    uuid::Uuid::parse_str(id).map_err(|_| "Invalid recovery group ID")?;
                    if groups.insert(id) {
                        items.push(Item::group(id));
                    }
                }
                if items.len() > 100000 {
                    return Err("Select at most 100000 groups".into());
                }
                return Ok(items);
            }
            for file in &request.files {
                if file.file_id <= 0 || (file.file_path.is_empty() && !request.from_hidden) {
                    return Err("Invalid original file".into());
                }
                if let Some(old) = seen.insert(file.file_id, file.file_path.clone()) {
                    if old != file.file_path {
                        return Err("One file ID has conflicting paths".into());
                    }
                    continue;
                }
                items.push(Item::File {
                    file_id: file.file_id,
                    file_path: file.file_path.clone(),
                });
            }
        }
        _ => {
            if !request.files.is_empty() || request.from_hidden {
                return Err("Restore/cleanup accepts recovery groups only".into());
            }
            let mut seen = HashSet::new();
            for id in &request.group_ids {
                uuid::Uuid::parse_str(id).map_err(|_| "Invalid recovery group ID")?;
                if seen.insert(id) {
                    items.push(Item::group(id));
                }
            }
        }
    }
    if items.is_empty() || items.len() > 100000 {
        return Err(
            "Select between 1 and 100000 media items; empty requests never affect a library".into(),
        );
    }
    Ok(items)
}
fn launch(
    app: tauri::AppHandle,
    owner: String,
    mut request: Request,
    items: Vec<Item>,
    conn: crate::t_sqlite::PooledConn,
) -> Result<Summary, String> {
    request.files.clear();
    request.group_ids.clear();
    let event_app = app.clone();
    let event_owner = owner.clone();
    let change_app = app.clone();
    let library = request.library_id.clone();
    let control = Arc::new(Control::new(
        &request,
        items.clone(),
        true,
        move |progress| {
            let _ = event_app.emit_to(event_owner.as_str(), "media-job-progress", progress);
        },
        move || {
            crate::t_media_lifecycle::event(&change_app, &library);
        },
    ));
    let job = Arc::new(Job {
        request: request.clone(),
        owner,
        control: control.clone(),
    });
    {
        let mut jobs = JOBS.lock().map_err(|e| e.to_string())?;
        if jobs.contains_key(&request.job_id) {
            return Err("Media job ID was already used".into());
        }
        if jobs.values().any(|j| {
            j.request.library_id == request.library_id
                && j.control.snapshot().is_ok_and(|s| !s.finished)
        }) {
            return Err("A media operation is already running in this library".into());
        }
        if jobs.len() >= 16 {
            let finished = jobs
                .iter()
                .find(|(_, j)| j.control.snapshot().is_ok_and(|s| s.finished))
                .map(|(id, _)| id.clone());
            if let Some(id) = finished {
                jobs.remove(&id);
            } else {
                return Err("Too many running media operations".into());
            }
        }
        jobs.insert(request.job_id.clone(), job);
    }
    let initial = control.snapshot()?;
    tauri::async_runtime::spawn_blocking(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            with_control(control.clone(), || match request.action {
                Action::Inspect => crate::t_media_lifecycle::inspect_job_on(&conn),
                Action::Move => {
                    crate::t_media_lifecycle::move_job_on(&conn, items, request.from_hidden)
                        .map(|_| ())
                }
                action => crate::t_media_lifecycle::operate_on(
                    &conn,
                    items
                        .into_iter()
                        .filter_map(|i| match i {
                            Item::Group { group_id } => Some(group_id),
                            _ => None,
                        })
                        .collect(),
                    action == Action::Purge,
                    &request.options,
                )
                .map(|_| ()),
            })
        }));
        match result {
            Ok(Err(error)) => control.fail(error),
            Err(_) => control.fail(
                "Unexpected media worker failure; registered recovery files were retained".into(),
            ),
            _ => {}
        }
        control.finish();
    });
    Ok(initial)
}
#[tauri::command]
pub async fn start_media_job(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    request: Request,
    session_token: Option<String>,
) -> Result<Summary, String> {
    let items = validate(&request)?;
    let owner = window.label().to_string();
    let conn = {
        let _scope = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
            .read()
            .map_err(|e| e.to_string())?;
        if crate::t_config::current_library_id()? != request.library_id {
            return Err("Library changed".into());
        }
        if request.action != Action::Move || request.from_hidden {
            crate::t_private_lock::authorize(
                &request.library_id,
                session_token.as_deref(),
                &owner,
            )?;
        }
        crate::t_sqlite::open_conn()?
    };
    launch(app, owner, request, items, conn)
}
#[tauri::command]
pub fn get_media_jobs(
    window: tauri::WebviewWindow,
    library_id: String,
) -> Result<Vec<Summary>, String> {
    if crate::t_config::current_library_id()? != library_id {
        return Err("Library changed".into());
    }
    JOBS.lock()
        .map_err(|e| e.to_string())?
        .values()
        .filter(|j| j.request.library_id == library_id && j.owner == window.label())
        .map(|j| j.control.snapshot())
        .collect()
}
#[tauri::command]
pub fn cancel_media_job(
    window: tauri::WebviewWindow,
    library_id: String,
    job_id: String,
) -> Result<(), String> {
    let job = lookup(&library_id, &job_id, window.label())?;
    job.control.cancelled.store(true, Ordering::Release);
    job.control
        .mutate(true, |s| s.summary.cancel_requested = true);
    Ok(())
}
#[tauri::command]
pub fn get_media_job_failures(
    window: tauri::WebviewWindow,
    library_id: String,
    job_id: String,
    session_token: Option<String>,
) -> Result<Vec<Failure>, String> {
    if crate::t_config::current_library_id()? != library_id {
        return Err("Library changed".into());
    }
    let permit =
        crate::t_private_lock::authorize(&library_id, session_token.as_deref(), window.label())?;
    let job = lookup(&library_id, &job_id, window.label())?;
    let failures = job
        .control
        .state
        .lock()
        .map_err(|e| e.to_string())?
        .failures
        .clone();
    crate::t_private_lock::recheck(&permit)?;
    Ok(failures)
}
#[tauri::command]
pub async fn retry_media_job(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    library_id: String,
    job_id: String,
    new_job_id: String,
    session_token: Option<String>,
) -> Result<Summary, String> {
    uuid::Uuid::parse_str(&new_job_id).map_err(|_| "Invalid media job ID")?;
    let job = lookup(&library_id, &job_id, window.label())?;
    let items = {
        let state = job.control.state.lock().map_err(|e| e.to_string())?;
        if !state.summary.finished || state.summary.retry_job_id.is_some() {
            return Err("Wait for completion, and do not retry an already superseded batch".into());
        }
        if state.remaining.is_empty() {
            return Err("This media job has no unfinished items".into());
        }
        state.remaining.clone()
    };
    let mut request = job.request.clone();
    request.job_id = new_job_id;
    let conn = {
        let _scope = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
            .read()
            .map_err(|e| e.to_string())?;
        if crate::t_config::current_library_id()? != library_id {
            return Err("Library changed".into());
        }
        if request.action != Action::Move
            || request.from_hidden
            || items.iter().any(|i| matches!(i, Item::Group { .. }))
        {
            crate::t_private_lock::authorize(
                &library_id,
                session_token.as_deref(),
                window.label(),
            )?;
        }
        crate::t_sqlite::open_conn()?
    };
    {
        let mut state = job.control.state.lock().map_err(|e| e.to_string())?;
        if state.summary.retry_job_id.is_some() {
            return Err("This batch already has a retry".into());
        }
        state.summary.retry_job_id = Some(request.job_id.clone());
    }
    let launched = launch(app, window.label().into(), request, items, conn);
    job.control.mutate(true, |state| {
        if launched.is_err() {
            state.summary.retry_job_id = None;
        }
    });
    launched
}

#[cfg(test)]
pub(crate) fn test_control(
    items: Vec<Item>,
    cancel: Arc<AtomicBool>,
    update: impl Fn(&Summary) + Send + Sync + 'static,
) -> Arc<Control> {
    let request = Request {
        library_id: "test".into(),
        job_id: uuid::Uuid::new_v4().to_string(),
        action: Action::Move,
        files: vec![],
        group_ids: vec![],
        from_hidden: false,
        options: Default::default(),
    };
    let mut control = Control::new(&request, items, false, update, || {});
    control.cancelled = cancel;
    control.observe_every_update = true;
    Arc::new(control)
}
#[cfg(test)]
pub(crate) fn test_remaining(control: &Arc<Control>) -> Vec<Item> {
    control.state.lock().unwrap().remaining.clone()
}
#[cfg(test)]
mod tests {
    use super::*;
    fn file(id: i64) -> Item {
        Item::File {
            file_id: id,
            file_path: format!("/private/{id}.jpg"),
        }
    }
    #[test]
    fn cancellation_stops_copy_at_a_chunk_boundary_without_changing_the_input() {
        struct Reader {
            bytes: std::io::Cursor<Vec<u8>>,
            cancel: Arc<AtomicBool>,
        }
        impl Read for Reader {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                let n = self.bytes.read(buf)?;
                self.cancel.store(true, Ordering::Release);
                Ok(n)
            }
        }
        let token = Arc::new(AtomicBool::new(false));
        let control = test_control(vec![file(1)], token.clone(), |_| {});
        let data = vec![42; 1024 * 1024];
        let mut reader = Reader {
            bytes: std::io::Cursor::new(data.clone()),
            cancel: token,
        };
        let mut output = Vec::new();
        assert!(
            with_control(control, || copy(
                &mut reader,
                &mut output,
                data.len() as u64
            ))
            .is_err()
        );
        assert!(output.len() <= 64 * 1024);
        assert_eq!(reader.bytes.into_inner(), data);
    }
    #[test]
    fn unfinished_ledger_replaces_registered_files_and_never_retries_successes() {
        let control = test_control(
            vec![file(1), file(2)],
            Arc::new(AtomicBool::new(false)),
            |_| {},
        );
        control.select(file(1));
        control.replace(&file(1), Item::group("registered"));
        control.complete();
        control.select(file(2));
        control.fail("source unavailable".into());
        control.finish();
        let state = control.state.lock().unwrap();
        assert_eq!(state.remaining, vec![file(2)]);
        assert_eq!(state.summary.succeeded, 1);
        assert_eq!(state.summary.failed, 1);
        assert_eq!(state.summary.completed, 2);
        assert_eq!(state.summary.stage, "failed");
    }
    #[test]
    fn contexts_are_nested_thread_local_and_restore_after_unwinding() {
        let outer = test_control(vec![], Arc::new(AtomicBool::new(false)), |_| {});
        let inner = test_control(vec![], Arc::new(AtomicBool::new(true)), |_| {});
        with_control(outer, || {
            assert!(check().is_ok());
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                with_control(inner, || {
                    assert!(check().is_err());
                    panic!("test unwind");
                })
            }));
            assert!(result.is_err());
            assert!(check().is_ok());
        });
        assert!(check().is_ok());
    }
    #[test]
    fn public_progress_never_contains_original_paths_errors_or_session_tokens() {
        let control = test_control(vec![file(1)], Arc::new(AtomicBool::new(false)), |_| {});
        control.select(file(1));
        control.fail("private original /home/user/secret.jpg".into());
        let summary = serde_json::to_string(&control.snapshot().unwrap()).unwrap();
        assert!(!summary.contains("/private"));
        assert!(!summary.contains("secret.jpg"));
        assert!(!summary.contains("filePath"));
        assert!(!summary.contains("session"));
    }
    #[test]
    fn duplicate_requests_and_cross_window_or_library_job_access_are_rejected() {
        let id = uuid::Uuid::new_v4().to_string();
        let request = Request {
            library_id: "A".into(),
            job_id: id.clone(),
            action: Action::Move,
            files: vec![
                FileInput {
                    file_id: 1,
                    file_path: "one".into(),
                },
                FileInput {
                    file_id: 1,
                    file_path: "two".into(),
                },
            ],
            group_ids: vec![],
            from_hidden: false,
            options: Default::default(),
        };
        assert!(validate(&request).is_err());
        let control = test_control(vec![file(1)], Arc::new(AtomicBool::new(false)), |_| {});
        let job = Arc::new(Job {
            request,
            owner: "main".into(),
            control,
        });
        JOBS.lock().unwrap().insert(id.clone(), job);
        assert!(lookup("B", &id, "main").is_err());
        assert!(lookup("A", &id, "other").is_err());
        assert!(lookup("A", &id, "main").is_ok());
        assert!(assert_library_idle("A").is_err());
        assert!(assert_library_idle("B").is_ok());
        lookup("A", &id, "main").unwrap().control.finish();
        assert!(assert_library_idle("A").is_ok());
        JOBS.lock().unwrap().remove(&id);
    }
    #[test]
    fn large_failed_batches_keep_all_retry_targets_but_bound_private_diagnostics() {
        let items = (1..=201).map(file).collect::<Vec<_>>();
        let control = test_control(items.clone(), Arc::new(AtomicBool::new(false)), |_| {});
        for item in items {
            control.select(item);
            control.fail("failure".repeat(1000));
        }
        control.fail("same item again".into());
        let state = control.state.lock().unwrap();
        assert_eq!(state.summary.failed, 201);
        assert_eq!(state.remaining.len(), 201);
        assert_eq!(state.failures.len(), 200);
        assert!(state.failures[0].error.len() <= 4096);
    }
    #[test]
    fn revision_is_monotonic_and_empty_requests_never_expand_to_the_whole_library() {
        let control = test_control(vec![], Arc::new(AtomicBool::new(false)), |_| {});
        let before = control.snapshot().unwrap().revision;
        control.select(file(1));
        assert!(control.snapshot().unwrap().revision > before);
        let request = Request {
            library_id: "A".into(),
            job_id: uuid::Uuid::new_v4().to_string(),
            action: Action::Move,
            files: vec![],
            group_ids: vec![],
            from_hidden: false,
            options: Default::default(),
        };
        assert!(validate(&request).is_err());
    }
}
