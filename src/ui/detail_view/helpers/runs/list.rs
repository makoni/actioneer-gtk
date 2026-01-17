use super::filters::summarize_visible_runs;
use super::row::{RunRowContext, create_run_expander_row};
use crate::api::models::WorkflowRun;
use crate::ui::detail_view::RunFilters;
use glib::subclass::types::ObjectSubclassIsExt;
use gtk4::prelude::*;
use gtk4::{self as gtk, gio, glib};
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::Arc;
use tracing::{debug, info};

const STATE_IDLE: &str = "idle";
const STATE_LOADING: &str = "loading";
const STATE_EMPTY: &str = "empty";
const STATE_FILTERED: &str = "filtered";
const STATE_ERROR: &str = "error";
const STATE_CONTENT: &str = "content";

type RetryHandler = Rc<RefCell<Option<Box<dyn Fn() + 'static>>>>;

#[derive(Clone)]
pub(crate) struct WorkflowRunListModel {
    stack: gtk::Stack,
    header_label: gtk::Label,
    list_store: gio::ListStore,
    error_detail: gtk::Label,
    retry_handler: RetryHandler,
    last_runs: Rc<RefCell<Arc<Vec<WorkflowRun>>>>,
    has_loaded: Rc<Cell<bool>>,
    expanded_runs: Rc<RefCell<HashSet<i64>>>,
}

#[cfg(test)]
pub(crate) fn test_run_list_model() -> WorkflowRunListModel {
    let stack = gtk::Stack::new();
    let header_label = gtk::Label::new(None);
    let list_store = gio::ListStore::new::<RunListEntry>();
    let error_detail = gtk::Label::new(None);
    let retry_handler: RetryHandler = Rc::new(RefCell::new(None));

    // Minimal children to satisfy state switches in tests
    let filtered_dummy = gtk::Box::new(gtk::Orientation::Vertical, 0);
    stack.add_named(&filtered_dummy, Some(STATE_FILTERED));

    let empty_dummy = gtk::Box::new(gtk::Orientation::Vertical, 0);
    stack.add_named(&empty_dummy, Some(STATE_EMPTY));

    let content_dummy = gtk::Box::new(gtk::Orientation::Vertical, 0);
    stack.add_named(&content_dummy, Some(STATE_CONTENT));

    WorkflowRunListModel {
        stack,
        header_label,
        list_store,
        error_detail,
        retry_handler,
        last_runs: Rc::new(RefCell::new(Arc::new(Vec::new()))),
        has_loaded: Rc::new(Cell::new(false)),
        expanded_runs: Rc::new(RefCell::new(HashSet::new())),
    }
}

impl WorkflowRunListModel {
    pub(crate) fn new(context: RunRowContext) -> Self {
        let list_store = gio::ListStore::new::<RunListEntry>();
        let selection = gtk::NoSelection::new(Some(list_store.clone()));
        let factory = gtk::SignalListItemFactory::new();

        factory.connect_setup(|_, list_item| {
            let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
            list_item.set_child(Some(&container));
        });

        let bind_context = context.clone();
        let expanded_runs = Rc::new(RefCell::new(HashSet::new()));
        let expanded_runs_for_bind = expanded_runs.clone();
        factory.connect_bind(move |_, list_item| {
            let context = bind_context.clone();
            let Some(container) = list_item
                .child()
                .and_then(|child| child.downcast::<gtk::Box>().ok())
            else {
                return;
            };

            while let Some(child) = container.first_child() {
                container.remove(&child);
            }

            let Some(entry_obj) = list_item
                .item()
                .and_then(|obj| obj.downcast::<RunListEntry>().ok())
            else {
                return;
            };

            let widget =
                create_run_expander_row(&entry_obj.run(), &context, entry_obj.expand_jobs());
            container.append(&widget);

            if let Some(expander) = find_run_expander(widget.upcast_ref()) {
                let run_id = entry_obj.run().id;
                if expander.is_expanded() {
                    expanded_runs_for_bind.borrow_mut().insert(run_id);
                }

                let expanded_runs = expanded_runs_for_bind.clone();
                expander.connect_expanded_notify(move |exp| {
                    let mut set = expanded_runs.borrow_mut();
                    if exp.is_expanded() {
                        set.insert(run_id);
                    } else {
                        set.remove(&run_id);
                    }
                });
            }
        });

        factory.connect_unbind(|_, list_item| {
            if let Some(container) = list_item
                .child()
                .and_then(|child| child.downcast::<gtk::Box>().ok())
            {
                while let Some(child) = container.first_child() {
                    container.remove(&child);
                }
            }
        });

        let list_view = gtk::ListView::new(Some(selection.clone()), Some(factory.clone()));
        list_view.add_css_class("boxed-list");
        list_view.add_css_class("hoverless-list");
        list_view.set_single_click_activate(false);
        list_view.set_valign(gtk::Align::Start);
        list_view.set_vexpand(false);
        list_view.set_margin_top(12);
        list_view.set_margin_bottom(12);
        list_view.set_margin_start(12);
        list_view.set_margin_end(12);

        let header_label = gtk::Label::new(None);
        header_label.add_css_class("dim-label");
        header_label.add_css_class("caption");
        header_label.set_halign(gtk::Align::Start);
        header_label.set_margin_bottom(8);
        header_label.set_visible(false);

        let content_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        content_box.append(&header_label);
        content_box.append(&list_view);

        let idle_box = build_idle_placeholder();
        let spinner_box = build_spinner();
        let empty_box = build_empty_placeholder();
        let filtered_box = build_filtered_placeholder();
        let (error_box, error_detail, retry_handler) = build_error_placeholder();

        let stack = gtk::Stack::new();
        stack.add_named(&idle_box, Some(STATE_IDLE));
        stack.add_named(&spinner_box, Some(STATE_LOADING));
        stack.add_named(&empty_box, Some(STATE_EMPTY));
        stack.add_named(&filtered_box, Some(STATE_FILTERED));
        stack.add_named(&error_box, Some(STATE_ERROR));
        stack.add_named(&content_box, Some(STATE_CONTENT));
        stack.set_visible_child_name(STATE_IDLE);
        stack.set_margin_start(24);
        stack.set_margin_end(12);
        stack.set_margin_top(8);
        stack.set_margin_bottom(8);

        Self {
            stack,
            header_label,
            list_store,
            error_detail,
            retry_handler,
            last_runs: Rc::new(RefCell::new(Arc::new(Vec::new()))),
            has_loaded: Rc::new(Cell::new(false)),
            expanded_runs,
        }
    }

    pub(crate) fn widget(&self) -> gtk::Widget {
        self.stack.clone().upcast()
    }

    pub(crate) fn show_loading(&self) {
        self.header_label.set_visible(false);
        self.list_store.remove_all();
        self.set_state(STATE_LOADING);
    }

    pub(crate) fn show_empty(&self) {
        self.header_label.set_visible(false);
        self.list_store.remove_all();
        self.set_state(STATE_EMPTY);
    }

    pub(crate) fn show_filtered_placeholder(&self) {
        self.header_label.set_visible(false);
        self.list_store.remove_all();
        self.set_state(STATE_FILTERED);
    }

    pub(crate) fn show_runs(
        &self,
        visible_count: usize,
        filtered_total: usize,
        overall_total: usize,
        runs: &[WorkflowRun],
        expanded_runs: &HashSet<i64>,
    ) {
        self.header_label.set_visible(true);
        self.header_label.set_text(&format_runs_header(
            visible_count,
            filtered_total,
            overall_total,
        ));
        self.replace_runs(runs, expanded_runs);
        self.set_state(STATE_CONTENT);
    }

    pub(crate) fn show_error<F>(&self, detail: String, retry: F)
    where
        F: Fn() + 'static,
    {
        self.header_label.set_visible(false);
        self.list_store.remove_all();
        self.error_detail.set_text(&detail);
        *self.retry_handler.borrow_mut() = Some(Box::new(retry));
        self.set_state(STATE_ERROR);
    }

    pub(crate) fn should_load_runs(&self) -> bool {
        state_requires_load(self.stack.visible_child_name())
    }

    pub(crate) fn set_runs(&self, runs: Arc<Vec<WorkflowRun>>) {
        *self.last_runs.borrow_mut() = runs;
        self.has_loaded.set(true);
    }

    pub(crate) fn reapply_filters(
        &self,
        filters: &RunFilters,
        expanded_runs: &HashSet<i64>,
    ) -> bool {
        if !self.has_loaded.get() {
            debug!("reapply skipped; no cached runs yet");
            return false;
        }

        let cached = self.last_runs.borrow().clone();

        if cached.is_empty() {
            self.show_empty();
            debug!("reapply showed empty state (cached runs empty)");
            return true;
        }

        let summary = summarize_visible_runs(&cached, filters);
        if summary.visible_runs.is_empty() {
            self.show_filtered_placeholder();
            debug!(
                filtered_total = summary.filtered_total,
                "reapply showed filtered placeholder (no visible runs)"
            );
        } else {
            self.show_runs(
                summary.visible_runs.len(),
                summary.filtered_total,
                cached.len(),
                &summary.visible_runs,
                expanded_runs,
            );
        }

        debug!(
            visible = summary.visible_runs.len(),
            filtered_total = summary.filtered_total,
            overall_total = cached.len(),
            "reapplied run filters against cached runs"
        );
        info!(
            visible = summary.visible_runs.len(),
            filtered_total = summary.filtered_total,
            overall_total = cached.len(),
            "run list reapply completed"
        );

        true
    }

    pub(crate) fn expanded_run_ids(&self) -> HashSet<i64> {
        self.expanded_runs.borrow().clone()
    }

    fn update_expanded_runs(&self, expanded: &HashSet<i64>) {
        let mut current = self.expanded_runs.borrow_mut();
        current.clear();
        current.extend(expanded.iter().copied());
    }

    fn replace_runs(&self, runs: &[WorkflowRun], expanded_runs: &HashSet<i64>) {
        self.update_expanded_runs(expanded_runs);
        self.list_store.remove_all();
        for run in runs {
            let entry = RunListEntry::new(run.clone(), expanded_runs.contains(&run.id));
            self.list_store.append(&entry);
        }
    }

    fn set_state(&self, state: &str) {
        self.stack.set_visible_child_name(state);
    }
}

fn build_spinner() -> gtk::Widget {
    let container = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    container.set_halign(gtk::Align::Center);
    container.set_valign(gtk::Align::Center);
    let spinner = gtk::Spinner::new();
    spinner.start();
    spinner.set_margin_top(8);
    spinner.set_margin_bottom(8);
    container.append(&spinner);
    container.upcast()
}

fn build_idle_placeholder() -> gtk::Widget {
    let label = gtk::Label::new(Some("Click to load runs..."));
    label.add_css_class("dim-label");
    label.set_halign(gtk::Align::Start);
    let container = gtk::Box::new(gtk::Orientation::Vertical, 4);
    container.set_halign(gtk::Align::Start);
    container.append(&label);
    container.upcast()
}

fn build_empty_placeholder() -> gtk::Widget {
    let container = gtk::Box::new(gtk::Orientation::Vertical, 6);
    container.set_halign(gtk::Align::Start);
    let label = gtk::Label::new(Some("No recent runs"));
    label.add_css_class("dim-label");
    label.set_halign(gtk::Align::Start);
    container.append(&label);

    let info_label = gtk::Label::new(Some("Triggered runs may take 10-30 seconds to appear"));
    info_label.add_css_class("dim-label");
    info_label.add_css_class("caption");
    info_label.set_halign(gtk::Align::Start);
    container.append(&info_label);
    container.upcast()
}

fn build_filtered_placeholder() -> gtk::Widget {
    let container = gtk::Box::new(gtk::Orientation::Vertical, 6);
    container.set_halign(gtk::Align::Start);

    let label = gtk::Label::new(Some("No runs match the current filters"));
    label.add_css_class("dim-label");
    label.set_halign(gtk::Align::Start);
    container.append(&label);

    let hint = gtk::Label::new(Some("Adjust the status chips above to see more runs."));
    hint.add_css_class("dim-label");
    hint.add_css_class("caption");
    hint.set_halign(gtk::Align::Start);
    container.append(&hint);
    container.upcast()
}

fn build_error_placeholder() -> (gtk::Widget, gtk::Label, RetryHandler) {
    let container = gtk::Box::new(gtk::Orientation::Vertical, 8);
    container.set_halign(gtk::Align::Start);

    let label = gtk::Label::new(Some("Unable to load workflow runs"));
    label.add_css_class("dim-label");
    label.set_halign(gtk::Align::Start);
    container.append(&label);

    let detail_label = gtk::Label::new(None);
    detail_label.add_css_class("caption");
    detail_label.add_css_class("dim-label");
    detail_label.set_halign(gtk::Align::Start);
    container.append(&detail_label);

    let retry_button = gtk::Button::with_label("Retry");
    retry_button.add_css_class("suggested-action");
    retry_button.set_halign(gtk::Align::Start);
    retry_button.set_margin_top(8);
    let retry_handler: RetryHandler = Rc::new(RefCell::new(None));
    let handler_ref = retry_handler.clone();
    retry_button.connect_clicked(move |_| {
        if let Some(callback) = handler_ref.borrow().as_ref() {
            callback();
        }
    });
    container.append(&retry_button);

    (container.upcast(), detail_label, retry_handler)
}

fn format_runs_header(visible_count: usize, filtered_total: usize, overall_total: usize) -> String {
    if filtered_total == overall_total || visible_count == filtered_total {
        if visible_count < overall_total {
            format!(
                "Recent runs (showing {} of {})",
                visible_count, overall_total
            )
        } else {
            format!("Recent runs ({})", overall_total)
        }
    } else {
        format!(
            "Recent runs (showing {} of {} matching filters)",
            visible_count, filtered_total
        )
    }
}

fn state_requires_load(state: Option<glib::GString>) -> bool {
    matches!(
        state.as_deref(),
        Some(STATE_IDLE) | Some(STATE_ERROR) | None
    )
}

fn find_run_expander(widget: &gtk::Widget) -> Option<gtk::Expander> {
    if let Some(expander) = widget.downcast_ref::<gtk::Expander>()
        && expander.widget_name().as_str().starts_with("run_")
    {
        return Some(expander.clone());
    }

    let mut child = widget.first_child();
    while let Some(current) = child {
        if let Some(found) = find_run_expander(&current) {
            return Some(found);
        }
        child = current.next_sibling();
    }

    None
}

glib::wrapper! {
    pub struct RunListEntry(ObjectSubclass<imp::RunListEntry>);
}

impl RunListEntry {
    fn new(run: WorkflowRun, expand_jobs: bool) -> Self {
        let obj: Self = glib::Object::new::<RunListEntry>();
        {
            let imp = obj.imp();
            *imp.run.borrow_mut() = Some(run);
            imp.expand_jobs.set(expand_jobs);
        }
        obj
    }

    fn run(&self) -> WorkflowRun {
        self.imp()
            .run
            .borrow()
            .as_ref()
            .cloned()
            .expect("Run should be set")
    }

    fn expand_jobs(&self) -> bool {
        self.imp().expand_jobs.get()
    }
}

mod imp {
    use super::*;
    use glib::subclass::prelude::*;
    use std::cell::{Cell, RefCell};

    #[derive(Default)]
    pub struct RunListEntry {
        pub run: RefCell<Option<WorkflowRun>>,
        pub expand_jobs: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for RunListEntry {
        const NAME: &'static str = "ActioneerRunListEntry";
        type Type = super::RunListEntry;
        type ParentType = glib::Object;
    }

    impl ObjectImpl for RunListEntry {}
}

#[cfg(test)]
mod tests {
    use super::test_run_list_model;
    use super::{STATE_CONTENT, STATE_ERROR, STATE_IDLE, format_runs_header, state_requires_load};
    use crate::api::models::WorkflowRun;
    use crate::ui::detail_view::RunFilters;
    use crate::ui::test_helpers::gtk_test_guard;
    use gtk4::prelude::ListModelExt;
    use std::collections::HashSet;

    #[test]
    fn formats_header_with_partial_visible() {
        let text = format_runs_header(5, 5, 12);
        assert_eq!(text, "Recent runs (showing 5 of 12)");
    }

    #[test]
    fn formats_header_with_matching_filters() {
        let text = format_runs_header(3, 4, 10);
        assert_eq!(text, "Recent runs (showing 3 of 4 matching filters)");
    }

    #[test]
    fn formats_header_with_exact_count() {
        let text = format_runs_header(10, 10, 10);
        assert_eq!(text, "Recent runs (10)");
    }

    #[test]
    fn requires_load_for_idle_or_error() {
        assert!(state_requires_load(Some(STATE_IDLE.into())));
        assert!(state_requires_load(Some(STATE_ERROR.into())));
    }

    #[test]
    fn does_not_require_load_for_content() {
        assert!(!state_requires_load(Some(STATE_CONTENT.into())));
    }

    #[test]
    fn treats_missing_state_as_needing_load() {
        assert!(state_requires_load(None));
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn reapply_filters_updates_from_last_runs() {
        let Some(_guard) = gtk_test_guard("reapply_filters_updates_from_last_runs") else {
            return;
        };

        // This test exercises the filter reapplication path without hitting the network.
        let model = test_run_list_model();

        let runs = vec![
            WorkflowRun {
                id: 1,
                run_number: Some(1),
                name: Some("Run 1".into()),
                display_title: Some("Run 1".into()),
                head_branch: Some("main".into()),
                status: Some("completed".into()),
                conclusion: Some("success".into()),
                run_started_at: None,
                event: None,
                created_at: None,
                updated_at: None,
                html_url: None,
            },
            WorkflowRun {
                id: 2,
                run_number: Some(2),
                name: Some("Run 2".into()),
                display_title: Some("Run 2".into()),
                head_branch: Some("main".into()),
                status: Some("completed".into()),
                conclusion: Some("failure".into()),
                run_started_at: None,
                event: None,
                created_at: None,
                updated_at: None,
                html_url: None,
            },
        ];

        model.set_runs(std::sync::Arc::new(runs));

        let mut expanded = HashSet::new();
        expanded.insert(2);

        let filters = RunFilters {
            include_success: false,
            include_failed: true,
            include_running: false,
        };

        let updated = model.reapply_filters(&filters, &expanded);
        assert!(updated, "reapply should run when data was loaded");
        assert_eq!(
            model.list_store.n_items(),
            1,
            "only failed run should remain"
        );
    }
}
