use gtk4 as gtk;
use libadwaita as adw;
use std::sync::{Mutex, OnceLock};
use std::thread::ThreadId;

pub struct GtkTestGuard {
    _guard: std::sync::MutexGuard<'static, ()>,
}

impl GtkTestGuard {
    fn new(test_name: &str) -> Option<Self> {
        if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
            eprintln!("Skipping {test_name}: no display available");
            return None;
        }

        static GTK_TEST_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();
        static GTK_THREAD_ID: OnceLock<ThreadId> = OnceLock::new();
        static GTK_INIT: OnceLock<Result<(), String>> = OnceLock::new();

        let mutex = GTK_TEST_MUTEX.get_or_init(|| Mutex::new(()));
        let guard = match mutex.lock() {
            Ok(guard) => guard,
            Err(err) => {
                eprintln!("Skipping {test_name}: GTK test mutex poisoned ({err})");
                return None;
            }
        };

        let current_id = std::thread::current().id();
        let main_id = GTK_THREAD_ID.get_or_init(|| current_id);
        if *main_id != current_id {
            eprintln!("Skipping {test_name}: GTK tests must run on a single thread");
            drop(guard);
            return None;
        }

        let init_result = GTK_INIT.get_or_init(|| {
            if gtk::is_initialized() {
                return Ok(());
            }

            gtk::init()
                .map_err(|err| format!("failed to init GTK ({err})"))
                .and_then(|_| adw::init().map_err(|err| format!("failed to init Adwaita ({err})")))
        });

        if let Err(err) = init_result {
            eprintln!("Skipping {test_name}: {err}");
            drop(guard);
            return None;
        }

        Some(Self { _guard: guard })
    }
}

pub fn gtk_test_guard(test_name: &str) -> Option<GtkTestGuard> {
    GtkTestGuard::new(test_name)
}
