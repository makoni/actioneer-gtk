use gtk4 as gtk;
use gtk4::prelude::*;
use libadwaita as adw;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

/// How long a single UI test body may occupy the shared worker. Without a bound,
/// one hung body wedges every other GTK test and CI burns its whole job budget
/// instead of going red.
const GTK_TEST_TIMEOUT: Duration = Duration::from_secs(30);

/// The worker inherits libtest's output capture from whichever test happened to
/// start it, so anything it prints is swallowed into that test's buffer. The
/// panic hook stashes the message here instead, and `run_gtk_test` prints it on
/// the calling thread — where it lands in the failing test's own output.
static LAST_PANIC: Mutex<Option<String>> = Mutex::new(None);

/// Set once a body overruns: the worker is still busy with it, so every later
/// test fails immediately rather than queueing behind a job that never ends.
static WORKER_WEDGED: AtomicBool = AtomicBool::new(false);

type GtkJob = Box<dyn FnOnce() + Send>;

/// GTK objects are bound to the thread that initialised them, but libtest gives
/// every `#[test]` its own thread — even at `--test-threads=1`. The previous
/// guard reacted by *skipping* any test that did not happen to run first, which
/// silently disabled 30 of the 31 UI tests while still reporting them as passed.
///
/// Instead, one worker thread owns GTK and every UI test body is executed on it.
fn gtk_worker() -> Result<&'static mpsc::Sender<GtkJob>, &'static str> {
    static WORKER: OnceLock<Result<mpsc::Sender<GtkJob>, String>> = OnceLock::new();

    WORKER
        .get_or_init(|| {
            let (job_tx, job_rx) = mpsc::channel::<GtkJob>();
            let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();

            std::thread::Builder::new()
                .name("gtk-test-worker".into())
                .spawn(move || {
                    install_panic_hook();
                    let init = gtk::init()
                        .map_err(|err| format!("failed to init GTK ({err})"))
                        .and_then(|_| {
                            adw::init().map_err(|err| format!("failed to init Adwaita ({err})"))
                        });
                    let started = init.is_ok();
                    let _ = ready_tx.send(init);
                    if !started {
                        return;
                    }
                    // Serialised by construction: jobs run one at a time here.
                    while let Ok(job) = job_rx.recv() {
                        job();
                    }
                })
                .map_err(|err| format!("failed to spawn GTK test thread ({err})"))?;

            match ready_rx.recv() {
                Ok(Ok(())) => Ok(job_tx),
                Ok(Err(err)) => Err(err),
                Err(_) => Err("GTK test thread exited during startup".to_string()),
            }
        })
        .as_ref()
        .map_err(|err| err.as_str())
}

/// Routes panics raised on the worker into `LAST_PANIC` (with their location)
/// instead of the worker's captured stdout, leaving other threads untouched.
fn install_panic_hook() {
    static HOOK: OnceLock<()> = OnceLock::new();
    HOOK.get_or_init(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if std::thread::current().name() == Some("gtk-test-worker") {
                if let Ok(mut slot) = LAST_PANIC.lock() {
                    *slot = Some(info.to_string());
                }
            } else {
                previous(info);
            }
        }));
    });
}

/// Runs a UI test body on the shared GTK thread.
///
/// Skips only when there is genuinely no display; any other failure is reported
/// as a test failure rather than silently swallowed, and a panic inside `body`
/// is re-raised on the calling thread so the test fails normally.
pub fn run_gtk_test<F>(test_name: &str, body: F)
where
    F: FnOnce() + Send + 'static,
{
    let has_display = |name: &str| std::env::var_os(name).is_some_and(|value| !value.is_empty());
    if !has_display("DISPLAY") && !has_display("WAYLAND_DISPLAY") {
        eprintln!("Skipping {test_name}: no display available");
        return;
    }

    assert!(
        !WORKER_WEDGED.load(Ordering::SeqCst),
        "{test_name}: skipped because an earlier UI test wedged the GTK worker"
    );

    let worker = match gtk_worker() {
        Ok(worker) => worker,
        Err(err) => panic!("{test_name}: {err}"),
    };

    let (done_tx, done_rx) = mpsc::channel();
    worker
        .send(Box::new(move || {
            let outcome = std::panic::catch_unwind(AssertUnwindSafe(body));
            // Toplevels are held by GTK's global list, so a test that builds a
            // window pins it — and its whole tree — for the rest of the process.
            let toplevels = gtk::Window::toplevels();
            let windows: Vec<gtk::Window> = (0..toplevels.n_items())
                .filter_map(|index| toplevels.item(index))
                .filter_map(|object| object.downcast::<gtk::Window>().ok())
                .collect();
            for window in windows {
                window.destroy();
            }
            let _ = done_tx.send(outcome);
        }))
        .unwrap_or_else(|_| panic!("{test_name}: GTK test thread is gone"));

    match done_rx.recv_timeout(GTK_TEST_TIMEOUT) {
        Ok(Ok(())) => {}
        Ok(Err(payload)) => {
            if let Some(details) = LAST_PANIC.lock().ok().and_then(|mut slot| slot.take()) {
                eprintln!("{test_name} failed on the GTK worker: {details}");
            }
            std::panic::resume_unwind(payload)
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {
            WORKER_WEDGED.store(true, Ordering::SeqCst);
            panic!(
                "{test_name}: still running after {}s on the GTK worker",
                GTK_TEST_TIMEOUT.as_secs()
            )
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            panic!("{test_name}: GTK test thread died while running the test")
        }
    }
}

/// Collects weak references to every widget under `widget`, including the parts
/// hung off a `GtkExpander` (its label widget and its child), which a plain
/// first_child/next_sibling walk does not reach.
///
/// Leak tests assert that *all* of these die with the row: a cycle can pin a
/// single nested widget — and through it a model, a timer, or the whole pane —
/// while the outer container is released normally.
#[cfg(test)]
pub fn collect_widget_weaks(
    widget: &gtk::Widget,
    out: &mut Vec<(String, gtk::glib::WeakRef<gtk::Widget>)>,
) {
    use gtk::prelude::*;

    out.push((widget.type_().name().to_string(), widget.downgrade()));

    if let Some(expander) = widget.downcast_ref::<gtk::Expander>() {
        if let Some(label) = expander.label_widget() {
            collect_widget_weaks(&label, out);
        }
        if let Some(child) = expander.child() {
            collect_widget_weaks(&child, out);
        }
    }

    let mut child = widget.first_child();
    while let Some(current) = child {
        collect_widget_weaks(&current, out);
        child = current.next_sibling();
    }
}
