use crate::runtime::channel::MainContextChannelExt;
/// Background task: Observe favorites changes
use crate::services::favorites::FavoritesManager;
use gtk4::glib;
use parking_lot::Mutex;
use std::collections::HashSet;
use std::sync::Arc;

/// Observe favorites manager and update local state
pub fn observe_favorites<F>(
    manager: Arc<FavoritesManager>,
    favorites_state: Arc<Mutex<HashSet<i64>>>,
    on_change: F,
) where
    F: Fn() + 'static,
{
    let mut receiver = manager.subscribe();

    {
        let initial = receiver.borrow().clone();
        let mut favorites = favorites_state.lock();
        *favorites = initial;
    }
    on_change();

    let favorites_state_clone = favorites_state.clone();
    let (sender, receiver_channel) =
        glib::MainContext::default().channel::<HashSet<i64>>(glib::Priority::default());

    receiver_channel.attach(None, move |latest| {
        {
            let mut favorites = favorites_state_clone.lock();
            *favorites = latest;
        }
        on_change();
        glib::ControlFlow::Continue
    });

    crate::runtime::handle().spawn(async move {
        loop {
            if receiver.changed().await.is_err() {
                break;
            }

            if sender.send(receiver.borrow().clone()).is_err() {
                break;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::test_helpers::run_gtk_test;
    use std::cell::Cell;
    use std::rc::Rc;

    #[test]
    #[ignore = "requires GTK display"]
    fn the_observer_seeds_local_state_before_it_waits_for_changes() {
        run_gtk_test("favorites_observer_seeds", || {
            crate::runtime::init_test_runtime();
            let dir = tempfile::tempdir().expect("temp dir");
            let manager = Arc::new(
                FavoritesManager::with_path(dir.path().join("favorites.json"))
                    .expect("favorites manager builds"),
            );
            let state = Arc::new(Mutex::new(HashSet::from([999_i64])));
            let calls = Rc::new(Cell::new(0));
            let counter = calls.clone();

            observe_favorites(manager, state.clone(), move || {
                counter.set(counter.get() + 1);
            });

            // The seed is synchronous: the caller repaints from `on_change`, so
            // a sidebar built before the first watch tick must already show the
            // stored favourites rather than whatever was in the state before.
            assert_eq!(calls.get(), 1, "on_change fires once for the initial seed");
            assert!(
                !state.lock().contains(&999),
                "the pre-existing state is replaced, not merged into"
            );
        });
    }
}
