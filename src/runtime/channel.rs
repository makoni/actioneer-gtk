use futures::StreamExt;
use futures::channel::mpsc::{self, Receiver as BoundedReceiver, Sender as BoundedSender};
use gtk4::glib::{ControlFlow, MainContext, Priority};
use parking_lot::Mutex;
use std::sync::Arc;

const UI_CHANNEL_CAPACITY: usize = 1024;

pub struct Sender<T: Send + 'static>(Arc<Mutex<BoundedSender<T>>>);

pub struct Receiver<T: Send + 'static>(BoundedReceiver<T>);

pub trait MainContextChannelExt {
    fn channel<T: Send + 'static>(&self, priority: Priority) -> (Sender<T>, Receiver<T>);
}

impl MainContextChannelExt for MainContext {
    fn channel<T: Send + 'static>(&self, _priority: Priority) -> (Sender<T>, Receiver<T>) {
        let (tx, rx) = mpsc::channel(UI_CHANNEL_CAPACITY);
        (Sender(Arc::new(Mutex::new(tx))), Receiver(rx))
    }
}

impl<T: Send + 'static> Sender<T> {
    pub fn send(&self, value: T) -> Result<(), mpsc::TrySendError<T>> {
        let mut sender = self.0.lock();
        sender.try_send(value)
    }
}

impl<T: Send + 'static> Clone for Sender<T> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<T: Send + 'static> Receiver<T> {
    pub fn attach<F>(self, _context: Option<&MainContext>, mut func: F)
    where
        F: FnMut(T) -> ControlFlow + 'static,
    {
        let mut receiver = self.0;
        MainContext::default().spawn_local(async move {
            loop {
                let next_item = receiver.next().await;
                let Some(msg) = next_item else {
                    break;
                };

                if matches!(func(msg), ControlFlow::Break) {
                    break;
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_is_bounded() {
        let (sender, _receiver) =
            MainContextChannelExt::channel::<u32>(&MainContext::default(), Priority::default());
        let mut saw_backpressure = false;
        for value in 0..(UI_CHANNEL_CAPACITY * 4) {
            if sender.send(value as u32).is_err() {
                saw_backpressure = true;
                break;
            }
        }
        assert!(saw_backpressure);
    }
}
