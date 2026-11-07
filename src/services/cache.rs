//! The client-side cache of what the live gateway fetched.
//!
//! What it is for, and what it is not for, matters more than how it works: an
//! earlier version served workflow runs from this cache *instead of* the
//! network, and the app showed run statuses that had long changed. So:
//!
//! - **Mutable data never skips the network.** Repositories, workflows and the
//!   latest run of each workflow are kept only so a later version can paint
//!   something before the network answers, or when it cannot answer at all.
//!   Every read of them is a hint, stamped with when it was fetched.
//! - **Only immutable data does.** The jobs of a completed run cannot change
//!   until the run is re-run, and a re-run changes the run's `updated_at`. The
//!   cache hands jobs back only while the run's latest *live* version — fed by
//!   every run list the gateway fetches, never by the disk — is still the
//!   completed one the jobs were stored under.
//!
//! The data belongs to one token — in practice one account, though signing in
//! again with a new token starts from an empty cache. The snapshot carries a
//! fingerprint of the token it was fetched with, and every write carries the
//! [`CacheOwner`] handed out when that token was bound. A request still in
//! flight when the user signs out holds a stale owner, and its write is dropped
//! instead of bringing the previous account's data back into the next one's
//! cache.
//!
//! Persistence is a single JSON file under the user cache directory, written
//! atomically (a private temporary file renamed over it), debounced, and
//! flushed once more at shutdown. Several instances of the app may run at once;
//! the last one to write wins, and a second instance still signed in can write
//! the file again after the first one signed out and deleted it. That is the
//! same account's data, and the fingerprint keeps it from any other account.

use crate::services::api::models::{Job, Repo, Workflow, WorkflowRun};
use dirs::cache_dir;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::watch;
use tracing::{debug, info, warn};

const CACHE_FILE_NAME: &str = "data-cache.json";

/// Bumped whenever the on-disk layout changes; a file with another version is
/// ignored rather than migrated — it is only a cache.
const SNAPSHOT_VERSION: u32 = 1;

/// How long an entry stays usable for an offline paint. Long enough to cover a
/// weekend away from the network; the age is always shown next to the data.
pub const DEFAULT_RETENTION: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// Repositories whose workflows and latest runs are kept; the least recently
/// fetched ones are evicted first. Comfortably above the sidebar's status
/// fan-out, so routine refreshes do not keep evicting and re-adding entries —
/// each of which would count as a change and cost a disk write.
const MAX_DETAILED_REPOS: usize = 50;

/// Completed runs whose jobs are kept in memory.
const MAX_CACHED_JOB_RUNS: usize = 200;

/// Live run versions remembered for the jobs check. Past this the index is
/// simply dropped: losing it only costs a refetch.
const MAX_RUN_VERSIONS: usize = 10_000;

/// Coalesces bursts of stores — the detail pane refreshes every few seconds —
/// into one disk write.
const WRITE_DEBOUNCE: Duration = Duration::from_secs(30);

#[derive(Debug, Clone)]
pub struct CachePersistenceConfig {
    path: PathBuf,
    retention: Duration,
}

impl CachePersistenceConfig {
    pub fn new(path: PathBuf, retention: Duration) -> Self {
        Self { path, retention }
    }

    pub fn for_app(app_id: &str) -> Option<Self> {
        let mut path = cache_dir()?;
        path.push(app_id);
        path.push(CACHE_FILE_NAME);
        Some(Self::new(path, DEFAULT_RETENTION))
    }
}

/// Proof that a token was bound to the cache, and which binding it was.
///
/// Rebinding another token, or clearing the cache, retires every owner handed
/// out before; reads and writes through a retired owner are no-ops.
#[derive(Debug, Clone)]
pub struct CacheOwner {
    fingerprint: Arc<str>,
    generation: u64,
}

/// A cached value and when it was fetched from the network.
#[derive(Debug, Clone)]
pub struct Cached<T> {
    pub value: Arc<T>,
    pub fetched_at: SystemTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Stamped<T> {
    /// Unix seconds.
    fetched_at: u64,
    value: Arc<T>,
    /// Hash of the serialized value, so a store that brings nothing new does
    /// not schedule a disk write. Recomputed on load.
    #[serde(skip)]
    hash: u64,
}

impl<T: Serialize> Stamped<T> {
    fn new(value: T, fetched_at: u64) -> Self {
        let hash = value_hash(&value);
        Self {
            fetched_at,
            value: Arc::new(value),
            hash,
        }
    }

    fn rehash(&mut self) {
        self.hash = value_hash(self.value.as_ref());
    }

    fn to_cached(&self) -> Cached<T> {
        Cached {
            value: self.value.clone(),
            fetched_at: UNIX_EPOCH + Duration::from_secs(self.fetched_at),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct RepoEntry {
    #[serde(default)]
    workflows: Option<Stamped<Vec<Workflow>>>,
    /// The most recent run of each workflow, taken from the repository-wide
    /// run list.
    #[serde(default)]
    latest_runs: Option<Stamped<Vec<WorkflowRun>>>,
    /// Unix seconds of the last store, for eviction.
    #[serde(default)]
    touched_at: u64,
}

impl RepoEntry {
    fn is_empty(&self) -> bool {
        self.workflows.is_none() && self.latest_runs.is_none()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Snapshot {
    version: u32,
    /// Fingerprint of the token the data was fetched with.
    owner: String,
    #[serde(default)]
    repos: Option<Stamped<Vec<Repo>>>,
    /// Keyed by repository id, which survives renames and transfers.
    #[serde(default)]
    by_repo: HashMap<i64, RepoEntry>,
}

impl Snapshot {
    fn empty(owner: &str) -> Self {
        Self {
            version: SNAPSHOT_VERSION,
            owner: owner.to_string(),
            repos: None,
            by_repo: HashMap::new(),
        }
    }

    fn repo_id(&self, owner_login: &str, repo_name: &str) -> Option<i64> {
        let full_name = format!("{owner_login}/{repo_name}");
        self.repos.as_ref().and_then(|repos| {
            repos
                .value
                .iter()
                .find(|repo| repo.full_name.eq_ignore_ascii_case(&full_name))
                .map(|repo| repo.id)
        })
    }

    /// Drops what is older than `retention` and recomputes the change hashes.
    fn prepare_loaded(&mut self, now: u64, retention: Duration) {
        let fresh = |fetched_at: u64| now.saturating_sub(fetched_at) <= retention.as_secs();

        if self
            .repos
            .as_ref()
            .is_some_and(|repos| !fresh(repos.fetched_at))
        {
            self.repos = None;
        }
        for entry in self.by_repo.values_mut() {
            if entry
                .workflows
                .as_ref()
                .is_some_and(|w| !fresh(w.fetched_at))
            {
                entry.workflows = None;
            }
            if entry
                .latest_runs
                .as_ref()
                .is_some_and(|r| !fresh(r.fetched_at))
            {
                entry.latest_runs = None;
            }
        }
        self.by_repo.retain(|_, entry| !entry.is_empty());

        if let Some(repos) = self.repos.as_mut() {
            repos.rehash();
        }
        for entry in self.by_repo.values_mut() {
            if let Some(workflows) = entry.workflows.as_mut() {
                workflows.rehash();
            }
            if let Some(runs) = entry.latest_runs.as_mut() {
                runs.rehash();
            }
        }
    }

    /// Fills whatever this snapshot lacks from `older`. Used when the disk
    /// finishes loading after the network has already answered: the network's
    /// data wins wherever both have something.
    fn fill_from(&mut self, older: Snapshot) {
        if self.repos.is_none() {
            self.repos = older.repos;
        }
        let listed: Option<std::collections::HashSet<i64>> = self
            .repos
            .as_ref()
            .map(|repos| repos.value.iter().map(|repo| repo.id).collect());
        for (id, old_entry) in older.by_repo {
            // A repository the newer list no longer has stays gone.
            if listed.as_ref().is_some_and(|ids| !ids.contains(&id)) {
                continue;
            }
            let entry = self.by_repo.entry(id).or_default();
            if entry.workflows.is_none() {
                entry.workflows = old_entry.workflows;
            }
            if entry.latest_runs.is_none() {
                entry.latest_runs = old_entry.latest_runs;
            }
            entry.touched_at = entry.touched_at.max(old_entry.touched_at);
        }
        self.evict_beyond(MAX_DETAILED_REPOS);
    }

    fn evict_beyond(&mut self, max: usize) {
        while self.by_repo.len() > max {
            let Some(oldest) = self
                .by_repo
                .iter()
                .min_by_key(|(_, entry)| entry.touched_at)
                .map(|(id, _)| *id)
            else {
                break;
            };
            self.by_repo.remove(&oldest);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RunVersion {
    completed: bool,
    updated_at: Option<String>,
}

impl RunVersion {
    fn of(run: &WorkflowRun) -> Self {
        Self {
            completed: run
                .status
                .as_deref()
                .is_some_and(|status| status.eq_ignore_ascii_case("completed")),
            updated_at: run.updated_at.clone(),
        }
    }

    /// Only a completed run with a known `updated_at` can vouch for its jobs.
    fn is_settled(&self) -> bool {
        self.completed && self.updated_at.is_some()
    }

    /// Folds a newly fetched version into the known one. Several run lists are
    /// in flight at once, and a response that left before a re-run can arrive
    /// after one that left later: `updated_at` never moves backwards here, so a
    /// late answer cannot make a re-run run look completed again. When two
    /// answers carry the same `updated_at` but disagree, it is not settled.
    fn merged_with(self, incoming: RunVersion) -> RunVersion {
        // GitHub's timestamps are ISO-8601 in UTC, which order as text.
        match (&self.updated_at, &incoming.updated_at) {
            (Some(known), Some(new)) if new < known => self,
            (Some(known), Some(new)) if new == known => RunVersion {
                completed: self.completed && incoming.completed,
                updated_at: incoming.updated_at,
            },
            (Some(_), None) => RunVersion {
                completed: false,
                updated_at: self.updated_at,
            },
            _ => incoming,
        }
    }
}

/// Taken before a jobs request goes out: the jobs are kept only if the run is
/// still at this version when they come back.
#[derive(Debug, Clone)]
pub struct JobsFetch {
    run_id: i64,
    version: Option<RunVersion>,
}

#[derive(Debug, Default)]
struct State {
    /// The fingerprint of the bound token; `None` while signed out.
    owner: Option<Arc<str>>,
    /// Bumped on every bind of a different token and on every clear.
    generation: u64,
    /// Bumped on every clear only: a disk load that started before a clear
    /// must not land after it.
    clears: u64,
    data: Snapshot,
    /// A snapshot loaded from disk before any token was bound.
    pending: Option<Snapshot>,
    run_versions: HashMap<i64, RunVersion>,
    jobs: HashMap<i64, (RunVersion, Arc<Vec<Job>>)>,
    jobs_order: VecDeque<i64>,
    /// A value changed since the last write.
    dirty: bool,
    /// Only fetch times changed since the last write; written with the next
    /// real change or at shutdown.
    touched: bool,
    write_scheduled: bool,
}

impl State {
    fn accepts(&self, owner: &CacheOwner) -> bool {
        self.generation == owner.generation
            && self.owner.as_deref() == Some(owner.fingerprint.as_ref())
    }

    fn reset_session(&mut self) {
        self.run_versions.clear();
        self.jobs.clear();
        self.jobs_order.clear();
        self.dirty = false;
        self.touched = false;
    }

    fn remove_jobs(&mut self, run_id: i64) {
        if self.jobs.remove(&run_id).is_some() {
            self.jobs_order.retain(|id| *id != run_id);
        }
    }

    fn note_run_versions(&mut self, runs: &[WorkflowRun]) {
        if self.run_versions.len() + runs.len() > MAX_RUN_VERSIONS {
            // The jobs go with the index: without their versions' history a
            // stale list could vouch for them again.
            self.run_versions.clear();
            self.jobs.clear();
            self.jobs_order.clear();
        }
        for run in runs {
            let incoming = RunVersion::of(run);
            let merged = match self.run_versions.remove(&run.id) {
                Some(known) => known.merged_with(incoming),
                None => incoming,
            };
            // Jobs stored under another version can never be served again.
            if self
                .jobs
                .get(&run.id)
                .is_some_and(|(stored, _)| *stored != merged)
            {
                self.remove_jobs(run.id);
            }
            self.run_versions.insert(run.id, merged);
        }
    }
}

#[derive(Debug)]
struct Persistence {
    path: PathBuf,
    retention: Duration,
    write_debounce: Duration,
    /// Serializes file writes and deletions against each other.
    io_lock: std::sync::Mutex<()>,
}

#[derive(Debug)]
pub struct DataCache {
    state: Mutex<State>,
    persistence: Option<Persistence>,
    ready: watch::Sender<bool>,
}

impl Default for DataCache {
    fn default() -> Self {
        Self::new()
    }
}

impl DataCache {
    /// A cache that lives in memory only.
    pub fn new() -> Self {
        Self {
            state: Mutex::new(State::default()),
            persistence: None,
            ready: watch::Sender::new(true),
        }
    }

    /// A cache backed by a file. Nothing is read until
    /// [`hydrate_from_disk`](Self::hydrate_from_disk).
    pub fn with_persistence(config: CachePersistenceConfig) -> Self {
        Self {
            state: Mutex::new(State::default()),
            persistence: Some(Persistence {
                path: config.path,
                retention: config.retention,
                write_debounce: WRITE_DEBOUNCE,
                io_lock: std::sync::Mutex::new(()),
            }),
            ready: watch::Sender::new(false),
        }
    }

    /// Shortens the write debounce, so a test can watch the debounced writer
    /// run without waiting half a minute.
    #[cfg(test)]
    fn with_write_debounce(mut self, debounce: Duration) -> Self {
        if let Some(persistence) = self.persistence.as_mut() {
            persistence.write_debounce = debounce;
        }
        self
    }

    pub fn has_persistence(&self) -> bool {
        self.persistence.is_some()
    }

    /// Loads the file, if any. Returns whether a usable snapshot was found.
    ///
    /// The load may finish before or after a token is bound; either way the
    /// data reaches only the account it belongs to, and never lands after a
    /// [`clear_all`](Self::clear_all) that happened while it was reading.
    pub async fn hydrate_from_disk(&self) -> bool {
        let Some(persistence) = &self.persistence else {
            self.ready.send_replace(true);
            return false;
        };

        let clears_at_start = self.state.lock().clears;
        let path = persistence.path.clone();
        let retention = persistence.retention;
        let loaded = tokio::task::spawn_blocking(move || read_snapshot(&path, retention))
            .await
            .unwrap_or_else(|err| {
                warn!("Cache load task failed: {}", err);
                None
            });

        let found = loaded.is_some();
        if let Some(snapshot) = loaded {
            let mut state = self.state.lock();
            if state.clears != clears_at_start {
                debug!("Discarding cache snapshot: the cache was cleared while it loaded");
            } else {
                match state.owner.clone() {
                    Some(owner) if owner.as_ref() == snapshot.owner => {
                        state.data.fill_from(snapshot);
                    }
                    Some(_) => {
                        debug!("Discarding cache snapshot: it belongs to another account");
                    }
                    None => state.pending = Some(snapshot),
                }
            }
        }

        self.ready.send_replace(true);
        found
    }

    /// Resolves once the disk load has finished (immediately for a memory-only
    /// cache).
    pub async fn ready(&self) {
        let mut receiver = self.ready.subscribe();
        let _ = receiver.wait_for(|ready| *ready).await;
    }

    /// Binds the cache to the account behind `token`.
    ///
    /// Binding the token already bound keeps the data and the owner; binding
    /// another one starts that account from whatever the disk held for it.
    pub fn bind_owner(&self, token: &str) -> CacheOwner {
        let fingerprint: Arc<str> = Arc::from(token_fingerprint(token));
        let mut state = self.state.lock();

        if state.owner.as_deref() != Some(fingerprint.as_ref()) {
            state.generation += 1;
            state.owner = Some(fingerprint.clone());
            state.data = match state.pending.take() {
                Some(snapshot) if snapshot.owner == fingerprint.as_ref() => snapshot,
                _ => Snapshot::empty(&fingerprint),
            };
            state.reset_session();
        }

        CacheOwner {
            fingerprint,
            generation: state.generation,
        }
    }

    /// Forgets everything in memory and retires every owner. The file is left
    /// alone — see [`remove_persisted`](Self::remove_persisted) — because this
    /// also runs when the keyring merely failed to answer, and the next start
    /// may still want the data. Nothing is written until a token is bound again.
    pub fn clear_all(&self) {
        let mut state = self.state.lock();
        state.generation += 1;
        state.clears += 1;
        state.owner = None;
        state.data = Snapshot::default();
        state.pending = None;
        state.reset_session();
    }

    /// Deletes the file. Blocking; call it off the main thread.
    pub fn remove_persisted(&self) {
        let Some(persistence) = &self.persistence else {
            return;
        };
        let _io = persistence
            .io_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match std::fs::remove_file(&persistence.path) {
            Ok(()) => info!("Removed the data cache file"),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => warn!("Failed to remove the data cache file: {}", err),
        }
    }

    // ---- writes -----------------------------------------------------------

    /// Stores the repository list and drops the entries of repositories that
    /// are no longer on it.
    pub fn store_repos(self: &Arc<Self>, owner: &CacheOwner, repos: &[Repo]) {
        let mut state = self.state.lock();
        if !state.accepts(owner) {
            return;
        }
        let now = unix_now();
        let changed = replace_stamped(&mut state.data.repos, repos.to_vec(), now);

        let before = state.data.by_repo.len();
        state
            .data
            .by_repo
            .retain(|id, _| repos.iter().any(|repo| repo.id == *id));
        let pruned = state.data.by_repo.len() != before;

        self.note_change(&mut state, changed || pruned);
    }

    pub fn store_workflows(
        self: &Arc<Self>,
        owner: &CacheOwner,
        owner_login: &str,
        repo_name: &str,
        workflows: &[Workflow],
    ) {
        let mut state = self.state.lock();
        if !state.accepts(owner) {
            return;
        }
        let Some(repo_id) = state.data.repo_id(owner_login, repo_name) else {
            return;
        };
        let now = unix_now();
        let entry = state.data.by_repo.entry(repo_id).or_default();
        entry.touched_at = now;
        let changed = replace_stamped(&mut entry.workflows, workflows.to_vec(), now);
        state.data.evict_beyond(MAX_DETAILED_REPOS);
        self.note_change(&mut state, changed);
    }

    /// Stores the latest run of each workflow from a repository-wide run list,
    /// and records every run's live version.
    pub fn store_repository_runs(
        self: &Arc<Self>,
        owner: &CacheOwner,
        owner_login: &str,
        repo_name: &str,
        runs: &[WorkflowRun],
    ) {
        let mut state = self.state.lock();
        if !state.accepts(owner) {
            return;
        }
        state.note_run_versions(runs);

        let Some(repo_id) = state.data.repo_id(owner_login, repo_name) else {
            return;
        };
        let now = unix_now();
        let entry = state.data.by_repo.entry(repo_id).or_default();
        entry.touched_at = now;
        let changed = replace_stamped(&mut entry.latest_runs, latest_run_per_workflow(runs), now);
        state.data.evict_beyond(MAX_DETAILED_REPOS);
        self.note_change(&mut state, changed);
    }

    /// Records the live version of each run, for the jobs check.
    pub fn note_runs(self: &Arc<Self>, owner: &CacheOwner, runs: &[WorkflowRun]) {
        let mut state = self.state.lock();
        if state.accepts(owner) {
            state.note_run_versions(runs);
        }
    }

    /// Forgets what is known about a run that was just re-run or cancelled, so
    /// its jobs are fetched again even before a run list shows the change.
    pub fn forget_run(self: &Arc<Self>, owner: &CacheOwner, run_id: i64) {
        let mut state = self.state.lock();
        if !state.accepts(owner) {
            return;
        }
        state.remove_jobs(run_id);
        if let Some(version) = state.run_versions.get_mut(&run_id) {
            version.completed = false;
        }
    }

    /// Notes the run's version before its jobs are requested; see
    /// [`store_jobs`](Self::store_jobs).
    pub fn begin_jobs_fetch(&self, owner: &CacheOwner, run_id: i64) -> JobsFetch {
        let state = self.state.lock();
        let version = state
            .accepts(owner)
            .then(|| state.run_versions.get(&run_id).cloned())
            .flatten();
        JobsFetch { run_id, version }
    }

    /// Keeps the jobs of a run, but only when the run was completed when they
    /// were requested and still is at that same version, and every job has
    /// finished: anything still moving would be frozen in its intermediate
    /// state. An empty list is not kept either — it is more likely a transient
    /// answer than a run without jobs.
    pub fn store_jobs(self: &Arc<Self>, owner: &CacheOwner, fetch: JobsFetch, jobs: &[Job]) {
        let mut state = self.state.lock();
        if !state.accepts(owner) {
            return;
        }
        let run_id = fetch.run_id;
        let current = state.run_versions.get(&run_id);
        let unchanged = fetch
            .version
            .as_ref()
            .is_some_and(|version| version.is_settled() && Some(version) == current);
        let all_completed = !jobs.is_empty()
            && jobs.iter().all(|job| {
                job.status
                    .as_deref()
                    .is_some_and(|status| status.eq_ignore_ascii_case("completed"))
            });
        state.remove_jobs(run_id);
        let (true, true, Some(version)) = (unchanged, all_completed, fetch.version) else {
            return;
        };

        state
            .jobs
            .insert(run_id, (version, Arc::new(jobs.to_vec())));
        state.jobs_order.push_back(run_id);
        while state.jobs_order.len() > MAX_CACHED_JOB_RUNS {
            if let Some(evicted) = state.jobs_order.pop_front() {
                state.jobs.remove(&evicted);
            }
        }
    }

    // ---- reads ------------------------------------------------------------

    pub fn repos(&self, owner: &CacheOwner) -> Option<Cached<Vec<Repo>>> {
        let state = self.state.lock();
        if !state.accepts(owner) {
            return None;
        }
        state.data.repos.as_ref().map(Stamped::to_cached)
    }

    pub fn workflows(&self, owner: &CacheOwner, repo_id: i64) -> Option<Cached<Vec<Workflow>>> {
        let state = self.state.lock();
        if !state.accepts(owner) {
            return None;
        }
        state
            .data
            .by_repo
            .get(&repo_id)
            .and_then(|entry| entry.workflows.as_ref())
            .map(Stamped::to_cached)
    }

    pub fn latest_runs(
        &self,
        owner: &CacheOwner,
        repo_id: i64,
    ) -> Option<Cached<Vec<WorkflowRun>>> {
        let state = self.state.lock();
        if !state.accepts(owner) {
            return None;
        }
        state
            .data
            .by_repo
            .get(&repo_id)
            .and_then(|entry| entry.latest_runs.as_ref())
            .map(Stamped::to_cached)
    }

    /// The jobs of a run, if the run's latest live version is still the
    /// completed one they were stored under.
    pub fn completed_jobs(&self, owner: &CacheOwner, run_id: i64) -> Option<Arc<Vec<Job>>> {
        let state = self.state.lock();
        if !state.accepts(owner) {
            return None;
        }
        let live = state.run_versions.get(&run_id)?;
        let (stored, jobs) = state.jobs.get(&run_id)?;
        (live.is_settled() && live == stored).then(|| jobs.clone())
    }

    // ---- persistence ------------------------------------------------------

    /// Writes pending changes now. Blocking: for shutdown, and for the
    /// debounced writer, which calls it from a blocking thread.
    pub fn flush_blocking(&self) {
        let Some(persistence) = &self.persistence else {
            return;
        };
        let _io = persistence
            .io_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        let Some((bytes, generation)) = self.take_pending_write() else {
            return;
        };
        let temp = match write_private_temp(&persistence.path, &bytes) {
            Ok(temp) => temp,
            Err(err) => {
                // Retried with the next change or at shutdown.
                warn!("Failed to write the data cache: {}", err);
                self.state.lock().dirty = true;
                return;
            }
        };
        self.commit_write(&persistence.path, &temp, generation);
    }

    /// The serialized snapshot and the generation it belongs to, if there is
    /// anything to write for a bound owner.
    fn take_pending_write(&self) -> Option<(Vec<u8>, u64)> {
        let (snapshot, generation) = {
            let mut state = self.state.lock();
            state.write_scheduled = false;
            if state.owner.is_none() || !(state.dirty || state.touched) {
                return None;
            }
            state.dirty = false;
            state.touched = false;
            (state.data.clone(), state.generation)
        };
        match serde_json::to_vec(&snapshot) {
            Ok(bytes) => Some((bytes, generation)),
            Err(err) => {
                warn!("Failed to serialize the data cache: {}", err);
                None
            }
        }
    }

    /// Moves a written temporary file over the cache file, unless the cache
    /// was cleared or rebound since the snapshot was taken.
    ///
    /// The check and the rename are not atomic, and need not be: a clear that
    /// slips in between leaves a file the next start ignores unless it belongs
    /// to the account signing in, and an explicit sign-out follows its clear
    /// with [`remove_persisted`](Self::remove_persisted), which waits for this
    /// write on the I/O lock and deletes the file after it. Renaming outside
    /// the state lock keeps a slow filesystem from stalling the main thread,
    /// where `clear_all` runs.
    fn commit_write(&self, path: &Path, temp: &Path, generation: u64) -> bool {
        if self.state.lock().generation != generation {
            let _ = std::fs::remove_file(temp);
            return false;
        }
        if let Err(err) = std::fs::rename(temp, path) {
            // Retried with the next change or at shutdown.
            warn!("Failed to replace the data cache file: {}", err);
            let _ = std::fs::remove_file(temp);
            self.state.lock().dirty = true;
            return false;
        }
        true
    }

    fn note_change(self: &Arc<Self>, state: &mut State, changed: bool) {
        if changed {
            state.dirty = true;
        } else {
            state.touched = true;
        }
        if state.dirty {
            self.schedule_write(state);
        }
    }

    fn schedule_write(self: &Arc<Self>, state: &mut State) {
        if self.persistence.is_none() || state.write_scheduled {
            return;
        }
        // Stores happen on Tokio tasks; outside a runtime (unit tests) the
        // shutdown flush is the only write.
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let Some(debounce) = self.persistence.as_ref().map(|p| p.write_debounce) else {
            return;
        };
        state.write_scheduled = true;
        let cache = self.clone();
        handle.spawn(async move {
            tokio::time::sleep(debounce).await;
            if let Err(err) = tokio::task::spawn_blocking(move || cache.flush_blocking()).await {
                warn!("Cache write task failed: {}", err);
            }
        });
    }
}

/// Replaces a stamped value, returning whether the value itself changed. The
/// fetch time moves either way.
fn replace_stamped<T: Serialize>(slot: &mut Option<Stamped<T>>, value: T, now: u64) -> bool {
    let fresh = Stamped::new(value, now);
    let changed = slot.as_ref().is_none_or(|old| old.hash != fresh.hash);
    *slot = Some(fresh);
    changed
}

/// The first run seen for each workflow. GitHub lists runs newest first.
fn latest_run_per_workflow(runs: &[WorkflowRun]) -> Vec<WorkflowRun> {
    let mut seen = std::collections::HashSet::new();
    runs.iter()
        .filter(|run| run.workflow_id.is_some_and(|id| seen.insert(id)))
        .cloned()
        .collect()
}

fn value_hash<T: Serialize + ?Sized>(value: &T) -> u64 {
    let mut hasher = DefaultHasher::new();
    serde_json::to_vec(value)
        .unwrap_or_default()
        .hash(&mut hasher);
    hasher.finish()
}

/// A short, one-way identifier for a token. Enough to tell two accounts apart;
/// useless for recovering the token.
fn token_fingerprint(token: &str) -> String {
    Sha256::digest(token.as_bytes())
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

fn read_snapshot(path: &Path, retention: Duration) -> Option<Snapshot> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return None,
        Err(err) => {
            warn!("Failed to read the data cache: {}", err);
            return None;
        }
    };
    let mut snapshot: Snapshot = match serde_json::from_slice(&bytes) {
        Ok(snapshot) => snapshot,
        Err(err) => {
            warn!("Ignoring an unreadable data cache: {}", err);
            return None;
        }
    };
    if snapshot.version != SNAPSHOT_VERSION {
        info!(
            "Ignoring a data cache with layout version {}",
            snapshot.version
        );
        return None;
    }
    snapshot.prepare_loaded(unix_now(), retention);
    Some(snapshot)
}

/// Writes `bytes` to a new file next to `path`, readable by the user only, and
/// returns its path. The name is unique per process and call: the app allows
/// several instances, and they must not write through each other's file.
fn write_private_temp(path: &Path, bytes: &[u8]) -> std::io::Result<PathBuf> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;

    let mut suffix = [0u8; 8];
    if getrandom::fill(&mut suffix).is_err() {
        suffix = unix_now().to_le_bytes();
    }
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(CACHE_FILE_NAME);
    let temp = parent.join(format!(
        ".{file_name}.{}.{:016x}.tmp",
        std::process::id(),
        u64::from_le_bytes(suffix)
    ));

    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = options.open(&temp).and_then(|mut file| {
        file.write_all(bytes)?;
        file.sync_all()
    });
    if let Err(err) = result {
        let _ = std::fs::remove_file(&temp);
        return Err(err);
    }
    Ok(temp)
}

#[cfg(test)]
mod tests;
