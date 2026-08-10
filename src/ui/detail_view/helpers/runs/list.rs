use super::super::workflows::{WorkflowRowHeader, update_workflow_row_header};
use super::filters::summarize_visible_runs;
use super::row::{RunRowContext, create_run_expander_row};
use crate::api::models::{JobSummary, WorkflowRun};
use crate::i18n::tr;
use crate::ui::detail_view::RunFilters;
use crate::ui::detail_view::header_state::DetailHeaderState;
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
    workflow_id: i64,
    job_summaries: super::super::context::RunBadgeSummaryMap,
    row_header: Rc<RefCell<Option<WorkflowRowHeader>>>,
    detail_header: Rc<RefCell<Option<DetailHeaderState>>>,
    counts_label: Rc<RefCell<Option<gtk::Label>>>,
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
        workflow_id: 0,
        job_summaries: Rc::new(RefCell::new(std::collections::HashMap::new())),
        row_header: Rc::new(RefCell::new(None)),
        detail_header: Rc::new(RefCell::new(None)),
        counts_label: Rc::new(RefCell::new(None)),
    }
}

impl WorkflowRunListModel {
    pub(crate) fn new(context: RunRowContext) -> Self {
        let workflow_id = context.workflow_id;
        let job_summaries = context.job_summaries.clone();
        let list_store = gio::ListStore::new::<RunListEntry>();
        let selection = gtk::NoSelection::new(Some(list_store.clone()));
        let factory = gtk::SignalListItemFactory::new();

        factory.connect_setup(|_, list_item| {
            let Some(list_item) = list_item.downcast_ref::<gtk::ListItem>() else {
                return;
            };
            let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
            list_item.set_child(Some(&container));
        });

        let bind_context = context.clone();
        let expanded_runs = Rc::new(RefCell::new(HashSet::new()));
        let expanded_runs_for_bind = expanded_runs.clone();
        factory.connect_bind(move |_, list_item| {
            let Some(list_item) = list_item.downcast_ref::<gtk::ListItem>() else {
                return;
            };
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
            if entry_obj.is_first() {
                widget.add_css_class("run-item-first");
            }
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
            let Some(list_item) = list_item.downcast_ref::<gtk::ListItem>() else {
                return;
            };
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
        list_view.add_css_class("hoverless-list");
        list_view.add_css_class("runs-list");
        list_view.set_single_click_activate(false);
        list_view.set_valign(gtk::Align::Start);
        list_view.set_vexpand(false);

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

        Self {
            stack,
            header_label,
            list_store,
            error_detail,
            retry_handler,
            last_runs: Rc::new(RefCell::new(Arc::new(Vec::new()))),
            has_loaded: Rc::new(Cell::new(false)),
            expanded_runs,
            workflow_id,
            job_summaries,
            row_header: Rc::new(RefCell::new(None)),
            detail_header: Rc::new(RefCell::new(None)),
            counts_label: Rc::new(RefCell::new(None)),
        }
    }

    pub(crate) fn widget(&self) -> gtk::Widget {
        self.stack.clone().upcast()
    }

    pub(crate) fn set_row_header(&self, header: WorkflowRowHeader) {
        *self.row_header.borrow_mut() = Some(header);
    }

    pub(crate) fn row_header(&self) -> Option<WorkflowRowHeader> {
        self.row_header.borrow().clone()
    }

    pub(crate) fn set_detail_header(&self, header: DetailHeaderState) {
        *self.detail_header.borrow_mut() = Some(header);
    }

    pub(crate) fn detail_header(&self) -> Option<DetailHeaderState> {
        self.detail_header.borrow().clone()
    }

    /// External "shown N of M" label rendered in the workflow's sub-header.
    pub(crate) fn set_counts_label(&self, label: gtk::Label) {
        *self.counts_label.borrow_mut() = Some(label);
    }

    pub(crate) fn job_summaries(&self) -> super::super::context::RunBadgeSummaryMap {
        self.job_summaries.clone()
    }

    /// Re-renders the workflow-level progress indicator when fresh job counts
    /// arrive for the workflow's latest run.
    pub(crate) fn refresh_progress_from_summary(&self, run_id: i64, summary: Option<&JobSummary>) {
        let Some(header) = self.row_header() else {
            return;
        };
        let Some(detail) = self.detail_header() else {
            return;
        };
        let Some(latest) = detail.latest_run(self.workflow_id) else {
            return;
        };
        if latest.id != run_id {
            return;
        }
        update_workflow_row_header(&header, Some(&latest), summary);
    }

    pub(crate) fn show_loading(&self) {
        self.header_label.set_visible(false);
        self.set_counts_visible(false);
        self.list_store.remove_all();
        self.set_state(STATE_LOADING);
    }

    pub(crate) fn show_empty(&self) {
        self.header_label.set_visible(false);
        self.set_counts_visible(false);
        self.list_store.remove_all();
        self.set_state(STATE_EMPTY);
    }

    pub(crate) fn show_filtered_placeholder(&self) {
        self.header_label.set_visible(false);
        self.set_counts_visible(false);
        self.list_store.remove_all();
        self.set_state(STATE_FILTERED);
    }

    fn set_counts_visible(&self, visible: bool) {
        if let Some(label) = self.counts_label.borrow().as_ref() {
            label.set_visible(visible);
        }
    }

    pub(crate) fn show_runs(
        &self,
        visible_count: usize,
        filtered_total: usize,
        overall_total: usize,
        runs: &[WorkflowRun],
        expanded_runs: &HashSet<i64>,
    ) {
        if let Some(counts_label) = self.counts_label.borrow().as_ref() {
            counts_label.set_text(&format_runs_counts(
                visible_count,
                filtered_total,
                overall_total,
            ));
            counts_label.set_visible(true);
        } else {
            self.header_label.set_visible(true);
            self.header_label.set_text(&format_runs_header(
                visible_count,
                filtered_total,
                overall_total,
            ));
        }
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

    pub(crate) fn has_loaded_runs(&self) -> bool {
        self.has_loaded.get()
    }

    pub(crate) fn has_active_runs(&self) -> bool {
        self.last_runs.borrow().iter().any(WorkflowRun::is_active)
    }

    pub(crate) fn run_completed(&self, run_id: i64) -> bool {
        self.last_runs
            .borrow()
            .iter()
            .find(|run| run.id == run_id)
            .is_some_and(|run| !run.is_active())
    }

    pub(crate) fn prepend_run(&self, run: WorkflowRun, filters: &RunFilters) {
        let run_id = run.id;
        let mut merged = vec![run];
        merged.extend(
            self.last_runs
                .borrow()
                .iter()
                .filter(|existing| existing.id != run_id)
                .cloned(),
        );

        self.set_runs(Arc::new(merged));
        let expanded = self.expanded_run_ids();
        let _ = self.reapply_filters(filters, &expanded);
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
        for (index, run) in runs.iter().enumerate() {
            let entry = RunListEntry::new(run.clone(), expanded_runs.contains(&run.id), index == 0);
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
    let label = gtk::Label::new(Some(tr("Click to load runs...").as_str()));
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
    let label = gtk::Label::new(Some(tr("No recent runs").as_str()));
    label.add_css_class("dim-label");
    label.set_halign(gtk::Align::Start);
    container.append(&label);

    let info_label = gtk::Label::new(Some(
        tr("Triggered runs may take 10-30 seconds to appear").as_str(),
    ));
    info_label.add_css_class("dim-label");
    info_label.add_css_class("caption");
    info_label.set_halign(gtk::Align::Start);
    container.append(&info_label);
    container.upcast()
}

fn build_filtered_placeholder() -> gtk::Widget {
    let container = gtk::Box::new(gtk::Orientation::Vertical, 6);
    container.set_halign(gtk::Align::Start);

    let label = gtk::Label::new(Some(tr("No runs match the current filters").as_str()));
    label.add_css_class("dim-label");
    label.set_halign(gtk::Align::Start);
    container.append(&label);

    let hint = gtk::Label::new(Some(
        tr("Adjust the status chips above to see more runs.").as_str(),
    ));
    hint.add_css_class("dim-label");
    hint.add_css_class("caption");
    hint.set_halign(gtk::Align::Start);
    container.append(&hint);
    container.upcast()
}

fn build_error_placeholder() -> (gtk::Widget, gtk::Label, RetryHandler) {
    let container = gtk::Box::new(gtk::Orientation::Vertical, 8);
    container.set_halign(gtk::Align::Start);

    let label = gtk::Label::new(Some(tr("Unable to load workflow runs").as_str()));
    label.add_css_class("dim-label");
    label.set_halign(gtk::Align::Start);
    container.append(&label);

    let detail_label = gtk::Label::new(None);
    detail_label.add_css_class("caption");
    detail_label.add_css_class("dim-label");
    detail_label.set_halign(gtk::Align::Start);
    container.append(&detail_label);

    let retry_button = gtk::Button::with_label(tr("Retry").as_str());
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

/// Short "shown N of M" variant for the workflow sub-header.
fn format_runs_counts(visible_count: usize, filtered_total: usize, overall_total: usize) -> String {
    if filtered_total == overall_total || visible_count == filtered_total {
        tr("Showing {visible} of {overall}")
            .replace("{visible}", visible_count.to_string().as_str())
            .replace("{overall}", overall_total.to_string().as_str())
    } else {
        tr("Showing {visible} of {filtered} matching filters")
            .replace("{visible}", visible_count.to_string().as_str())
            .replace("{filtered}", filtered_total.to_string().as_str())
    }
}

fn format_runs_header(visible_count: usize, filtered_total: usize, overall_total: usize) -> String {
    if filtered_total == overall_total || visible_count == filtered_total {
        if visible_count < overall_total {
            tr("Recent runs (showing {visible} of {overall})")
                .replace("{visible}", visible_count.to_string().as_str())
                .replace("{overall}", overall_total.to_string().as_str())
        } else {
            tr("Recent runs ({overall})").replace("{overall}", overall_total.to_string().as_str())
        }
    } else {
        tr("Recent runs (showing {visible} of {filtered} matching filters)")
            .replace("{visible}", visible_count.to_string().as_str())
            .replace("{filtered}", filtered_total.to_string().as_str())
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
    fn new(run: WorkflowRun, expand_jobs: bool, is_first: bool) -> Self {
        let obj: Self = glib::Object::new::<RunListEntry>();
        {
            let imp = obj.imp();
            *imp.run.borrow_mut() = Some(run);
            imp.expand_jobs.set(expand_jobs);
            imp.is_first.set(is_first);
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

    fn is_first(&self) -> bool {
        self.imp().is_first.get()
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
        pub is_first: Cell<bool>,
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
    use crate::i18n::{i18n_test_guard, tr};
    use crate::ui::detail_view::RunFilters;
    use crate::ui::test_helpers::gtk_test_guard;
    use gtk4::prelude::ListModelExt;
    use std::collections::HashSet;

    #[test]
    fn formats_header_with_partial_visible() {
        let _guard = i18n_test_guard();
        let text = format_runs_header(5, 5, 12);
        assert_eq!(
            text,
            tr("Recent runs (showing {visible} of {overall})")
                .replace("{visible}", "5")
                .replace("{overall}", "12")
        );
    }

    #[test]
    fn formats_header_with_matching_filters() {
        let _guard = i18n_test_guard();
        let text = format_runs_header(3, 4, 10);
        assert_eq!(
            text,
            tr("Recent runs (showing {visible} of {filtered} matching filters)")
                .replace("{visible}", "3")
                .replace("{filtered}", "4")
        );
    }

    #[test]
    fn formats_header_with_exact_count() {
        let _guard = i18n_test_guard();
        let text = format_runs_header(10, 10, 10);
        assert_eq!(
            text,
            tr("Recent runs ({overall})").replace("{overall}", "10")
        );
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
                workflow_id: None,
                name: Some("Run 1".into()),
                display_title: Some("Run 1".into()),
                head_branch: Some("main".into()),
                status: Some("completed".into()),
                conclusion: Some("success".into()),
                run_started_at: None,
                event: None,
                created_at: None,
                updated_at: None,
                actor: None,

                head_commit: None,

                triggering_actor: None,

                html_url: None,
            },
            WorkflowRun {
                id: 2,
                run_number: Some(2),
                workflow_id: None,
                name: Some("Run 2".into()),
                display_title: Some("Run 2".into()),
                head_branch: Some("main".into()),
                status: Some("completed".into()),
                conclusion: Some("failure".into()),
                run_started_at: None,
                event: None,
                created_at: None,
                updated_at: None,
                actor: None,

                head_commit: None,

                triggering_actor: None,

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

    #[test]
    #[ignore = "requires GTK display"]
    fn prepend_run_deduplicates_cached_runs() {
        let Some(_guard) = gtk_test_guard("prepend_run_deduplicates_cached_runs") else {
            return;
        };

        let model = test_run_list_model();
        let existing = WorkflowRun {
            id: 2,
            run_number: Some(2),
            workflow_id: None,
            name: Some("Existing".into()),
            display_title: Some("Existing".into()),
            head_branch: Some("main".into()),
            status: Some("completed".into()),
            conclusion: Some("success".into()),
            run_started_at: None,
            event: None,
            created_at: None,
            updated_at: None,
            actor: None,

            head_commit: None,

            triggering_actor: None,

            html_url: None,
        };
        model.set_runs(std::sync::Arc::new(vec![existing.clone()]));

        let replacement = WorkflowRun {
            id: 2,
            status: Some("queued".into()),
            conclusion: None,
            ..existing
        };

        model.prepend_run(replacement, &RunFilters::default());

        let cached = model.last_runs.borrow().clone();
        assert_eq!(cached.len(), 1);
        assert_eq!(cached[0].status.as_deref(), Some("queued"));
    }
}
