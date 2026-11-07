use super::*;
use crate::services::api::models::User;

fn repo(id: i64, full_name: &str) -> Repo {
    let (owner, name) = full_name.split_once('/').expect("owner/name");
    Repo {
        id,
        name: name.into(),
        full_name: full_name.into(),
        owner: User {
            login: owner.into(),
        },
        is_private: true,
        permissions: None,
        default_branch: Some("main".into()),
    }
}

fn workflow(id: i64) -> Workflow {
    Workflow {
        id,
        name: format!("wf-{id}"),
        path: format!(".github/workflows/{id}.yml"),
    }
}

fn run(id: i64, workflow_id: i64, status: &str, updated_at: &str) -> WorkflowRun {
    WorkflowRun {
        id,
        run_number: Some(id),
        workflow_id: Some(workflow_id),
        name: Some("CI".into()),
        display_title: None,
        head_branch: Some("main".into()),
        head_commit: None,
        status: Some(status.into()),
        conclusion: (status == "completed").then(|| "success".into()),
        run_started_at: None,
        event: Some("push".into()),
        created_at: None,
        updated_at: Some(updated_at.into()),
        html_url: None,
        actor: None,
        triggering_actor: None,
    }
}

fn job(id: i64, run_id: i64, status: &str) -> Job {
    Job {
        id,
        run_id,
        status: Some(status.into()),
        conclusion: None,
        started_at: None,
        completed_at: None,
        name: Some(format!("job-{id}")),
        steps: Vec::new(),
        html_url: None,
    }
}

fn store_jobs_now(cache: &Arc<DataCache>, owner: &CacheOwner, run_id: i64, jobs: &[Job]) {
    let fetch = cache.begin_jobs_fetch(owner, run_id);
    cache.store_jobs(owner, fetch, jobs);
}

fn memory_cache() -> Arc<DataCache> {
    Arc::new(DataCache::new())
}

fn file_cache(dir: &Path) -> Arc<DataCache> {
    Arc::new(DataCache::with_persistence(CachePersistenceConfig::new(
        dir.join("data-cache.json"),
        DEFAULT_RETENTION,
    )))
}

fn ids<T>(cached: &Cached<Vec<T>>, id: impl Fn(&T) -> i64) -> Vec<i64> {
    cached.value.iter().map(id).collect()
}

#[test]
fn stores_and_reads_back_per_account() {
    let cache = memory_cache();
    let owner = cache.bind_owner("token-a");

    cache.store_repos(&owner, &[repo(1, "octo/one"), repo(2, "octo/two")]);
    cache.store_workflows(&owner, "octo", "one", &[workflow(10), workflow(11)]);

    let repos = cache.repos(&owner).expect("repos");
    assert_eq!(ids(&repos, |r| r.id), vec![1, 2]);
    let workflows = cache.workflows(&owner, 1).expect("workflows of repo 1");
    assert_eq!(ids(&workflows, |w| w.id), vec![10, 11]);
    assert!(cache.workflows(&owner, 2).is_none());
}

#[test]
fn per_repo_data_is_keyed_by_id_and_matches_names_case_insensitively() {
    let cache = memory_cache();
    let owner = cache.bind_owner("token-a");
    cache.store_repos(&owner, &[repo(7, "Octo/Hello")]);

    cache.store_workflows(&owner, "octo", "hello", &[workflow(1)]);
    assert!(cache.workflows(&owner, 7).is_some());

    // A repository that is not on the list has no id to key by.
    cache.store_workflows(&owner, "octo", "unknown", &[workflow(2)]);
    assert_eq!(cache.state.lock().data.by_repo.len(), 1);
}

#[test]
fn a_new_repo_list_prunes_repos_that_are_gone() {
    let cache = memory_cache();
    let owner = cache.bind_owner("token-a");
    cache.store_repos(&owner, &[repo(1, "octo/one"), repo(2, "octo/two")]);
    cache.store_workflows(&owner, "octo", "one", &[workflow(10)]);
    cache.store_workflows(&owner, "octo", "two", &[workflow(20)]);

    cache.store_repos(&owner, &[repo(2, "octo/two")]);

    assert!(cache.workflows(&owner, 1).is_none());
    assert!(cache.workflows(&owner, 2).is_some());
}

#[test]
fn only_the_latest_run_of_each_workflow_is_kept() {
    let cache = memory_cache();
    let owner = cache.bind_owner("token-a");
    cache.store_repos(&owner, &[repo(1, "octo/one")]);

    cache.store_repository_runs(
        &owner,
        "octo",
        "one",
        &[
            run(103, 10, "in_progress", "t3"),
            run(102, 20, "completed", "t2"),
            run(101, 10, "completed", "t1"),
        ],
    );

    let latest = cache.latest_runs(&owner, 1).expect("latest runs");
    assert_eq!(ids(&latest, |r| r.id), vec![103, 102]);
}

#[test]
fn detailed_entries_are_capped_by_least_recent_store() {
    let cache = memory_cache();
    let owner = cache.bind_owner("token-a");
    let repos: Vec<Repo> = (0..=MAX_DETAILED_REPOS as i64)
        .map(|id| repo(id, &format!("octo/r{id}")))
        .collect();
    cache.store_repos(&owner, &repos);

    for (age, repo) in repos.iter().enumerate() {
        cache.store_workflows(&owner, "octo", &repo.name, &[workflow(1)]);
        // Same-second stores would tie; age them explicitly.
        cache
            .state
            .lock()
            .data
            .by_repo
            .get_mut(&repo.id)
            .expect("entry")
            .touched_at = age as u64;
    }
    cache.store_workflows(&owner, "octo", "r0", &[workflow(2)]);

    let state = cache.state.lock();
    assert_eq!(state.data.by_repo.len(), MAX_DETAILED_REPOS);
    assert!(
        state.data.by_repo.contains_key(&0),
        "the fresh store survives"
    );
    assert!(
        !state.data.by_repo.contains_key(&1),
        "the oldest is evicted"
    );
}

#[test]
fn another_account_sees_nothing_of_the_first() {
    let cache = memory_cache();
    let first = cache.bind_owner("token-a");
    cache.store_repos(&first, &[repo(1, "octo/one")]);

    let second = cache.bind_owner("token-b");
    assert!(cache.repos(&second).is_none());
    assert!(cache.repos(&first).is_none(), "the first owner is retired");
}

#[test]
fn rebinding_the_same_token_keeps_data_and_owner() {
    let cache = memory_cache();
    let first = cache.bind_owner("token-a");
    cache.store_repos(&first, &[repo(1, "octo/one")]);

    let again = cache.bind_owner("token-a");
    assert!(cache.repos(&again).is_some());
    assert!(
        cache.repos(&first).is_some(),
        "gateways built before the rebuild keep working"
    );
}

#[test]
fn a_store_after_clear_all_is_dropped() {
    let cache = memory_cache();
    let owner = cache.bind_owner("token-a");
    cache.clear_all();

    // The request that was in flight when the user signed out lands now.
    cache.store_repos(&owner, &[repo(1, "octo/one")]);

    let next = cache.bind_owner("token-a");
    assert!(cache.repos(&next).is_none());
}

#[test]
fn a_store_from_before_a_clear_does_not_land_after_signing_back_in() {
    let cache = memory_cache();
    let stale = cache.bind_owner("token-a");
    cache.clear_all();
    let fresh = cache.bind_owner("token-a");

    // Same account, but the request belongs to the session that was cleared.
    cache.store_repos(&stale, &[repo(1, "octo/one")]);
    assert!(cache.repos(&fresh).is_none());
}

#[test]
fn jobs_are_served_only_for_a_settled_live_run() {
    let cache = memory_cache();
    let owner = cache.bind_owner("token-a");

    // Unknown run: nothing to vouch for the jobs.
    store_jobs_now(&cache, &owner, 5, &[job(1, 5, "completed")]);
    assert!(cache.completed_jobs(&owner, 5).is_none());

    cache.note_runs(&owner, &[run(5, 10, "completed", "t1")]);
    store_jobs_now(&cache, &owner, 5, &[job(1, 5, "completed")]);
    assert_eq!(
        cache.completed_jobs(&owner, 5).expect("cached jobs").len(),
        1
    );

    // A re-run: the live version moves, the cached jobs stop matching.
    cache.note_runs(&owner, &[run(5, 10, "queued", "t2")]);
    assert!(cache.completed_jobs(&owner, 5).is_none());
}

#[test]
fn jobs_still_moving_are_not_cached() {
    let cache = memory_cache();
    let owner = cache.bind_owner("token-a");
    cache.note_runs(&owner, &[run(5, 10, "completed", "t1")]);

    // GitHub can report the run completed before every job says so.
    store_jobs_now(
        &cache,
        &owner,
        5,
        &[job(1, 5, "completed"), job(2, 5, "in_progress")],
    );
    assert!(cache.completed_jobs(&owner, 5).is_none());
}

#[test]
fn jobs_of_an_active_run_are_not_cached() {
    let cache = memory_cache();
    let owner = cache.bind_owner("token-a");
    cache.note_runs(&owner, &[run(5, 10, "in_progress", "t1")]);
    store_jobs_now(&cache, &owner, 5, &[job(1, 5, "completed")]);
    assert!(cache.completed_jobs(&owner, 5).is_none());
}

#[test]
fn a_run_without_updated_at_never_vouches_for_jobs() {
    let cache = memory_cache();
    let owner = cache.bind_owner("token-a");
    let mut settled = run(5, 10, "completed", "t1");
    settled.updated_at = None;
    cache.note_runs(&owner, &[settled]);
    store_jobs_now(&cache, &owner, 5, &[job(1, 5, "completed")]);
    assert!(cache.completed_jobs(&owner, 5).is_none());
}

#[test]
fn an_unchanged_store_only_touches() {
    let cache = memory_cache();
    let owner = cache.bind_owner("token-a");
    cache.store_repos(&owner, &[repo(1, "octo/one")]);
    {
        let mut state = cache.state.lock();
        assert!(state.dirty);
        state.dirty = false;
        state.touched = false;
    }

    cache.store_repos(&owner, &[repo(1, "octo/one")]);
    let state = cache.state.lock();
    assert!(!state.dirty);
    assert!(state.touched);
}

#[test]
fn round_trips_through_the_file() {
    let dir = tempfile::tempdir().expect("temp dir");

    let writer = file_cache(dir.path());
    let owner = writer.bind_owner("token-a");
    writer.store_repos(&owner, &[repo(1, "octo/one")]);
    writer.store_workflows(&owner, "octo", "one", &[workflow(10)]);
    writer.flush_blocking();

    let reader = file_cache(dir.path());
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    assert!(runtime.block_on(reader.hydrate_from_disk()));
    let owner = reader.bind_owner("token-a");
    assert_eq!(
        ids(&reader.repos(&owner).expect("repos"), |r| r.id),
        vec![1]
    );
    assert!(reader.workflows(&owner, 1).is_some());
}

#[cfg(unix)]
#[test]
fn the_file_is_private_to_the_user() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().expect("temp dir");
    let cache = file_cache(dir.path());
    let owner = cache.bind_owner("token-a");
    cache.store_repos(&owner, &[repo(1, "octo/one")]);
    cache.flush_blocking();

    let mode = std::fs::metadata(dir.path().join("data-cache.json"))
        .expect("cache file")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[test]
fn nothing_is_written_without_an_owner() {
    let dir = tempfile::tempdir().expect("temp dir");
    let cache = file_cache(dir.path());
    let owner = cache.bind_owner("token-a");
    cache.store_repos(&owner, &[repo(1, "octo/one")]);
    cache.clear_all();
    // `clear_all` resets the change flags too; set one again so the owner
    // check is the only thing standing between the flush and the disk.
    cache.state.lock().dirty = true;

    cache.flush_blocking();
    assert!(!dir.path().join("data-cache.json").exists());
}

#[test]
fn clear_all_keeps_the_file_and_remove_persisted_deletes_it() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("data-cache.json");
    let cache = file_cache(dir.path());
    let owner = cache.bind_owner("token-a");
    cache.store_repos(&owner, &[repo(1, "octo/one")]);
    cache.flush_blocking();

    // A keyring hiccup signs the UI out; the offline data must survive it.
    cache.clear_all();
    assert!(path.exists());

    cache.remove_persisted();
    assert!(!path.exists());
}

#[test]
fn a_file_from_another_account_is_ignored() {
    let dir = tempfile::tempdir().expect("temp dir");
    let writer = file_cache(dir.path());
    let owner = writer.bind_owner("token-a");
    writer.store_repos(&owner, &[repo(1, "octo/one")]);
    writer.flush_blocking();

    let runtime = tokio::runtime::Runtime::new().expect("runtime");

    // Loaded before the token is known…
    let early = file_cache(dir.path());
    runtime.block_on(early.hydrate_from_disk());
    let other = early.bind_owner("token-b");
    assert!(early.repos(&other).is_none());

    // …and after.
    let late = file_cache(dir.path());
    let other = late.bind_owner("token-b");
    runtime.block_on(late.hydrate_from_disk());
    assert!(late.repos(&other).is_none());
}

#[test]
fn a_load_finishing_after_bind_fills_in_without_overwriting_live_data() {
    let dir = tempfile::tempdir().expect("temp dir");
    let writer = file_cache(dir.path());
    let owner = writer.bind_owner("token-a");
    writer.store_repos(&owner, &[repo(1, "octo/one")]);
    writer.store_workflows(&owner, "octo", "one", &[workflow(10)]);
    writer.flush_blocking();

    let cache = file_cache(dir.path());
    let owner = cache.bind_owner("token-a");
    // The network answered first with a newer repository list.
    cache.store_repos(&owner, &[repo(1, "octo/one"), repo(2, "octo/two")]);

    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    runtime.block_on(cache.hydrate_from_disk());

    assert_eq!(
        ids(&cache.repos(&owner).expect("repos"), |r| r.id),
        vec![1, 2]
    );
    assert!(cache.workflows(&owner, 1).is_some(), "filled from disk");
}

#[test]
fn a_load_finishing_after_clear_all_is_dropped() {
    let dir = tempfile::tempdir().expect("temp dir");
    let writer = file_cache(dir.path());
    let owner = writer.bind_owner("token-a");
    writer.store_repos(&owner, &[repo(1, "octo/one")]);
    writer.flush_blocking();

    let cache = file_cache(dir.path());
    // Simulate the clear landing while the file was being read: the clear
    // counter moves between the start of the load and its end.
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    runtime.block_on(async {
        let load = cache.hydrate_from_disk();
        tokio::pin!(load);
        // Drive the load up to its blocking read, then clear.
        let _ = futures::poll!(&mut load);
        cache.clear_all();
        load.await;
    });

    let owner = cache.bind_owner("token-a");
    assert!(cache.repos(&owner).is_none());
}

#[test]
fn entries_past_retention_are_dropped_on_load() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("data-cache.json");
    let writer = file_cache(dir.path());
    let owner = writer.bind_owner("token-a");
    writer.store_repos(&owner, &[repo(1, "octo/one")]);
    writer.store_workflows(&owner, "octo", "one", &[workflow(10)]);
    {
        let mut state = writer.state.lock();
        let stale = unix_now() - DEFAULT_RETENTION.as_secs() - 60;
        state
            .data
            .by_repo
            .get_mut(&1)
            .and_then(|entry| entry.workflows.as_mut())
            .expect("workflows")
            .fetched_at = stale;
    }
    writer.flush_blocking();

    let snapshot = read_snapshot(&path, DEFAULT_RETENTION).expect("snapshot");
    assert!(snapshot.repos.is_some());
    assert!(snapshot.by_repo.is_empty(), "the stale entry is gone");
}

#[test]
fn an_unreadable_or_foreign_version_file_is_ignored() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("data-cache.json");

    std::fs::write(&path, b"{ not json").expect("write");
    assert!(read_snapshot(&path, DEFAULT_RETENTION).is_none());

    std::fs::write(&path, br#"{"version": 999, "owner": "x"}"#).expect("write");
    assert!(read_snapshot(&path, DEFAULT_RETENTION).is_none());
}

#[test]
fn ready_resolves_after_the_load() {
    let dir = tempfile::tempdir().expect("temp dir");
    let cache = file_cache(dir.path());
    assert!(!*cache.ready.borrow());

    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    runtime.block_on(async {
        assert!(!cache.hydrate_from_disk().await, "no file yet");
        tokio::time::timeout(Duration::from_secs(5), cache.ready())
            .await
            .expect("ready after the load");
        // A memory-only cache is ready from the start.
        tokio::time::timeout(Duration::from_secs(5), memory_cache().ready())
            .await
            .expect("memory cache is ready");
    });
}

#[test]
fn the_fingerprint_does_not_contain_the_token() {
    let fingerprint = token_fingerprint("gho_secret_token");
    assert_eq!(fingerprint.len(), 16);
    assert!(!fingerprint.contains("secret"));
    assert_ne!(fingerprint, token_fingerprint("gho_other_token"));
}

#[test]
fn a_late_older_run_list_cannot_make_a_rerun_look_completed() {
    let cache = memory_cache();
    let owner = cache.bind_owner("token-a");
    cache.note_runs(&owner, &[run(5, 10, "completed", "2026-11-01T09:00:00Z")]);
    store_jobs_now(&cache, &owner, 5, &[job(1, 5, "completed")]);

    cache.note_runs(&owner, &[run(5, 10, "queued", "2026-11-01T10:00:00Z")]);
    // A repository-wide list that left before the re-run lands now.
    cache.note_runs(&owner, &[run(5, 10, "completed", "2026-11-01T09:00:00Z")]);

    assert!(cache.completed_jobs(&owner, 5).is_none());
}

#[test]
fn a_rerun_that_completed_again_does_not_serve_the_old_attempt() {
    let cache = memory_cache();
    let owner = cache.bind_owner("token-a");
    cache.note_runs(&owner, &[run(5, 10, "completed", "2026-11-01T09:00:00Z")]);
    store_jobs_now(&cache, &owner, 5, &[job(1, 5, "completed")]);

    cache.note_runs(&owner, &[run(5, 10, "completed", "2026-11-01T11:00:00Z")]);
    assert!(cache.completed_jobs(&owner, 5).is_none());
    assert!(
        cache.state.lock().jobs.is_empty(),
        "jobs under a superseded version are dropped, not just hidden"
    );
}

#[test]
fn lists_that_disagree_at_the_same_timestamp_are_not_settled() {
    let cache = memory_cache();
    let owner = cache.bind_owner("token-a");
    cache.note_runs(&owner, &[run(5, 10, "in_progress", "2026-11-01T09:00:00Z")]);
    cache.note_runs(&owner, &[run(5, 10, "completed", "2026-11-01T09:00:00Z")]);
    store_jobs_now(&cache, &owner, 5, &[job(1, 5, "completed")]);
    assert!(cache.completed_jobs(&owner, 5).is_none());
}

#[test]
fn jobs_fetched_under_an_older_version_are_not_kept() {
    let cache = memory_cache();
    let owner = cache.bind_owner("token-a");
    cache.note_runs(&owner, &[run(5, 10, "completed", "2026-11-01T09:00:00Z")]);
    let fetch = cache.begin_jobs_fetch(&owner, 5);

    // While the request was out, the run was re-run and finished again.
    cache.note_runs(&owner, &[run(5, 10, "completed", "2026-11-01T11:00:00Z")]);
    cache.store_jobs(&owner, fetch, &[job(1, 5, "completed")]);

    assert!(cache.completed_jobs(&owner, 5).is_none());
}

#[test]
fn forgetting_a_run_drops_its_jobs_until_it_settles_again() {
    let cache = memory_cache();
    let owner = cache.bind_owner("token-a");
    cache.note_runs(&owner, &[run(5, 10, "completed", "2026-11-01T09:00:00Z")]);
    store_jobs_now(&cache, &owner, 5, &[job(1, 5, "completed")]);

    cache.forget_run(&owner, 5);
    assert!(cache.completed_jobs(&owner, 5).is_none());
    // A stale list repeating the old version does not settle it again.
    cache.note_runs(&owner, &[run(5, 10, "completed", "2026-11-01T09:00:00Z")]);
    store_jobs_now(&cache, &owner, 5, &[job(1, 5, "completed")]);
    assert!(cache.completed_jobs(&owner, 5).is_none());
}

#[test]
fn an_empty_job_list_is_not_cached() {
    let cache = memory_cache();
    let owner = cache.bind_owner("token-a");
    cache.note_runs(&owner, &[run(5, 10, "completed", "2026-11-01T09:00:00Z")]);
    store_jobs_now(&cache, &owner, 5, &[]);
    assert!(cache.completed_jobs(&owner, 5).is_none());
}

#[test]
fn the_job_eviction_order_stays_in_step_with_the_jobs() {
    let cache = memory_cache();
    let owner = cache.bind_owner("token-a");
    cache.note_runs(&owner, &[run(5, 10, "completed", "2026-11-01T09:00:00Z")]);
    for _ in 0..3 {
        store_jobs_now(&cache, &owner, 5, &[job(1, 5, "completed")]);
    }
    store_jobs_now(&cache, &owner, 5, &[job(1, 5, "in_progress")]);
    store_jobs_now(&cache, &owner, 5, &[job(1, 5, "completed")]);

    let state = cache.state.lock();
    assert_eq!(state.jobs.len(), 1);
    assert_eq!(
        state.jobs_order.iter().copied().collect::<Vec<_>>(),
        vec![5]
    );
}

#[test]
fn the_debounced_writer_writes_the_file() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("data-cache.json");
    let cache = Arc::new(
        DataCache::with_persistence(CachePersistenceConfig::new(path.clone(), DEFAULT_RETENTION))
            .with_write_debounce(Duration::from_millis(300)),
    );

    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    runtime.block_on(async {
        let owner = cache.bind_owner("token-a");
        cache.store_repos(&owner, &[repo(1, "octo/one")]);
        assert!(!path.exists(), "nothing is written before the debounce");

        tokio::time::timeout(Duration::from_secs(5), async {
            while !path.exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("the debounced write lands");
    });
}

#[test]
fn a_write_overtaken_by_a_clear_is_not_committed() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("data-cache.json");
    let cache = file_cache(dir.path());
    let owner = cache.bind_owner("token-a");
    cache.store_repos(&owner, &[repo(1, "octo/one")]);

    let (bytes, generation) = cache.take_pending_write().expect("a pending write");
    let temp = write_private_temp(&path, &bytes).expect("temp file");
    // The user signs out between the snapshot and the rename.
    cache.clear_all();

    assert!(!cache.commit_write(&path, &temp, generation));
    assert!(!path.exists());
    assert!(!temp.exists(), "the temporary file is cleaned up");
}

#[test]
fn a_late_load_does_not_bring_back_repos_the_network_removed() {
    let dir = tempfile::tempdir().expect("temp dir");
    let writer = file_cache(dir.path());
    let owner = writer.bind_owner("token-a");
    writer.store_repos(&owner, &[repo(1, "octo/one"), repo(2, "octo/two")]);
    writer.store_workflows(&owner, "octo", "one", &[workflow(10)]);
    writer.store_workflows(&owner, "octo", "two", &[workflow(20)]);
    writer.flush_blocking();

    let cache = file_cache(dir.path());
    let owner = cache.bind_owner("token-a");
    // The network answered first: repository 1 is gone.
    cache.store_repos(&owner, &[repo(2, "octo/two")]);

    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    runtime.block_on(cache.hydrate_from_disk());

    assert!(cache.workflows(&owner, 1).is_none());
    assert!(cache.workflows(&owner, 2).is_some());
}

#[test]
fn a_late_older_run_list_cannot_settle_a_rerun_for_its_new_jobs() {
    let cache = memory_cache();
    let owner = cache.bind_owner("token-a");
    cache.note_runs(&owner, &[run(5, 10, "completed", "2026-11-01T09:00:00Z")]);
    cache.note_runs(&owner, &[run(5, 10, "queued", "2026-11-01T10:00:00Z")]);
    cache.note_runs(&owner, &[run(5, 10, "completed", "2026-11-01T09:00:00Z")]);

    // The new attempt's jobs, fetched while the index should still say queued.
    store_jobs_now(&cache, &owner, 5, &[job(2, 5, "completed")]);
    assert!(cache.completed_jobs(&owner, 5).is_none());
}
