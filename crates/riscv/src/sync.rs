//! Synchronization primitives.

use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicUsize, Ordering};

/// A type which provides exclusive access to a resource by using a ticket lock.
// TODO: https://lwn.net/Articles/590243/
pub struct Mutex<T> {
    now_serving: AtomicUsize,
    next_ticket: AtomicUsize,
    data: UnsafeCell<T>,
}

impl<T> Mutex<T> {
    /// Create a new mutex that holds the given value.
    pub const fn new(val: T) -> Self {
        Self {
            now_serving: AtomicUsize::new(0),
            next_ticket: AtomicUsize::new(0),
            data: UnsafeCell::new(val),
        }
    }

    /// Lock this mutex. If the mutex is already locked, spin until it's available.
    pub fn lock(&self) -> MutexGuard<'_, T> {
        let ticket = self.next_ticket.fetch_add(1, Ordering::Relaxed);

        while self.now_serving.load(Ordering::Acquire) != ticket {
            core::hint::spin_loop();
        }

        MutexGuard { lock: self }
    }
}

unsafe impl<T: Send> Sync for Mutex<T> {}
unsafe impl<T: Send> Send for Mutex<T> {}

/// The guard providing protected access to the data of a `Mutex`.
pub struct MutexGuard<'lock, T> {
    lock: &'lock Mutex<T>,
}

impl<T> Deref for MutexGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.data.get() }
    }
}

impl<T> DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<T> Drop for MutexGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.now_serving.fetch_add(1, Ordering::Release);
    }
}
