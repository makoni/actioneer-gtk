//! Demo data: an alternate backend behind the gateway, not an interceptor.
//!
//! Before Phase 2 this module exposed free functions over a process-global
//! `OnceLock<Mutex<Option<DemoData>>>`, and the live HTTP client branched on
//! that global in all fourteen of its public methods. Demo mode is now a
//! `DemoBackend` installed into the gateway slot; the live client no longer
//! names this module at all.

mod backend;
mod data;

pub use backend::DemoBackend;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_backend_serves_the_fixture_repositories() {
        let backend = DemoBackend::new();
        let (repos, rate) = backend.seed();
        assert!(!repos.is_empty(), "the demo fixtures have repositories");
        assert!(rate.is_some(), "the seed carries a rate limit");
    }

    #[test]
    fn two_backends_are_independent() {
        // The replacement for the old "enable/disable a global" test: each
        // backend owns its own fixtures, so one test can never disturb another.
        let a = DemoBackend::new();
        let b = DemoBackend::new();
        let before =
            futures::executor::block_on(b.list_repository_runs("demo-org", "actioneer-demo-app"))
                .expect("runs")
                .len();

        futures::executor::block_on(a.dispatch_workflow(
            "demo-org",
            "actioneer-demo-app",
            "21001",
            "main",
            None,
        ))
        .expect("dispatch succeeds");

        let after =
            futures::executor::block_on(b.list_repository_runs("demo-org", "actioneer-demo-app"))
                .expect("runs")
                .len();
        assert_eq!(
            before, after,
            "a mutation on one backend does not reach another"
        );
    }

    #[test]
    fn a_clone_shares_the_same_fixtures() {
        // The other half, and the one that matters for the UI: call sites read
        // the gateway out of its slot by cloning, so a clone must be a handle
        // and not a copy.
        let backend = DemoBackend::new();
        let clone = backend.clone();
        let before = futures::executor::block_on(
            backend.list_repository_runs("demo-org", "actioneer-demo-app"),
        )
        .expect("runs")
        .len();

        futures::executor::block_on(clone.dispatch_workflow(
            "demo-org",
            "actioneer-demo-app",
            "21001",
            "main",
            None,
        ))
        .expect("dispatch succeeds");

        let after = futures::executor::block_on(
            backend.list_repository_runs("demo-org", "actioneer-demo-app"),
        )
        .expect("runs")
        .len();
        assert_eq!(after, before + 1, "a clone mutates the shared fixtures");
    }
}
