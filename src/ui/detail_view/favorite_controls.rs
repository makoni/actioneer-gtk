use crate::favorites::FavoritesManager;
use crate::i18n::tr;
use crate::ui::utils::MainContextChannelExt;
use crate::ui::utils::apply_favorite_result;
use gtk4::prelude::*;
use gtk4::{self as gtk, glib};
use parking_lot::Mutex;
use std::collections::HashSet;
use std::sync::Arc;

pub(super) fn setup_favorite_button(
    button: &gtk::ToggleButton,
    repo_id: i64,
    favorites_manager: Option<Arc<FavoritesManager>>,
    favorites_state: Arc<Mutex<HashSet<i64>>>,
) {
    if let Some(manager) = favorites_manager {
        button.set_sensitive(true);
        button.set_tooltip_text(Some(tr("Toggle favorite").as_str()));

        let manager_for_toggle = manager.clone();
        let favorites_state = favorites_state.clone();

        button.connect_toggled(move |button| {
            let is_active = button.is_active();

            let favorites_state = favorites_state.clone();
            let previous_state = {
                let favorites = favorites_state.lock();
                favorites.contains(&repo_id)
            };

            update_detail_favorite_button(button, is_active);

            // The error-path revert below calls `set_active`, which re-enters this
            // handler. Once the live cache matches the button's state the toggle
            // is done, so bail out instead of spawning another write — a failed
            // write used to chain into a runaway loop of failed writes.
            if previous_state == is_active {
                return;
            }

            let button_clone = button.clone();
            let (sender, receiver) =
                glib::MainContext::default()
                    .channel::<Result<bool, (anyhow::Error, bool)>>(glib::Priority::default());

            receiver.attach(None, move |result| {
                apply_favorite_result(&favorites_state, repo_id, &button_clone, result);
                glib::ControlFlow::Break
            });

            let manager_for_task = manager_for_toggle.clone();
            crate::runtime_handle().spawn(async move {
                let outcome = match if is_active {
                    manager_for_task.add_favorite(repo_id).await
                } else {
                    manager_for_task.remove_favorite(repo_id).await
                } {
                    Ok(next_state) => Ok(next_state),
                    Err(err) => {
                        let current_state = manager_for_task.is_favorite(repo_id).await;
                        Err((err, current_state))
                    }
                };

                let _ = sender.send(outcome);
            });
        });
    } else {
        button.set_sensitive(false);
        button.set_tooltip_text(Some(tr("Favorites unavailable").as_str()));
        update_detail_favorite_button(button, false);
    }
}

/// Mirrors the favourite state onto `button` until the returned handle is
/// aborted. Without aborting it, a replaced pane's observer sleeps on the
/// broadcast channel until the next favourite change anywhere in the app.
pub(super) fn observe_favorites(
    button: &gtk::ToggleButton,
    repo_id: i64,
    favorites_manager: Option<Arc<FavoritesManager>>,
) -> Option<tokio::task::JoinHandle<()>> {
    let Some(manager) = favorites_manager else {
        update_detail_favorite_button(button, false);
        button.set_sensitive(false);
        return None;
    };

    let receiver = manager.subscribe();
    let button_weak = button.downgrade();

    let (sender, receiver_channel) =
        glib::MainContext::default().channel::<bool>(glib::Priority::default());

    receiver_channel.attach(None, move |is_favorite| {
        let Some(button) = button_weak.upgrade() else {
            return glib::ControlFlow::Break;
        };
        if button.is_active() != is_favorite {
            button.set_active(is_favorite);
        }
        update_detail_favorite_button(&button, is_favorite);

        glib::ControlFlow::Continue
    });

    Some(crate::runtime_handle().spawn(async move {
        let mut receiver_local = receiver;

        if sender
            .send(receiver_local.borrow().contains(&repo_id))
            .is_err()
        {
            return;
        }

        loop {
            if receiver_local.changed().await.is_err() {
                break;
            }

            let is_favorite = receiver_local.borrow().contains(&repo_id);
            if sender.send(is_favorite).is_err() {
                break;
            }
        }
    }))
}

fn update_detail_favorite_button(button: &gtk::ToggleButton, is_active: bool) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::test_helpers::{pump_frames, run_gtk_test};

    #[test]
    #[ignore = "requires GTK display"]
    fn observing_favorites_keeps_the_button_sensitive() {
        run_gtk_test("observing_favorites_keeps_the_button_sensitive", || {
            crate::init_test_runtime();
            let manager = FavoritesManager::new().expect("favorites manager builds");
            let button = gtk::ToggleButton::new();
            button.set_sensitive(true);

            let observer = observe_favorites(&button, i64::MAX, Some(Arc::new(manager)));

            assert!(
                button.is_sensitive(),
                "observing must not disable the favorite button"
            );

            if let Some(observer) = observer {
                observer.abort();
            }
        });
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn update_detail_favorite_button_toggles_css_classes() {
        run_gtk_test("update_detail_favorite_button_toggles_css_classes", || {
            let button = gtk::ToggleButton::new();

            update_detail_favorite_button(&button, true);
            assert!(button.has_css_class("suggested-action"));
            assert!(!button.has_css_class("flat"));
            assert_eq!(button.opacity(), 1.0);

            update_detail_favorite_button(&button, false);
            assert!(button.has_css_class("flat"));
            assert!(!button.has_css_class("suggested-action"));
            assert!((button.opacity() - 0.5).abs() < 0.01);
        });
    }

    #[test]
    #[ignore = "requires GTK display"]
    fn revert_set_active_does_not_spawn_a_favorite_write() {
        run_gtk_test("revert_set_active_does_not_spawn_a_favorite_write", || {
            crate::init_test_runtime();
            let manager = FavoritesManager::new().expect("favorites manager builds");
            let updates = manager.subscribe();
            let repo_id = i64::MAX;
            let favorites = Arc::new(Mutex::new(HashSet::new()));
            let button = gtk::ToggleButton::new();
            // An "on" star over an empty cache is the pending-write moment: the
            // user just asked to favorite it and the write is still in flight.
            button.set_active(true);

            setup_favorite_button(&button, repo_id, Some(Arc::new(manager)), favorites.clone());

            // The error path reverts the star to its stored state with a plain
            // `set_active`, which re-enters `toggled`. The live cache already
            // matches the star, so the handler must bail — not spawn the next
            // write (the runaway loop) or wedge the GTK thread.
            button.set_active(false);
            pump_frames();

            assert!(!button.is_active(), "the reverted star stays off");
            assert!(
                !favorites.lock().contains(&repo_id),
                "the cache is left clean"
            );
            assert!(
                !updates.has_changed().unwrap_or(false),
                "the re-entrant revert must not spawn another favorite write"
            );
        });
    }
}
