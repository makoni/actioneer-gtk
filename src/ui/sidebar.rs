use crate::api::models::Repo;
use crate::favorites::FavoritesManager;
use crate::i18n::tr;
use crate::ui::state::{RepoActionsState, WorkflowStatusCounts};
use crate::ui::utils::MainContextChannelExt;
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
enum SectionKind {
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
enum RowIdentity {
    Repo(i64),
    Section(SectionKind),
    Owner { section: SectionKind, login: String },
}

/// Reads a row's identity from the values stashed on it when it was built.
fn read_row_identity(row: &gtk::Widget) -> Option<RowIdentity> {
    if let Some(id) = get_data_copy::<i64, _>(row, REPO_ID_KEY) {
        return Some(RowIdentity::Repo(id));
    }
    let section =
        get_data_copy::<i32, _>(row, ROW_SECTION_KIND_KEY).and_then(SectionKind::from_kind)?;
    match get_data_clone::<String, _>(row, ROW_OWNER_LOGIN_KEY) {
        Some(login) => Some(RowIdentity::Owner { section, login }),
        None => Some(RowIdentity::Section(section)),
    }
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

/// Maps each row's identity to the widget currently in the store, so a rebuild
/// can reuse the same widgets (and therefore the scroll anchor) instead of
/// recreating them.
///
/// Invariant: the identities are unique within a list, which holds because the
/// API returns at most one repository per id. If a repository id ever appeared
/// twice, `target` would hold the same widget twice and `sync_store` would
/// parent it twice — a GTK-critical with no fallback — so the uniqueness is
/// load-bearing, not incidental.
fn snapshot_rows(store: &gio::ListStore) -> HashMap<RowIdentity, gtk::Widget> {
    let mut existing = HashMap::new();
    for i in 0..store.n_items() {
        let Some(item) = store.item(i) else {
            continue;
        };
        let Some(row) = item.downcast_ref::<gtk::Widget>() else {
            continue;
        };
        let Some(identity) = read_row_identity(row) else {
            continue;
        };
        existing.insert(identity, row.clone());
    }
    existing
}

/// Replaces `store`'s contents with `target` with a minimal diff, so rows that
/// survive keep their widget identity and stay parented.
///
/// `GtkListBase`'s scroll anchor is a tracker bound to a row's widget; its
/// item manager re-positions the tracker across model changes while a widget
/// survives, so the list holds its place without any restore. Rows absent from
/// `target` (stale headers, filtered-out repos) are removed and rows absent
/// from the store are inserted; rows present in both are never touched.
fn sync_store(store: &gio::ListStore, target: &[gtk::Widget]) {
    let n = store.n_items() as usize;
    let m = target.len();

    // Fast path for the common case — a no-op rebuild (nothing changed) leaves
    // the model identical, so a pointer walk decides it in O(n) with no
    // allocation. The LCS below is only paid when something actually moved.
    if n == m
        && (0..n).all(|i| {
            let Some(obj) = store.item(i as u32) else {
                return false;
            };
            (obj.as_ptr() as *const ())
                == (target[i].upcast_ref::<glib::Object>().as_ptr() as *const ())
        })
    {
        return;
    }

    let current: Vec<*const ()> = (0..n)
        .filter_map(|i| store.item(i as u32))
        .map(|item| item.as_ptr() as *const ())
        .collect();
    let desired: Vec<*const ()> = target
        .iter()
        .map(|row| row.upcast_ref::<glib::Object>().as_ptr() as *const ())
        .collect();

    // Longest common subsequence by widget identity.
    let mut dp = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = if current[i] == desired[j] {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }

    let mut matched_store = vec![false; n];
    let mut matched_target = vec![false; m];
    let mut i = 0usize;
    let mut j = 0usize;
    while i < n && j < m {
        if current[i] == desired[j] {
            matched_store[i] = true;
            matched_target[j] = true;
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }

    // Remove the rows that are not part of the common subsequence, back to
    // front so the indices stay valid.
    for i in (0..n).rev() {
        if !matched_store[i] {
            store.remove(i as u32);
        }
    }

    // Insert the missing rows in order. After the removals the matched rows
    // already sit at their target indices minus the inserts before them, so
    // position k of `target` is where `target[k]` belongs.
    for (k, row) in target.iter().enumerate() {
        if !matched_target[k] {
            store.insert(k as u32, row);
        }
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

fn find_label_by_name(widget: &gtk::Widget, name: &str) -> Option<gtk::Label> {
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
    workflow_counts: WorkflowStatusCounts,
    favorites_arc: Arc<Mutex<HashSet<i64>>>,
    favorites_manager: Option<Arc<FavoritesManager>>,
) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let repo_id = repo.id;
    row.set_margin_top(8);
    row.set_margin_bottom(8);
    row.set_margin_start(10);
    row.set_margin_end(8);
    row.set_hexpand(true);
    row.set_can_focus(false);
    row.add_css_class("activatable");

    let icon = gtk::Image::from_icon_name("folder-symbolic");
    icon.set_pixel_size(16);
    icon.set_valign(gtk::Align::Center);
    icon.set_halign(gtk::Align::Center);
    icon.add_css_class("dim-label");
    row.append(&icon);

    let favorite_button = gtk::ToggleButton::new();
    favorite_button.add_css_class("flat");
    favorite_button.add_css_class("sidebar-fav");
    favorite_button.set_valign(gtk::Align::Center);
    // Pinned to the trailing edge: packed loosely it trails the name label, so
    // the column of stars zig-zags with the length of each repository's name.
    favorite_button.set_halign(gtk::Align::End);
    favorite_button.set_icon_name(crate::ui::utils::favorite_icon_name());
    crate::ui::utils::describe_control(&favorite_button, tr("Toggle favorite").as_str());
    favorite_button.set_active(is_favorite);
    set_data(&row, REPO_FAV_BUTTON_KEY, favorite_button.clone());

    let favorites_arc_for_update = favorites_arc.clone();
    let favorites_manager_for_update = favorites_manager.clone();

    favorite_button.connect_toggled(move |button| {
        let desired_state = button.is_active();

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
            // A genuine user toggle is in flight until the receiver lands the
            // response; mark it so a concurrent rebuild leaves this star alone
            // instead of snapping it back to the (still stale) persisted state.
            set_data(button, REPO_FAV_PENDING_KEY, true);
            let (sender, receiver) =
                glib::MainContext::default()
                    .channel::<Result<bool, (anyhow::Error, bool)>>(glib::Priority::default());

            receiver.attach(None, move |result| {
                set_data(&button_clone, REPO_FAV_PENDING_KEY, false);
                apply_favorite_result(&favorites_arc_clone, repo_id, &button_clone, result);
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

    let content_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
    content_box.set_valign(gtk::Align::Center);
    // Takes the whole gap between the folder icon and the star, which is what
    // both keeps the star at the edge and lets the name label ellipsize.
    content_box.set_hexpand(true);

    let name_label = gtk::Label::new(Some(&repo.full_name));
    name_label.set_halign(gtk::Align::Start);
    name_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    name_label.add_css_class("heading");
    name_label.set_widget_name("repo-name-label");
    content_box.append(&name_label);
    set_data(&row, REPO_NAME_LABEL_KEY, name_label.clone());

    let visibility_label = create_meta_label(if repo.is_private {
        tr("Private")
    } else {
        tr("Public")
    });
    content_box.append(&visibility_label);
    set_data(&row, REPO_VISIBILITY_LABEL_KEY, visibility_label.clone());

    // The two meta labels are created once and updated in place thereafter, so
    // a rebuild never allocates new widgets for a row (each is hidden when its
    // count is zero).
    let meta_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    meta_box.set_halign(gtk::Align::Start);
    let meta_active = create_meta_label(String::new());
    let meta_failed = create_meta_label(String::new());
    meta_failed.add_css_class("error");
    meta_box.append(&meta_active);
    meta_box.append(&meta_failed);
    set_data(&meta_box, REPO_META_ACTIVE_LABEL_KEY, meta_active.clone());
    set_data(&meta_box, REPO_META_FAILED_LABEL_KEY, meta_failed.clone());
    update_meta_box(&meta_box, &workflow_counts);
    content_box.append(&meta_box);
    set_data(&row, REPO_META_BOX_KEY, meta_box.clone());

    row.append(&content_box);
    row.append(&favorite_button);

    set_data(&row, REPO_ID_KEY, repo_id);
    set_data(&row, REPO_FULL_NAME_KEY, repo.full_name.clone());
    set_data(&row, REPO_MODEL_KEY, repo);
    set_data(&row, SELECTABLE_KEY, true);
    set_data(&row, ACTIVATABLE_KEY, true);
    set_data(&row, FAVORITE_ROW_KEY, is_favorite);
    set_data(&row, ACTIVE_RUNS_ROW_KEY, workflow_counts.active > 0);

    row
}

/// Updates the mutable parts of an existing repository row in place, keeping
/// the row widget's identity (which `GtkListBase`'s scroll anchor tracks). The
/// name label, star, meta box, and visibility label were all stashed on the
/// row when it was built, so nothing here walks the child tree.
fn refresh_repo_row_in_place(
    row: &gtk::Widget,
    repo: &Repo,
    is_favorite: bool,
    workflow_counts: &WorkflowStatusCounts,
) {
    if let Some(name_label) = get_data_clone::<gtk::Label, _>(row, REPO_NAME_LABEL_KEY) {
        name_label.set_text(&repo.full_name);
    }

    if let Some(visibility) = get_data_clone::<gtk::Label, _>(row, REPO_VISIBILITY_LABEL_KEY) {
        let text = if repo.is_private {
            tr("Private")
        } else {
            tr("Public")
        };
        visibility.set_text(text.as_str());
    }

    if let Some(button) = get_data_clone::<gtk::ToggleButton, _>(row, REPO_FAV_BUTTON_KEY) {
        // While a favorite toggle is in flight the star is ahead of the
        // persisted set (the receiver reconciles it when the response lands);
        // forcing it back to `is_favorite` here is what blinks it off. So the
        // star is only synced to the live set when nothing is pending.
        let pending = get_data_copy(&button, REPO_FAV_PENDING_KEY).unwrap_or(false);
        if !pending {
            button.set_active(is_favorite);
        }
    }

    if let Some(meta_box) = get_data_clone::<gtk::Box, _>(row, REPO_META_BOX_KEY) {
        update_meta_box(&meta_box, workflow_counts);
    }

    set_data(row, REPO_MODEL_KEY, repo.clone());
    set_data(row, REPO_FULL_NAME_KEY, repo.full_name.clone());
    set_data(row, FAVORITE_ROW_KEY, is_favorite);
    set_data(row, ACTIVE_RUNS_ROW_KEY, workflow_counts.active > 0);
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

pub(crate) fn repo_id_from_object(obj: &glib::Object) -> Option<i64> {
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
    use crate::ui::test_helpers::run_gtk_test;
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
    fn favourite_buttons_line_up_regardless_of_name_length() {
        run_gtk_test(
            "favourite_buttons_line_up_regardless_of_name_length",
            || {
                fn row_for(full_name: &str) -> gtk::Box {
                    build_repo_row(
                        Repo {
                            id: full_name.len() as i64,
                            name: full_name.rsplit('/').next().unwrap_or(full_name).into(),
                            full_name: full_name.into(),
                            owner: User {
                                login: "makoni".into(),
                            },
                            is_private: false,
                            permissions: None,
                            default_branch: Some("main".into()),
                        },
                        false,
                        WorkflowStatusCounts::default(),
                        Arc::new(Mutex::new(HashSet::new())),
                        None,
                    )
                }

                // A short name and one long enough to be ellipsized: packed without
                // hexpand the star trails the label, so the column zig-zags.
                let rows = [
                    row_for("makoni/imetrik"),
                    row_for("makoni/Google-Maps-SDK-for-something-long"),
                ];
                let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
                for row in &rows {
                    list.append(row);
                }

                let window = gtk::Window::new();
                window.set_default_size(320, 200);
                window.set_child(Some(&list));
                window.present();
                while glib::MainContext::default().pending() {
                    let _ = glib::MainContext::default().iteration(false);
                }

                let right_edges: Vec<i32> = rows
                    .iter()
                    .map(|row| {
                        let button = row
                            .last_child()
                            .and_then(|child| child.downcast::<gtk::ToggleButton>().ok())
                            .expect("each row ends with its favourite button");
                        let bounds = button
                            .compute_bounds(row)
                            .expect("button bounds in row coordinates");
                        (bounds.x() + bounds.width()).round() as i32
                    })
                    .collect();

                window.destroy();

                assert_eq!(
                    right_edges[0], right_edges[1],
                    "favourite buttons must share a trailing edge, got {right_edges:?}"
                );
            },
        );
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn repo_rows_use_widget_metadata_without_listbox_rows() {
        run_gtk_test("repo_rows_use_widget_metadata_without_listbox_rows", || {
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
        });
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn header_rows_are_not_activatable_and_repo_rows_are() {
        run_gtk_test("header_rows_are_not_activatable_and_repo_rows_are", || {
            let store = gio::ListStore::new::<gtk::Widget>();
            let repo = Repo {
                id: 7,
                name: "repo-7".into(),
                full_name: "makoni/repo-7".into(),
                owner: User {
                    login: "makoni".into(),
                },
                is_private: false,
                permissions: None,
                default_branch: Some("main".into()),
            };

            rebuild_repo_list(
                store.clone(),
                RepoListRenderContext {
                    repos: vec![repo],
                    actions_snapshot: HashMap::new(),
                    workflow_snapshot: HashMap::new(),
                    favorites_state: Arc::new(Mutex::new(HashSet::new())),
                    favorites_manager: None,
                },
            );

            // Load-bearing for the hover: the factory turns this flag into
            // `set_activatable`, and GTK turns that into the `.activatable`
            // class on the list row — which is what the hover rule in
            // `style.rs` selects. Headers must stay out of it, or they light up
            // under the pointer as if they were clickable.
            let mut saw_repo = false;
            let mut saw_header = false;
            for i in 0..store.n_items() {
                let row = store.item(i).expect("store item");
                let is_repo = repo_id_from_object(&row).is_some();
                assert_eq!(
                    row_activatable_from_object(&row),
                    is_repo,
                    "row {i} (is_repo={is_repo}): only repo rows may be activatable"
                );
                saw_repo |= is_repo;
                saw_header |= !is_repo;
            }
            assert!(saw_repo, "expected at least one repo row");
            assert!(saw_header, "expected at least one section/owner header");
        });
    }

    #[test]
    // NOTE: on regression this test does NOT fail fast — the wedged worker hits
    // `GTK_TEST_TIMEOUT` (30 s), is marked wedged, and every GTK test queued after
    // it fails too, so the whole UI suite looks collapsed. A red `favorite_*` test
    // here means the favorites path broke; start the hunt here, not in the cascade.
    #[ignore = "requires GTK display"]
    fn favorite_err_result_applied_to_button_does_not_deadlock() {
        run_gtk_test(
            "favorite_err_result_applied_to_button_does_not_deadlock",
            || {
                let favorites = Arc::new(Mutex::new(HashSet::new()));
                let row = build_repo_row(
                    Repo {
                        id: 42,
                        name: "repo-42".into(),
                        full_name: "makoni/repo-42".into(),
                        owner: User {
                            login: "makoni".into(),
                        },
                        is_private: false,
                        permissions: None,
                        default_branch: Some("main".into()),
                    },
                    false,
                    WorkflowStatusCounts::default(),
                    favorites.clone(),
                    None,
                );
                let button = get_data_clone::<gtk::ToggleButton, _>(&row, REPO_FAV_BUTTON_KEY)
                    .expect("the favorite button is stashed on the row");

                // A user click flips the star on, so the button is now active.
                button.set_active(true);

                // The favorite write then failed, so the stored (pre-click) state is
                // `false` and diverges from the active star. Applying that result must
                // not wedge the GTK thread: pre-fix the receiver held the favorites
                // lock across `set_active`, which re-entered `toggled` and locked the
                // same non-reentrant Mutex.
                apply_favorite_result(
                    &favorites,
                    42,
                    &button,
                    Err((anyhow::anyhow!("simulated disk full"), false)),
                );

                // The star is restored to the stored state and the cache agrees.
                assert!(!button.is_active());
                assert!(!favorites.lock().contains(&42));
            },
        );
    }
}
