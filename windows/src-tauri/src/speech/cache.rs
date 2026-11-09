//! A shared model may be evicted only after it has been idle, with no borrowers.
use std::sync::Arc;
use std::time::{Duration, Instant};

pub(super) const IDLE_TIMEOUT: Duration = Duration::from_secs(30);

pub(super) struct Cache<T> {
    value: Option<Arc<T>>,
    last_used: Instant,
}

impl<T> Cache<T> {
    pub(super) fn new() -> Self {
        Self { value: None, last_used: Instant::now() }
    }

    pub(super) fn get(&mut self) -> Option<Arc<T>> {
        self.last_used = Instant::now();
        self.value.clone()
    }

    pub(super) fn set(&mut self, value: Option<Arc<T>>) {
        self.last_used = Instant::now();
        self.value = value;
    }

    pub(super) fn take_idle(&mut self, now: Instant, timeout: Duration, keep_warm: bool) -> Option<Arc<T>> {
        let value = self.value.as_ref()?;
        if keep_warm || Arc::strong_count(value) > 1 {
            // Count idle time from the end of use, including long transcriptions.
            self.last_used = now;
            return None;
        }
        if now.saturating_duration_since(self.last_used) < timeout {
            return None;
        }
        self.value.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TIMEOUT: Duration = IDLE_TIMEOUT;

    #[test]
    fn idle_model_is_released_and_can_be_loaded_again() {
        let mut cache = Cache::new();
        let model = Arc::new(42);
        let weak = Arc::downgrade(&model);
        cache.set(Some(model));
        let start = cache.last_used;
        assert!(cache.take_idle(start + TIMEOUT - Duration::from_secs(1), TIMEOUT, false).is_none());
        drop(cache.take_idle(start + TIMEOUT, TIMEOUT, false));
        assert!(weak.upgrade().is_none());
        assert!(cache.get().is_none());
        cache.set(Some(Arc::new(43)));
        assert_eq!(*cache.get().unwrap(), 43);
    }

    #[test]
    fn long_use_and_wake_listening_keep_model_alive() {
        let mut cache = Cache::new();
        cache.set(Some(Arc::new(42)));
        let borrower = cache.get().unwrap();
        let later = cache.last_used + TIMEOUT * 2;
        assert!(cache.take_idle(later, TIMEOUT, false).is_none());
        drop(borrower);
        assert!(cache.take_idle(later + TIMEOUT - Duration::from_secs(1), TIMEOUT, false).is_none());
        assert!(cache.take_idle(later + TIMEOUT, TIMEOUT, true).is_none());
        assert!(cache.take_idle(later + TIMEOUT * 2, TIMEOUT, false).is_some());
    }

    #[test]
    fn getting_the_model_restarts_the_idle_timeout() {
        let mut cache = Cache::new();
        cache.set(Some(Arc::new(42)));
        // Simulate a model last used more than one timeout ago.
        cache.last_used = Instant::now() - TIMEOUT * 2;
        drop(cache.get());
        let accessed = cache.last_used;
        assert!(cache.take_idle(accessed + TIMEOUT - Duration::from_secs(1), TIMEOUT, false).is_none());
        assert!(cache.take_idle(accessed + TIMEOUT, TIMEOUT, false).is_some());
    }
}
