//! Building one workflow row's widget tree.
//!
//! Split out of `create_workflow_expander_row`, which was 1055 lines. This is
//! the construction half — header, status dot, title and meta labels, the three
//! action buttons, the expander and the runs card. The behaviour wired onto
//! them stays next door.
//!
//! Only five widgets escape: the three buttons, the expander and the run list.
//! The other twenty-two locals are plumbing that never leaves this function,
//! which is what made the seam clean.

use super::*;

pub(super) struct RowWidgets {
    pub logs_btn: gtk::Button,
    pub trigger_btn: gtk::Button,
    pub cancel_btn: gtk::Button,
    pub expander: gtk::Expander,
    pub run_list: WorkflowRunListModel,
}

pub(super) fn build_row_widgets(
    main_box: &gtk::Box,
    context: &WorkflowRowContext,
    workflow: &Workflow,
) -> RowWidgets {
    let client = context.client.clone();
    let owner = context.owner.clone();
    let repo = context.repo.clone();
    let repo_model = context.repo_model.clone();
    let parent_window = context.parent_window.clone();
    let toast_overlay = context.toast_overlay.clone();
    let job_contexts = context.job_contexts.clone();
    let workflow_file = workflow_file_name(&workflow.path).to_string();

    // ---- Row header: chevron (expander arrow) + status dot + title/meta + actions
    let header_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    header_box.set_margin_start(6);
    header_box.set_margin_end(10);
    header_box.set_margin_top(12);
    header_box.set_margin_bottom(12);
    header_box.set_valign(gtk::Align::Center);

    let status_dot = build_status_dot("window-minimize-symbolic", "idle", WORKFLOW_DOT_SIZE);
    header_box.append(&status_dot);

    let text_box = gtk::Box::new(gtk::Orientation::Vertical, 3);
    text_box.set_hexpand(true);
    text_box.set_valign(gtk::Align::Center);

    let workflow_name_label = gtk::Label::new(Some(&workflow.name));
    workflow_name_label.set_halign(gtk::Align::Start);
    workflow_name_label.set_hexpand(true);
    workflow_name_label.set_ellipsize(pango::EllipsizeMode::End);
    workflow_name_label.add_css_class("workflow-title");
    text_box.append(&workflow_name_label);

    let meta_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    meta_box.set_halign(gtk::Align::Start);

    let file_label = gtk::Label::new(Some(&workflow_file));
    file_label.add_css_class("workflow-file");
    file_label.add_css_class("dim-label");
    file_label.add_css_class("caption");
    meta_box.append(&file_label);

    let meta_separator = gtk::Label::new(Some("·"));
    meta_separator.add_css_class("dim-label");
    meta_separator.add_css_class("caption");
    meta_box.append(&meta_separator);

    let meta_label = gtk::Label::new(Some(tr("No runs yet").as_str()));
    meta_label.add_css_class("dim-label");
    meta_label.add_css_class("caption");
    meta_label.set_halign(gtk::Align::Start);
    meta_label.set_ellipsize(pango::EllipsizeMode::End);
    meta_box.append(&meta_label);

    text_box.append(&meta_box);
    header_box.append(&text_box);

    // ---- Trailing actions: logs, trigger (play) / cancel (stop while running)
    let actions_box = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    actions_box.set_valign(gtk::Align::Center);
    actions_box.set_halign(gtk::Align::End);

    let logs_btn = gtk::Button::from_icon_name("text-x-generic-symbolic");
    crate::ui::utils::describe_control(&logs_btn, tr("View logs").as_str());
    logs_btn.add_css_class("row-action-btn");
    logs_btn.set_valign(gtk::Align::Center);
    logs_btn.set_focus_on_click(false);
    actions_box.append(&logs_btn);

    let trigger_btn = gtk::Button::from_icon_name("media-playback-start-symbolic");
    crate::ui::utils::describe_control(&trigger_btn, tr("Trigger workflow").as_str());
    trigger_btn.add_css_class("row-action-btn");
    trigger_btn.add_css_class("run-action");
    trigger_btn.set_valign(gtk::Align::Center);
    trigger_btn.set_focus_on_click(false);
    actions_box.append(&trigger_btn);

    let cancel_btn = gtk::Button::from_icon_name("process-stop-symbolic");
    crate::ui::utils::describe_control(&cancel_btn, tr("Cancel run").as_str());
    cancel_btn.add_css_class("row-action-btn");
    cancel_btn.add_css_class("cancel-action");
    cancel_btn.set_valign(gtk::Align::Center);
    cancel_btn.set_focus_on_click(false);
    cancel_btn.set_visible(false);
    actions_box.append(&cancel_btn);

    header_box.append(&actions_box);

    let expander = gtk::Expander::new(None);
    // Horizontal breathing room around the disclosure arrow: `margin_start`
    // insets the arrow from the card edge, the header's own `margin_start`
    // (set above) leaves a gap between the arrow and the row content.
    expander.set_margin_start(6);
    expander.set_label_widget(Some(&header_box));
    expander.set_widget_name(&format!("workflow_{}", workflow.id));
    set_data(&expander, "actioneer-workflow-name", workflow.name.clone());
    set_data(&expander, "actioneer-workflow-id", workflow.id);

    // ---- Expanded area: progress + "Recent runs" sub-header + runs card
    let detail_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
    detail_box.add_css_class("workflow-detail");
    detail_box.set_margin_start(62);
    detail_box.set_margin_end(14);
    detail_box.set_margin_bottom(14);

    let progress_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    progress_row.set_valign(gtk::Align::Center);
    progress_row.set_visible(false);

    let progress_bar = gtk::ProgressBar::new();
    progress_bar.add_css_class("workflow-progress");
    progress_bar.set_hexpand(true);
    progress_bar.set_valign(gtk::Align::Center);
    progress_row.append(&progress_bar);

    let progress_label = gtk::Label::new(None);
    progress_label.add_css_class("mono");
    progress_label.add_css_class("dim-label");
    progress_label.add_css_class("caption");
    progress_row.append(&progress_label);

    detail_box.append(&progress_row);

    let subheader = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    subheader.set_margin_top(2);

    let (recent_heading, recent_suppress_tracking) =
        crate::ui::utils::section_heading(&tr("Recent runs"));
    let recent_label = gtk::Label::new(Some(&recent_heading));
    if recent_suppress_tracking {
        recent_label.add_css_class("no-tracking");
    }
    recent_label.add_css_class("section-label");
    recent_label.set_halign(gtk::Align::Start);
    recent_label.set_hexpand(true);
    subheader.append(&recent_label);

    let counts_label = gtk::Label::new(None);
    counts_label.add_css_class("dim-label");
    counts_label.add_css_class("caption");
    counts_label.set_visible(false);
    subheader.append(&counts_label);

    let counts_separator = gtk::Label::new(Some("·"));
    counts_separator.add_css_class("dim-label");
    counts_separator.add_css_class("caption");
    subheader.append(&counts_separator);
    counts_label
        .bind_property("visible", &counts_separator, "visible")
        .sync_create()
        .build();

    let actions_url = format!(
        "https://github.com/{}/{}/actions/workflows/{}",
        owner, repo, workflow_file
    );
    let all_link = gtk::LinkButton::with_label(&actions_url, tr("All on GitHub").as_str());
    all_link.add_css_class("caption");
    all_link.set_valign(gtk::Align::Center);
    subheader.append(&all_link);

    detail_box.append(&subheader);

    let run_row_context = RunRowContext::new(
        client.clone(),
        owner.clone(),
        repo.clone(),
        repo_model.clone(),
        parent_window.clone(),
        workflow.id,
        toast_overlay.clone(),
        job_contexts.clone(),
        context.run_badge_summaries.clone(),
    );
    let run_list = WorkflowRunListModel::new(run_row_context);

    let runs_card = gtk::Box::new(gtk::Orientation::Vertical, 0);
    runs_card.add_css_class("runs-card");
    runs_card.set_overflow(gtk::Overflow::Hidden);
    runs_card.append(&run_list.widget());
    detail_box.append(&runs_card);

    expander.set_child(Some(&detail_box));

    let row_header = WorkflowRowHeader {
        status_dot,
        meta_label,
        progress_row,
        progress_bar,
        progress_label,
        trigger_btn: trigger_btn.downgrade(),
        cancel_btn: cancel_btn.downgrade(),
        elapsed: Rc::new(RefCell::new(ElapsedTicker::default())),
    };
    run_list.set_row_header(row_header.clone());
    run_list.set_detail_header(context.header.clone());
    run_list.set_counts_label(counts_label);
    set_data(&expander, "actioneer-run-list", run_list.clone());
    main_box.append(&expander);

    // Render the header from the latest run we already know about (if any).
    {
        let latest = context.header.latest_run(workflow.id);
        let summary = latest
            .as_ref()
            .and_then(|run| context.run_badge_summaries.borrow().get(&run.id).cloned());
        update_workflow_row_header(&row_header, latest.as_ref(), summary.as_ref());
    }
    RowWidgets {
        logs_btn,
        trigger_btn,
        cancel_btn,
        expander,
        run_list,
    }
}
