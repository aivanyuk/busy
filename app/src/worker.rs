//! Latest-value worker threads: each runs one job at a time on the newest value submitted, skipping
//! values superseded before their turn. The config writer is one.

use std::sync::mpsc::{Sender, channel};
use std::thread::JoinHandle;

pub(crate) struct Worker<T: Send + 'static> {
    tx: Option<Sender<T>>,
    thread: Option<JoinHandle<()>>,
}

impl<T: Send + 'static> Worker<T> {
    /// Runs `job` on a dedicated thread for each submitted value, skipping values superseded before
    /// their turn. For the config writer, serialising keeps two saves from racing on the same temp file.
    pub(crate) fn start(name: &str, mut job: impl FnMut(T) + Send + 'static) -> Self {
        let (tx, rx) = channel::<T>();
        let thread = std::thread::Builder::new()
            .name(name.into())
            .spawn(move || {
                while let Ok(v) = rx.recv() {
                    job(rx.try_iter().last().unwrap_or(v));
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

impl<T: Send + 'static> Drop for Worker<T> {
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
    use super::Worker;
    use std::sync::{Arc, Mutex};

    #[test]
    fn runs_in_order_and_flushes_the_latest_on_drop() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let w = {
            let seen = seen.clone();
            Worker::start("test-worker", move |v: u32| seen.lock().unwrap().push(v))
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
