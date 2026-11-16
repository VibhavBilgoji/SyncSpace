//! Counters for the measurements of the project (`docs/ARCHITECTURE.md` section 2, `docs/TESTING.md`
//! "load"): how many edits the server received, how many snapshot writes it made to MongoDB and
//! how long it took to apply an update and hand it to the other connections. The ratio of the
//! first two over time is the "edits per second versus writes per second" number.
//!
//! They are plain atomics, read at `GET /metrics` in the Prometheus text format. The page carries
//! counts only: no document ids, titles, user ids or any other data.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// Upper bounds of the broadcast latency buckets, in milliseconds.
const BOUNDS_MS: [u64; 8] = [1, 2, 5, 10, 25, 50, 100, 250];

#[derive(Default)]
pub struct Metrics {
    edits: AtomicU64,
    snapshot_writes: AtomicU64,
    broadcast_count: AtomicU64,
    broadcast_sum_micros: AtomicU64,
    /// One counter per bound, plus one for everything above the last bound. Not cumulative.
    broadcast_buckets: [AtomicU64; BOUNDS_MS.len() + 1],
}

impl Metrics {
    /// An update was applied to a document.
    pub fn edit(&self) {
        self.edits.fetch_add(1, Ordering::Relaxed);
    }

    /// A snapshot was written to MongoDB.
    pub fn snapshot_write(&self) {
        self.snapshot_writes.fetch_add(1, Ordering::Relaxed);
    }

    /// How long an update took from arriving to being handed to the other connections.
    pub fn observe_broadcast(&self, elapsed: Duration) {
        let micros = u64::try_from(elapsed.as_micros()).unwrap_or(u64::MAX);
        self.broadcast_count.fetch_add(1, Ordering::Relaxed);
        self.broadcast_sum_micros
            .fetch_add(micros, Ordering::Relaxed);
        let millis = micros / 1000;
        let index = BOUNDS_MS
            .iter()
            .position(|bound| millis < *bound || (millis == *bound && micros % 1000 == 0))
            .unwrap_or(BOUNDS_MS.len());
        self.broadcast_buckets[index].fetch_add(1, Ordering::Relaxed);
    }

    pub fn edits(&self) -> u64 {
        self.edits.load(Ordering::Relaxed)
    }

    pub fn snapshot_writes(&self) -> u64 {
        self.snapshot_writes.load(Ordering::Relaxed)
    }

    /// The Prometheus text format. `open_rooms` and `open_sockets` are read by the caller.
    pub fn render(&self, open_rooms: usize, open_sockets: u64) -> String {
        let mut out = String::new();
        let mut line = |text: String| {
            out.push_str(&text);
            out.push('\n');
        };
        line("# HELP syncspace_edits_total Updates applied to documents.".into());
        line("# TYPE syncspace_edits_total counter".into());
        line(format!("syncspace_edits_total {}", self.edits()));
        line("# HELP syncspace_mongo_snapshot_writes_total Snapshot writes to MongoDB.".into());
        line("# TYPE syncspace_mongo_snapshot_writes_total counter".into());
        line(format!(
            "syncspace_mongo_snapshot_writes_total {}",
            self.snapshot_writes()
        ));
        line("# TYPE syncspace_open_rooms gauge".into());
        line(format!("syncspace_open_rooms {open_rooms}"));
        line("# TYPE syncspace_open_sockets gauge".into());
        line(format!("syncspace_open_sockets {open_sockets}"));

        line("# HELP syncspace_broadcast_seconds Time to apply an update and hand it to the other connections.".into());
        line("# TYPE syncspace_broadcast_seconds histogram".into());
        let mut cumulative = 0;
        for (bound, bucket) in BOUNDS_MS.iter().zip(&self.broadcast_buckets) {
            cumulative += bucket.load(Ordering::Relaxed);
            line(format!(
                "syncspace_broadcast_seconds_bucket{{le=\"{:.3}\"}} {cumulative}",
                *bound as f64 / 1000.0
            ));
        }
        let count = self.broadcast_count.load(Ordering::Relaxed);
        line(format!(
            "syncspace_broadcast_seconds_bucket{{le=\"+Inf\"}} {count}"
        ));
        let sum = self.broadcast_sum_micros.load(Ordering::Relaxed) as f64 / 1_000_000.0;
        line(format!("syncspace_broadcast_seconds_sum {sum:.6}"));
        line(format!("syncspace_broadcast_seconds_count {count}"));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(text: &str, name: &str) -> f64 {
        text.lines()
            .find(|l| l.starts_with(name) && l[name.len()..].starts_with(' '))
            .unwrap_or_else(|| panic!("{name} missing in\n{text}"))
            .rsplit(' ')
            .next()
            .unwrap()
            .parse()
            .unwrap()
    }

    #[test]
    fn counters_count_and_render() {
        let m = Metrics::default();
        for _ in 0..7 {
            m.edit();
        }
        m.snapshot_write();
        let text = m.render(2, 5);
        assert_eq!(value(&text, "syncspace_edits_total"), 7.0);
        assert_eq!(value(&text, "syncspace_mongo_snapshot_writes_total"), 1.0);
        assert_eq!(value(&text, "syncspace_open_rooms"), 2.0);
        assert_eq!(value(&text, "syncspace_open_sockets"), 5.0);
    }

    #[test]
    fn the_latency_histogram_is_cumulative_and_adds_up() {
        let m = Metrics::default();
        m.observe_broadcast(Duration::from_micros(300)); // under 1 ms
        m.observe_broadcast(Duration::from_millis(3)); // under 5 ms
        m.observe_broadcast(Duration::from_millis(40)); // under 50 ms
        m.observe_broadcast(Duration::from_secs(2)); // above every bound
        let text = m.render(0, 0);
        let bucket = |le: &str| {
            value(
                &text,
                &format!("syncspace_broadcast_seconds_bucket{{le=\"{le}\"}}"),
            )
        };
        assert_eq!(bucket("0.001"), 1.0);
        assert_eq!(bucket("0.005"), 2.0);
        assert_eq!(bucket("0.050"), 3.0);
        assert_eq!(bucket("0.250"), 3.0);
        assert_eq!(bucket("+Inf"), 4.0);
        assert_eq!(value(&text, "syncspace_broadcast_seconds_count"), 4.0);
        assert!((value(&text, "syncspace_broadcast_seconds_sum") - 2.04330).abs() < 0.001);
    }

    #[test]
    fn the_page_carries_no_identifiers() {
        let text = Metrics::default().render(1, 1);
        for line in text.lines() {
            assert!(
                line.starts_with('#') || line.starts_with("syncspace_"),
                "unexpected line {line:?}"
            );
        }
    }
}
