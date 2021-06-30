//! Synchronization primitives.

use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicUsize, Ordering};

/// A type which provides exclusive access to a resource by using a ticket lock.
// TODO: https://lwn.net/Articles/590243/
pub struct Mutex<T: ?Sized> {
    now_serving: AtomicUsize,
    next_ticket: AtomicUsize,
    data: UnsafeCell<T>,
}

impl<T> Mutex<T> {
    /// Create a new mutex that holds the given value.
    #[inline]
    pub const fn new(val: T) -> Self {
        Self {
            now_serving: AtomicUsize::new(0),
            next_ticket: AtomicUsize::new(0),
            data: UnsafeCell::new(val),
        }
    }
}

impl<T: ?Sized> Mutex<T> {
    /// Lock this mutex. If the mutex is already locked, spin until it's available.
    #[inline]
    pub fn lock(&self) -> MutexGuard<'_, T> {
        let ticket = self.next_ticket.fetch_add(1, Ordering::Relaxed);

        while self.now_serving.load(Ordering::Acquire) != ticket {
            core::hint::spin_loop();
        }

        MutexGuard {
            now_serving: &self.now_serving,
            data: &self.data,
            ticket,
        }
    }
}

unsafe impl<T: Send> Sync for Mutex<T> {}
unsafe impl<T: Send> Send for Mutex<T> {}

/// The guard providing protected access to the data of a `Mutex`.
pub struct MutexGuard<'lock, T: ?Sized> {
    now_serving: &'lock AtomicUsize,
    data: &'lock UnsafeCell<T>,
    ticket: usize,
}

impl<T: ?Sized> Deref for MutexGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.data.get() }
    }
}

impl<T: ?Sized> DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.data.get() }
    }
}

impl<T: ?Sized> Drop for MutexGuard<'_, T> {
    fn drop(&mut self) {
        self.now_serving.store(self.ticket + 1, Ordering::Release);
    }
}

const READER: usize = 1 << 1;
const WRITER: usize = 1;

/// A lock that provides data access to either one writer or many readers.
pub struct RwLock<T: ?Sized> {
    lock: AtomicUsize,
    data: UnsafeCell<T>,
}

unsafe impl<T: ?Sized + Send> Send for RwLock<T> {}
unsafe impl<T: ?Sized + Send + Sync> Sync for RwLock<T> {}

/// A guard that provides immutable data access.
///
/// When the guard falls out of scope it will decrement the read count,
/// potentially releasing the lock.
pub struct RwLockReadGuard<'a, T: ?Sized> {
    lock: &'a AtomicUsize,
    data: &'a T,
}

/// A guard that provides mutable data access.
///
/// When the guard falls out of scope it will release the lock.
pub struct RwLockWriteGuard<'a, T: ?Sized> {
    inner: &'a RwLock<T>,
    data: &'a mut T,
}

impl<T> RwLock<T> {
    /// Creates a new read-write spinlock wrapping the supplied data.
    #[inline]
    pub const fn new(data: T) -> Self {
        RwLock {
            lock: AtomicUsize::new(0),
            data: UnsafeCell::new(data),
        }
    }
}

impl<T: ?Sized> RwLock<T> {
    /// Locks this rwlock with shared read access, blocking the current thread
    /// until it can be acquired.
    #[inline]
    pub fn read(&self) -> RwLockReadGuard<'_, T> {
        loop {
            let value = self.lock.fetch_add(READER, Ordering::Acquire);

            // check if there's a writer holding this lock
            if value & WRITER != 0 {
                // undo the operation
                self.lock.fetch_sub(READER, Ordering::Release);
                core::hint::spin_loop();
            } else {
                // there's no writer for this lock, so we can safely take it
                return RwLockReadGuard {
                    lock: &self.lock,
                    data: unsafe { &*self.data.get() },
                };
            }
        }
    }

    /// Lock this rwlock with exclusive write access, blocking the current
    /// thread until it can be acquired.
    #[inline]
    pub fn write(&self) -> RwLockWriteGuard<'_, T> {
        loop {
            if self
                .lock
                .compare_exchange(0, WRITER, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
            {
                return RwLockWriteGuard {
                    inner: self,
                    data: unsafe { &mut *self.data.get() },
                };
            } else {
                core::hint::spin_loop();
            }
        }
    }
}

impl<T: ?Sized> Deref for RwLockReadGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.data
    }
}

impl<T: ?Sized> Drop for RwLockReadGuard<'_, T> {
    fn drop(&mut self) {
        // decrement the reader count
        self.lock.fetch_sub(READER, Ordering::Release);
    }
}

impl<T: ?Sized> Deref for RwLockWriteGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.data
    }
}

impl<T: ?Sized> DerefMut for RwLockWriteGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.data
    }
}

impl<T: ?Sized> Drop for RwLockWriteGuard<'_, T> {
    fn drop(&mut self) {
        // clear the writer bit of this rwlock
        self.inner.lock.fetch_and(!WRITER, Ordering::Release);
    }
}
