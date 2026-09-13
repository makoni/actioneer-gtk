/// Background task: Observe favorites changes
use crate::favorites::FavoritesManager;
use crate::runtime::channel::MainContextChannelExt;
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
