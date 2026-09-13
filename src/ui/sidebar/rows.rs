//! Building and reading a single repository row.
//!
//! Split out of `sidebar.rs`, which was 869 lines: the row is a leaf widget and
//! its accessors are read back from GObject data, neither of which has anything
//! to do with how the list is assembled.

use super::*;

/// Reads a row's identity from the values stashed on it when it was built.
pub(crate) fn read_row_identity(row: &gtk::Widget) -> Option<RowIdentity> {
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

pub(crate) fn find_label_by_name(widget: &gtk::Widget, name: &str) -> Option<gtk::Label> {
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

pub(crate) fn build_repo_row(
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
            crate::runtime::handle().spawn(async move {
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
pub(crate) fn refresh_repo_row_in_place(
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

pub(crate) fn repo_from_row(row: &gtk::Widget) -> Option<Repo> {
    get_data_clone(row, REPO_MODEL_KEY)
}

pub(crate) fn repo_from_object(obj: &glib::Object) -> Option<Repo> {
    obj.downcast_ref::<gtk::Widget>().and_then(repo_from_row)
}

pub(crate) fn repo_id_from_row(row: &gtk::Widget) -> Option<i64> {
    get_data_copy(row, REPO_ID_KEY)
}

pub(crate) fn repo_id_from_object(obj: &glib::Object) -> Option<i64> {
    obj.downcast_ref::<gtk::Widget>().and_then(repo_id_from_row)
}

pub(crate) fn row_selectable_from_object(obj: &glib::Object) -> bool {
    obj.downcast_ref::<gtk::Widget>()
        .and_then(|row| get_data_copy(row, SELECTABLE_KEY))
        .unwrap_or(false)
}

pub(crate) fn row_activatable_from_object(obj: &glib::Object) -> bool {
    obj.downcast_ref::<gtk::Widget>()
        .and_then(|row| get_data_copy(row, ACTIVATABLE_KEY))
        .unwrap_or(false)
}
