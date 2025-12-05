use crate::api::models::Repo;
use crate::favorites::FavoritesManager;
use crate::ui::state::{RepoActionsState, WorkflowStatusCounts};
use crate::ui::utils::MainContextChannelExt;
use gtk::prelude::*;
use gtk4::{self as gtk, gio, glib};
use parking_lot::Mutex;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, warn};

const MAX_WORKFLOWS_PER_REPO: usize = 3;

#[derive(Clone)]
pub struct RepoListRenderContext {
    pub repos: Vec<Repo>,
    pub favorites_snapshot: HashSet<i64>,
    pub actions_snapshot: HashMap<i64, RepoActionsState>,
    pub workflow_snapshot: HashMap<i64, WorkflowStatusCounts>,
    pub favorites_state: Arc<Mutex<HashSet<i64>>>,
    pub favorites_manager: Option<Arc<FavoritesManager>>,
}

pub fn rebuild_repo_list(store: gio::ListStore, context: RepoListRenderContext) {
    let RepoListRenderContext {
        repos,
        favorites_snapshot,
        actions_snapshot,
        workflow_snapshot,
        favorites_state,
        favorites_manager,
    } = context;

    let render_start = Instant::now();

    store.remove_all();

    let mut favorites_section = Vec::new();
    let mut enabled_section = Vec::new();
    let mut disabled_section = Vec::new();

    for repo in repos.iter().cloned() {
        if favorites_snapshot.contains(&repo.id) {
            favorites_section.push(repo);
        } else {
            let state = actions_snapshot
                .get(&repo.id)
                .copied()
                .unwrap_or(RepoActionsState::Unknown);
            match state {
                RepoActionsState::Disabled => disabled_section.push(repo),
                _ => enabled_section.push(repo),
            }
        }
    }

    let favorites_state_for_rows = favorites_state.clone();
    let favorites_manager_for_rows = favorites_manager.clone();
    let actions_snapshot_for_rows = actions_snapshot.clone();
    let workflow_snapshot_for_rows = workflow_snapshot.clone();
    let store_ref = store.clone();

    let append_section =
        move |title: &str, icon_name: &str, repos: Vec<Repo>, favorites_snapshot: &HashSet<i64>| {
            if repos.is_empty() {
                return;
            }

            let header = create_section_header(title, icon_name);
            store_ref.append(&header);

            let grouped = group_repos_by_owner(repos);

            for (owner, repos) in grouped {
                let owner_row = create_owner_header(&owner);
                store_ref.append(&owner_row);

                for repo in repos {
                    let repo_id = repo.id;
                    let actions_state = actions_snapshot_for_rows
                        .get(&repo_id)
                        .copied()
                        .unwrap_or(RepoActionsState::Unknown);
                    let workflow_counts = workflow_snapshot_for_rows
                        .get(&repo_id)
                        .cloned()
                        .unwrap_or_default();

                    let row = build_repo_row(
                        repo.clone(),
                        favorites_snapshot.contains(&repo_id),
                        actions_state,
                        workflow_counts,
                        favorites_state_for_rows.clone(),
                        favorites_manager_for_rows.clone(),
                    );

                    store_ref.append(&row);
                }
            }
        };

    append_section(
        "Favorites",
        "emblem-favorite-symbolic",
        favorites_section,
        &favorites_snapshot,
    );
    append_section(
        "Actions Enabled",
        "media-playback-start-symbolic",
        enabled_section,
        &favorites_snapshot,
    );
    append_section(
        "Actions Disabled",
        "process-stop-symbolic",
        disabled_section,
        &favorites_snapshot,
    );

    let elapsed = render_start.elapsed();
    if repos.len() >= 100 {
        debug!(
            repo_count = repos.len(),
            duration_ms = elapsed.as_millis(),
            "Rebuilt repo sidebar list store"
        );
    }
}

pub fn row_matches_query(row: &gtk::ListBoxRow, query: &str) -> bool {
    let query = query.trim();

    if query.is_empty() {
        return true;
    }

    if let Some(child) = row.child()
        && let Some(label) = find_label_by_name(&child, "repo-name-label")
    {
        return label.text().to_lowercase().contains(query);
    }

    false
}

pub fn find_label_by_name(widget: &gtk::Widget, name: &str) -> Option<gtk::Label> {
    if widget.widget_name() == name {
        return widget.clone().downcast::<gtk::Label>().ok();
    }

    let mut child = widget.first_child();
    while let Some(current) = child {
        if let Some(label) = find_label_by_name(&current, name) {
            return Some(label);
        }
        child = current.next_sibling();
    }
    None
}

fn build_repo_row(
    repo: Repo,
    is_favorite: bool,
    _actions_state: RepoActionsState,
    workflow_counts: WorkflowStatusCounts,
    favorites_arc: Arc<Mutex<HashSet<i64>>>,
    favorites_manager: Option<Arc<FavoritesManager>>,
) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.set_activatable(true);
    row.set_selectable(true);

    let repo_id = repo.id;

    let wrapper = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    wrapper.set_margin_top(12);
    wrapper.set_margin_bottom(12);
    wrapper.set_margin_start(12);
    wrapper.set_margin_end(12);

    let favorite_button = gtk::ToggleButton::new();
    favorite_button.add_css_class("flat");
    favorite_button.set_valign(gtk::Align::Center);
    favorite_button.set_icon_name("emblem-favorite-symbolic");
    favorite_button.set_tooltip_text(Some("Toggle favorite"));
    favorite_button.set_active(is_favorite);
    update_favorite_button_visual(&favorite_button, is_favorite);

    let favorites_arc_for_update = favorites_arc.clone();
    let favorites_manager_for_update = favorites_manager.clone();

    favorite_button.connect_toggled(move |button| {
        let desired_state = button.is_active();
        update_favorite_button_visual(button, desired_state);

        let favorites_arc = favorites_arc_for_update.clone();
        let favorites_manager = favorites_manager_for_update.clone();

        let previous_state = {
            let favorites = favorites_arc.lock();
            favorites.contains(&repo_id)
        };

        if previous_state == desired_state {
            return;
        }

        if let Some(manager) = favorites_manager {
            let button_clone = button.clone();
            let favorites_arc_clone = favorites_arc.clone();
            let (sender, receiver) =
                glib::MainContext::default()
                    .channel::<Result<bool, (anyhow::Error, bool)>>(glib::Priority::default());

            receiver.attach(None, move |result| {
                match result {
                    Ok(is_now_favorite) => {
                        let mut favorites = favorites_arc_clone.lock();
                        if is_now_favorite {
                            favorites.insert(repo_id);
                        } else {
                            favorites.remove(&repo_id);
                        }

                        if button_clone.is_active() != is_now_favorite {
                            button_clone.set_active(is_now_favorite);
                            update_favorite_button_visual(&button_clone, is_now_favorite);
                        }
                    }
                    Err((err, stored_state)) => {
                        warn!("Failed to update favorite {}: {}", repo_id, err);

                        let mut favorites = favorites_arc_clone.lock();
                        if stored_state {
                            favorites.insert(repo_id);
                        } else {
                            favorites.remove(&repo_id);
                        }

                        if button_clone.is_active() != stored_state {
                            button_clone.set_active(stored_state);
                            update_favorite_button_visual(&button_clone, stored_state);
                        }
                    }
                }

                glib::ControlFlow::Break
            });

            let manager_for_task = manager.clone();
            crate::runtime_handle().spawn(async move {
                let outcome = match manager_for_task.toggle_favorite(repo_id).await {
                    Ok(next_state) => Ok(next_state),
                    Err(err) => {
                        let current_state = manager_for_task.is_favorite(repo_id).await;
                        Err((err, current_state))
                    }
                };

                let _ = sender.send(outcome);
            });
        } else {
            let mut favorites = favorites_arc.lock();
            if desired_state {
                favorites.insert(repo_id);
            } else {
                favorites.remove(&repo_id);
            }
        }
    });

    wrapper.append(&favorite_button);

    let icon = gtk::Image::from_icon_name("folder-symbolic");
    icon.set_pixel_size(24);
    icon.set_valign(gtk::Align::Center);
    icon.set_halign(gtk::Align::Center);
    wrapper.append(&icon);

    let content_box = gtk::Box::new(gtk::Orientation::Vertical, 6);

    let name_label = gtk::Label::new(Some(&repo.full_name));
    name_label.set_halign(gtk::Align::Start);
    name_label.add_css_class("heading");
    name_label.set_widget_name("repo-name-label");
    content_box.append(&name_label);

    if repo.is_private {
        let private_label = create_meta_label("Private".to_string());
        content_box.append(&private_label);
    } else {
        let public_label = create_meta_label("Public".to_string());
        content_box.append(&public_label);
    }

    let meta_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    meta_box.set_halign(gtk::Align::Start);

    if workflow_counts.active > 0 {
        meta_box.append(&create_meta_label(format!(
            "Active runs: {}",
            workflow_counts.active
        )));
    }

    if workflow_counts.failed > 0 {
        let failed_label = create_meta_label(format!("Failures: {}", workflow_counts.failed));
        failed_label.add_css_class("error");
        meta_box.append(&failed_label);
    }

    if meta_box.first_child().is_some() {
        content_box.append(&meta_box);
    }

    wrapper.append(&content_box);

    row.set_child(Some(&wrapper));

    unsafe {
        row.set_data("actioneer-repo-id", repo_id);
        row.set_data("actioneer-repo-full-name", repo.full_name.clone());
        row.set_data("actioneer-repo-model", repo);
    }

    row
}

fn repo_from_row(row: &gtk::ListBoxRow) -> Option<Repo> {
    unsafe {
        row.data::<Repo>("actioneer-repo-model")
            .map(|ptr| ptr.as_ref().clone())
    }
}

pub fn repo_from_object(obj: &glib::Object) -> Option<Repo> {
    obj.downcast_ref::<gtk::ListBoxRow>()
        .and_then(repo_from_row)
}

fn repo_id_from_row(row: &gtk::ListBoxRow) -> Option<i64> {
    unsafe {
        row.data::<i64>("actioneer-repo-id")
            .map(|ptr| *ptr.as_ref())
    }
}

pub fn repo_id_from_object(obj: &glib::Object) -> Option<i64> {
    obj.downcast_ref::<gtk::ListBoxRow>()
        .and_then(repo_id_from_row)
}

pub fn find_repo_index(model: &gtk::FilterListModel, repo_id: i64) -> Option<u32> {
    for idx in 0..model.n_items() {
        if let Some(obj) = model.item(idx)
            && let Some(id) = repo_id_from_object(&obj)
            && id == repo_id
        {
            return Some(idx);
        }
    }
    None
}

pub fn find_first_repo_index(model: &gtk::FilterListModel) -> Option<u32> {
    for idx in 0..model.n_items() {
        if let Some(obj) = model.item(idx)
            && repo_id_from_object(&obj).is_some()
        {
            return Some(idx);
        }
    }
    None
}

fn create_section_header(title: &str, icon_name: &str) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.set_selectable(false);
    row.set_activatable(false);
    row.set_can_focus(false);
    row.set_can_target(false);
    row.add_css_class("section-header");
    row.add_css_class("hoverless-row");

    let container = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    container.set_margin_top(18);
    container.set_margin_bottom(6);
    container.set_margin_start(12);
    container.set_margin_end(12);

    let icon = gtk::Image::from_icon_name(icon_name);
    icon.set_pixel_size(16);
    container.append(&icon);

    let label = gtk::Label::new(Some(title));
    label.set_halign(gtk::Align::Start);
    label.set_hexpand(true);
    label.add_css_class("dim-label");
    label.add_css_class("heading");

    container.append(&label);
    row.set_child(Some(&container));

    row
}

fn create_owner_header(owner: &str) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.set_selectable(false);
    row.set_activatable(false);
    row.set_can_focus(false);
    row.set_can_target(false);
    row.add_css_class("owner-header");
    row.add_css_class("hoverless-row");

    let container = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    container.set_margin_top(4);
    container.set_margin_bottom(4);
    container.set_margin_start(28);
    container.set_margin_end(12);

    let label = gtk::Label::new(Some(owner));
    label.set_halign(gtk::Align::Start);
    label.add_css_class("caption");
    label.add_css_class("dim-label");
    container.append(&label);

    row.set_child(Some(&container));
    row
}

fn group_repos_by_owner(repos: Vec<Repo>) -> BTreeMap<String, Vec<Repo>> {
    let mut grouped: BTreeMap<String, Vec<Repo>> = BTreeMap::new();

    for repo in repos {
        grouped
            .entry(repo.owner.login.clone())
            .or_default()
            .push(repo);
    }

    for repos in grouped.values_mut() {
        repos.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    }

    grouped
}

fn create_meta_label(text: String) -> gtk::Label {
    let label = gtk::Label::new(Some(&text));
    label.set_halign(gtk::Align::Start);
    label.add_css_class("dim-label");
    label.add_css_class("caption");
    label
}

fn update_favorite_button_visual(button: &gtk::ToggleButton, is_active: bool) {
    if is_active {
        button.remove_css_class("flat");
        button.add_css_class("suggested-action");
        button.set_opacity(1.0);
    } else {
        button.remove_css_class("suggested-action");
        button.add_css_class("flat");
        button.set_opacity(0.5);
    }
}

/// Gather workflow status counts for a repository
pub async fn gather_workflow_status_counts(
    client: &crate::api::GitHubClient,
    owner: &str,
    repo: &str,
) -> Result<WorkflowStatusCounts, crate::api::GitHubError> {
    use crate::ui::utils::{is_run_active, is_run_failure};

    let workflows = client.list_workflows(owner, repo).await?;

    let mut active_count = 0;
    let mut failed_count = 0;

    for workflow in workflows.into_iter().take(MAX_WORKFLOWS_PER_REPO) {
        let runs_result = client.list_runs(owner, repo, workflow.id).await;
        match runs_result {
            Ok(runs) => {
                if let Some(latest) = runs.first() {
                    if is_run_active(latest) {
                        active_count += 1;
                    } else if is_run_failure(latest) {
                        failed_count += 1;
                    }
                }
            }
            Err(e) => {
                warn!("Failed to list runs for workflow {}: {}", workflow.id, e);
            }
        }
    }

    Ok(WorkflowStatusCounts {
        active: active_count,
        failed: failed_count,
    })
}
