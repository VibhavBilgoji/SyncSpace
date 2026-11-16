//! Token buckets for the per-connection rate limits of `docs/API.md` section 3: document updates
//! 60 per second with a burst of 120, awareness 20 per second with a burst of 40, each in a
//! bucket of its own.

use std::time::{Duration, Instant};

/// How many frames a bucket allows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BucketConfig {
    /// The most frames that can be sent at once after a quiet spell.
    pub burst: u32,
    /// How many frames per second the bucket refills.
    pub per_second: u32,
}

#[derive(Debug)]
pub struct TokenBucket {
    capacity: f64,
    refill_per_second: f64,
    tokens: f64,
    last: Instant,
}

impl TokenBucket {
    pub fn new(config: BucketConfig) -> Self {
        Self::new_at(config, Instant::now())
    }

    pub fn new_at(config: BucketConfig, now: Instant) -> Self {
        Self {
            capacity: f64::from(config.burst),
            refill_per_second: f64::from(config.per_second),
            tokens: f64::from(config.burst),
            last: now,
        }
    }

    /// Takes one token if there is one.
    pub fn try_take(&mut self) -> bool {
        self.try_take_at(Instant::now())
    }

    /// [`try_take`](Self::try_take) with the time passed in, so tests need no sleeping.
    pub fn try_take_at(&mut self, now: Instant) -> bool {
        let elapsed = now.saturating_duration_since(self.last);
        self.last = self.last.max(now);
        self.tokens =
            (self.tokens + elapsed.as_secs_f64() * self.refill_per_second).min(self.capacity);
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

/// How long until a bucket that is empty has one token again (for tests and messages).
pub fn time_to_one_token(config: BucketConfig) -> Duration {
    if config.per_second == 0 {
        Duration::MAX
    } else {
        Duration::from_secs_f64(1.0 / f64::from(config.per_second))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const UPDATES: BucketConfig = BucketConfig {
        burst: 120,
        per_second: 60,
    };

    #[test]
    fn a_full_bucket_allows_its_burst_and_then_refuses() {
        let start = Instant::now();
        let mut bucket = TokenBucket::new_at(UPDATES, start);
        for n in 0..120 {
            assert!(bucket.try_take_at(start), "frame {n} is inside the burst");
        }
        assert!(!bucket.try_take_at(start), "frame 121 is over the burst");
    }

    #[test]
    fn the_bucket_refills_at_the_configured_rate() {
        let start = Instant::now();
        let mut bucket = TokenBucket::new_at(UPDATES, start);
        for _ in 0..120 {
            assert!(bucket.try_take_at(start));
        }
        // After half a second, 30 frames are allowed again, and not 31.
        let later = start + Duration::from_millis(500);
        for n in 0..30 {
            assert!(bucket.try_take_at(later), "refilled frame {n}");
        }
        assert!(!bucket.try_take_at(later));
    }

    #[test]
    fn the_bucket_never_holds_more_than_its_burst() {
        let start = Instant::now();
        let mut bucket = TokenBucket::new_at(
            BucketConfig {
                burst: 5,
                per_second: 100,
            },
            start,
        );
        // A long quiet spell does not store more than the burst.
        let later = start + Duration::from_secs(3600);
        for _ in 0..5 {
            assert!(bucket.try_take_at(later));
        }
        assert!(!bucket.try_take_at(later));
    }

    #[test]
    fn the_two_buckets_are_independent() {
        let start = Instant::now();
        let mut updates = TokenBucket::new_at(UPDATES, start);
        let mut awareness = TokenBucket::new_at(
            BucketConfig {
                burst: 40,
                per_second: 20,
            },
            start,
        );
        for _ in 0..40 {
            assert!(awareness.try_take_at(start));
        }
        assert!(!awareness.try_take_at(start));
        assert!(
            updates.try_take_at(start),
            "awareness does not drain updates"
        );
    }

    #[test]
    fn time_going_backwards_does_not_panic_or_refill() {
        let start = Instant::now();
        let mut bucket = TokenBucket::new_at(
            BucketConfig {
                burst: 1,
                per_second: 1,
            },
            start + Duration::from_secs(10),
        );
        assert!(bucket.try_take_at(start));
        assert!(!bucket.try_take_at(start));
        assert_eq!(time_to_one_token(UPDATES).as_millis(), 16);
    }
}
