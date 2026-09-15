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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::test_helpers::run_gtk_test;

    fn cache(ids: &[i64]) -> Arc<Mutex<HashSet<i64>>> {
        Arc::new(Mutex::new(ids.iter().copied().collect()))
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn a_successful_toggle_updates_both_the_cache_and_the_button() {
        run_gtk_test("favorite_result_ok", || {
            let favorites = cache(&[]);
            let button = gtk::ToggleButton::new();

            apply_favorite_result(&favorites, 7, &button, Ok(true));
            assert!(favorites.lock().contains(&7));
            assert!(button.is_active());

            apply_favorite_result(&favorites, 7, &button, Ok(false));
            assert!(!favorites.lock().contains(&7));
            assert!(!button.is_active());
        });
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn a_failed_toggle_falls_back_to_the_stored_state() {
        run_gtk_test("favorite_result_err", || {
            let favorites = cache(&[7]);
            let button = gtk::ToggleButton::new();
            button.set_active(true);

            // The write failed, so the UI must snap back to what is stored
            // rather than keep the optimistic state.
            apply_favorite_result(
                &favorites,
                7,
                &button,
                Err((anyhow::anyhow!("network down"), false)),
            );

            assert!(
                !favorites.lock().contains(&7),
                "the cache follows the stored state"
            );
            assert!(!button.is_active(), "the button follows the stored state");
        });
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn the_cache_lock_is_released_before_the_button_is_touched() {
        run_gtk_test("favorite_result_no_deadlock", || {
            let favorites = cache(&[]);
            let button = gtk::ToggleButton::new();

            // `set_active` re-enters the star's `toggled` handler, which locks
            // this same non-reentrant mutex to read the live state. If
            // `apply_favorite_result` ever holds its guard across that call the
            // GTK thread wedges — this test is what catches it. The handler
            // below stands in for the real one.
            let handler_saw = Arc::new(Mutex::new(Vec::<bool>::new()));
            let probe = Arc::clone(&favorites);
            let seen = Arc::clone(&handler_saw);
            button.connect_toggled(move |b| {
                // Would deadlock if the caller still held the guard.
                let contains = probe.lock().contains(&7);
                seen.lock().push(contains);
                let _ = b;
            });

            apply_favorite_result(&favorites, 7, &button, Ok(true));

            assert!(button.is_active());
            assert_eq!(
                handler_saw.lock().as_slice(),
                &[true],
                "the handler ran once and could read the already-updated cache"
            );
        });
    }
}
