//! The download engine: owns the job queue, schedules items within the
//! concurrency limit, retries with backoff, resumes after pauses and restarts,
//! records finished files and writes the history.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::{Mutex, RwLock};
use tokio::sync::Notify;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use crate::logs::Logger;
use crate::net::limiter::Cancelled;
use crate::net::throttle::Throttle;
use crate::pawchive::client::{ApiError, PawchiveClient};
use crate::pawchive::relay::{find_relay, RelayIndex};
use crate::pawchive::types::Post;
use crate::settings::{ContentFormat, Settings};
use crate::store::records::{FileCounts, HistoryEntry};
use crate::store::Store;

use super::archive::{self, ArchiveEntry, EntryData};
use super::model::*;
use super::plan::{hashes_of, plan_post};
use super::transfer::{
    choose_target, finish, part_path, verify, write_text, FetchError, Fetcher, Progress,
};

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Notified when a job ends (for notifications and the watchlist).
/// Why no new transfer starts right now, although downloads are queued.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Hold {
    /// Outside the download window; it opens at this time (epoch ms).
    #[serde(rename_all = "camelCase")]
    Schedule { opens_at: i64 },
    /// The download drive is nearly full.
    #[serde(rename_all = "camelCase")]
    Disk {
        free: u64,
        need: u64,
        folder: String,
    },
}

pub trait Hooks: Send + Sync {
    /// `still_running` counts the jobs that have not finished yet, so the
    /// app can wait for the whole batch before it notifies.
    fn job_finished(&self, job: &Job, entry: &HistoryEntry, still_running: usize);
}

pub struct NoHooks;
impl Hooks for NoHooks {
    fn job_finished(&self, _: &Job, _: &HistoryEntry, _: usize) {}
}

/// Waits for the network when there is none: 15 s apart, for up to an hour.
const OFFLINE_RETRY_MS: i64 = 15_000;
const OFFLINE_WAITS: u32 = 240;

/// Targets compare like the file system does: case-insensitively on Windows
/// and macOS, where `A.png` and `a.png` are the same file.
fn fold(path: &Path) -> String {
    let text = path.to_string_lossy();
    if cfg!(any(windows, target_os = "macos")) {
        text.to_lowercase()
    } else {
        text.into_owned()
    }
}

/// What a request downloads, to notice the same download queued twice.
fn signature(request: &JobRequest) -> String {
    let value = match request {
        JobRequest::Posts { posts, options, .. } => serde_json::json!({
            "posts": posts.iter().map(|p| (&p.post.service, &p.post.creator_id, &p.post.id, &p.file_indexes)).collect::<Vec<_>>(),
            "options": options,
        }),
        other => serde_json::to_value(other).unwrap_or_default(),
    };
    value.to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stop {
    Pause,
    Cancel,
    Remove,
}

struct Active {
    cancel: CancellationToken,
    stop: Option<Stop>,
    progress: Arc<Progress>,
    label: String,
    job_id: String,
}

#[derive(Default)]
struct State {
    jobs: Vec<Job>,
    /// Keyed by `job_id/item_id`.
    active: HashMap<String, Active>,
    resolving: HashMap<String, CancellationToken>,
    /// Targets claimed by running transfers (folded, see `fold`).
    reserved: HashSet<String>,
    /// Hash of each job as last saved, so unchanged jobs are not rewritten.
    saved: HashMap<String, u64>,
    speed: HashMap<String, VecDeque<(Instant, u64)>>,
}

enum Outcome {
    Done { target: PathBuf, bytes: u64 },
    Skipped(SkipReason),
    Failed(FetchError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobAction {
    Pause,
    Resume,
    Cancel,
    Retry,
    Remove,
    Top,
    Up,
    Down,
}

type RelaySlot = Arc<tokio::sync::Mutex<Option<(Instant, RelayIndex)>>>;

const RELAY_TTL: Duration = Duration::from_secs(10 * 60);
const STAGING: &str = ".pawchive-staging";

fn key(job_id: &str, item_id: &str) -> String {
    format!("{job_id}/{item_id}")
}

fn api_to_fetch(e: ApiError) -> FetchError {
    match e {
        ApiError::Cancelled => FetchError::Cancelled,
        ApiError::Status { status, .. } => FetchError::Status(status),
        ApiError::Network { message, .. } => FetchError::Network(message),
        ApiError::Format { .. } => FetchError::Network("unexpected answer".into()),
    }
}

impl From<Cancelled> for FetchError {
    fn from(_: Cancelled) -> Self {
        FetchError::Cancelled
    }
}

/// `dir/a.png` -> `dir/a_preview.png`, the thumbnail's file name.
fn preview_path(path: &str) -> String {
    let (dir, name) = match path.rfind('/') {
        Some(i) => (&path[..=i], &path[i + 1..]),
        None => ("", path),
    };
    let preview = match name.rfind('.') {
        Some(dot) if dot > 0 => format!("{}_preview{}", &name[..dot], &name[dot..]),
        _ => format!("{name}_preview.jpg"),
    };
    format!("{dir}{preview}")
}

/// The deepest folder that holds every path.
fn common_folder(paths: &[PathBuf]) -> Option<PathBuf> {
    let first = paths.first()?.parent()?.to_path_buf();
    let mut common: Vec<_> = first.components().collect();
    for p in &paths[1..] {
        let parent: Vec<_> = p
            .parent()
            .map(|d| d.components().collect())
            .unwrap_or_default();
        let n = common
            .iter()
            .zip(&parent)
            .take_while(|(a, b)| a == b)
            .count();
        common.truncate(n);
    }
    Some(common.iter().collect())
}

pub struct Engine {
    state: Mutex<State>,
    pub client: Arc<PawchiveClient>,
    fetcher: Fetcher,
    store: Arc<Store>,
    settings: Arc<RwLock<Settings>>,
    log: Arc<Logger>,
    hooks: RwLock<Arc<dyn Hooks>>,
    wake: Notify,
    /// Bumped on every change, so the UI knows when to refresh.
    version: AtomicU64,
    relays: Mutex<HashMap<String, RelaySlot>>,
    shutdown: CancellationToken,
    /// The runtime tasks run on. Commands and menu events arrive on threads
    /// without one (Tauri runs synchronous commands on the main thread), so
    /// the engine keeps its own handle instead of relying on the caller's.
    runtime: std::sync::OnceLock<tokio::runtime::Handle>,
    hold: RwLock<Option<Hold>>,
    /// Jobs finished since the app started, so watchers notice even a job
    /// that came and went between two of their checks.
    finished_total: AtomicU64,
}

/// The message of a caught panic, for the log and the item's error.
fn panic_message(error: &tokio::task::JoinError) -> String {
    if !error.is_panic() {
        return "stopped".into();
    }
    format!("internal error ({error})")
}

impl Engine {
    pub fn new(
        client: Arc<PawchiveClient>,
        throttle: Arc<Throttle>,
        store: Arc<Store>,
        settings: Arc<RwLock<Settings>>,
        log: Arc<Logger>,
    ) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(State::default()),
            fetcher: Fetcher {
                client: Arc::clone(&client),
                throttle,
            },
            client,
            store,
            settings,
            log,
            hooks: RwLock::new(Arc::new(NoHooks)),
            wake: Notify::new(),
            version: AtomicU64::new(1),
            relays: Mutex::new(HashMap::new()),
            shutdown: CancellationToken::new(),
            runtime: tokio::runtime::Handle::try_current()
                .map(std::sync::OnceLock::from)
                .unwrap_or_default(),
            hold: RwLock::new(None),
            finished_total: AtomicU64::new(0),
        })
    }

    pub fn hold(&self) -> Option<Hold> {
        self.hold.read().clone()
    }

    /// Stops (Some) or allows (None) new transfers. Running ones go on.
    /// Returns whether it changed.
    pub fn set_hold(&self, hold: Option<Hold>) -> bool {
        let changed = {
            let mut current = self.hold.write();
            let changed = *current != hold;
            *current = hold;
            changed
        };
        if changed {
            self.changed();
            self.wake.notify_one();
        }
        changed
    }

    /// Download folders of the jobs that still have something to fetch.
    pub fn pending_folders(&self) -> Vec<String> {
        let state = self.state.lock();
        let mut out: Vec<String> = Vec::new();
        for job in state.jobs.iter().filter(|j| runnable(j)) {
            let waiting = job.status == JobStatus::Resolving
                || job
                    .items
                    .iter()
                    .any(|i| matches!(i.status, ItemStatus::Pending | ItemStatus::Active));
            if waiting && !out.contains(&job.options.download_dir) {
                out.push(job.options.download_dir.clone());
            }
        }
        out
    }

    /// Any job not finished yet, paused ones included.
    pub fn has_unfinished(&self) -> bool {
        self.state.lock().jobs.iter().any(|j| !j.status.finished())
    }

    pub fn finished_total(&self) -> u64 {
        self.finished_total.load(Ordering::Relaxed)
    }

    /// Transfers running right now.
    pub fn active_count(&self) -> usize {
        self.state.lock().active.len()
    }

    /// Gives the engine the runtime to spawn its tasks on.
    pub fn attach_runtime(&self, handle: tokio::runtime::Handle) {
        let _ = self.runtime.set(handle);
    }

    /// Spawns a task from any thread. Returns None, and logs it, only if no
    /// runtime is known at all, which would be a wiring mistake.
    fn spawn<F>(&self, task: F) -> Option<tokio::task::JoinHandle<F::Output>>
    where
        F: std::future::Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let handle = tokio::runtime::Handle::try_current()
            .ok()
            .or_else(|| self.runtime.get().cloned());
        match handle {
            Some(h) => Some(h.spawn(task)),
            None => {
                self.log
                    .error("Could not start a background task: no runtime", None);
                None
            }
        }
    }

    pub fn set_hooks(&self, hooks: Arc<dyn Hooks>) {
        *self.hooks.write() = hooks;
    }

    pub fn version(&self) -> u64 {
        self.version.load(Ordering::Relaxed)
    }

    fn changed(&self) {
        self.version.fetch_add(1, Ordering::Relaxed);
        self.wake.notify_one();
    }

    // ------------------------------------------------------------- lifecycle

    /// Loads saved jobs. Whatever was transferring when the app stopped goes
    /// back to the queue and resumes from its part file.
    pub fn restore(self: &Arc<Self>) {
        let mut jobs = self.store.load_jobs();
        let mut resolve = Vec::new();
        for job in &mut jobs {
            for item in &mut job.items {
                if item.status == ItemStatus::Active {
                    item.status = ItemStatus::Pending;
                }
            }
            if job.request.is_some() && !job.status.finished() {
                resolve.push(job.id.clone());
            }
        }
        let count = jobs.len();
        self.state.lock().jobs = jobs;
        for id in resolve {
            self.spawn_resolve(id);
        }
        if count > 0 {
            self.log
                .info(format!("Restored {count} jobs from the last session"));
        }
        self.changed();
    }

    /// Runs the scheduler and the autosave until shutdown.
    pub async fn run(self: Arc<Self>) {
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            self.attach_runtime(handle);
        }
        let saver = Arc::clone(&self);
        self.spawn(async move {
            let mut saved = 0;
            let mut tick = tokio::time::interval(Duration::from_secs(2));
            loop {
                tokio::select! {
                    _ = tick.tick() => {},
                    _ = saver.shutdown.cancelled() => break,
                }
                let v = saver.version();
                if v != saved {
                    saved = v;
                    saver.save();
                }
            }
        });
        loop {
            self.pump();
            let wait = self
                .next_wake()
                .unwrap_or(Duration::from_secs(5))
                .min(Duration::from_secs(5));
            tokio::select! {
                _ = self.wake.notified() => {},
                _ = tokio::time::sleep(wait) => {},
                _ = self.shutdown.cancelled() => break,
            }
        }
    }

    /// Saves the queue with the live byte counts of running items. Only jobs
    /// that changed since the last save are written, so a huge creator does
    /// not rewrite megabytes every few seconds.
    pub fn save(&self) {
        use std::hash::{Hash, Hasher};
        let (order, changed) = {
            let mut state = self.state.lock();
            let state = &mut *state;
            let mut changed = Vec::new();
            let mut hashes = HashMap::new();
            for job in &state.jobs {
                let mut job = job.clone();
                for item in &mut job.items {
                    if let Some(a) = state.active.get(&key(&job.id, &item.id)) {
                        let (bytes, total) = a.progress.get();
                        item.bytes = bytes;
                        item.total_bytes = total;
                    }
                }
                let Ok(json) = serde_json::to_string(&job) else {
                    continue;
                };
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                json.hash(&mut hasher);
                let hash = hasher.finish();
                if state.saved.get(&job.id) != Some(&hash) {
                    changed.push((job.id.clone(), json));
                }
                hashes.insert(job.id.clone(), hash);
            }
            state.saved = hashes;
            let order: Vec<String> = state.jobs.iter().map(|j| j.id.clone()).collect();
            (order, changed)
        };
        if let Err(e) = self.store.sync_jobs(&order, &changed) {
            self.state.lock().saved.clear();
            self.log.error("Could not save the download queue", Some(e));
        }
    }

    /// Stops every transfer (keeping part files) and saves the queue.
    pub fn shutdown(&self) {
        self.shutdown.cancel();
        {
            let state = self.state.lock();
            for a in state.active.values() {
                a.cancel.cancel();
            }
            for c in state.resolving.values() {
                c.cancel();
            }
        }
        self.save();
    }

    fn next_wake(&self) -> Option<Duration> {
        let state = self.state.lock();
        let now = now_ms();
        state
            .jobs
            .iter()
            .filter(|j| runnable(j))
            .flat_map(|j| j.items.iter())
            .filter(|i| i.status == ItemStatus::Pending)
            .filter_map(|i| i.not_before)
            .map(|t| Duration::from_millis((t - now).max(0) as u64))
            .min()
    }

    // ------------------------------------------------------------ queries

    pub fn summaries(&self) -> Vec<JobSummary> {
        let mut state = self.state.lock();
        let state = &mut *state;
        let now = Instant::now();
        let mut out = Vec::with_capacity(state.jobs.len());
        for job in &state.jobs {
            let mut items_live = Vec::new();
            let mut current = Vec::new();
            for item in &job.items {
                if item.status == ItemStatus::Active {
                    if let Some(a) = state.active.get(&key(&job.id, &item.id)) {
                        let (bytes, total) = a.progress.get();
                        items_live.push((bytes, total));
                        if current.len() < 4 {
                            current.push(a.label.clone());
                        }
                        continue;
                    }
                }
            }
            let mut stats = compute_stats(&job.items);
            // Replace saved counts of running items with live ones.
            stats.partial = 0.0;
            for item in job.items.iter().filter(|i| i.status == ItemStatus::Active) {
                stats.bytes -= item.bytes;
                stats.total_bytes -= item.total_bytes.max(item.bytes);
            }
            for (bytes, total) in items_live {
                stats.bytes += bytes;
                stats.total_bytes += total.max(bytes);
                if total > 0 {
                    stats.partial += (bytes as f64 / total as f64).min(1.0);
                }
            }
            let samples = state.speed.entry(job.id.clone()).or_default();
            samples.push_back((now, stats.bytes));
            while samples.len() > 2 && now.duration_since(samples[0].0) > Duration::from_secs(4) {
                samples.pop_front();
            }
            let speed = match (samples.front(), samples.back()) {
                (Some(a), Some(b)) if b.0 > a.0 && stats.active > 0 => {
                    b.1.saturating_sub(a.1) as f64 / b.0.duration_since(a.0).as_secs_f64()
                }
                _ => 0.0,
            };
            out.push(JobSummary {
                id: job.id.clone(),
                title: job.title.clone(),
                subtitle: job.subtitle.clone(),
                creator: job.creator.clone(),
                status: job.status,
                mode: job.options.mode,
                created_at: job.created_at,
                started_at: job.started_at,
                finished_at: job.finished_at,
                post_count: job.post_count,
                stats,
                scan: job.scan,
                error: job.error.clone(),
                speed,
                current,
                folder: job_folder(job).to_string_lossy().into_owned(),
            });
        }
        let ids: HashSet<&String> = state.jobs.iter().map(|j| &j.id).collect();
        state.speed.retain(|id, _| ids.contains(id));
        out
    }

    pub fn items(&self, job_id: &str) -> Vec<ItemView> {
        let state = self.state.lock();
        let Some(job) = state.jobs.iter().find(|j| j.id == job_id) else {
            return vec![];
        };
        job.items
            .iter()
            .map(|item| {
                let (bytes, total_bytes) = state
                    .active
                    .get(&key(job_id, &item.id))
                    .map(|a| a.progress.get())
                    .unwrap_or((item.bytes, item.total_bytes));
                ItemView {
                    id: item.id.clone(),
                    kind: item.kind_name(),
                    label: item.label.clone(),
                    path: item.path.clone(),
                    target: item.target.clone(),
                    status: item.status,
                    bytes,
                    total_bytes,
                    attempts: item.attempts,
                    error: item.error.clone(),
                    skip_reason: item.skip_reason,
                    fallback: item.fallback,
                    post_id: item.post.post_id.clone(),
                    building: state
                        .active
                        .get(&key(job_id, &item.id))
                        .is_some_and(|a| a.progress.building.load(Ordering::Relaxed)),
                }
            })
            .collect()
    }

    pub fn job(&self, job_id: &str) -> Option<Job> {
        self.state
            .lock()
            .jobs
            .iter()
            .find(|j| j.id == job_id)
            .cloned()
    }

    /// Jobs not finished and not paused.
    pub fn busy_count(&self) -> usize {
        self.state
            .lock()
            .jobs
            .iter()
            .filter(|j| !j.status.finished() && j.status != JobStatus::Paused)
            .count()
    }

    // ------------------------------------------------------------- create

    pub fn create(self: &Arc<Self>, request: JobRequest) -> String {
        self.create_checked(request).0
    }

    /// Queues a request, or returns the job already doing exactly the same
    /// (true) so a double click does not download everything twice.
    pub fn create_checked(self: &Arc<Self>, request: JobRequest) -> (String, bool) {
        let sig = signature(&request);
        if let Some(existing) = self
            .state
            .lock()
            .jobs
            .iter()
            .find(|j| !j.status.finished() && j.signature.as_deref() == Some(sig.as_str()))
        {
            return (existing.id.clone(), true);
        }
        let settings = self.settings.read().clone();
        let overrides = match &request {
            JobRequest::Posts { options, .. }
            | JobRequest::Refs { options, .. }
            | JobRequest::Creator { options, .. } => options.clone(),
        };
        let options = JobOptions::from_settings(&settings, &overrides);
        let mut job = Job {
            id: uuid::Uuid::new_v4().to_string(),
            created_at: now_ms(),
            started_at: None,
            finished_at: None,
            title: String::new(),
            subtitle: String::new(),
            creator: None,
            status: JobStatus::Resolving,
            options,
            request: None,
            scan: None,
            post_count: 0,
            items: vec![],
            error: None,
            refs: vec![],
            signature: Some(sig),
        };
        let mut resolve = false;
        match &request {
            JobRequest::Posts {
                title,
                creator,
                posts,
                ..
            } => {
                job.creator = creator.clone();
                job.title = title
                    .clone()
                    .or_else(|| {
                        posts.first().map(|p| {
                            if p.post.title.is_empty() {
                                p.post.id.clone()
                            } else {
                                p.post.title.clone()
                            }
                        })
                    })
                    .unwrap_or_else(|| "Posts".into());
                job.subtitle = job
                    .creator
                    .as_ref()
                    .map(|c| c.name.clone())
                    .unwrap_or_default();
                let selected: Vec<(Post, Option<Vec<u32>>)> = posts
                    .iter()
                    .map(|p| (p.post.clone(), p.file_indexes.clone()))
                    .collect();
                let mut known = self.known_for(&job, selected.iter().map(|(p, _)| p));
                self.plan_into(&mut job, &selected, &mut known);
                job.status = JobStatus::Queued;
            }
            JobRequest::Refs {
                title,
                creator,
                refs,
                ..
            } => {
                job.creator = creator.clone();
                job.title = title
                    .clone()
                    .unwrap_or_else(|| format!("{} posts", refs.len()));
                job.subtitle = job
                    .creator
                    .as_ref()
                    .map(|c| c.name.clone())
                    .unwrap_or_default();
                job.request = Some(request.clone());
                resolve = true;
            }
            JobRequest::Creator {
                service,
                creator_id,
                ..
            } => {
                job.title = creator_id.clone();
                job.creator = Some(CreatorInfo {
                    service: service.clone(),
                    id: creator_id.clone(),
                    name: creator_id.clone(),
                });
                job.request = Some(request.clone());
                resolve = true;
            }
        }
        let id = job.id.clone();
        self.log.info(if resolve {
            format!("Queued \"{}\", looking up posts", job.title)
        } else {
            format!("Queued \"{}\" ({} items)", job.title, job.items.len())
        });
        {
            let mut state = self.state.lock();
            state.jobs.push(job);
            if !resolve {
                self.check_finished(&mut state, &id);
            }
        }
        if resolve {
            self.spawn_resolve(id.clone());
        }
        self.changed();
        self.save();
        (id, false)
    }

    fn known_for<'a>(
        &self,
        job: &Job,
        posts: impl IntoIterator<Item = &'a Post>,
    ) -> HashSet<String> {
        if job.options.skip_downloaded {
            self.store.known_hashes(&hashes_of(posts))
        } else {
            HashSet::new()
        }
    }

    /// Plans posts into the job. `known` collects hashes so the same file in
    /// several posts is fetched once per job.
    fn plan_into(
        &self,
        job: &mut Job,
        posts: &[(Post, Option<Vec<u32>>)],
        known: &mut HashSet<String>,
    ) {
        let now = now_ms();
        for (post, indexes) in posts {
            let items = plan_post(
                post,
                job.creator.as_ref(),
                &job.options,
                indexes.as_deref(),
                known,
                now,
            );
            if job.options.skip_downloaded {
                for item in &items {
                    if let (Some(h), ItemStatus::Pending) = (item.hash(), item.status) {
                        known.insert(h.to_string());
                    }
                }
            }
            job.items.extend(items);
            job.refs.push(PostRef {
                service: post.service.clone(),
                creator_id: post.creator_id.clone(),
                post_id: post.id.clone(),
            });
            job.post_count += 1;
        }
    }

    // ------------------------------------------------------------ resolve

    fn spawn_resolve(self: &Arc<Self>, job_id: String) {
        let spawned_id = job_id.clone();
        let cancel = self.shutdown.child_token();
        self.state
            .lock()
            .resolving
            .insert(job_id.clone(), cancel.clone());
        let engine = Arc::clone(self);
        let spawned = self.spawn(async move {
            // The lookup runs in its own task so a panic in it is caught here
            // and fails the job instead of leaving it looking up forever.
            let worker = Arc::clone(&engine);
            let (id, token) = (job_id.clone(), cancel.clone());
            let result = match tokio::spawn(async move { worker.resolve(&id, &token).await }).await
            {
                Ok(result) => result,
                Err(error) => {
                    engine.log.error(
                        "Looking up posts stopped unexpectedly",
                        Some(panic_message(&error)),
                    );
                    Err(ApiError::Network {
                        message: panic_message(&error),
                        url: String::new(),
                    })
                }
            };
            let mut state = engine.state.lock();
            state.resolving.remove(&job_id);
            let Some(job) = state.jobs.iter_mut().find(|j| j.id == job_id) else {
                return;
            };
            match result {
                Ok(()) => {
                    job.request = None;
                    if job.post_count == 0 {
                        job.error = Some("nothing-found".into());
                    }
                    if job.status == JobStatus::Resolving {
                        job.status = JobStatus::Queued;
                    }
                    engine.log.info(format!(
                        "Resolved \"{}\": {} posts, {} items",
                        job.title,
                        job.post_count,
                        job.items.len()
                    ));
                    engine.check_finished(&mut state, &job_id);
                }
                Err(ApiError::Cancelled) => {}
                Err(error) => {
                    job.status = JobStatus::Failed;
                    job.error = Some(error.to_string());
                    engine.log.error(
                        format!("Could not look up \"{}\"", job.title),
                        Some(error.to_string()),
                    );
                    engine.finalize(&mut state, &job_id);
                }
            }
            drop(state);
            engine.changed();
        });
        if spawned.is_none() {
            self.state.lock().resolving.remove(&spawned_id);
        }
    }

    fn with_job<R>(&self, job_id: &str, f: impl FnOnce(&mut Job) -> R) -> Option<R> {
        let mut state = self.state.lock();
        state.jobs.iter_mut().find(|j| j.id == job_id).map(f)
    }

    /// Fetches the full post when the listing left files or text out.
    async fn hydrate(
        &self,
        post: Post,
        options: &JobOptions,
        cancel: &CancellationToken,
    ) -> Result<Post, ApiError> {
        let filters_text = options.filter.scope == super::filter::FilterScope::All
            && !(options.filter.keywords.is_empty() && options.filter.skip_words.is_empty());
        let wants_text = options.save_content != ContentFormat::None
            || options.save_metadata
            || options.save_links
            || filters_text;
        if !post.incomplete && !wants_text && !post.content_html.contains("<img") {
            return Ok(post);
        }
        match self
            .client
            .get_post(&post.service, &post.creator_id, &post.id, cancel)
            .await
        {
            Ok(full) => Ok(full),
            Err(ApiError::Cancelled) => Err(ApiError::Cancelled),
            Err(e) => {
                self.log.warn(
                    format!("Using the listing for post {}", post.id),
                    Some(e.to_string()),
                );
                Ok(post)
            }
        }
    }

    /// Looks up the posts a job asked for and plans them as they arrive, so
    /// downloads start while a large creator is still being scanned.
    async fn resolve(
        self: &Arc<Self>,
        job_id: &str,
        cancel: &CancellationToken,
    ) -> Result<(), ApiError> {
        let Some((request, options, planned)) = self.with_job(job_id, |j| {
            j.scan.get_or_insert_with(Scan::default);
            let planned: HashSet<String> = j.refs.iter().map(|r| r.post_id.clone()).collect();
            (j.request.clone(), j.options.clone(), planned)
        }) else {
            return Ok(());
        };
        let Some(request) = request else {
            return Ok(());
        };
        let mut known: HashSet<String> = HashSet::new();
        match request {
            JobRequest::Posts { .. } => {}
            JobRequest::Refs {
                refs,
                creator,
                title,
                ..
            } => {
                let one_post = refs.len() == 1;
                let untitled = title.is_none();
                let mut creators: HashMap<String, CreatorInfo> = HashMap::new();
                if let Some(c) = creator {
                    creators.insert(format!("{}:{}", c.service, c.id), c);
                }
                let single = refs
                    .iter()
                    .map(|r| (&r.service, &r.creator_id))
                    .collect::<HashSet<_>>()
                    .len()
                    == 1;
                for r in refs {
                    if planned.contains(&r.post_id) {
                        continue;
                    }
                    let ckey = format!("{}:{}", r.service, r.creator_id);
                    if !creators.contains_key(&ckey) {
                        let info = match self
                            .client
                            .get_creator(&r.service, &r.creator_id, cancel)
                            .await
                        {
                            Ok(c) => CreatorInfo {
                                service: c.service,
                                id: c.id,
                                name: c.name,
                            },
                            Err(ApiError::Cancelled) => return Err(ApiError::Cancelled),
                            Err(_) => CreatorInfo {
                                service: r.service.clone(),
                                id: r.creator_id.clone(),
                                name: r.creator_id.clone(),
                            },
                        };
                        creators.insert(ckey.clone(), info);
                    }
                    let post = match self
                        .client
                        .get_post(&r.service, &r.creator_id, &r.post_id, cancel)
                        .await
                    {
                        Ok(p) => Some(p),
                        Err(ApiError::Cancelled) => return Err(ApiError::Cancelled),
                        Err(e) => {
                            self.log.warn(
                                format!("Could not load post {}", r.post_id),
                                Some(e.to_string()),
                            );
                            None
                        }
                    };
                    let creator = creators.get(&ckey).cloned();
                    let mut job_known = std::mem::take(&mut known);
                    let mut fresh = post
                        .as_ref()
                        .map(|p| self.known_for_options(&options, [p]))
                        .unwrap_or_default();
                    job_known.extend(fresh.drain());
                    self.with_job(job_id, |job| {
                        if single && job.creator.is_none() {
                            job.creator = creator.clone();
                            job.subtitle =
                                creator.as_ref().map(|c| c.name.clone()).unwrap_or_default();
                        }
                        let scan = job.scan.get_or_insert_with(Scan::default);
                        scan.posts += 1;
                        if let Some(post) = post {
                            scan.matched += 1;
                            let saved = job.creator.clone();
                            job.creator = creator.clone();
                            let title = if post.title.is_empty() {
                                post.id.clone()
                            } else {
                                post.title.clone()
                            };
                            self.plan_into(job, &[(post, None)], &mut job_known);
                            job.creator = saved;
                            if one_post && untitled {
                                job.title = title;
                            }
                        }
                    });
                    known = job_known;
                    self.changed();
                }
            }
            JobRequest::Creator {
                service,
                creator_id,
                query,
                post_ids,
                since,
                until,
                ..
            } => {
                let creator = match self.client.get_creator(&service, &creator_id, cancel).await {
                    Ok(c) => Some(c),
                    Err(ApiError::Cancelled) => return Err(ApiError::Cancelled),
                    Err(e) if e.not_found() => return Err(e),
                    Err(_) => None,
                };
                let name = creator
                    .as_ref()
                    .map(|c| c.name.clone())
                    .unwrap_or_else(|| creator_id.clone());
                self.with_job(job_id, |job| {
                    let info = CreatorInfo {
                        service: service.clone(),
                        id: creator_id.clone(),
                        name: name.clone(),
                    };
                    if job.options.save_announcements
                        && !job.items.iter().any(|i| i.id == "creator:announcements")
                    {
                        let item = super::plan::announcements_item(&job.options, &info, now_ms());
                        job.items.push(item);
                    }
                    job.creator = Some(info);
                    job.title = name.clone();
                    if let Some(q) = &query {
                        job.subtitle = format!("\"{q}\"");
                    }
                });
                self.changed();
                let mut wanted: Option<HashSet<String>> =
                    post_ids.clone().map(|v| v.into_iter().collect());
                // A hand-picked list of posts is never filtered further.
                let filtering = post_ids.is_none();
                let matcher = options.filter.matcher();
                let full_text = options.filter.scope == super::filter::FilterScope::All;
                let limit = if filtering {
                    options.filter.max_posts
                } else {
                    0
                };
                let mut taken = planned.len() as u32;
                let mut offset = 0u32;
                let mut pages = 0u32;
                let mut seen = 0u32;
                loop {
                    let page = self
                        .client
                        .get_creator_page(&service, &creator_id, offset, query.as_deref(), cancel)
                        .await?;
                    pages += 1;
                    seen += page.len() as u32;
                    let full_page = page.len() as u32 >= crate::pawchive::urls::PAGE_SIZE;
                    let mut stop = !full_page;
                    // Posts come newest first.
                    if let Some(since) = &since {
                        if page
                            .iter()
                            .all(|p| p.published.as_deref().unwrap_or("") <= since.as_str())
                        {
                            stop = true;
                        }
                    }
                    let matched: Vec<Post> = page
                        .into_iter()
                        .filter(|p| !planned.contains(&p.id))
                        .filter(|p| post_ids.as_ref().is_none_or(|ids| ids.contains(&p.id)))
                        .filter(|p| {
                            since
                                .as_ref()
                                .is_none_or(|s| p.published.as_deref().unwrap_or("") > s.as_str())
                        })
                        .filter(|p| {
                            until
                                .as_ref()
                                .is_none_or(|u| p.published.as_deref().unwrap_or("") <= u.as_str())
                        })
                        .collect();
                    if let Some(w) = wanted.as_mut() {
                        for p in &matched {
                            w.remove(&p.id);
                        }
                        if w.is_empty() {
                            stop = true;
                        }
                    }
                    self.with_job(job_id, |job| {
                        job.scan = Some(Scan {
                            pages,
                            posts: seen,
                            matched: job.scan.map(|s| s.matched).unwrap_or(0),
                        });
                    });
                    self.changed();
                    for post in matched {
                        if limit > 0 && taken >= limit {
                            stop = true;
                            break;
                        }
                        if filtering && !full_text && !matcher.accepts_post(&post) {
                            continue;
                        }
                        let post = self.hydrate(post, &options, cancel).await?;
                        if filtering && full_text && !matcher.accepts_post(&post) {
                            continue;
                        }
                        taken += 1;
                        let fresh = self.known_for_options(&options, [&post]);
                        known.extend(fresh);
                        self.with_job(job_id, |job| {
                            if let Some(s) = job.scan.as_mut() {
                                s.matched += 1;
                            }
                            self.plan_into(job, &[(post, None)], &mut known);
                        });
                        self.changed();
                    }
                    if stop {
                        break;
                    }
                    offset += crate::pawchive::urls::PAGE_SIZE;
                }
            }
        }
        Ok(())
    }

    fn known_for_options<'a>(
        &self,
        options: &JobOptions,
        posts: impl IntoIterator<Item = &'a Post>,
    ) -> HashSet<String> {
        if options.skip_downloaded {
            self.store.known_hashes(&hashes_of(posts))
        } else {
            HashSet::new()
        }
    }

    // ---------------------------------------------------------- scheduling

    /// Starts as many pending items as the concurrency setting allows.
    pub fn pump(self: &Arc<Self>) {
        if self.shutdown.is_cancelled() || self.hold.read().is_some() {
            return;
        }
        let concurrency = self.settings.read().concurrency.max(1) as usize;
        let mut state = self.state.lock();
        let mut running = state.active.len();
        if running >= concurrency {
            return;
        }
        let now = now_ms();
        let mut starts = Vec::new();
        'jobs: for job in state.jobs.iter_mut().filter(|j| runnable(j)) {
            for item in job.items.iter_mut() {
                if running >= concurrency {
                    break 'jobs;
                }
                if item.status != ItemStatus::Pending || item.not_before.is_some_and(|t| t > now) {
                    continue;
                }
                item.status = ItemStatus::Active;
                if job.status == JobStatus::Queued {
                    job.status = JobStatus::Running;
                }
                job.started_at.get_or_insert(now);
                starts.push((job.id.clone(), item.clone(), job.options.clone()));
                running += 1;
            }
        }
        let mut unstarted = Vec::new();
        for (job_id, item, options) in starts {
            let progress = Progress::new();
            progress.bytes.store(item.bytes, Ordering::Relaxed);
            progress.total.store(item.total_bytes, Ordering::Relaxed);
            let cancel = self.shutdown.child_token();
            state.active.insert(
                key(&job_id, &item.id),
                Active {
                    cancel: cancel.clone(),
                    stop: None,
                    progress: Arc::clone(&progress),
                    label: item.label.clone(),
                    job_id: job_id.clone(),
                },
            );
            let engine = Arc::clone(self);
            let (failed_job, failed_item) = (job_id.clone(), item.id.clone());
            let spawned = self.spawn(async move {
                // Run the transfer in its own task: if it panics, the item
                // fails with a message instead of staying active forever.
                let worker = Arc::clone(&engine);
                let item_id = item.id.clone();
                let task_job = job_id.clone();
                let outcome = match tokio::spawn(async move {
                    worker
                        .run_item(&task_job, &item, &options, &cancel, &progress)
                        .await
                })
                .await
                {
                    Ok(outcome) => outcome,
                    Err(error) => Outcome::Failed(FetchError::Internal(panic_message(&error))),
                };
                engine.settle(&job_id, &item_id, outcome);
            });
            if spawned.is_none() {
                unstarted.push((failed_job, failed_item));
            }
        }
        drop(state);
        for (job_id, item_id) in unstarted {
            self.settle(
                &job_id,
                &item_id,
                Outcome::Failed(FetchError::Internal("could not start".into())),
            );
        }
        self.version.fetch_add(1, Ordering::Relaxed);
    }

    async fn run_item(
        self: &Arc<Self>,
        job_id: &str,
        item: &Item,
        options: &JobOptions,
        cancel: &CancellationToken,
        progress: &Arc<Progress>,
    ) -> Outcome {
        let result = match &item.kind {
            ItemKind::File { source, hash, .. } => {
                self.run_file(
                    job_id,
                    item,
                    source,
                    hash.as_deref(),
                    options,
                    cancel,
                    progress,
                )
                .await
            }
            ItemKind::Text { text } => self.run_text(job_id, item, text, options).await,
            ItemKind::Zip { entries, .. } => {
                self.run_zip(job_id, item, entries, options, cancel, progress)
                    .await
            }
            ItemKind::Comments { title, url } => {
                let p = &item.post;
                match self
                    .client
                    .get_comments(&p.service, &p.creator_id, &p.post_id, cancel)
                    .await
                {
                    Ok(list) if list.is_empty() => Ok(Outcome::Skipped(SkipReason::Empty)),
                    Ok(list) => {
                        let text = super::plan::comments_text(title, url, &list);
                        self.run_text(job_id, item, &text, options).await
                    }
                    Err(e) => Err(api_to_fetch(e)),
                }
            }
            ItemKind::Announcements { name } => {
                let p = &item.post;
                match self
                    .client
                    .get_announcements(&p.service, &p.creator_id, cancel)
                    .await
                {
                    Ok(list) if list.is_empty() => Ok(Outcome::Skipped(SkipReason::Empty)),
                    Ok(list) => {
                        let text = super::plan::announcements_text(name, &list);
                        self.run_text(job_id, item, &text, options).await
                    }
                    Err(e) => Err(api_to_fetch(e)),
                }
            }
        };
        match result {
            Ok(outcome) => outcome,
            Err(e) => Outcome::Failed(e),
        }
    }

    /// The file this item writes, decided once and remembered.
    fn ensure_target(&self, job_id: &str, item: &Item, options: &JobOptions) -> Option<PathBuf> {
        let mut state = self.state.lock();
        let current = state
            .jobs
            .iter()
            .find(|j| j.id == job_id)
            .and_then(|j| j.items.iter().find(|i| i.id == item.id))
            .and_then(|i| i.target.clone());
        if let Some(t) = current {
            let t = PathBuf::from(t);
            state.reserved.insert(fold(&t));
            return Some(t);
        }
        let wanted = Path::new(&options.download_dir).join(&item.path);
        let reserved = state.reserved.clone();
        let target = choose_target(&wanted, options.conflict_action, |p| {
            reserved.contains(&fold(p))
        })?;
        state.reserved.insert(fold(&target));
        if let Some(i) = state
            .jobs
            .iter_mut()
            .find(|j| j.id == job_id)
            .and_then(|j| j.items.iter_mut().find(|i| i.id == item.id))
        {
            i.target = Some(target.to_string_lossy().into_owned());
        }
        Some(target)
    }

    async fn source_url(
        &self,
        post: &PostRef,
        source: &FileSource,
        cancel: &CancellationToken,
    ) -> Result<Option<String>, FetchError> {
        if !source.relay {
            return Ok(source.url.clone());
        }
        for fresh in [false, true] {
            let index = self
                .relay_index(post, fresh, cancel)
                .await
                .map_err(api_to_fetch)?;
            if let Some(link) = find_relay(&index, &source.path, &source.name, now_ms()) {
                return Ok(Some(link.url));
            }
        }
        Ok(None)
    }

    /// Relay links of a post page, shared by every file of the post.
    async fn relay_index(
        &self,
        post: &PostRef,
        fresh: bool,
        cancel: &CancellationToken,
    ) -> Result<RelayIndex, ApiError> {
        let slot = Arc::clone(self.relays.lock().entry(post.key()).or_default());
        let mut cached = slot.lock().await;
        if let Some((at, index)) = cached.as_ref() {
            if !fresh && at.elapsed() < RELAY_TTL {
                return Ok(index.clone());
            }
        }
        let index = self
            .client
            .get_relay_index(&post.service, &post.creator_id, &post.post_id, cancel)
            .await?;
        *cached = Some((Instant::now(), index.clone()));
        Ok(index)
    }

    #[allow(clippy::too_many_arguments)]
    async fn run_file(
        &self,
        job_id: &str,
        item: &Item,
        source: &FileSource,
        hash: Option<&str>,
        options: &JobOptions,
        cancel: &CancellationToken,
        progress: &Arc<Progress>,
    ) -> Result<Outcome, FetchError> {
        // Another job may have saved the same file since this one was planned.
        if options.skip_downloaded && !item.fallback {
            if let Some(h) = hash {
                if !self.store.known_hashes(&[h.to_string()]).is_empty() {
                    if let Some(t) = &item.target {
                        let _ = tokio::fs::remove_file(part_path(Path::new(t))).await;
                    }
                    return Ok(Outcome::Skipped(SkipReason::Duplicate));
                }
            }
        }
        let Some(target) = self.ensure_target(job_id, item, options) else {
            return Ok(Outcome::Skipped(SkipReason::Exists));
        };
        let part = part_path(&target);
        // Finished in an earlier session but not recorded before it ended.
        if item.target.is_some() && target.exists() && !part.exists() {
            let bytes = std::fs::metadata(&target).map(|m| m.len()).unwrap_or(0);
            return Ok(Outcome::Done { target, bytes });
        }
        let Some(url) = self.source_url(&item.post, source, cancel).await? else {
            return Ok(Outcome::Skipped(SkipReason::NoRelay));
        };
        let _permit = self.client.limiter.acquire(&url, cancel).await?;
        let bytes = self
            .fetcher
            .fetch(&url, &part, &source.name, progress, cancel)
            .await?;
        if options.verify_hashes && !item.fallback {
            verify(&part, hash).await?;
        }
        finish(&part, &target).await?;
        Ok(Outcome::Done { target, bytes })
    }

    async fn run_text(
        &self,
        job_id: &str,
        item: &Item,
        text: &str,
        options: &JobOptions,
    ) -> Result<Outcome, FetchError> {
        let Some(target) = self.ensure_target(job_id, item, options) else {
            return Ok(Outcome::Skipped(SkipReason::Exists));
        };
        let bytes = write_text(&target, text).await?;
        Ok(Outcome::Done { target, bytes })
    }

    fn staging_dir(options: &JobOptions, job_id: &str, item_id: &str) -> PathBuf {
        let safe: String = item_id
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        Path::new(&options.download_dir)
            .join(STAGING)
            .join(job_id)
            .join(safe)
    }

    async fn run_zip(
        &self,
        job_id: &str,
        item: &Item,
        entries: &[ZipEntry],
        options: &JobOptions,
        cancel: &CancellationToken,
        progress: &Arc<Progress>,
    ) -> Result<Outcome, FetchError> {
        let Some(target) = self.ensure_target(job_id, item, options) else {
            return Ok(Outcome::Skipped(SkipReason::Exists));
        };
        let staging = Self::staging_dir(options, job_id, &item.id);
        tokio::fs::create_dir_all(&staging)
            .await
            .map_err(|e| FetchError::Disk(e.to_string()))?;
        let max_retries = self.settings.read().max_retries;
        let mut permit = None;
        let mut fetched_bytes: u64 = entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.fetched && e.source.is_some())
            .filter_map(|(i, _)| {
                std::fs::metadata(staging.join(format!("{i}.bin")))
                    .ok()
                    .map(|m| m.len())
            })
            .sum();
        let mut status: Vec<(bool, bool)> = entries.iter().map(|e| (e.fetched, e.failed)).collect();
        for (i, entry) in entries.iter().enumerate() {
            let Some(source) = &entry.source else {
                continue;
            };
            let done_file = staging.join(format!("{i}.bin"));
            if status[i].0 && done_file.exists() {
                continue;
            }
            if status[i].1 {
                continue;
            }
            progress.base.store(fetched_bytes, Ordering::Relaxed);
            let mut attempt = 0;
            let mut offline_waits = 0u32;
            let ok = loop {
                let url = match self.source_url(&item.post, source, cancel).await {
                    Ok(Some(u)) => u,
                    Ok(None) => break false,
                    Err(FetchError::Cancelled) => return Err(FetchError::Cancelled),
                    Err(e) if e.retryable() && attempt < max_retries => {
                        attempt += 1;
                        sleep_cancel(self.client.limiter.backoff(attempt), cancel).await?;
                        continue;
                    }
                    Err(_) => break false,
                };
                if permit.is_none() {
                    permit = Some(self.client.limiter.acquire(&url, cancel).await?);
                } else {
                    self.client.limiter.pace(&url, cancel).await?;
                }
                let part = staging.join(format!("{i}.part"));
                let result = async {
                    let size = self
                        .fetcher
                        .fetch(&url, &part, &source.name, progress, cancel)
                        .await?;
                    if options.verify_hashes {
                        verify(&part, entry.hash.as_deref()).await?;
                    }
                    finish(&part, &done_file).await?;
                    Ok::<u64, FetchError>(size)
                }
                .await;
                match result {
                    Ok(size) => {
                        fetched_bytes += size;
                        break true;
                    }
                    Err(FetchError::Cancelled) => return Err(FetchError::Cancelled),
                    Err(FetchError::RateLimited(d)) => sleep_cancel(d, cancel).await?,
                    Err(FetchError::Offline) if offline_waits < OFFLINE_WAITS => {
                        offline_waits += 1;
                        sleep_cancel(Duration::from_millis(OFFLINE_RETRY_MS as u64), cancel)
                            .await?;
                    }
                    Err(e @ FetchError::Disk(_)) => return Err(e),
                    Err(e) if e.retryable() && attempt < max_retries => {
                        attempt += 1;
                        sleep_cancel(self.client.limiter.backoff(attempt), cancel).await?;
                    }
                    Err(e) => {
                        self.log.warn(
                            format!("Left {} out of {}", entry.name, item.label),
                            Some(e.to_string()),
                        );
                        break false;
                    }
                }
            };
            status[i] = (ok, !ok);
            self.with_item(job_id, &item.id, |it| {
                if let ItemKind::Zip { entries, .. } = &mut it.kind {
                    if let Some(e) = entries.get_mut(i) {
                        e.fetched = ok;
                        e.failed = !ok;
                    }
                }
            });
            self.version.fetch_add(1, Ordering::Relaxed);
        }
        progress.base.store(0, Ordering::Relaxed);
        progress.bytes.store(fetched_bytes, Ordering::Relaxed);
        progress.total.store(fetched_bytes, Ordering::Relaxed);
        drop(permit);

        let archive_entries: Vec<ArchiveEntry> = entries
            .iter()
            .enumerate()
            .filter_map(|(i, e)| {
                let data = match (&e.source, &e.text) {
                    (Some(_), _) if status[i].0 => {
                        EntryData::File(staging.join(format!("{i}.bin")))
                    }
                    (None, Some(t)) => EntryData::Text(t.clone()),
                    _ => return None,
                };
                Some(ArchiveEntry {
                    name: e.name.clone(),
                    data,
                    category: e.category,
                })
            })
            .collect();
        if !archive_entries
            .iter()
            .any(|e| matches!(e.data, EntryData::File(_)))
        {
            let _ = tokio::fs::remove_dir_all(&staging).await;
            return Err(FetchError::Network(
                "no file of this post could be fetched".into(),
            ));
        }
        progress.building.store(true, Ordering::Relaxed);
        let max_part = u64::from(options.zip_max_size_mb) * 1024 * 1024;
        let target_for_build = target.clone();
        let built = tokio::task::spawn_blocking(move || {
            archive::build(&target_for_build, &archive_entries, max_part, |_| {})
        })
        .await
        .map_err(|e| FetchError::Disk(e.to_string()))?
        .map_err(|e| FetchError::Disk(e.to_string()))?;
        let _ = tokio::fs::remove_dir_all(&staging).await;
        let parts = built.0.len() as u32;
        let failed: Vec<String> = entries
            .iter()
            .enumerate()
            .filter(|(i, _)| status[*i].1)
            .map(|(_, e)| e.name.clone())
            .collect();
        self.with_item(job_id, &item.id, |it| {
            if let ItemKind::Zip { parts: p, .. } = &mut it.kind {
                *p = parts;
            }
            if !failed.is_empty() {
                it.error = Some(format!("missing: {}", failed.join(", ")));
            }
        });
        Ok(Outcome::Done {
            target,
            bytes: built.1,
        })
    }

    fn with_item(&self, job_id: &str, item_id: &str, f: impl FnOnce(&mut Item)) {
        let mut state = self.state.lock();
        if let Some(item) = state
            .jobs
            .iter_mut()
            .find(|j| j.id == job_id)
            .and_then(|j| j.items.iter_mut().find(|i| i.id == item_id))
        {
            f(item);
        }
    }

    /// Applies the result of a transfer.
    fn settle(&self, job_id: &str, item_id: &str, outcome: Outcome) {
        let max_retries = self.settings.read().max_retries;
        let mut state = self.state.lock();
        let active = state.active.remove(&key(job_id, item_id));
        let stop = active.as_ref().and_then(|a| a.stop);
        let (live_bytes, live_total) = active.as_ref().map(|a| a.progress.get()).unwrap_or((0, 0));
        let Some(job_index) = state.jobs.iter().position(|j| j.id == job_id) else {
            return;
        };
        let options = state.jobs[job_index].options.clone();
        let Some(item_index) = state.jobs[job_index]
            .items
            .iter()
            .position(|i| i.id == item_id)
        else {
            return;
        };
        if let Some(t) = state.jobs[job_index].items[item_index].target.clone() {
            state.reserved.remove(&fold(Path::new(&t)));
        }
        let limiter = Arc::clone(&self.client.limiter);
        let mut record: Option<(String, String, String)> = None;
        let mut cleanup: Option<Item> = None;
        {
            let job = &mut state.jobs[job_index];
            let item = &mut job.items[item_index];
            item.bytes = live_bytes.max(item.bytes);
            item.total_bytes = live_total.max(item.total_bytes);
            match outcome {
                Outcome::Done { target, bytes } => {
                    item.status = ItemStatus::Done;
                    item.waits = 0;
                    item.bytes = bytes;
                    item.total_bytes = bytes;
                    item.not_before = None;
                    item.target = Some(target.to_string_lossy().into_owned());
                    if !matches!(item.kind, ItemKind::Zip { .. }) {
                        item.error = None;
                    }
                    if let Some(hash) = item.hash() {
                        record = Some((
                            hash.to_string(),
                            item.post.key(),
                            target.to_string_lossy().into_owned(),
                        ));
                    }
                    if let ItemKind::Zip { entries, .. } = &item.kind {
                        for e in entries.iter().filter(|e| e.fetched) {
                            if let Some(h) = &e.hash {
                                let _ = self.store.record_file(
                                    h,
                                    &item.post.key(),
                                    &target.to_string_lossy(),
                                    now_ms(),
                                );
                            }
                        }
                    }
                }
                Outcome::Skipped(reason) => {
                    item.status = ItemStatus::Skipped;
                    item.skip_reason = Some(reason);
                }
                Outcome::Failed(FetchError::Cancelled) => match stop {
                    Some(Stop::Cancel) => {
                        item.status = ItemStatus::Cancelled;
                        cleanup = Some(item.clone());
                    }
                    Some(Stop::Remove) => cleanup = Some(item.clone()),
                    // Paused, or the app is closing: resume later.
                    _ => item.status = ItemStatus::Pending,
                },
                Outcome::Failed(FetchError::RateLimited(wait)) => {
                    item.status = ItemStatus::Pending;
                    item.not_before = Some(now_ms() + wait.as_millis() as i64);
                }
                Outcome::Failed(FetchError::Offline) if item.waits < OFFLINE_WAITS => {
                    // No network: wait for it without using up the attempts.
                    if item.waits == 0 {
                        self.log.warn(
                            format!("No connection for {}, waiting for the network", item.label),
                            None,
                        );
                    }
                    item.waits += 1;
                    item.status = ItemStatus::Pending;
                    item.error = Some(FetchError::Offline.to_string());
                    item.not_before = Some(now_ms() + OFFLINE_RETRY_MS);
                }
                Outcome::Failed(error) => {
                    let can_fallback = options.thumbnail_fallback
                        && !item.fallback
                        && matches!(&item.kind, ItemKind::File { source, .. } if source.thumbnail.is_some());
                    if error.retryable() && item.attempts < max_retries {
                        item.attempts += 1;
                        item.status = ItemStatus::Pending;
                        item.error = Some(error.to_string());
                        item.not_before =
                            Some(now_ms() + limiter.backoff(item.attempts).as_millis() as i64);
                        self.log.debug(format!(
                            "Retrying {} ({}/{}): {error}",
                            item.label, item.attempts, max_retries
                        ));
                    } else if can_fallback && (error.missing() || error.retryable()) {
                        if let ItemKind::File { source, hash, .. } = &mut item.kind {
                            source.url = source.thumbnail.take();
                            source.relay = false;
                            *hash = None;
                        }
                        cleanup = Some(item.clone());
                        item.fallback = true;
                        item.attempts = 0;
                        item.status = ItemStatus::Pending;
                        item.target = None;
                        item.bytes = 0;
                        item.total_bytes = 0;
                        item.not_before = None;
                        item.path = preview_path(&item.path);
                        item.label = preview_path(&item.label);
                        self.log.warn(
                            format!("Saving the thumbnail of {} instead", item.label),
                            Some(error.to_string()),
                        );
                    } else {
                        item.status = ItemStatus::Failed;
                        item.error = Some(error.to_string());
                        self.log
                            .error(format!("Failed: {}", item.label), Some(error.to_string()));
                    }
                }
            }
        }
        if let Some((hash, post, path)) = record {
            let _ = self.store.record_file(&hash, &post, &path, now_ms());
        }
        if let Some(item) = cleanup {
            remove_partial(&options, job_id, &item);
        }
        self.check_finished(&mut state, job_id);
        drop(state);
        self.changed();
    }

    /// Marks a job done once nothing is left to do, and writes its history.
    fn check_finished(&self, state: &mut State, job_id: &str) {
        let Some(job) = state.jobs.iter_mut().find(|j| j.id == job_id) else {
            return;
        };
        if job.request.is_some() || job.status.finished() || job.status == JobStatus::Paused {
            return;
        }
        if job.items.iter().any(Item::is_open) {
            return;
        }
        let stats = compute_stats(&job.items);
        job.status = if stats.failed > 0 || (job.post_count == 0 && job.error.is_some()) {
            JobStatus::Failed
        } else {
            JobStatus::Done
        };
        self.finalize(state, job_id);
    }

    fn finalize(&self, state: &mut State, job_id: &str) {
        let Some(job) = state.jobs.iter_mut().find(|j| j.id == job_id) else {
            return;
        };
        job.finished_at = Some(now_ms());
        self.finished_total.fetch_add(1, Ordering::Relaxed);
        let stats = compute_stats(&job.items);
        let targets: Vec<PathBuf> = job
            .items
            .iter()
            .filter(|i| i.status == ItemStatus::Done)
            .filter_map(|i| i.target.as_ref().map(PathBuf::from))
            .collect();
        let folder = common_folder(&targets).unwrap_or_else(|| job_folder(job));
        // A post is downloaded once every one of its items is done or skipped.
        let mut posts_done: HashMap<String, bool> = HashMap::new();
        for item in &job.items {
            let ok = matches!(item.status, ItemStatus::Done | ItemStatus::Skipped);
            posts_done
                .entry(item.post.key())
                .and_modify(|v| *v &= ok)
                .or_insert(ok);
        }
        for (k, ok) in posts_done {
            if ok {
                let _ = self.store.record_post(&k, &job.title, now_ms());
            }
        }
        let errors: Vec<String> = job
            .items
            .iter()
            .filter(|i| i.status == ItemStatus::Failed)
            .map(|i| format!("{}: {}", i.label, i.error.clone().unwrap_or_default()))
            .chain(job.error.clone())
            .take(50)
            .collect();
        let entry = HistoryEntry {
            id: job.id.clone(),
            title: job.title.clone(),
            subtitle: job.subtitle.clone(),
            creator: job.creator.clone(),
            mode: job.options.mode,
            status: job.status,
            created_at: job.created_at,
            started_at: job.started_at,
            finished_at: job.finished_at.unwrap_or_else(now_ms),
            post_count: job.post_count,
            files: FileCounts {
                done: stats.done,
                failed: stats.failed,
                skipped: stats.skipped,
            },
            bytes: stats.bytes,
            folder: folder.to_string_lossy().into_owned(),
            first_file: targets.first().map(|p| p.to_string_lossy().into_owned()),
            refs: job.refs.clone(),
            errors,
        };
        let msg = format!(
            "Finished \"{}\": {} done, {} skipped, {} failed",
            job.title, stats.done, stats.skipped, stats.failed
        );
        if stats.failed > 0 {
            self.log.warn(msg, None)
        } else {
            self.log.info(msg)
        }
        if self.settings.read().record_history {
            if let Err(e) = self.store.add_history(&entry) {
                self.log.error("Could not write the history", Some(e));
            }
        }
        let staging = Path::new(&job.options.download_dir)
            .join(STAGING)
            .join(&job.id);
        if staging.exists() {
            let _ = std::fs::remove_dir_all(&staging);
        }
        let hooks = self.hooks.read().clone();
        let job = job.clone();
        let still_running = state
            .jobs
            .iter()
            .filter(|j| !j.status.finished() && j.status != JobStatus::Paused)
            .count();
        hooks.job_finished(&job, &entry, still_running);
    }

    // ------------------------------------------------------------- actions

    pub fn act(self: &Arc<Self>, job_id: &str, action: JobAction) {
        let mut state = self.state.lock();
        let Some(index) = state.jobs.iter().position(|j| j.id == job_id) else {
            return;
        };
        match action {
            JobAction::Pause => {
                let job = &mut state.jobs[index];
                if job.status.finished() || job.status == JobStatus::Paused {
                    return;
                }
                job.status = JobStatus::Paused;
                stop_active(&mut state, job_id, Stop::Pause);
                self.log
                    .info(format!("Paused \"{}\"", state.jobs[index].title));
            }
            JobAction::Resume => {
                let resolving = state.resolving.contains_key(job_id);
                let job = &mut state.jobs[index];
                if job.status != JobStatus::Paused {
                    return;
                }
                job.status = if job.request.is_some() {
                    JobStatus::Resolving
                } else {
                    JobStatus::Queued
                };
                let restart = job.request.is_some() && !resolving;
                self.log.info(format!("Resumed \"{}\"", job.title));
                self.check_finished(&mut state, job_id);
                if restart {
                    drop(state);
                    self.spawn_resolve(job_id.to_string());
                    self.changed();
                    return;
                }
            }
            JobAction::Cancel => {
                if state.jobs[index].status.finished() {
                    return;
                }
                if let Some(c) = state.resolving.remove(job_id) {
                    c.cancel();
                }
                stop_active(&mut state, job_id, Stop::Cancel);
                let job = &mut state.jobs[index];
                job.request = None;
                let options = job.options.clone();
                let mut leftovers = Vec::new();
                for item in job
                    .items
                    .iter_mut()
                    .filter(|i| i.status == ItemStatus::Pending)
                {
                    item.status = ItemStatus::Cancelled;
                    leftovers.push(item.clone());
                }
                job.status = JobStatus::Cancelled;
                for item in &leftovers {
                    remove_partial(&options, job_id, item);
                }
                self.log.info(format!("Cancelled \"{}\"", job.title));
                self.finalize(&mut state, job_id);
            }
            JobAction::Retry => {
                let job = &mut state.jobs[index];
                let mut any = false;
                for item in job
                    .items
                    .iter_mut()
                    .filter(|i| matches!(i.status, ItemStatus::Failed | ItemStatus::Cancelled))
                {
                    item.status = ItemStatus::Pending;
                    item.attempts = 0;
                    item.error = None;
                    item.not_before = None;
                    if let ItemKind::Zip { entries, .. } = &mut item.kind {
                        for e in entries.iter_mut() {
                            e.failed = false;
                        }
                    }
                    any = true;
                }
                let rescan = job.request.is_some();
                if !any && !rescan {
                    return;
                }
                job.status = if rescan {
                    JobStatus::Resolving
                } else {
                    JobStatus::Queued
                };
                job.finished_at = None;
                job.error = None;
                self.log.info(format!("Retrying \"{}\"", job.title));
                if rescan && !state.resolving.contains_key(job_id) {
                    drop(state);
                    self.spawn_resolve(job_id.to_string());
                    self.changed();
                    return;
                }
            }
            JobAction::Remove => {
                if let Some(c) = state.resolving.remove(job_id) {
                    c.cancel();
                }
                stop_active(&mut state, job_id, Stop::Remove);
                let job = state.jobs.remove(index);
                // Its running transfers will not be settled (the job is gone),
                // so release the names they claimed here.
                for target in job.items.iter().filter_map(|i| i.target.as_deref()) {
                    state.reserved.remove(&fold(Path::new(target)));
                }
                state.saved.remove(job_id);
                if !job.status.finished() {
                    for item in job.items.iter().filter(|i| i.is_open()) {
                        remove_partial(&job.options, job_id, item);
                    }
                    let staging = Path::new(&job.options.download_dir)
                        .join(STAGING)
                        .join(&job.id);
                    let _ = std::fs::remove_dir_all(staging);
                }
            }
            JobAction::Top => {
                let job = state.jobs.remove(index);
                state.jobs.insert(0, job);
            }
            JobAction::Up if index > 0 => state.jobs.swap(index, index - 1),
            JobAction::Down if index + 1 < state.jobs.len() => state.jobs.swap(index, index + 1),
            _ => {}
        }
        drop(state);
        self.changed();
        // Saved right away, so even a hard kill does not bring a removed or
        // cancelled job back. Saves are incremental, so this is cheap.
        self.save();
    }

    pub fn act_all(self: &Arc<Self>, action: JobAction) {
        let ids: Vec<String> = self
            .state
            .lock()
            .jobs
            .iter()
            .map(|j| j.id.clone())
            .collect();
        for id in ids {
            self.act(&id, action);
        }
    }

    pub fn clear_finished(&self) {
        self.state.lock().jobs.retain(|j| !j.status.finished());
        self.changed();
        self.save();
    }

    /// Retries one item (from the job details).
    pub fn retry_item(&self, job_id: &str, item_id: &str) {
        let mut state = self.state.lock();
        let Some(job) = state.jobs.iter_mut().find(|j| j.id == job_id) else {
            return;
        };
        let Some(item) = job.items.iter_mut().find(|i| i.id == item_id) else {
            return;
        };
        if !matches!(
            item.status,
            ItemStatus::Failed | ItemStatus::Cancelled | ItemStatus::Skipped
        ) {
            return;
        }
        item.status = ItemStatus::Pending;
        item.attempts = 0;
        item.error = None;
        item.skip_reason = None;
        item.not_before = None;
        if job.status.finished() {
            job.status = JobStatus::Queued;
            job.finished_at = None;
        }
        drop(state);
        self.changed();
    }
}

fn runnable(job: &Job) -> bool {
    matches!(
        job.status,
        JobStatus::Queued | JobStatus::Running | JobStatus::Resolving
    )
}

fn job_folder(job: &Job) -> PathBuf {
    let targets: Vec<PathBuf> = job
        .items
        .iter()
        .filter_map(|i| i.target.as_ref().map(PathBuf::from))
        .collect();
    common_folder(&targets).unwrap_or_else(|| {
        let paths: Vec<PathBuf> = job
            .items
            .iter()
            .map(|i| Path::new(&job.options.download_dir).join(&i.path))
            .collect();
        common_folder(&paths).unwrap_or_else(|| PathBuf::from(&job.options.download_dir))
    })
}

fn stop_active(state: &mut State, job_id: &str, how: Stop) {
    for a in state.active.values_mut().filter(|a| a.job_id == job_id) {
        a.stop = Some(how);
        a.cancel.cancel();
    }
}

/// Deletes what an unfinished item left on disk.
fn remove_partial(options: &JobOptions, job_id: &str, item: &Item) {
    if let Some(t) = &item.target {
        let _ = std::fs::remove_file(part_path(Path::new(t)));
    }
    if matches!(item.kind, ItemKind::Zip { .. }) {
        let _ = std::fs::remove_dir_all(Engine::staging_dir(options, job_id, &item.id));
    }
}

async fn sleep_cancel(d: Duration, cancel: &CancellationToken) -> Result<(), FetchError> {
    tokio::select! {
        _ = tokio::time::sleep(d) => Ok(()),
        _ = cancel.cancelled() => Err(FetchError::Cancelled),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_previews_and_folders() {
        assert_eq!(preview_path("a/b/c.png"), "a/b/c_preview.png");
        assert_eq!(preview_path("c"), "c_preview.jpg");
        let paths = vec![
            PathBuf::from("/d/x/a/1.png"),
            PathBuf::from("/d/x/b/2.png"),
            PathBuf::from("/d/x/a/3.png"),
        ];
        assert_eq!(common_folder(&paths), Some(PathBuf::from("/d/x")));
        assert_eq!(common_folder(&[]), None);
    }
}
