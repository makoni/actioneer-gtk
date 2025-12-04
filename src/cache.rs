use crate::api::models::{Job, Workflow, WorkflowRun};
use dirs::cache_dir;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

const CACHE_FILE_NAME: &str = "data-cache.json";
const DEFAULT_CACHE_TTL: Duration = Duration::from_secs(900);

#[derive(Debug, Clone)]
pub struct CachePersistenceConfig {
    _path: PathBuf,
    _ttl: Duration,
}

impl CachePersistenceConfig {
    pub fn new(path: PathBuf, ttl: Duration) -> Self {
        Self {
            _path: path,
            _ttl: ttl,
        }
    }

    pub fn for_app(app_id: &str) -> Option<Self> {
        let mut path = cache_dir()?;
        path.push(app_id);
        path.push(CACHE_FILE_NAME);
        Some(Self::new(path, DEFAULT_CACHE_TTL))
    }
}

/// A no-op cache implementation used while GitHub's APIs remain too volatile for
/// reliable client-side caching.
#[derive(Debug, Clone, Default)]
pub struct DataCache;

#[allow(dead_code)]
impl DataCache {
    pub fn new() -> Self {
        Self
    }

    pub fn with_persistence(_: CachePersistenceConfig) -> Self {
        Self
    }

    pub fn has_persistence(&self) -> bool {
        false
    }

    pub async fn hydrate_from_disk(&self) -> bool {
        false
    }

    pub async fn workflows(&self, _key: &str) -> Option<Arc<Vec<Workflow>>> {
        None
    }

    pub async fn store_workflows(&self, _workflows: Arc<Vec<Workflow>>, _key: &str) {}

    pub async fn runs(&self, _key: &str, _workflow_id: i64) -> Option<Arc<Vec<WorkflowRun>>> {
        None
    }

    pub async fn store_runs(&self, _runs: Arc<Vec<WorkflowRun>>, _key: &str, _workflow_id: i64) {}

    pub async fn jobs(&self, _key: &str, _workflow_id: i64, _run_id: i64) -> Option<Arc<Vec<Job>>> {
        None
    }

    pub async fn store_jobs(
        &self,
        _jobs: Arc<Vec<Job>>,
        _key: &str,
        _workflow_id: i64,
        _run_id: i64,
    ) {
    }

    pub async fn clear_repo(&self, _key: &str) {}

    pub async fn clear_all(&self) {}
}
