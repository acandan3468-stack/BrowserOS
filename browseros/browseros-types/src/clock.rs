use chrono::{DateTime, TimeDelta, Utc};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Abstract time provider.
///
/// Every time-dependent component receives this through `RuntimeContext`
/// instead of calling `std::time` or `chrono` directly, enabling deterministic
/// testing with [`MockClock`].
pub trait Clock: Send + Sync {
    /// Returns the current wall-clock time.
    fn now(&self) -> DateTime<Utc>;
    /// Returns the duration elapsed since `earlier`.
    fn elapsed(&self, earlier: &DateTime<Utc>) -> Duration;
}

// ---------------------------------------------------------------------------
// SystemClock
// ---------------------------------------------------------------------------

/// Real system clock backed by [`Utc::now`].
#[derive(Debug, Clone)]
pub struct SystemClock;

impl SystemClock {
    pub fn new() -> Self {
        Self
    }
}

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }

    fn elapsed(&self, earlier: &DateTime<Utc>) -> Duration {
        let now = self.now();
        (now - *earlier).to_std().unwrap_or(Duration::ZERO)
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// MockClock
// ---------------------------------------------------------------------------

/// Mock clock for tests.
///
/// Returns a configurable fixed or programmatically-advanced time.  Cloneable
/// so it can be shared across components under test; all clones share the
/// same underlying time state.
#[derive(Debug, Clone)]
pub struct MockClock {
    current: Arc<Mutex<DateTime<Utc>>>,
}

impl MockClock {
    /// Creates a mock clock frozen at `fixed_time`.
    pub fn new(fixed_time: DateTime<Utc>) -> Self {
        MockClock {
            current: Arc::new(Mutex::new(fixed_time)),
        }
    }

    /// Advances the stored time by `dur`.
    pub fn advance(&self, dur: Duration) {
        let mut guard = self.current.lock().unwrap();
        *guard += TimeDelta::from_std(dur).unwrap_or(TimeDelta::zero());
    }

    /// Sets the stored time to an exact value.
    pub fn set_time(&self, dt: DateTime<Utc>) {
        let mut guard = self.current.lock().unwrap();
        *guard = dt;
    }
}

impl Clock for MockClock {
    fn now(&self) -> DateTime<Utc> {
        *self.current.lock().unwrap()
    }

    fn elapsed(&self, earlier: &DateTime<Utc>) -> Duration {
        let now = self.now();
        (now - *earlier).to_std().unwrap_or(Duration::ZERO)
    }
}

impl Default for MockClock {
    fn default() -> Self {
        Self::new(DateTime::from_timestamp_nanos(0))
    }
}

// ---------------------------------------------------------------------------
// CancellationToken
// ---------------------------------------------------------------------------

/// A lightweight cancellation flag.
///
/// Minimal, allocation-free, and tokio-free — safe for use in this crate and
/// compatible with the "no async, no I/O" invariant.
#[derive(Debug, Clone)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    /// Creates a new token in the non-cancelled state.
    pub fn new() -> Self {
        CancellationToken {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Signals cancellation.
    ///
    /// Once set, all clones of this token will return `true` from
    /// [`is_cancelled`](CancellationToken::is_cancelled).
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    /// Returns `true` if [`cancel`](CancellationToken::cancel) has been called.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// A cancelled error value, produced by [`CancelledExt`].
#[derive(Debug, Clone)]
pub struct Cancelled;

impl std::fmt::Display for Cancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("operation cancelled")
    }
}

impl std::error::Error for Cancelled {}

/// Extension trait for checking a [`CancellationToken`] on a type.
pub trait CancelledExt {
    type Ok;
    /// Returns `Err(Cancelled)` when the token has been signalled, otherwise
    /// passes through the value.
    fn check(self, token: &CancellationToken) -> Result<Self::Ok, Cancelled>;
}

impl<T> CancelledExt for Option<T> {
    type Ok = T;

    fn check(self, token: &CancellationToken) -> Result<T, Cancelled> {
        if token.is_cancelled() {
            return Err(Cancelled);
        }
        self.ok_or(Cancelled)
    }
}

impl<T, E> CancelledExt for Result<T, E> {
    type Ok = T;

    fn check(self, token: &CancellationToken) -> Result<T, Cancelled> {
        if token.is_cancelled() {
            return Err(Cancelled);
        }
        self.map_err(|_| Cancelled)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    // ─── SystemClock ──────────────────────────────────────────────────────

    #[test]
    fn system_clock_now() {
        let clock = SystemClock::new();
        let now = clock.now();
        let system_now = Utc::now();
        let diff = system_now - now;
        assert!(
            diff.num_seconds() < 5,
            "clock time should be close to system time"
        );
    }

    #[test]
    fn system_clock_elapsed() {
        let clock = SystemClock::new();
        let earlier = Utc::now() - chrono::TimeDelta::seconds(2);
        let elapsed = clock.elapsed(&earlier);
        assert!(elapsed >= std::time::Duration::from_secs(1));
    }

    #[test]
    fn system_clock_default() {
        let clock = SystemClock;
        let now = clock.now();
        let system_now = Utc::now();
        let diff = system_now - now;
        assert!(diff.num_seconds().abs() < 5);
    }

    #[test]
    fn system_clock_clone() {
        let clock = SystemClock::new();
        let cloned = clock.clone();
        assert!((cloned.now() - clock.now()).num_seconds() < 2);
    }

    // ─── MockClock ────────────────────────────────────────────────────────

    #[test]
    fn mock_clock_fixed_time() {
        let fixed = Utc::now();
        let clock = MockClock::new(fixed);
        assert_eq!(clock.now(), fixed);
    }

    #[test]
    fn mock_clock_advance() {
        let fixed = Utc::now();
        let clock = MockClock::new(fixed);

        clock.advance(std::time::Duration::from_secs(10));
        let expected = fixed + chrono::TimeDelta::seconds(10);
        assert_eq!(clock.now(), expected);
    }

    #[test]
    fn mock_clock_advance_multiple() {
        let fixed = Utc::now();
        let clock = MockClock::new(fixed);

        clock.advance(std::time::Duration::from_secs(5));
        clock.advance(std::time::Duration::from_secs(15));
        let expected = fixed + chrono::TimeDelta::seconds(20);
        assert_eq!(clock.now(), expected);
    }

    #[test]
    fn mock_clock_set_time() {
        let clock = MockClock::new(Utc::now());
        let new_time = Utc::now() + chrono::TimeDelta::hours(1);
        clock.set_time(new_time);
        assert_eq!(clock.now(), new_time);
    }

    #[test]
    fn mock_clock_elapsed() {
        let fixed = Utc::now();
        let clock = MockClock::new(fixed);

        let earlier = fixed - chrono::TimeDelta::seconds(30);
        // elapsed before advancing
        let elapsed = clock.elapsed(&earlier);
        assert_eq!(elapsed, std::time::Duration::from_secs(30));

        // elapsed after advancing
        clock.advance(std::time::Duration::from_secs(10));
        let elapsed = clock.elapsed(&earlier);
        assert_eq!(elapsed, std::time::Duration::from_secs(40));
    }

    #[test]
    fn mock_clock_default() {
        let clock = MockClock::default();
        assert_eq!(clock.now(), DateTime::from_timestamp_nanos(0));
    }

    #[test]
    fn mock_clock_clone_shares_state() {
        let fixed = Utc::now();
        let clock1 = MockClock::new(fixed);
        let clock2 = clock1.clone();

        clock1.advance(std::time::Duration::from_secs(42));
        // clock2 should see the same advanced time
        assert_eq!(clock2.now(), fixed + chrono::TimeDelta::seconds(42));
    }

    #[test]
    fn mock_clock_set_time_via_clone() {
        let clock1 = MockClock::new(Utc::now());
        let clock2 = clock1.clone();

        let new_time = Utc::now() + chrono::TimeDelta::days(1);
        clock2.set_time(new_time);

        // both clones see the new time
        assert_eq!(clock1.now(), new_time);
        assert_eq!(clock2.now(), new_time);
    }

    #[test]
    fn mock_clock_concurrent_access() {
        use std::sync::Arc;
        use std::thread;

        let clock = Arc::new(MockClock::new(Utc::now()));

        let mut handles = vec![];
        for i in 0..10 {
            let c = Arc::clone(&clock);
            handles.push(thread::spawn(move || {
                c.advance(std::time::Duration::from_millis(i));
                let _now = c.now();
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        // no panic = success
    }

    // ─── CancellationToken ────────────────────────────────────────────────

    #[test]
    fn cancellation_token_new_not_cancelled() {
        let token = CancellationToken::new();
        assert!(!token.is_cancelled());
    }

    #[test]
    fn cancellation_token_cancel() {
        let token = CancellationToken::new();
        token.cancel();
        assert!(token.is_cancelled());
    }

    #[test]
    fn cancellation_token_clone_shares_state() {
        let token1 = CancellationToken::new();
        let token2 = token1.clone();

        assert!(!token2.is_cancelled());

        token1.cancel();
        assert!(token2.is_cancelled());
        assert!(token1.is_cancelled());
    }

    #[test]
    fn cancellation_token_cancel_via_clone() {
        let token1 = CancellationToken::new();
        let token2 = token1.clone();

        token2.cancel();
        assert!(token1.is_cancelled());
    }

    #[test]
    fn cancellation_token_default_not_cancelled() {
        let token = CancellationToken::default();
        assert!(!token.is_cancelled());
    }

    #[test]
    fn cancellation_token_multiple_cancel_idempotent() {
        let token = CancellationToken::new();
        token.cancel();
        token.cancel();
        token.cancel();
        assert!(token.is_cancelled());
    }

    // ─── Cancelled ────────────────────────────────────────────────────────

    #[test]
    fn cancelled_display() {
        let c = Cancelled;
        assert_eq!(c.to_string(), "operation cancelled");
    }

    #[test]
    fn cancelled_error_trait() {
        let c = Cancelled;
        let err: &dyn std::error::Error = &c;
        assert_eq!(err.to_string(), "operation cancelled");
    }

    // ─── CancelledExt for Option ──────────────────────────────────────────

    #[test]
    fn cancelled_ext_option_some_not_cancelled() {
        let token = CancellationToken::new();
        let result: Result<i32, Cancelled> = Some(42).check(&token);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn cancelled_ext_option_none_not_cancelled() {
        let token = CancellationToken::new();
        let result: Result<i32, Cancelled> = None::<i32>.check(&token);
        assert!(result.is_err());
    }

    #[test]
    fn cancelled_ext_option_some_cancelled() {
        let token = CancellationToken::new();
        token.cancel();
        let result: Result<i32, Cancelled> = Some(42).check(&token);
        assert!(result.is_err());
    }

    #[test]
    fn cancelled_ext_option_none_cancelled() {
        let token = CancellationToken::new();
        token.cancel();
        let result: Result<i32, Cancelled> = None::<i32>.check(&token);
        assert!(result.is_err());
    }

    // ─── CancelledExt for Result ──────────────────────────────────────────

    #[test]
    fn cancelled_ext_result_ok_not_cancelled() {
        let token = CancellationToken::new();
        let result: Result<i32, Cancelled> = Ok::<i32, ()>(42).check(&token);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn cancelled_ext_result_err_not_cancelled() {
        let token = CancellationToken::new();
        let result: Result<i32, Cancelled> = Err::<i32, _>("fail").check(&token);
        assert!(result.is_err());
    }

    #[test]
    fn cancelled_ext_result_ok_cancelled() {
        let token = CancellationToken::new();
        token.cancel();
        let result: Result<i32, Cancelled> = Ok::<i32, ()>(42).check(&token);
        assert!(result.is_err());
    }

    #[test]
    fn cancelled_ext_result_err_cancelled() {
        let token = CancellationToken::new();
        token.cancel();
        let result: Result<i32, Cancelled> = Err::<i32, _>("fail").check(&token);
        assert!(result.is_err());
    }
}
