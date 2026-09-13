//! Async runtime and the bridge back to the GLib main thread.
//!
//! The Tokio runtime lives on a parked background thread; [`handle`] hands out
//! the one global handle. [`channel`] is the main-context bridge — it is the
//! only part of this module that names `gtk4::glib`, and it lives here rather
//! than in `ui/` so that non-GTK services can post to the main thread without
//! depending on the UI layer.

pub mod channel;

use std::sync::OnceLock;
use tokio::runtime::{Builder, Handle};
use tracing::{info, warn};

const DEFAULT_TOKIO_WORKER_THREADS: usize = 4;
const TOKIO_WORKER_THREADS_ENV: &str = "ACTIONEER_TOKIO_WORKER_THREADS";

// Global runtime handle
static RUNTIME_HANDLE: OnceLock<Handle> = OnceLock::new();

/// The process-wide Tokio handle. Panics if [`install_runtime`] has not run.
pub fn handle() -> &'static Handle {
    RUNTIME_HANDLE.get().expect("Runtime not initialized")
}

pub fn available_parallelism_count() -> usize {
    std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(DEFAULT_TOKIO_WORKER_THREADS)
}

pub fn default_tokio_worker_threads_for(available_parallelism: usize) -> usize {
    available_parallelism.clamp(1, DEFAULT_TOKIO_WORKER_THREADS)
}

pub fn parse_tokio_worker_threads_override(value: &str) -> Option<usize> {
    value
        .trim()
        .parse::<usize>()
        .ok()
        .filter(|threads| *threads > 0)
}

pub fn tokio_worker_threads() -> usize {
    let default_workers = default_tokio_worker_threads_for(available_parallelism_count());
    match std::env::var(TOKIO_WORKER_THREADS_ENV) {
        Ok(value) => parse_tokio_worker_threads_override(&value).unwrap_or_else(|| {
            warn!(
                env_var = TOKIO_WORKER_THREADS_ENV,
                value = %value,
                default_workers,
                "Ignoring invalid Tokio worker thread override"
            );
            default_workers
        }),
        Err(_) => default_workers,
    }
}

/// Starts the multi-thread runtime on a parked background thread and publishes
/// its handle. Returns once [`handle`] is usable.
pub fn install_runtime(worker_threads: usize) {
    let available_parallelism = available_parallelism_count();
    let default_runtime_workers = default_tokio_worker_threads_for(available_parallelism);
    info!(
        available_parallelism,
        default_runtime_workers,
        runtime_worker_threads = worker_threads,
        env_var = TOKIO_WORKER_THREADS_ENV,
        "Configuring Tokio runtime"
    );

    // Start tokio runtime in background thread and keep it alive
    std::thread::spawn(move || {
        let rt = Builder::new_multi_thread()
            .worker_threads(worker_threads)
            .enable_all()
            .build()
            .expect("Failed to create Tokio runtime");
        let handle = rt.handle().clone();

        // Store the handle globally
        RUNTIME_HANDLE
            .set(handle)
            .expect("Failed to set runtime handle");

        // Keep the runtime alive
        rt.block_on(async {
            futures::future::pending::<()>().await;
        })
    });

    // Wait for runtime to be ready
    while RUNTIME_HANDLE.get().is_none() {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    info!("Tokio runtime initialized");
}

/// Gives tests the global runtime the app sets up in `main`, so code paths that
/// spawn (loading workflows, persisting preferences) can run under `cargo test`.
#[cfg(test)]
pub(crate) fn init_test_runtime() {
    static TEST_RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

    let runtime = TEST_RUNTIME.get_or_init(|| {
        Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .expect("test runtime should build")
    });
    let _ = RUNTIME_HANDLE.set(runtime.handle().clone());
}
