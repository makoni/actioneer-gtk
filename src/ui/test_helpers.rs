use gtk4 as gtk;
use gtk4::prelude::*;
use libadwaita as adw;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

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

/// How long `pump_frames` pumps before giving up, so a wedged main context
/// cannot hang the worker. A few hundred ms is enough for the frame clock to
/// tick and a rebuild's layout to settle, and keeps a slow test from eating
/// the worker's 30s budget.
const PUMP_FRAMES_DEADLINE: Duration = Duration::from_millis(300);

/// Consecutive empty drains after which `pump_frames` calls the layout settled.
/// This is a time window, not a frame count: at ~2 ms per drain it must exceed
/// one display frame period so a *pending* frame (a busy layout keeps ticking
/// the frame clock) cannot hide inside the quiet stretch.
const PUMP_FRAMES_IDLE_SETTLE: u32 = 20;

/// Pumps the default main context until the layout settles, letting a rebuild's
/// layout/adjustment work complete.
///
/// The layout a test needs to observe happens on the frame clock, which ticks
/// over time rather than per context iteration — a headless display (Xvfb)
/// still advances it, but a single drain (or a handful) is not enough for a
/// virtual list to finish measuring. So we drain every ready source and yield
/// briefly, repeating. A *busy* layout keeps scheduling frames, so the context
/// stays non-empty; once it is settled no frames are pending and the context
/// goes quiet — we exit after `PUMP_FRAMES_IDLE_SETTLE` consecutive empty
/// drains, bounded by the deadline so a wedged context still gives up.
pub fn pump_frames() {
    let context = gtk::glib::MainContext::default();
    let deadline = Instant::now() + PUMP_FRAMES_DEADLINE;
    let mut idle_streak = 0u32;
    while Instant::now() < deadline {
        let mut worked = false;
        while context.iteration(false) {
            worked = true;
        }
        thread::sleep(Duration::from_millis(2));
        if worked {
            idle_streak = 0;
        } else {
            idle_streak += 1;
            if idle_streak >= PUMP_FRAMES_IDLE_SETTLE {
                break;
            }
        }
    }
}

/// Presents `child` in a new window, allocated exactly `width` × `height`
/// however large the window itself ends up.
///
/// `set_default_size` is only a request: a tiling window manager (Hyprland,
/// GNOME's tiling, …) resizes the toplevel as it pleases, and when the tests
/// run on a desktop session GTK prefers Wayland over the Xvfb display, so the
/// window lands on the real desktop and gets tiled. A test that scrolls a list
/// then sees a viewport two or three times taller than it assumed. A
/// `gtk::Fixed` gives its child the child's own size rather than stretching it
/// to the window, so the size request below holds under any window manager.
pub fn present_at_size(child: &impl IsA<gtk::Widget>, width: i32, height: i32) -> gtk::Window {
    child.set_size_request(width, height);
    let fixed = gtk::Fixed::new();
    fixed.put(child, 0.0, 0.0);
    let window = gtk::Window::new();
    window.set_default_size(width, height);
    window.set_child(Some(&fixed));
    window.present();
    window
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

/// Widgets that stick out past the left or right edge of `root`, which is what
/// a phone-sized window shows as content cut off at the side.
///
/// Only the outermost offender of each overflowing subtree is listed, with the
/// chain below it that demands the width: at every step the child with the
/// largest minimum width, down to the widget that sets it — usually a label or
/// a row of buttons that cannot shrink.
///
/// Content that scrolls horizontally on purpose (a `ScrolledWindow` whose
/// horizontal policy is not `Never`) and unmapped widgets (pages that are not
/// on screen) are skipped.
#[cfg(test)]
pub fn horizontal_overflow(root: &impl IsA<gtk::Widget>) -> Vec<String> {
    let root = root.as_ref();
    let limit = root.width() as f32;
    let mut found = Vec::new();
    collect_overflow(root, root, limit, &mut found);
    found
}

#[cfg(test)]
fn collect_overflow(root: &gtk::Widget, widget: &gtk::Widget, limit: f32, found: &mut Vec<String>) {
    let mut child = widget.first_child();
    while let Some(current) = child {
        child = current.next_sibling();
        // Overlay scrollbars are drawn over the edge by design.
        if !current.is_mapped() || current.is::<gtk::Popover>() || current.is::<gtk::Scrollbar>() {
            continue;
        }
        let Some(bounds) = current.compute_bounds(root) else {
            continue;
        };
        // A pixel of slack: header bars reach a pixel past the window's edge
        // into the decoration's border.
        if bounds.x() < -1.5 || bounds.x() + bounds.width() > limit + 1.5 {
            found.push(format!(
                "{} spans {:.0}..{:.0} of {limit:.0}: {}",
                describe_widget(&current),
                bounds.x(),
                bounds.x() + bounds.width(),
                widest_chain(&current)
            ));
            continue;
        }
        let scrolls_sideways = current
            .downcast_ref::<gtk::ScrolledWindow>()
            .is_some_and(|scroller| scroller.hscrollbar_policy() != gtk::PolicyType::Never);
        if !scrolls_sideways {
            collect_overflow(root, &current, limit, found);
        }
    }
}

#[cfg(test)]
fn widest_chain(widget: &gtk::Widget) -> String {
    let mut steps = Vec::new();
    let mut node = Some(widget.clone());
    while let Some(current) = node {
        let (min, _, _, _) = current.measure(gtk::Orientation::Horizontal, -1);
        steps.push(format!("{} {min}", describe_widget(&current)));
        let mut widest: Option<(i32, gtk::Widget)> = None;
        let mut child = current.first_child();
        while let Some(candidate) = child {
            child = candidate.next_sibling();
            if !candidate.is_visible() {
                continue;
            }
            let (candidate_min, _, _, _) = candidate.measure(gtk::Orientation::Horizontal, -1);
            if widest
                .as_ref()
                .is_none_or(|(best, _)| candidate_min > *best)
            {
                widest = Some((candidate_min, candidate));
            }
        }
        node = widest.map(|(_, child)| child);
    }
    steps.join(" > ")
}

#[cfg(test)]
fn describe_widget(widget: &gtk::Widget) -> String {
    let classes: Vec<String> = widget.css_classes().iter().map(|c| c.to_string()).collect();
    let text = widget
        .downcast_ref::<gtk::Label>()
        .map(|label| format!(" {:?}", label.text()))
        .or_else(|| {
            widget
                .downcast_ref::<gtk::Button>()
                .and_then(|button| button.label())
                .map(|label| format!(" {label:?}"))
        })
        .unwrap_or_default();
    if classes.is_empty() {
        format!("{}{text}", widget.type_().name())
    } else {
        format!("{}.{}{text}", widget.type_().name(), classes.join("."))
    }
}

/// Renders `widget` to `<dir>/<name>.png` when `ACTIONEER_UI_SHOTS=<dir>` is
/// set, so a layout can be looked at rather than only asserted on. Does
/// nothing otherwise.
#[cfg(test)]
pub fn save_screenshot(widget: &impl IsA<gtk::Widget>, name: &str) {
    let Some(dir) = std::env::var_os("ACTIONEER_UI_SHOTS") else {
        return;
    };
    let path = std::path::Path::new(&dir).join(format!("{name}.png"));
    let _ = std::fs::create_dir_all(&dir);
    save_screenshot_to(widget, &path, 1.0);
}

/// Renders `widget` to a PNG at `scale` times its size. The widget is drawn
/// at that size rather than its pixels stretched, so text and icons stay sharp
/// at 2 — what store screenshots are taken at.
#[cfg(test)]
pub fn save_screenshot_to(widget: &impl IsA<gtk::Widget>, path: &std::path::Path, scale: f32) {
    let widget = widget.as_ref();
    let (width, height) = (
        widget.width() as f32 * scale,
        widget.height() as f32 * scale,
    );
    if width <= 0.0 || height <= 0.0 {
        return;
    }
    let paintable = gtk::WidgetPaintable::new(Some(widget));
    let snapshot = gtk::Snapshot::new();
    paintable.snapshot(&snapshot, width as f64, height as f64);
    let Some(node) = snapshot.to_node() else {
        return;
    };
    let Some(renderer) = widget.native().and_then(|native| native.renderer()) else {
        return;
    };
    let texture = renderer.render_texture(
        node,
        Some(&gtk::graphene::Rect::new(0.0, 0.0, width, height)),
    );
    if let Err(err) = texture.save_to_png(path) {
        eprintln!("could not save {}: {err}", path.display());
    }
}

/// Runs the main loop for `rounds` × 5 ms. Not "until idle": in demo mode the
/// running jobs tick their timers every second, so it never is.
#[cfg(test)]
pub fn settle_main_loop(rounds: u32) {
    let context = gtk::glib::MainContext::default();
    for _ in 0..rounds {
        while context.pending() {
            let _ = context.iteration(false);
        }
        thread::sleep(Duration::from_millis(5));
    }
}

/// The first mapped widget under `root`, `root` included, that is a `W` and
/// satisfies `predicate`, in depth-first order.
#[cfg(test)]
pub fn find_widget<W: IsA<gtk::Widget>>(
    root: &impl IsA<gtk::Widget>,
    predicate: impl Fn(&W) -> bool,
) -> Option<W> {
    fn walk<W: IsA<gtk::Widget>>(
        widget: &gtk::Widget,
        predicate: &dyn Fn(&W) -> bool,
    ) -> Option<W> {
        if widget.is_mapped()
            && let Some(candidate) = widget.downcast_ref::<W>()
            && predicate(candidate)
        {
            return Some(candidate.clone());
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            if let Some(found) = walk(&current, predicate) {
                return Some(found);
            }
            child = current.next_sibling();
        }
        None
    }
    walk(root.as_ref(), &predicate)
}

/// Every toplevel window that exists right now.
#[cfg(test)]
pub fn toplevel_windows() -> Vec<gtk::Window> {
    gtk::Window::list_toplevels()
        .into_iter()
        .filter_map(|window| window.downcast::<gtk::Window>().ok())
        .collect()
}

/// Waits a few seconds for a visible window that is not in `before` — one a
/// click opened, after whatever loading it does first.
#[cfg(test)]
pub fn wait_for_new_toplevel(before: &[gtk::Window]) -> Option<gtk::Window> {
    for _ in 0..20 {
        settle_main_loop(50);
        if let Some(window) = toplevel_windows()
            .into_iter()
            .find(|window| window.is_visible() && !before.contains(window))
        {
            return Some(window);
        }
    }
    None
}

/// Presents a toplevel at `width` × `height` and fails at once if it did not
/// get that size.
///
/// `present_at_size` cannot stand in here: a window's breakpoints follow the
/// window's own size, so the window itself has to be that big. A window manager
/// that resizes it (a tiling one, on a desktop session the run was not
/// isolated from) would otherwise turn every layout assertion after this into
/// a flaky one; this way it is one clear failure. The slack allows for the
/// client-side decorations taken out of the default size.
#[cfg(test)]
pub fn present_window_at(window: &impl IsA<gtk::Window>, width: i32, height: i32) {
    let window = window.as_ref();
    window.set_default_size(width, height);
    window.present();
    settle_main_loop(50);
    let (got_width, got_height) = (window.width(), window.height());
    assert!(
        (width - 24..=width).contains(&got_width) && (height - 24..=height).contains(&got_height),
        "asked for a {width}x{height} window and got {got_width}x{got_height}: run the GTK \
         tests under a bare Xvfb (`env -u WAYLAND_DISPLAY GDK_BACKEND=x11 xvfb-run ...`), \
         where no window manager resizes them"
    );
}

/// Installs the app's stylesheet once per test process. Every GTK test shares
/// one display, and `style::install_app_css` adds a new provider on each call,
/// so calling it per test would stack identical copies of every rule.
#[cfg(test)]
pub fn ensure_app_css() {
    thread_local! {
        static INSTALLED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    INSTALLED.with(|installed| {
        if !installed.replace(true) {
            crate::ui::style::install_app_css();
        }
    });
}
