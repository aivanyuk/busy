//! The config writer thread: every save goes through it, one at a time, latest value wins.

use std::sync::mpsc::{Sender, channel};
use std::thread::JoinHandle;

pub(crate) struct Writer<T: Send + 'static> {
    tx: Option<Sender<T>>,
    thread: Option<JoinHandle<()>>,
}

impl<T: Send + 'static> Writer<T> {
    /// Runs `write` on a dedicated thread for each submitted value, skipping values superseded before
    /// their turn. Serialising the writes keeps two saves from racing on the same temp file.
    pub(crate) fn start(name: &str, mut write: impl FnMut(T) + Send + 'static) -> Self {
        let (tx, rx) = channel::<T>();
        let thread = std::thread::Builder::new()
            .name(name.into())
            .spawn(move || {
                while let Ok(v) = rx.recv() {
                    write(rx.try_iter().last().unwrap_or(v));
                }
            })
            .ok();
        Self { tx: thread.is_some().then_some(tx), thread }
    }

    /// Queues `v`; never blocks.
    pub(crate) fn submit(&self, v: T) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(v);
        }
    }
}

impl<T: Send + 'static> Drop for Writer<T> {
    /// Flushes the pending value. Joins: the owner must have destroyed the taskbar widget first.
    fn drop(&mut self) {
        self.tx = None;
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Writer;
    use std::sync::{Arc, Mutex};

    #[test]
    fn writes_in_order_and_flushes_the_latest_on_drop() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let w = {
            let seen = seen.clone();
            Writer::start("test-writer", move |v: u32| seen.lock().unwrap().push(v))
        };
        for v in 1..=100 {
            w.submit(v);
        }
        drop(w);
        let seen = seen.lock().unwrap();
        assert_eq!(seen.last(), Some(&100));
        assert!(seen.windows(2).all(|p| p[0] < p[1]), "{seen:?}");
    }
}
