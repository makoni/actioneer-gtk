use crate::favorites::FavoritesManager;
use crate::ui::utils::MainContextChannelExt;
use gtk4::prelude::*;
use gtk4::{self as gtk, glib};
use parking_lot::Mutex;
use std::collections::HashSet;
use std::sync::Arc;
use tracing::warn;

pub(super) fn setup_favorite_button(
    button: &gtk::ToggleButton,
    repo_id: i64,
    favorites_manager: Option<Arc<FavoritesManager>>,
    favorites_state: Arc<Mutex<HashSet<i64>>>,
) {
    if let Some(manager) = favorites_manager {
        button.set_sensitive(true);
        button.set_tooltip_text(Some("Toggle favorite"));

        let manager_for_toggle = manager.clone();
        let favorites_state = favorites_state.clone();

        button.connect_toggled(move |button| {
            let is_active = button.is_active();
            update_detail_favorite_button(button, is_active);

            let manager = manager_for_toggle.clone();
            let favorites_state = favorites_state.clone();
            let button_clone = button.clone();
            let (sender, receiver) = glib::MainContext::default()
                .channel::<Result<(), anyhow::Error>>(glib::Priority::default());

            receiver.attach(None, move |result| {
                match result {
                    Ok(()) => {
                        let mut favorites = favorites_state.lock();
                        if is_active {
                            favorites.insert(repo_id);
                        } else {
                            favorites.remove(&repo_id);
                        }
                    }
                    Err(err) => {
                        warn!("Failed to update favorite {}: {}", repo_id, err);
                        let revert_state = !is_active;
                        button_clone.set_active(revert_state);
                        update_detail_favorite_button(&button_clone, revert_state);
                    }
                }

                glib::ControlFlow::Break
            });

            crate::runtime_handle().spawn(async move {
                let outcome = if is_active {
                    manager.add_favorite(repo_id).await
                } else {
                    manager.remove_favorite(repo_id).await
                };

                let _ = sender.send(outcome);
            });
        });
    } else {
        button.set_sensitive(false);
        button.set_tooltip_text(Some("Favorites unavailable"));
        update_detail_favorite_button(button, false);
    }
}

pub(super) fn observe_favorites(
    button: &gtk::ToggleButton,
    repo_id: i64,
    favorites_manager: Option<Arc<FavoritesManager>>,
) {
    if let Some(manager) = favorites_manager {
        let receiver = manager.subscribe();
        let button = button.clone();

        let (sender, receiver_channel) =
            glib::MainContext::default().channel::<bool>(glib::Priority::default());

        receiver_channel.attach(None, move |is_favorite| {
            if button.is_active() != is_favorite {
                button.set_active(is_favorite);
            }
            update_detail_favorite_button(&button, is_favorite);

            glib::ControlFlow::Continue
        });

        crate::runtime_handle().spawn(async move {
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
        });
    } else {
        update_detail_favorite_button(button, false);
        button.set_sensitive(false);
    }
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

    #[test]
    #[ignore = "requires GTK display"]
    fn update_detail_favorite_button_toggles_css_classes() {
        gtk::init().ok();
        let button = gtk::ToggleButton::new();

        update_detail_favorite_button(&button, true);
        assert!(button.has_css_class("suggested-action"));
        assert!(!button.has_css_class("flat"));
        assert_eq!(button.opacity(), 1.0);

        update_detail_favorite_button(&button, false);
        assert!(button.has_css_class("flat"));
        assert!(!button.has_css_class("suggested-action"));
        assert_eq!(button.opacity(), 0.5);
    }
}
