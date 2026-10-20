//! Optional UI privacy lock, not media encryption or protection from the OS account owner.
use base64::{Engine, engine::general_purpose::STANDARD};
use rand::RngCore;
use ring::pbkdf2;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    io::{Read, Seek, SeekFrom, Write},
    num::NonZeroU32,
    path::{Path, PathBuf},
    sync::{Arc, LazyLock, Mutex},
    time::{Duration, Instant, SystemTime},
};
use tauri::{Emitter, Manager};
const LOCKED: &str = "Private view locked or session expired";
const ITERATIONS: u32 = 600_000;
const MAX_IMAGE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_VIDEO_CHUNK: u64 = 4 * 1024 * 1024;
const MAX_LEASES: usize = 32;

#[derive(Clone, Serialize, Deserialize)]
struct PasswordRecord {
    salt: String,
    verifier: String,
}
impl PasswordRecord {
    fn create(password: &str) -> Result<Self, String> {
        if !(8..=128).contains(&password.chars().count()) {
            return Err("Use a password of 8–128 characters".into());
        }
        let mut salt = [0u8; 32];
        rand::rngs::OsRng
            .try_fill_bytes(&mut salt)
            .map_err(|_| "Secure random generator unavailable")?;
        let mut verifier = [0u8; 32];
        pbkdf2::derive(
            pbkdf2::PBKDF2_HMAC_SHA256,
            NonZeroU32::new(ITERATIONS).ok_or("Invalid password policy")?,
            &salt,
            password.as_bytes(),
            &mut verifier,
        );
        Ok(Self {
            salt: STANDARD.encode(salt),
            verifier: STANDARD.encode(verifier),
        })
    }
    fn verify(&self, password: &str) -> bool {
        if password.len() > 512 {
            return false;
        }
        let (Ok(salt), Ok(verifier)) =
            (STANDARD.decode(&self.salt), STANDARD.decode(&self.verifier))
        else {
            return false;
        };
        if salt.len() != 32 || verifier.len() != 32 {
            return false;
        }
        let Some(iterations) = NonZeroU32::new(ITERATIONS) else {
            return false;
        };
        pbkdf2::verify(
            pbkdf2::PBKDF2_HMAC_SHA256,
            iterations,
            &salt,
            password.as_bytes(),
            &verifier,
        )
        .is_ok()
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
struct Config {
    version: u32,
    password: Option<PasswordRecord>,
    idle_minutes: u32,
    lock_on_blur: bool,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            version: 1,
            password: None,
            idle_minutes: 5,
            lock_on_blur: true,
        }
    }
}
impl Config {
    fn validate(&self) -> Result<(), String> {
        if self.version != 1 || !(1..=60).contains(&self.idle_minutes) {
            return Err("Invalid private lock configuration".into());
        }
        if let Some(record) = &self.password {
            if STANDARD
                .decode(&record.salt)
                .map_or(true, |v| v.len() != 32)
                || STANDARD
                    .decode(&record.verifier)
                    .map_or(true, |v| v.len() != 32)
            {
                return Err("Invalid private password verifier".into());
            }
        }
        Ok(())
    }
}
struct Session {
    token: String,
    library: String,
    owner: String,
    deadline: Instant,
    wall_deadline: SystemTime,
    last_wall: SystemTime,
}
#[derive(Clone)]
pub(crate) struct Permit {
    epoch: u64,
    pub(crate) library: String,
    owner: String,
    token: Option<String>,
}
#[derive(Clone)]
pub(crate) struct MediaRef {
    pub(crate) file_id: i64,
    pub(crate) mode: String,
    pub(crate) group_id: Option<String>,
}
#[derive(Clone)]
struct Lease {
    permit: Permit,
    media: MediaRef,
    path: PathBuf,
    bytes: Option<Arc<Vec<u8>>>,
    mime: String,
    len: u64,
    modified: SystemTime,
}
struct LockState {
    config: Config,
    initialized: bool,
    load_error: Option<String>,
    epoch: u64,
    session: Option<Session>,
    failures: u32,
    cooldown: Option<Instant>,
    leases: HashMap<String, Lease>,
}
impl Default for LockState {
    fn default() -> Self {
        Self {
            config: Config::default(),
            initialized: false,
            load_error: None,
            epoch: 0,
            session: None,
            failures: 0,
            cooldown: None,
            leases: HashMap::new(),
        }
    }
}
static STATE: LazyLock<Mutex<LockState>> = LazyLock::new(|| Mutex::new(LockState::default()));
impl LockState {
    fn revoke(&mut self) {
        self.epoch = self.epoch.wrapping_add(1);
        self.session = None;
        self.leases.clear();
    }
    fn expire(&mut self, now: Instant, wall: SystemTime) {
        if self.config.password.is_some()
            && self
                .session
                .as_ref()
                .is_some_and(|s| now >= s.deadline || wall >= s.wall_deadline || wall < s.last_wall)
        {
            self.revoke();
        }
    }
    fn check(
        &mut self,
        library: &str,
        token: Option<&str>,
        owner: &str,
        touch: bool,
        now: Instant,
        wall: SystemTime,
    ) -> Result<Permit, String> {
        if let Some(error) = &self.load_error {
            return Err(error.clone());
        }
        self.expire(now, wall);
        if self.config.password.is_some() {
            let session = self.session.as_mut().ok_or(LOCKED)?;
            if session.library != library
                || session.owner != owner
                || Some(session.token.as_str()) != token
            {
                return Err(LOCKED.into());
            }
            if touch {
                let ttl = Duration::from_secs(self.config.idle_minutes as u64 * 60);
                session.deadline = now + ttl;
                session.wall_deadline = wall + ttl;
                session.last_wall = wall;
            }
        }
        Ok(Permit {
            epoch: self.epoch,
            library: library.into(),
            owner: owner.into(),
            token: token.map(str::to_string),
        })
    }
    fn still_valid(&mut self, permit: &Permit) -> Result<(), String> {
        self.check(
            &permit.library,
            permit.token.as_deref(),
            &permit.owner,
            false,
            Instant::now(),
            SystemTime::now(),
        )?;
        if permit.epoch != self.epoch {
            return Err(LOCKED.into());
        }
        Ok(())
    }
    fn password_ok(&mut self, password: &str, now: Instant) -> Result<(), String> {
        if self.cooldown.is_some_and(|deadline| now < deadline) {
            return Err("Too many password attempts. Wait 30 seconds before retrying".into());
        }
        if !self
            .config
            .password
            .as_ref()
            .is_some_and(|p| p.verify(password))
        {
            self.failures += 1;
            if self.failures >= 5 {
                self.cooldown = Some(now + Duration::from_secs(30));
                self.failures = 0;
            }
            return Err("Incorrect password".into());
        }
        self.failures = 0;
        self.cooldown = None;
        Ok(())
    }
    fn unlock(
        &mut self,
        library: &str,
        owner: &str,
        password: &str,
        now: Instant,
        wall: SystemTime,
    ) -> Result<(), String> {
        self.password_ok(password, now)?;
        self.revoke();
        let ttl = Duration::from_secs(self.config.idle_minutes as u64 * 60);
        self.session = Some(Session {
            token: format!(
                "{}{}",
                uuid::Uuid::new_v4().simple(),
                uuid::Uuid::new_v4().simple()
            ),
            library: library.into(),
            owner: owner.into(),
            deadline: now + ttl,
            wall_deadline: wall + ttl,
            last_wall: wall,
        });
        Ok(())
    }
}
fn config_path() -> Result<PathBuf, String> {
    Ok(crate::t_config::get_app_data_dir()?.join("private-lock.json"))
}
pub(crate) fn validate_backup(source: &str) -> Result<(), String> {
    serde_json::from_str::<Config>(source)
        .map_err(|e| format!("Invalid private lock configuration: {e}"))?
        .validate()
}
fn state() -> Result<std::sync::MutexGuard<'static, LockState>, String> {
    let mut state = STATE.lock().map_err(|_| "Private lock state unavailable")?;
    if !state.initialized {
        let result = (|| {
            let path = config_path()?;
            match fs::read(&path) {
                Ok(bytes) => {
                    let config: Config = serde_json::from_slice(&bytes).map_err(
                        |_| "Private lock configuration is damaged; no private access was granted",
                    )?;
                    config.validate()?;
                    Ok(config)
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
                Err(_) => Err(
                    "Cannot read private lock configuration; no private access was granted".into(),
                ),
            }
        })();
        match result {
            Ok(config) => state.config = config,
            Err(error) => state.load_error = Some(error),
        }
        state.initialized = true;
    }
    if let Some(error) = &state.load_error {
        return Err(error.clone());
    }
    Ok(state)
}
fn persist(path: &Path, config: &Config) -> Result<(), String> {
    config.validate()?;
    let parent = path
        .parent()
        .ok_or("Missing lock configuration directory")?;
    fs::create_dir_all(parent).map_err(|_| "Cannot create lock configuration directory")?;
    let temp = parent.join(format!(".private-lock-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|_| "Cannot create private lock configuration")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(fs::Permissions::from_mode(0o600))
                .map_err(|_| "Cannot secure private lock configuration")?;
        }
        file.write_all(
            &serde_json::to_vec(config)
                .map_err(|_| "Cannot serialize private lock configuration")?,
        )
        .map_err(|_| "Cannot write private lock configuration")?;
        file.sync_all()
            .map_err(|_| "Cannot flush private lock configuration")?;
        drop(file);
        fs::rename(&temp, path).map_err(|_| "Cannot atomically save private lock configuration")?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}
pub(crate) fn authorize(library: &str, token: Option<&str>, owner: &str) -> Result<Permit, String> {
    state()?.check(
        library,
        token,
        owner,
        true,
        Instant::now(),
        SystemTime::now(),
    )
}
pub(crate) fn recheck(permit: &Permit) -> Result<(), String> {
    if crate::t_config::current_library_id()? != permit.library {
        return Err(LOCKED.into());
    }
    state()?.still_valid(permit)
}
pub(crate) fn library_changed() {
    if let Ok(mut s) = STATE.lock() {
        s.revoke();
    }
}
pub(crate) fn window_lock(app: &tauri::AppHandle, blur: bool) {
    if let Ok(mut s) = state() {
        if !blur || (s.config.password.is_some() && s.config.lock_on_blur) {
            s.revoke();
            let _ = app.emit("private-lock-changed", ());
        }
    }
}
pub(crate) fn start_auto_lock(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut timer = tokio::time::interval(Duration::from_secs(1));
        loop {
            timer.tick().await;
            let changed = {
                let Ok(mut state) = STATE.try_lock() else {
                    continue;
                };
                let before = state.epoch;
                state.expire(Instant::now(), SystemTime::now());
                before != state.epoch
            };
            if changed {
                let _ = app.emit("private-lock-changed", ());
            }
        }
    });
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    enabled: bool,
    locked: bool,
    idle_minutes: u32,
    lock_on_blur: bool,
    retry_after: u64,
    session_token: Option<String>,
}
fn status_on(
    s: &mut LockState,
    library: &str,
    token: Option<&str>,
    owner: &str,
    include_token: bool,
) -> Status {
    let now = Instant::now();
    let locked = s
        .check(library, token, owner, false, now, SystemTime::now())
        .is_err();
    Status {
        enabled: s.config.password.is_some(),
        locked,
        idle_minutes: s.config.idle_minutes,
        lock_on_blur: s.config.lock_on_blur,
        retry_after: s
            .cooldown
            .filter(|d| now < *d)
            .map(|d| d.saturating_duration_since(now).as_secs() + 1)
            .unwrap_or(0),
        session_token: if include_token {
            s.session.as_ref().map(|v| v.token.clone())
        } else {
            None
        },
    }
}
fn current_scope(library: &str) -> Result<(), String> {
    if crate::t_config::current_library_id()? != library {
        return Err("Library changed".into());
    }
    Ok(())
}
#[tauri::command]
pub async fn private_lock_status(
    window: tauri::WebviewWindow,
    library_id: String,
    session_token: Option<String>,
    begin_view: Option<bool>,
) -> Result<Status, String> {
    let owner = window.label().to_string();
    tauri::async_runtime::spawn_blocking(move || {
        let _library = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
            .read()
            .map_err(|_| LOCKED)?;
        current_scope(&library_id)?;
        let mut s = state()?;
        let begin = begin_view.unwrap_or(false) && s.config.password.is_none();
        if begin {
            s.revoke();
            let now = Instant::now();
            let wall = SystemTime::now();
            let ttl = Duration::from_secs(3600);
            s.session = Some(Session {
                token: format!(
                    "{}{}",
                    uuid::Uuid::new_v4().simple(),
                    uuid::Uuid::new_v4().simple()
                ),
                library: library_id.clone(),
                owner: owner.clone(),
                deadline: now + ttl,
                wall_deadline: wall + ttl,
                last_wall: wall,
            });
        }
        Ok(status_on(
            &mut s,
            &library_id,
            session_token.as_deref(),
            &owner,
            begin,
        ))
    })
    .await
    .map_err(|_| "Cannot check private lock")?
}
#[tauri::command]
pub async fn unlock_private_view(
    window: tauri::WebviewWindow,
    library_id: String,
    password: String,
) -> Result<Status, String> {
    let owner = window.label().to_string();
    tauri::async_runtime::spawn_blocking(move || {
        let _library = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
            .read()
            .map_err(|_| LOCKED)?;
        current_scope(&library_id)?;
        let mut s = state()?;
        s.unlock(
            &library_id,
            &owner,
            &password,
            Instant::now(),
            SystemTime::now(),
        )?;
        let token = s.session.as_ref().map(|v| v.token.clone());
        Ok(status_on(
            &mut s,
            &library_id,
            token.as_deref(),
            &owner,
            true,
        ))
    })
    .await
    .map_err(|_| "Cannot unlock private view")?
}
#[tauri::command]
pub async fn configure_private_lock(
    window: tauri::WebviewWindow,
    library_id: String,
    current_password: String,
    new_password: Option<String>,
    enabled: bool,
    idle_minutes: u32,
    lock_on_blur: bool,
) -> Result<(), String> {
    let app = window.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _library = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
            .read()
            .map_err(|_| LOCKED)?;
        current_scope(&library_id)?;
        let mut s = state()?;
        if s.config.password.is_some() {
            s.password_ok(&current_password, Instant::now())?;
        }
        let password = if enabled {
            match new_password.filter(|v| !v.is_empty()) {
                Some(value) => Some(PasswordRecord::create(&value)?),
                None => Some(
                    s.config
                        .password
                        .clone()
                        .ok_or("Set a password before enabling the private lock")?,
                ),
            }
        } else {
            None
        };
        let config = Config {
            version: 1,
            password,
            idle_minutes,
            lock_on_blur,
        };
        persist(&config_path()?, &config)?;
        s.config = config;
        s.revoke();
        let _ = app.emit("private-lock-changed", ());
        Ok(())
    })
    .await
    .map_err(|_| "Cannot configure private lock")?
}
#[tauri::command]
pub async fn lock_private_view(
    window: tauri::WebviewWindow,
    library_id: String,
    session_token: Option<String>,
) -> Result<(), String> {
    let owner = window.label().to_string();
    let app = window.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _library = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
            .read()
            .map_err(|_| LOCKED)?;
        if crate::t_config::current_library_id()? != library_id {
            return Ok(());
        }
        let mut s = state()?;
        if s.session.as_ref().is_some_and(|v| {
            v.library != library_id
                || v.owner != owner
                || Some(v.token.as_str()) != session_token.as_deref()
        }) {
            return Ok(());
        }
        s.revoke();
        let _ = app.emit("private-lock-changed", ());
        Ok(())
    })
    .await
    .map_err(|_| "Cannot lock private view")?
}
#[tauri::command]
pub async fn touch_private_view(
    window: tauri::WebviewWindow,
    library_id: String,
    session_token: Option<String>,
) -> Result<(), String> {
    let owner = window.label().to_string();
    tauri::async_runtime::spawn_blocking(move || {
        let _library = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
            .read()
            .map_err(|_| LOCKED)?;
        current_scope(&library_id)?;
        authorize(&library_id, session_token.as_deref(), &owner)?;
        Ok(())
    })
    .await
    .map_err(|_| "Cannot renew private view activity")?
}

pub(crate) fn issue(
    permit: &Permit,
    media: MediaRef,
    path: PathBuf,
    bytes: Option<Vec<u8>>,
    mime: String,
) -> Result<String, String> {
    let metadata = fs::symlink_metadata(&path).map_err(|_| "Private media unavailable")?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || path
            .canonicalize()
            .map_err(|_| "Private media unavailable")?
            != path
    {
        return Err("Private media path changed".into());
    }
    let mut s = state()?;
    s.still_valid(permit)?;
    if s.leases.len() >= MAX_LEASES {
        s.leases.clear();
    } // A viewer uses at most the current primary/companion and one converted video.
    let memory: u64 = s
        .leases
        .values()
        .filter_map(|v| v.bytes.as_ref())
        .map(|v| v.len() as u64)
        .sum();
    if bytes
        .as_ref()
        .is_some_and(|v| memory + v.len() as u64 > MAX_IMAGE_BYTES)
    {
        return Err("Private preview exceeds memory limit".into());
    }
    let ticket = uuid::Uuid::new_v4().simple().to_string();
    s.leases.insert(
        ticket.clone(),
        Lease {
            permit: permit.clone(),
            media,
            path,
            bytes: bytes.map(Arc::new),
            mime,
            len: metadata.len(),
            modified: metadata
                .modified()
                .map_err(|_| "Private media timestamp unavailable")?,
        },
    );
    Ok(ticket)
}
fn lease_on(s: &mut LockState, ticket: &str, owner: &str) -> Result<Lease, String> {
    let lease = s.leases.get(ticket).cloned().ok_or(LOCKED)?;
    if lease.permit.owner != owner {
        return Err(LOCKED.into());
    }
    s.still_valid(&lease.permit)?;
    Ok(lease)
}
fn lease(ticket: &str, owner: &str) -> Result<Lease, String> {
    let mut s = state()?;
    lease_on(&mut s, ticket, owner)
}

fn media_still_private(media: &MediaRef) -> Result<(), String> {
    let conn = crate::t_sqlite::open_conn()?;
    let valid:bool=match media.mode.as_str() {
        "hidden"=>conn.query_row("SELECT EXISTS(SELECT 1 FROM media_states WHERE file_id=?1 AND hidden=1 AND deleted_at IS NULL)",[media.file_id],|r|r.get(0)),
        "deleted"=>conn.query_row("SELECT EXISTS(SELECT 1 FROM media_states ms JOIN media_trash_groups g ON g.id=ms.trash_group WHERE ms.file_id=?1 AND ms.deleted_at IS NOT NULL AND ms.trash_group=?2 AND g.status='trashed')",rusqlite::params![media.file_id,media.group_id],|r|r.get(0)),
        _=>return Err(LOCKED.into()),
    }.map_err(|_|LOCKED)?;
    if !valid {
        return Err(LOCKED.into());
    }
    Ok(())
}
fn byte_range(header: Option<&str>, len: u64, chunk: u64) -> Result<Option<(u64, u64)>, String> {
    let Some(header) = header else {
        return Ok(None);
    };
    if len == 0 {
        return Err("Invalid byte range".into());
    }
    let value = header.strip_prefix("bytes=").ok_or("Invalid byte range")?;
    let (start, end) = value.split_once('-').ok_or("Invalid byte range")?;
    if value.contains(',') {
        return Err("Multiple byte ranges are not supported".into());
    }
    let (start, end) = if start.is_empty() {
        let suffix = end.parse::<u64>().map_err(|_| "Invalid byte range")?;
        if suffix == 0 {
            return Err("Invalid byte range".into());
        }
        (len.saturating_sub(suffix), len - 1)
    } else {
        let start = start.parse::<u64>().map_err(|_| "Invalid byte range")?;
        let end = if end.is_empty() {
            len - 1
        } else {
            end.parse::<u64>()
                .map_err(|_| "Invalid byte range")?
                .min(len - 1)
        };
        (start, end)
    };
    if start >= len || start > end {
        return Err("Invalid byte range".into());
    }
    Ok(Some((start, end.min(start.saturating_add(chunk - 1)))))
}
fn response(status: u16, body: Vec<u8>, mime: &str) -> tauri::http::Response<Vec<u8>> {
    // The literal values and status codes are controlled by this module.
    tauri::http::Response::builder()
        .status(status)
        .header("Content-Type", mime)
        .header("Cache-Control", "no-store, max-age=0")
        .header("Pragma", "no-cache")
        .header("Access-Control-Allow-Origin", "*")
        .header("Access-Control-Allow-Headers", "Range")
        .header("Access-Control-Allow-Methods", "GET, HEAD, OPTIONS")
        .header(
            "Access-Control-Expose-Headers",
            "Content-Range, Accept-Ranges, Content-Length",
        )
        .body(body)
        .unwrap_or_default()
}
fn verify_lease_file(item: &Lease) -> Result<(), String> {
    let metadata = fs::symlink_metadata(&item.path).map_err(|_| "Private media unavailable")?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() != item.len
        || metadata.modified().ok() != Some(item.modified)
        || item.path.canonicalize().map_err(|_| LOCKED)? != item.path
    {
        return Err("Private media changed".into());
    }
    Ok(())
}
fn stream(
    ticket: &str,
    owner: Option<&str>,
    method: &str,
    range: Option<&str>,
) -> Result<tauri::http::Response<Vec<u8>>, String> {
    let item = if let Some(owner) = owner {
        lease(ticket, owner)?
    } else {
        let owner = {
            let s = state()?;
            s.leases.get(ticket).ok_or(LOCKED)?.permit.owner.clone()
        };
        lease(ticket, &owner)?
    };
    let _library = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
        .read()
        .map_err(|_| LOCKED)?;
    current_scope(&item.permit.library)?;
    media_still_private(&item.media)?;
    verify_lease_file(&item)?;
    let length = item
        .bytes
        .as_ref()
        .map(|v| v.len() as u64)
        .unwrap_or(item.len);
    let bounds = match byte_range(range, length, MAX_VIDEO_CHUNK) {
        Ok(v) => v,
        Err(_) => {
            let mut out = response(416, Vec::new(), &item.mime);
            out.headers_mut().insert(
                "Content-Range",
                format!("bytes */{length}").parse().map_err(|_| LOCKED)?,
            );
            return Ok(out);
        }
    };
    let (start, end) = bounds.unwrap_or((0, length.saturating_sub(1)));
    let count = if length == 0 { 0 } else { end - start + 1 };
    if method != "HEAD" && count > MAX_IMAGE_BYTES {
        return Err("Private media response is too large; use range playback".into());
    }
    let body = if method == "HEAD" {
        Vec::new()
    } else if let Some(bytes) = &item.bytes {
        bytes[start as usize..(start + count) as usize].to_vec()
    } else {
        let mut file = fs::File::open(&item.path).map_err(|_| "Private media unavailable")?;
        file.seek(SeekFrom::Start(start))
            .map_err(|_| "Cannot seek private media")?;
        let mut bytes = vec![0; count as usize];
        file.read_exact(&mut bytes)
            .map_err(|_| "Cannot read private media")?;
        bytes
    };
    recheck(&item.permit)?;
    let mut out = response(if bounds.is_some() { 206 } else { 200 }, body, &item.mime);
    out.headers_mut()
        .insert("Accept-Ranges", "bytes".parse().map_err(|_| LOCKED)?);
    out.headers_mut().insert(
        "Content-Length",
        count.to_string().parse().map_err(|_| LOCKED)?,
    );
    if bounds.is_some() {
        out.headers_mut().insert(
            "Content-Range",
            format!("bytes {start}-{end}/{length}")
                .parse()
                .map_err(|_| LOCKED)?,
        );
    }
    Ok(out)
}
pub(crate) fn register_protocol(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    builder.register_asynchronous_uri_scheme_protocol("private-media", |ctx, request, responder| {
        let owner = ctx.webview_label().to_string();
        let ticket = request.uri().path().trim_start_matches('/').to_string();
        let method = request.method().as_str().to_string();
        let range = request
            .headers()
            .get("Range")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        tauri::async_runtime::spawn_blocking(move || {
            let out = if method == "OPTIONS" {
                response(204, Vec::new(), "text/plain")
            } else if method != "GET" && method != "HEAD" {
                response(405, Vec::new(), "text/plain")
            } else {
                stream(&ticket, Some(&owner), &method, range.as_deref()).unwrap_or_else(|_| {
                    response(
                        403,
                        b"Private view locked, expired or unavailable".to_vec(),
                        "text/plain",
                    )
                })
            };
            responder.respond(out);
        });
    })
}
#[cfg(target_os = "linux")]
pub(crate) fn http_video_response(
    ticket: &str,
    method: &str,
    range: Option<&str>,
) -> tauri::http::Response<Vec<u8>> {
    stream(ticket, None, method, range).unwrap_or_else(|_| {
        response(
            403,
            b"Private view locked, expired or unavailable".to_vec(),
            "text/plain",
        )
    })
}
#[derive(Serialize)]
pub struct PrivateVideoResult {
    url: String,
    action: crate::t_video::VideoAction,
    mime: String,
}
#[tauri::command]
pub async fn prepare_private_video(
    window: tauri::WebviewWindow,
    ticket: String,
    player_id: String,
    force: Option<String>,
    request_id: Option<u64>,
) -> Result<PrivateVideoResult, String> {
    let owner = window.label().to_string();
    let item = tauri::async_runtime::spawn_blocking(move || lease(&ticket, &owner))
        .await
        .map_err(|_| LOCKED)??;
    if !item.mime.starts_with("video/") || item.bytes.is_some() {
        return Err("Not a private video source".into());
    }
    {
        let _library = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
            .read()
            .map_err(|_| LOCKED)?;
        current_scope(&item.permit.library)?;
        media_still_private(&item.media)?;
        verify_lease_file(&item)?;
    }
    let app = window.app_handle().clone();
    let result = crate::t_video::prepare_video_local(
        app.clone(),
        app.state::<crate::t_video::VideoManager>(),
        item.path.to_string_lossy().into_owned(),
        player_id,
        force,
        request_id,
    )
    .await?;
    let output = PathBuf::from(&result.url);
    let mime = if result.url.ends_with(".webm") {
        "video/webm"
    } else {
        "video/mp4"
    }
    .to_string();
    let output_mime = mime.clone();
    let ticket = tauri::async_runtime::spawn_blocking(move || {
        let _library = crate::t_cmds::FILE_REFRESH_LIBRARY_LOCK
            .read()
            .map_err(|_| LOCKED)?;
        recheck(&item.permit)?;
        media_still_private(&item.media)?;
        verify_lease_file(&item)?;
        issue(&item.permit, item.media, output, None, mime)
    })
    .await
    .map_err(|_| LOCKED)??;
    #[cfg(target_os = "linux")]
    let ticket = crate::t_http::make_private_video_stream_url(&ticket)?;
    Ok(PrivateVideoResult {
        url: ticket,
        action: result.action,
        mime: output_mime,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    const PASSWORD: &str = "private-lock-unit-password";
    fn enabled() -> LockState {
        LockState {
            initialized: true,
            config: Config {
                password: Some(PasswordRecord::create(PASSWORD).unwrap()),
                idle_minutes: 1,
                ..Config::default()
            },
            ..LockState::default()
        }
    }
    fn token(s: &LockState) -> String {
        s.session.as_ref().unwrap().token.clone()
    }
    #[test]
    fn password_verifiers_are_salted_strong_and_never_contain_plaintext() {
        let a = PasswordRecord::create(PASSWORD).unwrap();
        let b = PasswordRecord::create(PASSWORD).unwrap();
        assert_ne!(a.salt, b.salt);
        assert_ne!(a.verifier, b.verifier);
        assert!(a.verify(PASSWORD));
        assert!(!a.verify("wrong-password"));
        assert!(!a.verify(&"a".repeat(513)));
        assert!(!serde_json::to_string(&a).unwrap().contains(PASSWORD));
        assert!(PasswordRecord::create("short").is_err());
        assert!(PasswordRecord::create(&"x".repeat(129)).is_err());
        assert!(
            !PasswordRecord {
                salt: "broken".into(),
                verifier: "broken".into()
            }
            .verify(PASSWORD)
        );
    }
    #[test]
    fn sessions_bind_to_library_window_and_token_and_are_not_restored_on_restart() {
        let mut s = enabled();
        let now = Instant::now();
        let wall = SystemTime::now();
        s.unlock("A", "main", PASSWORD, now, wall).unwrap();
        let t = token(&s);
        assert_eq!(t.len(), 64);
        assert!(s.check("A", Some(&t), "main", false, now, wall).is_ok());
        for (lib, owner, token) in [
            ("B", "main", Some(t.as_str())),
            ("A", "other", Some(t.as_str())),
            ("A", "main", Some("wrong")),
            ("A", "main", None),
        ] {
            assert!(s.check(lib, token, owner, false, now, wall).is_err());
        }
        assert!(
            status_on(&mut s, "A", Some(&t), "main", false)
                .session_token
                .is_none()
        );
        let mut restarted = LockState {
            initialized: true,
            config: s.config.clone(),
            ..LockState::default()
        };
        assert!(
            restarted
                .check("A", Some(&t), "main", false, now, wall)
                .is_err()
        );
    }
    #[test]
    fn expiry_enforces_idle_sleep_and_clock_rollback_on_the_backend() {
        let mut s = enabled();
        let now = Instant::now();
        let wall = SystemTime::now();
        for (mono, clock) in [
            (now + Duration::from_secs(60), wall),
            (now, wall + Duration::from_secs(61)),
            (now, wall - Duration::from_secs(1)),
        ] {
            s.unlock("A", "main", PASSWORD, now, wall).unwrap();
            let t = token(&s);
            assert!(s.check("A", Some(&t), "main", false, mono, clock).is_err());
            assert!(s.session.is_none());
        }
    }
    #[test]
    fn status_polling_does_not_renew_idle_but_explicit_activity_does() {
        let mut s = enabled();
        let now = Instant::now();
        let wall = SystemTime::now();
        s.unlock("A", "main", PASSWORD, now, wall).unwrap();
        let t = token(&s);
        s.check(
            "A",
            Some(&t),
            "main",
            false,
            now + Duration::from_secs(40),
            wall + Duration::from_secs(40),
        )
        .unwrap();
        assert_eq!(
            s.session.as_ref().unwrap().deadline,
            now + Duration::from_secs(60)
        );
        s.check(
            "A",
            Some(&t),
            "main",
            true,
            now + Duration::from_secs(50),
            wall + Duration::from_secs(50),
        )
        .unwrap();
        assert!(
            s.check(
                "A",
                Some(&t),
                "main",
                false,
                now + Duration::from_secs(75),
                wall + Duration::from_secs(75)
            )
            .is_ok()
        );
        assert!(
            s.check(
                "A",
                Some(&t),
                "main",
                false,
                now + Duration::from_secs(110),
                wall + Duration::from_secs(110)
            )
            .is_err()
        );
    }
    #[test]
    fn locking_clears_leases_and_late_decode_permits_even_when_disabled() {
        let mut s = enabled();
        let now = Instant::now();
        let wall = SystemTime::now();
        s.unlock("A", "main", PASSWORD, now, wall).unwrap();
        let t = token(&s);
        let permit = s.check("A", Some(&t), "main", false, now, wall).unwrap();
        s.leases.insert(
            "old-url".into(),
            Lease {
                permit: permit.clone(),
                media: MediaRef {
                    file_id: 1,
                    mode: "hidden".into(),
                    group_id: None,
                },
                path: PathBuf::from("unused"),
                bytes: Some(Arc::new(vec![1, 2, 3])),
                mime: "image/jpeg".into(),
                len: 3,
                modified: wall,
            },
        );
        assert!(lease_on(&mut s, "old-url", "main").is_ok());
        assert!(lease_on(&mut s, "old-url", "other").is_err());
        assert!(lease_on(&mut s, "unregistered", "main").is_err());
        s.revoke();
        assert!(lease_on(&mut s, "old-url", "main").is_err());
        assert!(s.leases.is_empty());
        assert!(s.still_valid(&permit).is_err());
        s.unlock("A", "main", PASSWORD, now, wall).unwrap();
        assert_ne!(token(&s), t);
        assert!(s.still_valid(&permit).is_err());
        s.config.password = None;
        let permit = s.check("A", None, "main", false, now, wall).unwrap();
        s.revoke();
        assert!(s.still_valid(&permit).is_err());
    }
    #[test]
    fn password_attempts_are_throttled_across_configuration_and_library_revocation() {
        let mut s = enabled();
        let now = Instant::now();
        for _ in 0..5 {
            assert!(s.password_ok("wrong", now).is_err());
        }
        s.revoke();
        assert!(
            s.password_ok(PASSWORD, now + Duration::from_secs(29))
                .unwrap_err()
                .contains("30 seconds")
        );
        assert!(
            s.password_ok(PASSWORD, now + Duration::from_secs(31))
                .is_ok()
        );
        assert!(s.cooldown.is_none());
    }
    #[test]
    fn ranges_are_bounded_and_invalid_or_multiple_ranges_are_rejected() {
        assert_eq!(byte_range(None, 100, 10).unwrap(), None);
        for (value, expected) in [
            ("bytes=0-", (0, 9)),
            ("bytes=50-99", (50, 59)),
            ("bytes=-5", (95, 99)),
            ("bytes=98-999", (98, 99)),
        ] {
            assert_eq!(byte_range(Some(value), 100, 10).unwrap(), Some(expected));
        }
        for value in [
            "bytes=100-",
            "bytes=9-3",
            "bytes=-0",
            "bytes=0-1,4-6",
            "bad",
            "bytes=x-2",
            "bytes=18446744073709551615-",
        ] {
            assert!(byte_range(Some(value), 100, 10).is_err());
        }
        assert!(byte_range(Some("bytes=0-"), 0, 10).is_err());
        let out = response(403, vec![], "text/plain");
        assert_eq!(out.headers()["Cache-Control"], "no-store, max-age=0");
    }
    #[test]
    fn persisted_policy_is_atomic_reloadable_and_malformed_records_fail_closed() {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/private-lock-fixtures");
        fs::create_dir_all(&base).unwrap();
        let dir = base.join(format!("test-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&dir).unwrap();
        let dir = dir.canonicalize().unwrap();
        let path = dir.join("private-lock.json");
        let config = enabled().config;
        persist(&path, &config).unwrap();
        let bytes = fs::read(&path).unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains(PASSWORD));
        let loaded: Config = serde_json::from_slice(&bytes).unwrap();
        loaded.validate().unwrap();
        assert!(loaded.password.unwrap().verify(PASSWORD));
        persist(&path, &Config::default()).unwrap();
        let loaded: Config = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert!(loaded.password.is_none());
        for minutes in [0, 61] {
            assert!(
                Config {
                    idle_minutes: minutes,
                    ..Config::default()
                }
                .validate()
                .is_err()
            );
        }
        assert!(
            Config {
                version: 2,
                ..Config::default()
            }
            .validate()
            .is_err()
        );
        assert!(
            Config {
                password: Some(PasswordRecord {
                    salt: "bad".into(),
                    verifier: "bad".into()
                }),
                ..Config::default()
            }
            .validate()
            .is_err()
        );
        assert!(
            serde_json::from_str::<Config>(
                r#"{"password":{"salt":"bad","verifier":"bad"},"plaintextPassword":"never accept"}"#
            )
            .is_err()
        );
        // Only an owned UUID directory under this repository's target can be recursively removed.
        let canonical_base = base.canonicalize().unwrap();
        if dir.parent() == Some(canonical_base.as_path())
            && dir
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("test-")
        {
            fs::remove_dir_all(&dir).unwrap();
        }
    }
}
