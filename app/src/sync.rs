//! Locking shared by the router and the worker threads.

use std::sync::{Mutex, MutexGuard, PoisonError};

/// Locks `m`, ignoring poisoning. Release builds abort on panic (`panic = "abort"`), so poisoning happens
/// only in debug builds after a thread already died; the data is still whole, since every critical
/// section is a plain move or copy.
pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}
