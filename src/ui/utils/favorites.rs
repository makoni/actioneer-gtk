use gtk4::{self as gtk, prelude::*};
use parking_lot::Mutex;
use std::collections::HashSet;
use std::sync::Arc;
use tracing::warn;

/// Applies a favorite-toggle result to the shared favorite cache and the star
/// button that triggered the write.
///
/// The cache lock is scoped to the match so the guard is dropped *before*
/// `button.set_active`: that call re-enters the star's `toggled` handler, which
/// locks this same non-reentrant `parking_lot::Mutex` to read the live state,
/// so holding the guard across it wedges the GTK thread.
pub fn apply_favorite_result(
    favorites: &Arc<Mutex<HashSet<i64>>>,
    repo_id: i64,
    button: &gtk::ToggleButton,
    result: Result<bool, (anyhow::Error, bool)>,
) {
    let target_state = {
        let mut favorites = favorites.lock();
        let state = match result {
            Ok(is_now_favorite) => is_now_favorite,
            Err((err, stored_state)) => {
                warn!("Failed to update favorite {repo_id}: {err}");
                stored_state
            }
        };
        if state {
            favorites.insert(repo_id);
        } else {
            favorites.remove(&repo_id);
        }
        state
    };

    if button.is_active() != target_state {
        button.set_active(target_state);
    }
}
