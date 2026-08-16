use gtk4 as gtk;
use libadwaita as adw;
use std::panic::AssertUnwindSafe;
use std::sync::OnceLock;
use std::sync::mpsc;

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

/// Runs a UI test body on the shared GTK thread.
///
/// Skips only when there is genuinely no display; any other failure is reported
/// as a test failure rather than silently swallowed, and a panic inside `body`
/// is re-raised on the calling thread so the test fails normally.
pub fn run_gtk_test<F>(test_name: &str, body: F)
where
    F: FnOnce() + Send + 'static,
{
    if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
        eprintln!("Skipping {test_name}: no display available");
        return;
    }

    let worker = match gtk_worker() {
        Ok(worker) => worker,
        Err(err) => panic!("{test_name}: {err}"),
    };

    let (done_tx, done_rx) = mpsc::channel();
    worker
        .send(Box::new(move || {
            let outcome = std::panic::catch_unwind(AssertUnwindSafe(body));
            let _ = done_tx.send(outcome);
        }))
        .unwrap_or_else(|_| panic!("{test_name}: GTK test thread is gone"));

    match done_rx.recv() {
        Ok(Ok(())) => {}
        Ok(Err(payload)) => std::panic::resume_unwind(payload),
        Err(_) => panic!("{test_name}: GTK test thread died while running the test"),
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
