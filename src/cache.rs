use crate::api::models::{Job, Workflow, WorkflowRun};
use dirs::cache_dir;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io;
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{Mutex as TokioMutex, RwLock};
use tracing::{debug, warn};

const CACHE_VERSION: u32 = 1;
const CACHE_FILE_NAME: &str = "data-cache.json";
const DEFAULT_CACHE_TTL: Duration = Duration::from_secs(900);
#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorkflowCache {
    runs: Arc<Vec<WorkflowRun>>,
    jobs: HashMap<i64, Arc<Vec<Job>>>, // run_id -> jobs
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RepoCacheEntry {
    workflows: Arc<Vec<Workflow>>,
    workflow_data: HashMap<i64, WorkflowCache>, // workflow_id -> cache
}

#[derive(Debug, Clone, Serialize)]
struct PersistentCacheSnapshot<'a> {
    version: u32,
    saved_at: u64,
    repositories: &'a HashMap<String, RepoCacheEntry>,
}

#[derive(Debug, Deserialize)]
struct OwnedPersistentCacheSnapshot {
    version: u32,
    saved_at: u64,
    repositories: HashMap<String, RepoCacheEntry>,
}

#[derive(Debug, Clone)]
pub struct CachePersistenceConfig {
    path: PathBuf,
    ttl: Duration,
}

impl CachePersistenceConfig {
    pub fn new(path: PathBuf, ttl: Duration) -> Self {
        Self { path, ttl }
    }

    pub fn for_app(app_id: &str) -> Option<Self> {
        let mut path = cache_dir()?;
        path.push(app_id);
        path.push(CACHE_FILE_NAME);
        Some(Self::new(path, DEFAULT_CACHE_TTL))
    }
}
#[derive(Debug)]
struct CachePersistenceInner {
    path: PathBuf,
    ttl: Duration,
}

impl From<CachePersistenceConfig> for CachePersistenceInner {
    fn from(config: CachePersistenceConfig) -> Self {
        Self {
            path: config.path,
            ttl: config.ttl,
        }
    }
}

impl CachePersistenceInner {
    async fn load_snapshot(&self) -> io::Result<Option<HashMap<String, RepoCacheEntry>>> {
        let mut file = match fs::File::open(&self.path).await {
            Ok(file) => file,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(err),
        };

        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).await?;

        let snapshot: OwnedPersistentCacheSnapshot = match serde_json::from_slice(&bytes) {
            Ok(snapshot) => snapshot,
            Err(err) => {
                warn!(
                    path = %self.path.display(),
                    "Dropping corrupt cache snapshot: {}",
                    err
                );
                self.remove_file().await;
                return Ok(None);
            }
        };

        if snapshot.version != CACHE_VERSION {
            warn!(
                path = %self.path.display(),
                "Ignoring cache snapshot with incompatible version {}",
                snapshot.version
            );
            self.remove_file().await;
            return Ok(None);
        }

        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if now_secs.saturating_sub(snapshot.saved_at) > self.ttl.as_secs() {
            debug!(
                path = %self.path.display(),
                "Cache snapshot expired (saved_at: {}, ttl: {:?})",
                snapshot.saved_at,
                self.ttl
            );
            self.remove_file().await;
            return Ok(None);
        }

        Ok(Some(snapshot.repositories))
    }

    async fn write_snapshot(&self, payload: &[u8]) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).await?;
        }

        let tmp_path = self.path.with_extension("tmp");
        {
            let mut file = fs::File::create(&tmp_path).await?;
            file.write_all(payload).await?;
            file.sync_all().await?;
        }

        if let Err(err) = fs::rename(&tmp_path, &self.path).await {
            let _ = fs::remove_file(&tmp_path).await;
            return Err(err);
        }

        Ok(())
    }

    async fn remove_file(&self) {
        if let Err(err) = fs::remove_file(&self.path).await
            && err.kind() != io::ErrorKind::NotFound
        {
            warn!(
                path = %self.path.display(),
                "Failed to remove cache snapshot: {}",
                err
            );
        }
    }
}
#[derive(Debug, Clone)]
pub struct DataCache {
    repositories: Arc<RwLock<HashMap<String, RepoCacheEntry>>>,
    persistence: Option<Arc<CachePersistenceInner>>,
    save_guard: Arc<TokioMutex<()>>,
}

impl DataCache {
    pub fn new() -> Self {
        Self {
            repositories: Arc::new(RwLock::new(HashMap::new())),
            persistence: None,
            save_guard: Arc::new(TokioMutex::new(())),
        }
    }

    pub fn with_persistence(config: CachePersistenceConfig) -> Self {
        Self {
            repositories: Arc::new(RwLock::new(HashMap::new())),
            persistence: Some(Arc::new(CachePersistenceInner::from(config))),
            save_guard: Arc::new(TokioMutex::new(())),
        }
    }

    pub fn has_persistence(&self) -> bool {
        self.persistence.is_some()
    }

    pub async fn hydrate_from_disk(&self) -> bool {
        let Some(persistence) = &self.persistence else {
            return false;
        };

        match persistence.load_snapshot().await {
            Ok(Some(snapshot)) => {
                let mut repos = self.repositories.write().await;
                *repos = snapshot;
                debug!(
                    path = %persistence.path.display(),
                    "Hydrated DataCache from disk"
                );
                true
            }
            Ok(None) => false,
            Err(err) => {
                warn!(
                    path = %persistence.path.display(),
                    "Failed to hydrate cache: {}",
                    err
                );
                false
            }
        }
    }

    async fn persist_if_needed(&self) {
        let Some(persistence) = self.persistence.clone() else {
            return;
        };

        let saved_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let snapshot_bytes = {
            let repositories = self.repositories.read().await;
            let snapshot = PersistentCacheSnapshot {
                version: CACHE_VERSION,
                saved_at,
                repositories: repositories.deref(),
            };
            match serde_json::to_vec(&snapshot) {
                Ok(bytes) => bytes,
                Err(err) => {
                    warn!("Failed to serialize cache snapshot: {}", err);
                    return;
                }
            }
        };

        let _guard = self.save_guard.lock().await;
        if let Err(err) = persistence.write_snapshot(&snapshot_bytes).await {
            warn!(
                path = %persistence.path.display(),
                "Failed to persist cache snapshot: {}",
                err
            );
        } else {
            debug!(
                path = %persistence.path.display(),
                size = snapshot_bytes.len(),
                "Persisted DataCache to disk"
            );
        }
    }
    pub async fn workflows(&self, key: &str) -> Option<Arc<Vec<Workflow>>> {
        let repos = self.repositories.read().await;
        repos.get(key).map(|entry| entry.workflows.clone())
    }

    pub async fn store_workflows(&self, workflows: Arc<Vec<Workflow>>, key: &str) {
        {
            let mut repos = self.repositories.write().await;
            let entry = repos.entry(key.to_string()).or_insert(RepoCacheEntry {
                workflows: Arc::new(Vec::new()),
                workflow_data: HashMap::new(),
            });
            entry.workflows = workflows;
        }

        self.persist_if_needed().await;
    }

    pub async fn runs(&self, key: &str, workflow_id: i64) -> Option<Arc<Vec<WorkflowRun>>> {
        let repos = self.repositories.read().await;
        repos
            .get(key)
            .and_then(|entry| entry.workflow_data.get(&workflow_id))
            .map(|cache| cache.runs.clone())
    }

    pub async fn store_runs(&self, runs: Arc<Vec<WorkflowRun>>, key: &str, workflow_id: i64) {
        {
            let mut repos = self.repositories.write().await;
            let entry = repos.entry(key.to_string()).or_insert(RepoCacheEntry {
                workflows: Arc::new(Vec::new()),
                workflow_data: HashMap::new(),
            });

            let workflow_cache = entry
                .workflow_data
                .entry(workflow_id)
                .or_insert(WorkflowCache {
                    runs: Arc::new(Vec::new()),
                    jobs: HashMap::new(),
                });

            let valid_run_ids: std::collections::HashSet<_> = runs.iter().map(|r| r.id).collect();
            workflow_cache.runs = runs;
            workflow_cache
                .jobs
                .retain(|run_id, _| valid_run_ids.contains(run_id));
        }

        self.persist_if_needed().await;
    }

    pub async fn jobs(&self, key: &str, workflow_id: i64, run_id: i64) -> Option<Arc<Vec<Job>>> {
        let repos = self.repositories.read().await;
        repos
            .get(key)
            .and_then(|entry| entry.workflow_data.get(&workflow_id))
            .and_then(|cache| cache.jobs.get(&run_id))
            .cloned()
    }

    pub async fn store_jobs(&self, jobs: Arc<Vec<Job>>, key: &str, workflow_id: i64, run_id: i64) {
        {
            let mut repos = self.repositories.write().await;
            let entry = repos.entry(key.to_string()).or_insert(RepoCacheEntry {
                workflows: Arc::new(Vec::new()),
                workflow_data: HashMap::new(),
            });

            let workflow_cache = entry
                .workflow_data
                .entry(workflow_id)
                .or_insert(WorkflowCache {
                    runs: Arc::new(Vec::new()),
                    jobs: HashMap::new(),
                });

            workflow_cache.jobs.insert(run_id, jobs);
        }

        self.persist_if_needed().await;
    }

    pub async fn clear_repo(&self, key: &str) {
        {
            let mut repos = self.repositories.write().await;
            repos.remove(key);
        }
        self.persist_if_needed().await;
    }

    pub async fn clear_all(&self) {
        {
            let mut repos = self.repositories.write().await;
            repos.clear();
        }
        self.persist_if_needed().await;
    }
}
impl Default for DataCache {
    fn default() -> Self {
        Self::new()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    fn sample_workflow(id: i64) -> Workflow {
        Workflow {
            id,
            name: format!("workflow-{id}"),
            path: format!(".github/workflows/{id}.yml"),
        }
    }

    fn sample_run(id: i64, name: &str) -> WorkflowRun {
        WorkflowRun {
            id,
            run_number: Some(id),
            name: Some(name.to_string()),
            display_title: Some(name.to_string()),
            head_branch: Some("main".into()),
            status: Some("completed".into()),
            conclusion: Some("success".into()),
            run_started_at: None,
            event: None,
            created_at: None,
            updated_at: None,
            html_url: None,
        }
    }

    fn sample_job(id: i64, run_id: i64, name: &str) -> Job {
        Job {
            id,
            run_id,
            status: Some("completed".into()),
            conclusion: Some("success".into()),
            started_at: None,
            completed_at: None,
            name: Some(name.into()),
            html_url: None,
        }
    }

    #[tokio::test]
    async fn test_workflow_caching() {
        let cache = DataCache::new();
        let key = "owner/repo";

        assert!(cache.workflows(key).await.is_none());

        let workflows = Arc::new(Vec::new());
        cache.store_workflows(workflows.clone(), key).await;
        let cached = cache.workflows(key).await.unwrap();
        assert_eq!(cached.as_ref(), workflows.as_ref());
    }

    #[tokio::test]
    async fn test_clear_cache() {
        let cache = DataCache::new();
        let key = "owner/repo";

        cache.store_workflows(Arc::new(Vec::new()), key).await;
        assert!(cache.workflows(key).await.is_some());

        cache.clear_repo(key).await;
        assert!(cache.workflows(key).await.is_none());
    }

    #[tokio::test]
    async fn workflows_cache_returns_arc_snapshot() {
        let cache = DataCache::new();
        let key = "owner/repo";
        let workflows = Arc::new(vec![sample_workflow(7)]);

        cache.store_workflows(workflows.clone(), key).await;
        let cached = cache.workflows(key).await.unwrap();

        assert!(Arc::ptr_eq(&workflows, &cached));
    }

    #[tokio::test]
    async fn runs_cache_returns_arc_snapshot() {
        let cache = DataCache::new();
        let key = "owner/repo";
        let workflow_id = 99;
        let runs = Arc::new(vec![sample_run(1, "Run 1")]);

        cache.store_runs(runs.clone(), key, workflow_id).await;
        let cached = cache.runs(key, workflow_id).await.unwrap();

        assert!(Arc::ptr_eq(&runs, &cached));
    }

    #[tokio::test]
    async fn jobs_cache_returns_arc_snapshot() {
        let cache = DataCache::new();
        let key = "owner/repo";
        let workflow_id = 77;
        let run_id = 5;
        let jobs = Arc::new(vec![sample_job(1, run_id, "job")]);

        cache
            .store_jobs(jobs.clone(), key, workflow_id, run_id)
            .await;
        let cached = cache.jobs(key, workflow_id, run_id).await.unwrap();

        assert!(Arc::ptr_eq(&jobs, &cached));
    }

    #[tokio::test]
    async fn persistent_cache_round_trip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("cache.json");
        let config = CachePersistenceConfig::new(path.clone(), Duration::from_secs(3600));
        let cache = DataCache::with_persistence(config.clone());
        let key = "owner/repo";

        let workflow = Workflow {
            id: 42,
            name: "CI".into(),
            path: "ci.yml".into(),
        };
        cache
            .store_workflows(Arc::new(vec![workflow.clone()]), key)
            .await;

        let reloaded = DataCache::with_persistence(config);
        assert!(reloaded.hydrate_from_disk().await);
        let cached = reloaded.workflows(key).await.unwrap();
        assert_eq!(cached.as_ref(), &vec![workflow]);
    }

    #[tokio::test]
    async fn persistent_cache_ttl_expires() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("cache.json");
        let config = CachePersistenceConfig::new(path.clone(), Duration::from_secs(1));
        let cache = DataCache::with_persistence(config.clone());
        let key = "owner/repo";
        cache.store_workflows(Arc::new(Vec::new()), key).await;

        let mut file = fs::File::open(&path).await.unwrap();
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).await.unwrap();
        let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        value["saved_at"] = json!(0);
        drop(file);
        let mut writer = fs::File::create(&path).await.unwrap();
        writer
            .write_all(value.to_string().as_bytes())
            .await
            .unwrap();
        writer.sync_all().await.unwrap();

        let reloaded = DataCache::with_persistence(config);
        assert!(!reloaded.hydrate_from_disk().await);
        assert!(reloaded.workflows(key).await.is_none());
    }

    #[tokio::test]
    async fn persistent_cache_handles_corruption() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("cache.json");
        let config = CachePersistenceConfig::new(path.clone(), Duration::from_secs(60));
        let cache = DataCache::with_persistence(config.clone());
        cache
            .store_workflows(
                Arc::new(vec![Workflow {
                    id: 1,
                    name: "Test".into(),
                    path: "test.yml".into(),
                }]),
                "owner/repo",
            )
            .await;

        fs::write(&path, b"not-json").await.unwrap();

        let reloaded = DataCache::with_persistence(config);
        assert!(!reloaded.hydrate_from_disk().await);
        assert!(reloaded.workflows("owner/repo").await.is_none());
    }
}
