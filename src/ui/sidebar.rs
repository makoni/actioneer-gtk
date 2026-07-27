use crate::api::models::Repo;
use crate::favorites::FavoritesManager;
use crate::i18n::tr;
use crate::ui::state::{RepoActionsState, WorkflowStatusCounts};
use crate::ui::utils::MainContextChannelExt;
use crate::ui::utils::widget_data::{get_data_clone, get_data_copy, set_data};
use gtk::prelude::*;
use gtk4::{self as gtk, gio, glib};
use parking_lot::Mutex;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, warn};

const MAX_WORKFLOWS_PER_REPO: usize = 3;
const REPO_WIDE_RUNS_PAGE_SIZE: usize = 100;
const REPO_ID_KEY: &str = "actioneer-repo-id";
const REPO_FULL_NAME_KEY: &str = "actioneer-repo-full-name";
const REPO_MODEL_KEY: &str = "actioneer-repo-model";
const SELECTABLE_KEY: &str = "actioneer-sidebar-selectable";
const ACTIVATABLE_KEY: &str = "actioneer-sidebar-activatable";

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
        tr("Favorites").as_str(),
        "emblem-favorite-symbolic",
        favorites_section,
        &favorites_snapshot,
    );
    append_section(
        tr("Actions Enabled").as_str(),
        "media-playback-start-symbolic",
        enabled_section,
        &favorites_snapshot,
    );
    append_section(
        tr("Actions Disabled").as_str(),
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

pub fn row_matches_query(row: &gtk::Widget, query: &str) -> bool {
    let query = query.trim();

    if query.is_empty() {
        return true;
    }

    if let Some(label) = find_label_by_name(row, "repo-name-label") {
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
) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let repo_id = repo.id;
    row.set_margin_top(12);
    row.set_margin_bottom(12);
    row.set_margin_start(12);
    row.set_margin_end(12);
    row.set_hexpand(true);
    row.set_can_focus(false);
    row.add_css_class("activatable");

    let favorite_button = gtk::ToggleButton::new();
    favorite_button.add_css_class("flat");
    favorite_button.set_valign(gtk::Align::Center);
    favorite_button.set_icon_name("emblem-favorite-symbolic");
    favorite_button.set_tooltip_text(Some(tr("Toggle favorite").as_str()));
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

    row.append(&favorite_button);

    let icon = gtk::Image::from_icon_name("folder-symbolic");
    icon.set_pixel_size(24);
    icon.set_valign(gtk::Align::Center);
    icon.set_halign(gtk::Align::Center);
    row.append(&icon);

    let content_box = gtk::Box::new(gtk::Orientation::Vertical, 6);

    let name_label = gtk::Label::new(Some(&repo.full_name));
    name_label.set_halign(gtk::Align::Start);
    name_label.add_css_class("heading");
    name_label.set_widget_name("repo-name-label");
    content_box.append(&name_label);

    if repo.is_private {
        let private_label = create_meta_label(tr("Private"));
        content_box.append(&private_label);
    } else {
        let public_label = create_meta_label(tr("Public"));
        content_box.append(&public_label);
    }

    let meta_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    meta_box.set_halign(gtk::Align::Start);

    if workflow_counts.active > 0 {
        meta_box.append(&create_meta_label(
            tr("Active runs: {count}")
                .replace("{count}", workflow_counts.active.to_string().as_str()),
        ));
    }

    if workflow_counts.failed > 0 {
        let failed_label = create_meta_label(
            tr("Failures: {count}").replace("{count}", workflow_counts.failed.to_string().as_str()),
        );
        failed_label.add_css_class("error");
        meta_box.append(&failed_label);
    }

    if meta_box.first_child().is_some() {
        content_box.append(&meta_box);
    }

    row.append(&content_box);

    set_data(&row, REPO_ID_KEY, repo_id);
    set_data(&row, REPO_FULL_NAME_KEY, repo.full_name.clone());
    set_data(&row, REPO_MODEL_KEY, repo);
    set_data(&row, SELECTABLE_KEY, true);
    set_data(&row, ACTIVATABLE_KEY, true);

    row
}

fn repo_from_row(row: &gtk::Widget) -> Option<Repo> {
    get_data_clone(row, REPO_MODEL_KEY)
}

pub fn repo_from_object(obj: &glib::Object) -> Option<Repo> {
    obj.downcast_ref::<gtk::Widget>().and_then(repo_from_row)
}

fn repo_id_from_row(row: &gtk::Widget) -> Option<i64> {
    get_data_copy(row, REPO_ID_KEY)
}

pub fn repo_id_from_object(obj: &glib::Object) -> Option<i64> {
    obj.downcast_ref::<gtk::Widget>().and_then(repo_id_from_row)
}

pub fn row_selectable_from_object(obj: &glib::Object) -> bool {
    obj.downcast_ref::<gtk::Widget>()
        .and_then(|row| get_data_copy(row, SELECTABLE_KEY))
        .unwrap_or(false)
}

pub fn row_activatable_from_object(obj: &glib::Object) -> bool {
    obj.downcast_ref::<gtk::Widget>()
        .and_then(|row| get_data_copy(row, ACTIVATABLE_KEY))
        .unwrap_or(false)
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

fn create_section_header(title: &str, icon_name: &str) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    row.set_can_focus(false);
    row.set_can_target(false);
    row.add_css_class("section-header");
    row.add_css_class("hoverless-row");
    row.set_margin_top(18);
    row.set_margin_bottom(6);
    row.set_margin_start(12);
    row.set_margin_end(12);
    set_data(&row, SELECTABLE_KEY, false);
    set_data(&row, ACTIVATABLE_KEY, false);

    let icon = gtk::Image::from_icon_name(icon_name);
    icon.set_pixel_size(16);
    row.append(&icon);

    let label = gtk::Label::new(Some(title));
    label.set_halign(gtk::Align::Start);
    label.set_hexpand(true);
    label.add_css_class("dim-label");
    label.add_css_class("heading");

    row.append(&label);

    row
}

fn create_owner_header(owner: &str) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    row.set_can_focus(false);
    row.set_can_target(false);
    row.add_css_class("owner-header");
    row.add_css_class("hoverless-row");
    row.set_margin_top(4);
    row.set_margin_bottom(4);
    row.set_margin_start(28);
    row.set_margin_end(12);
    set_data(&row, SELECTABLE_KEY, false);
    set_data(&row, ACTIVATABLE_KEY, false);

    let label = gtk::Label::new(Some(owner));
    label.set_halign(gtk::Align::Start);
    label.add_css_class("caption");
    label.add_css_class("dim-label");
    row.append(&label);
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
        repos.sort_by_key(|repo| repo.name.to_lowercase());
    }

    grouped
}

fn create_meta_label(text: String) -> gtk::Label {
    let label = gtk::Label::new(Some(text.as_str()));
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

    let workflows: Vec<_> = client
        .list_workflows(owner, repo)
        .await?
        .into_iter()
        .take(MAX_WORKFLOWS_PER_REPO)
        .collect();

    if workflows.is_empty() {
        return Ok(WorkflowStatusCounts::default());
    }

    let workflow_ids: Vec<i64> = workflows.iter().map(|workflow| workflow.id).collect();
    let repo_runs_result = client.list_repository_runs(owner, repo).await;
    let repo_runs_truncated = repo_runs_result
        .as_ref()
        .map(|runs| runs.len() >= REPO_WIDE_RUNS_PAGE_SIZE)
        .unwrap_or(false);
    let (mut latest_runs, fallback_ids) = match repo_runs_result {
        Ok(repo_runs) => {
            select_latest_runs_for_workflows(&workflow_ids, &repo_runs, repo_runs_truncated)
        }
        Err(error) => {
            warn!(
                "Failed to list repository runs for {}/{}: {}; falling back to per-workflow requests",
                owner, repo, error
            );
            (HashMap::new(), workflow_ids.clone())
        }
    };

    for workflow_id in fallback_ids {
        let runs_result = client.list_runs(owner, repo, workflow_id).await;
        match runs_result {
            Ok(runs) => {
                if let Some(latest) = runs.first() {
                    latest_runs
                        .entry(workflow_id)
                        .or_insert_with(|| latest.clone());
                }
            }
            Err(e) => {
                warn!("Failed to list runs for workflow {}: {}", workflow_id, e);
            }
        }
    }

    let mut active_count = 0;
    let mut failed_count = 0;

    for workflow in workflows {
        let Some(latest) = latest_runs.get(&workflow.id) else {
            continue;
        };

        if is_run_active(latest) {
            active_count += 1;
        } else if is_run_failure(latest) {
            failed_count += 1;
        }
    }

    Ok(WorkflowStatusCounts {
        active: active_count,
        failed: failed_count,
    })
}

fn select_latest_runs_for_workflows(
    workflow_ids: &[i64],
    repo_runs: &[crate::api::models::WorkflowRun],
    repo_runs_truncated: bool,
) -> (HashMap<i64, crate::api::models::WorkflowRun>, Vec<i64>) {
    let selected_ids: HashSet<i64> = workflow_ids.iter().copied().collect();
    let mut latest_runs = HashMap::new();

    for run in repo_runs {
        let Some(workflow_id) = run.workflow_id else {
            continue;
        };

        if !selected_ids.contains(&workflow_id) || latest_runs.contains_key(&workflow_id) {
            continue;
        }

        latest_runs.insert(workflow_id, run.clone());

        if latest_runs.len() == selected_ids.len() {
            break;
        }
    }

    let missing = if repo_runs_truncated {
        workflow_ids
            .iter()
            .copied()
            .filter(|workflow_id| !latest_runs.contains_key(workflow_id))
            .collect()
    } else {
        Vec::new()
    };

    (latest_runs, missing)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::models::{Repo, User, WorkflowRun};
    use crate::ui::state::WorkflowStatusCounts;
    use crate::ui::test_helpers::gtk_test_guard;
    use gtk4::{self as gtk, gio};
    use parking_lot::Mutex;
    use std::collections::{HashMap, HashSet};
    use std::sync::Arc;

    fn run(id: i64, workflow_id: Option<i64>) -> WorkflowRun {
        WorkflowRun {
            id,
            run_number: Some(id),
            workflow_id,
            name: Some(format!("Run {id}")),
            display_title: Some(format!("Run {id}")),
            head_branch: Some("main".to_string()),
            status: Some("completed".to_string()),
            conclusion: Some("success".to_string()),
            run_started_at: None,
            event: None,
            created_at: None,
            updated_at: None,
            actor: None,
            head_commit: None,
            triggering_actor: None,
            html_url: None,
        }
    }

    #[test]
    fn select_latest_runs_keeps_first_repo_run_per_workflow() {
        let (latest, missing) = select_latest_runs_for_workflows(
            &[11, 22],
            &[
                run(200, Some(22)),
                run(199, Some(22)),
                run(150, None),
                run(100, Some(11)),
            ],
            false,
        );

        assert_eq!(latest.get(&22).map(|run| run.id), Some(200));
        assert_eq!(latest.get(&11).map(|run| run.id), Some(100));
        assert!(
            missing.is_empty(),
            "non-truncated responses should not trigger fallback"
        );
    }

    #[test]
    fn select_latest_runs_requests_fallback_only_for_truncated_missing_workflows() {
        let (_, missing_without_truncation) =
            select_latest_runs_for_workflows(&[11, 22, 33], &[run(300, Some(11))], false);
        assert!(
            missing_without_truncation.is_empty(),
            "missing workflows should be treated as having no runs when the repo-wide page is complete"
        );

        let (_, missing_with_truncation) =
            select_latest_runs_for_workflows(&[11, 22, 33], &[run(300, Some(11))], true);
        assert_eq!(missing_with_truncation, vec![22, 33]);
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn repo_rows_use_widget_metadata_without_listbox_rows() {
        let Some(_guard) = gtk_test_guard("repo_rows_use_widget_metadata_without_listbox_rows")
        else {
            return;
        };

        let store = gio::ListStore::new::<gtk::Widget>();
        let repo = Repo {
            id: 42,
            name: "actioneer".into(),
            full_name: "mak/actioneer".into(),
            owner: User {
                login: "mak".into(),
            },
            is_private: false,
            permissions: None,
            default_branch: Some("main".into()),
        };

        rebuild_repo_list(
            store.clone(),
            RepoListRenderContext {
                repos: vec![repo.clone()],
                favorites_snapshot: HashSet::new(),
                actions_snapshot: HashMap::new(),
                workflow_snapshot: HashMap::from([(repo.id, WorkflowStatusCounts::default())]),
                favorites_state: Arc::new(Mutex::new(HashSet::new())),
                favorites_manager: None,
            },
        );

        let repo_obj = (0..store.n_items())
            .find_map(|idx| {
                store
                    .item(idx)
                    .filter(|obj| repo_from_object(obj).is_some())
            })
            .expect("repo item should exist");

        assert!(repo_obj.downcast_ref::<gtk::ListBoxRow>().is_none());
        assert_eq!(
            repo_from_object(&repo_obj).map(|item| item.id),
            Some(repo.id)
        );
        assert!(row_selectable_from_object(repo_obj.as_ref()));
    }
}
