use crate::kernel::i18n::tr;
use crate::runtime::channel::MainContextChannelExt;
use crate::services::api::models::Repo;
use crate::services::favorites::FavoritesManager;
use crate::ui::state::{RepoActionsState, WorkflowStatusCounts};
use crate::ui::utils::apply_favorite_result;
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
pub(crate) const FAVORITE_ROW_KEY: &str = "actioneer-sidebar-favorite";
pub(crate) const ACTIVE_RUNS_ROW_KEY: &str = "actioneer-sidebar-active-runs";
const REPO_FAV_BUTTON_KEY: &str = "actioneer-sidebar-repo-fav-button";
const REPO_META_BOX_KEY: &str = "actioneer-sidebar-repo-meta-box";
const REPO_VISIBILITY_LABEL_KEY: &str = "actioneer-sidebar-repo-visibility";
const REPO_NAME_LABEL_KEY: &str = "actioneer-sidebar-repo-name";
const REPO_META_ACTIVE_LABEL_KEY: &str = "actioneer-sidebar-repo-meta-active";
const REPO_META_FAILED_LABEL_KEY: &str = "actioneer-sidebar-repo-meta-failed";
const REPO_FAV_PENDING_KEY: &str = "actioneer-sidebar-repo-fav-pending";
// A row's identity payload, read back in `snapshot_rows`. Repo rows carry their
// id (`REPO_ID_KEY`); a section header carries its section; an owner header
// carries its section plus the owner login. No `format!` per row per rebuild.
const ROW_SECTION_KIND_KEY: &str = "actioneer-sidebar-row-section";
const ROW_OWNER_LOGIN_KEY: &str = "actioneer-sidebar-row-owner";

/// Which section a row belongs to. This is the *stable* identity of a section
/// header (and, with the owner, of an owner header) — deliberately not the
/// translated title, so switching languages does not invalidate the rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum SectionKind {
    Enabled,
    Disabled,
}

impl SectionKind {
    const ENABLED: i32 = 0;
    const DISABLED: i32 = 1;

    fn from_kind(kind: i32) -> Option<Self> {
        match kind {
            Self::ENABLED => Some(Self::Enabled),
            Self::DISABLED => Some(Self::Disabled),
            _ => None,
        }
    }

    fn as_kind(self) -> i32 {
        match self {
            Self::Enabled => Self::ENABLED,
            Self::Disabled => Self::DISABLED,
        }
    }

    fn title(self) -> String {
        match self {
            Self::Enabled => tr("Actions Enabled"),
            Self::Disabled => tr("Actions Disabled"),
        }
    }
}

/// The stable identity of a sidebar row, used to reuse widgets across rebuilds
/// (which is what keeps `GtkListBase`'s scroll anchor alive). Each row carries
/// exactly one: repos by id, sections by kind, owners by (kind, login).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum RowIdentity {
    Repo(i64),
    Section(SectionKind),
    Owner { section: SectionKind, login: String },
}

#[derive(Clone)]
pub struct RepoListRenderContext {
    pub repos: Vec<Repo>,
    pub actions_snapshot: HashMap<i64, RepoActionsState>,
    pub workflow_snapshot: HashMap<i64, WorkflowStatusCounts>,
    pub favorites_state: Arc<Mutex<HashSet<i64>>>,
    pub favorites_manager: Option<Arc<FavoritesManager>>,
}

/// Rebuilds the store by diffing it against the current rows instead of
/// clearing and recreating everything.
///
/// Repositories are grouped into the "Actions Enabled" / "Actions Disabled"
/// sections only — by actions state, never by favorite state. Favoriting a
/// repository is a pure indicator (the row's star + `FAVORITE_ROW_KEY`); it
/// must not reorder the list, or the user's scroll position would jump. The
/// "Favorites" pill above the list filters on `FAVORITE_ROW_KEY`, so "show me
/// my favourites" is a filter, not a section.
///
/// Every row — repository, section header, and owner header — is reused by
/// widget identity and refreshed in place when its state changed, so a rebuild
/// that changes nothing leaves the model untouched. This is what keeps the
/// scroll position: `GtkListBase` tracks the user's place via an anchor bound
/// to a row's *widget*, and its item manager re-positions that tracker across
/// model changes as long as the widget survives. Destroying and recreating a
/// row — even a header the anchor happens to sit on — invalidates the anchor
/// and snaps the list back to the top.
pub(crate) fn rebuild_repo_list(store: gio::ListStore, context: RepoListRenderContext) {
    let RepoListRenderContext {
        repos,
        actions_snapshot,
        workflow_snapshot,
        favorites_state,
        favorites_manager,
    } = context;

    let render_start = Instant::now();

    // The rows that survive the rebuild, keyed by identity (repo, section
    // header, owner header). Holding strong references lets `sync_store` move
    // them around without destroying them.
    let existing_rows = snapshot_rows(&store);
    // Favorited state read from the live favorites at rebuild time, not a
    // schedule-time snapshot: a favorite toggled between planning and this
    // idle rebuild must not be reverted. Reading the same set the star handler
    // guards against is what keeps a programmatic `set_active` from re-firing
    // that handler and sending a spurious toggle.
    //
    // The lock is scoped to this statement on purpose: the guard must NOT live
    // across the rebuild, because `refresh_repo_row_in_place` calls
    // `set_active`, which can re-enter the `toggled` handler, and that handler
    // locks this very Mutex — parking_lot is not reentrant, so holding the
    // guard here would deadlock the GTK thread.
    let favorites_now = { favorites_state.lock().clone() };

    let mut enabled_section = Vec::new();
    let mut disabled_section = Vec::new();

    for repo in repos.iter().cloned() {
        let state = actions_snapshot
            .get(&repo.id)
            .copied()
            .unwrap_or(RepoActionsState::Unknown);
        match state {
            RepoActionsState::Disabled => disabled_section.push(repo),
            _ => enabled_section.push(repo),
        }
    }

    let mut target: Vec<gtk::Widget> = Vec::new();

    let append_section = |section: SectionKind, repos: Vec<Repo>, target: &mut Vec<gtk::Widget>| {
        if repos.is_empty() {
            return;
        }

        // Headers carry the aggregated pill flags of the rows beneath them,
        // so the sidebar filter can hide a header once its group is empty
        // instead of leaving a heading with nothing under it.
        let has_active = |repos: &[Repo]| {
            repos.iter().any(|repo| {
                workflow_snapshot
                    .get(&repo.id)
                    .is_some_and(|counts| counts.active > 0)
            })
        };
        let has_favorite =
            |repos: &[Repo]| repos.iter().any(|repo| favorites_now.contains(&repo.id));

        // Reuse the section header by identity so a no-op rebuild leaves the
        // model untouched; only its aggregated pill flags are refreshed. The
        // identity is the section kind, not the translated title, so a
        // language switch does not recreate the row.
        let section_header = match existing_rows.get(&RowIdentity::Section(section)) {
            Some(existing) => {
                set_data(existing, FAVORITE_ROW_KEY, has_favorite(&repos));
                set_data(existing, ACTIVE_RUNS_ROW_KEY, has_active(&repos));
                existing.clone()
            }
            None => {
                let title = section.title();
                let header = create_section_header(title.as_str(), section);
                set_data(&header, FAVORITE_ROW_KEY, has_favorite(&repos));
                set_data(&header, ACTIVE_RUNS_ROW_KEY, has_active(&repos));
                header.upcast()
            }
        };
        target.push(section_header);

        let grouped = group_repos_by_owner(repos);

        for (owner, repos) in grouped {
            // Keyed by (section, owner): the same owner can appear in both the
            // enabled and disabled sections, so the login alone is not unique.
            let owner_row = match existing_rows.get(&RowIdentity::Owner {
                section,
                login: owner.clone(),
            }) {
                Some(existing) => {
                    set_data(existing, FAVORITE_ROW_KEY, has_favorite(&repos));
                    set_data(existing, ACTIVE_RUNS_ROW_KEY, has_active(&repos));
                    existing.clone()
                }
                None => {
                    let owner_row = create_owner_header(&owner, section);
                    set_data(&owner_row, FAVORITE_ROW_KEY, has_favorite(&repos));
                    set_data(&owner_row, ACTIVE_RUNS_ROW_KEY, has_active(&repos));
                    owner_row.upcast()
                }
            };
            target.push(owner_row);

            for repo in repos {
                let repo_id = repo.id;
                let is_favorite = favorites_now.contains(&repo_id);
                let workflow_counts = workflow_snapshot.get(&repo_id).cloned().unwrap_or_default();

                let row = match existing_rows.get(&RowIdentity::Repo(repo_id)) {
                    Some(existing_row) => {
                        refresh_repo_row_in_place(
                            existing_row,
                            &repo,
                            is_favorite,
                            &workflow_counts,
                        );
                        existing_row.clone()
                    }
                    None => build_repo_row(
                        repo.clone(),
                        is_favorite,
                        workflow_counts,
                        favorites_state.clone(),
                        favorites_manager.clone(),
                    )
                    .upcast(),
                };

                target.push(row);
            }
        }
    };

    append_section(SectionKind::Enabled, enabled_section, &mut target);
    append_section(SectionKind::Disabled, disabled_section, &mut target);

    sync_store(&store, &target);

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

/// Pill filter modes shown above the repo list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SidebarFilter {
    #[default]
    All,
    Favorites,
    Active,
}

/// Repo rows only pass a non-`All` pill filter when they carry matching state.
pub fn row_matches_filter(row: &gtk::Widget, query: &str, filter: SidebarFilter) -> bool {
    // The pill test applies to headers too: they carry the aggregated flags of
    // their group, so a section/owner heading disappears together with the rows
    // beneath it instead of being left stranded on its own.
    let passes_pill = match filter {
        SidebarFilter::All => true,
        SidebarFilter::Favorites => get_data_copy(row, FAVORITE_ROW_KEY).unwrap_or(false),
        SidebarFilter::Active => get_data_copy(row, ACTIVE_RUNS_ROW_KEY).unwrap_or(false),
    };
    if !passes_pill {
        return false;
    }

    row_matches_query(row, query)
}
fn create_section_header(title: &str, section: SectionKind) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    row.set_can_focus(false);
    row.set_can_target(false);
    row.add_css_class("section-header");
    row.add_css_class("hoverless-row");
    row.set_margin_top(14);
    row.set_margin_bottom(4);
    row.set_margin_start(10);
    row.set_margin_end(10);
    set_data(&row, SELECTABLE_KEY, false);
    set_data(&row, ACTIVATABLE_KEY, false);
    set_data(&row, ROW_SECTION_KIND_KEY, section.as_kind());

    let (heading, suppress_tracking) = crate::ui::utils::section_heading(title);
    let label = gtk::Label::new(Some(&heading));
    if suppress_tracking {
        label.add_css_class("no-tracking");
    }
    label.set_halign(gtk::Align::Start);
    label.set_hexpand(true);
    label.add_css_class("sidebar-owner-label");

    row.append(&label);

    row
}

fn create_owner_header(owner: &str, section: SectionKind) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    row.set_can_focus(false);
    row.set_can_target(false);
    row.add_css_class("owner-header");
    row.add_css_class("hoverless-row");
    row.set_margin_top(10);
    row.set_margin_bottom(3);
    row.set_margin_start(10);
    row.set_margin_end(10);
    set_data(&row, SELECTABLE_KEY, false);
    set_data(&row, ACTIVATABLE_KEY, false);
    set_data(&row, ROW_SECTION_KIND_KEY, section.as_kind());
    set_data(&row, ROW_OWNER_LOGIN_KEY, owner.to_string());

    let (heading, suppress_tracking) = crate::ui::utils::section_heading(owner);
    let label = gtk::Label::new(Some(&heading));
    if suppress_tracking {
        label.add_css_class("no-tracking");
    }
    label.set_halign(gtk::Align::Start);
    label.add_css_class("sidebar-owner-label");
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

/// Updates a row's meta box in place: the two labels (created once when the
/// row was built) only change text + visibility, so a rebuild touching a row
/// never allocates new widgets. A label is hidden when its count is zero, and
/// GTK collapses a hidden child (and its spacing) out of the layout.
fn update_meta_box(meta_box: &gtk::Box, workflow_counts: &WorkflowStatusCounts) {
    let active = get_data_clone::<gtk::Label, _>(meta_box, REPO_META_ACTIVE_LABEL_KEY);
    let failed = get_data_clone::<gtk::Label, _>(meta_box, REPO_META_FAILED_LABEL_KEY);

    if let Some(active) = active {
        if workflow_counts.active > 0 {
            active.set_visible(true);
            active.set_text(
                tr("Active runs: {count}")
                    .replace("{count}", workflow_counts.active.to_string().as_str())
                    .as_str(),
            );
        } else {
            active.set_visible(false);
        }
    }

    if let Some(failed) = failed {
        if workflow_counts.failed > 0 {
            failed.set_visible(true);
            failed.set_text(
                tr("Failures: {count}")
                    .replace("{count}", workflow_counts.failed.to_string().as_str())
                    .as_str(),
            );
        } else {
            failed.set_visible(false);
        }
    }
}

/// Gather workflow status counts for a repository
pub async fn gather_workflow_status_counts(
    client: &crate::services::gateway::GitHubGateway,
    owner: &str,
    repo: &str,
) -> Result<WorkflowStatusCounts, crate::services::api::GitHubError> {
    use crate::domain::counts::select_latest_runs_for_workflows;
    use crate::domain::runs::{is_run_active, is_run_failure};

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

mod rows;
mod store;
pub(crate) use rows::*;
pub(crate) use store::*;

#[cfg(test)]
mod tests;
