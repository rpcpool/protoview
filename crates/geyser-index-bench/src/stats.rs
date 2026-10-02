//! Aggregating [`Sample`]s per update kind and rendering the report table.

use std::fmt::{self, Write as _};
use std::time::Duration;

use protoview::DecodeError;

use crate::measure::{Sample, UpdateKind};

/// Values below this are recorded exactly; above it, each power of two is split into
/// this many buckets.
const SUB_BUCKETS: u64 = 32;
/// `log2(SUB_BUCKETS)`.
const SUB_BUCKET_BITS: u32 = 5;
/// Bucket count covering the whole `u64` range: one exact block, then one block of
/// [`SUB_BUCKETS`] per power of two from `2^5` to `2^63`.
const BUCKETS: usize = ((64 - SUB_BUCKET_BITS + 1) * SUB_BUCKETS as u32) as usize;

/// A fixed-size latency histogram with logarithmic buckets.
///
/// Recording is O(1) and memory is constant however long the benchmark runs. Values are
/// exact below 32 and within 1/32 (about 3%) above, which is finer than the run-to-run
/// noise of a network benchmark. Count, sum, min and max are tracked exactly.
#[derive(Clone)]
struct Histogram {
    buckets: Box<[u64; BUCKETS]>,
    count: u64,
    sum: u128,
    min: u64,
    max: u64,
}

impl Histogram {
    /// Creates an empty histogram.
    ///
    /// # Returns
    ///
    /// A [`Histogram`] with no recorded values.
    fn new() -> Self {
        Self {
            buckets: Box::new([0; BUCKETS]),
            count: 0,
            sum: 0,
            min: u64::MAX,
            max: 0,
        }
    }

    /// Returns the bucket `value` falls into.
    ///
    /// # Arguments
    ///
    /// * `value` - The value to place.
    ///
    /// # Returns
    ///
    /// An index below [`BUCKETS`].
    fn bucket(value: u64) -> usize {
        if value < SUB_BUCKETS {
            return value as usize;
        }
        let exponent = 63 - value.leading_zeros(); // >= SUB_BUCKET_BITS
        let shift = exponent - SUB_BUCKET_BITS;
        let sub = (value >> shift) & (SUB_BUCKETS - 1);
        ((shift + 1) as usize) * SUB_BUCKETS as usize + sub as usize
    }

    /// Returns the smallest value that falls into `bucket`.
    ///
    /// # Arguments
    ///
    /// * `bucket` - An index produced by [`Histogram::bucket`].
    ///
    /// # Returns
    ///
    /// The bucket's lower bound.
    fn lower_bound(bucket: usize) -> u64 {
        let block = bucket as u64 / SUB_BUCKETS;
        let sub = bucket as u64 % SUB_BUCKETS;
        if block == 0 {
            sub
        } else {
            (SUB_BUCKETS + sub) << (block - 1)
        }
    }

    /// Records one value.
    ///
    /// # Arguments
    ///
    /// * `value` - The value to record.
    fn record(&mut self, value: u64) {
        self.buckets[Self::bucket(value)] += 1;
        self.count += 1;
        self.sum += u128::from(value);
        self.min = self.min.min(value);
        self.max = self.max.max(value);
    }

    /// Returns the value at percentile `p`, by nearest rank.
    ///
    /// # Arguments
    ///
    /// * `p` - The percentile, in `0.0..=100.0`.
    ///
    /// # Returns
    ///
    /// The lower bound of the bucket holding the requested rank, clamped to the exact
    /// recorded min and max; `0` if nothing was recorded.
    fn percentile(&self, p: f64) -> u64 {
        if self.count == 0 {
            return 0;
        }
        let rank = ((p / 100.0) * self.count as f64).ceil().max(1.0) as u64;
        let mut seen = 0;
        for (bucket, &n) in self.buckets.iter().enumerate() {
            seen += n;
            if seen >= rank {
                return Self::lower_bound(bucket).clamp(self.min, self.max);
            }
        }
        self.max
    }

    /// Returns the sum of all recorded values.
    ///
    /// # Returns
    ///
    /// The exact sum.
    const fn sum(&self) -> u128 {
        self.sum
    }
}

/// Everything recorded for one [`UpdateKind`].
#[derive(Clone)]
struct KindStats {
    view: Histogram,
    prost: Histogram,
    bytes: u64,
}

impl KindStats {
    /// Creates empty statistics.
    ///
    /// # Returns
    ///
    /// A [`KindStats`] with no samples.
    fn new() -> Self {
        Self {
            view: Histogram::new(),
            prost: Histogram::new(),
            bytes: 0,
        }
    }

    /// Returns how many updates were recorded.
    ///
    /// # Returns
    ///
    /// The sample count.
    const fn count(&self) -> u64 {
        self.view.count
    }

    /// Folds `other` into `self`.
    ///
    /// # Arguments
    ///
    /// * `other` - Statistics to add.
    fn merge(&mut self, other: &Self) {
        for (histogram, other) in [
            (&mut self.view, &other.view),
            (&mut self.prost, &other.prost),
        ] {
            for (bucket, n) in histogram.buckets.iter_mut().zip(other.buckets.iter()) {
                *bucket += n;
            }
            histogram.count += other.count;
            histogram.sum += other.sum;
            histogram.min = histogram.min.min(other.min);
            histogram.max = histogram.max.max(other.max);
        }
        self.bytes += other.bytes;
    }
}

/// Per-kind statistics for a benchmark run.
pub struct Stats {
    kinds: Vec<KindStats>,
    compare_prost: bool,
    parse_errors: u64,
    first_error: Option<DecodeError>,
}

impl Stats {
    /// Creates empty statistics.
    ///
    /// # Arguments
    ///
    /// * `compare_prost` - Whether samples carry `prost` timings, deciding which report
    ///   columns are shown.
    ///
    /// # Returns
    ///
    /// A [`Stats`] with no samples.
    pub fn new(compare_prost: bool) -> Self {
        Self {
            kinds: vec![KindStats::new(); UpdateKind::ALL.len()],
            compare_prost,
            parse_errors: 0,
            first_error: None,
        }
    }

    /// Records one timed update.
    ///
    /// # Arguments
    ///
    /// * `sample` - The [`Sample`] to record.
    pub fn record(&mut self, sample: &Sample) {
        let kind = &mut self.kinds[sample.kind.index()];
        kind.view.record(sample.view_ns);
        if let Some(prost_ns) = sample.prost_ns {
            kind.prost.record(prost_ns);
        }
        kind.bytes += sample.size as u64;
    }

    /// Records an update the view rejected.
    ///
    /// # Arguments
    ///
    /// * `error` - The [`DecodeError`] [`Timer::measure`](crate::measure::Timer::measure)
    ///   returned; the first one is kept for the report.
    pub fn record_error(&mut self, error: DecodeError) {
        self.parse_errors += 1;
        self.first_error.get_or_insert(error);
    }

    /// Returns how many updates were seen, including rejected ones.
    ///
    /// # Returns
    ///
    /// The total update count.
    pub fn total(&self) -> u64 {
        self.kinds.iter().map(KindStats::count).sum::<u64>() + self.parse_errors
    }

    /// Renders the report table.
    ///
    /// # Arguments
    ///
    /// * `elapsed` - Time since the subscription started, for the rate line.
    ///
    /// # Returns
    ///
    /// The formatted report, ending in a newline.
    pub fn render(&self, elapsed: Duration) -> String {
        let mut out = String::new();
        // Writing to a `String` cannot fail.
        let _ = self.write_report(&mut out, elapsed);
        out
    }

    /// Writes the report table to `out`.
    ///
    /// # Arguments
    ///
    /// * `out` - Where to write.
    /// * `elapsed` - Time since the subscription started.
    ///
    /// # Returns
    ///
    /// `Ok(())` once written.
    ///
    /// # Errors
    ///
    /// Any [`fmt::Error`] from `out`.
    fn write_report(&self, out: &mut String, elapsed: Duration) -> fmt::Result {
        let mut total = KindStats::new();
        for kind in &self.kinds {
            total.merge(kind);
        }
        let secs = elapsed.as_secs_f64().max(f64::EPSILON);
        writeln!(
            out,
            "\n== {:.1}s elapsed: {} updates ({:.0}/s), {} received ({}/s) ==",
            elapsed.as_secs_f64(),
            self.total(),
            self.total() as f64 / secs,
            bytes(total.bytes as f64),
            bytes(total.bytes as f64 / secs),
        )?;

        write!(
            out,
            "{:<20} {:>9} {:>9} | {:>8} {:>8} {:>8} {:>8} {:>10}",
            "kind", "count", "avg size", "p50", "p90", "p99", "max", "index/s"
        )?;
        if self.compare_prost {
            write!(
                out,
                " | {:>8} {:>8} {:>10} {:>7}",
                "prost50", "prost99", "decode/s", "x p50"
            )?;
        }
        writeln!(out)?;

        let rows = UpdateKind::ALL
            .iter()
            .map(|kind| (kind.label(), &self.kinds[kind.index()]))
            .filter(|(_, stats)| stats.count() > 0)
            .chain((total.count() > 0).then_some(("TOTAL", &total)));
        for (label, stats) in rows {
            let view = &stats.view;
            write!(
                out,
                "{:<20} {:>9} {:>9} | {:>8} {:>8} {:>8} {:>8} {:>10}",
                label,
                stats.count(),
                bytes(stats.bytes as f64 / stats.count() as f64),
                nanos(view.percentile(50.0)),
                nanos(view.percentile(90.0)),
                nanos(view.percentile(99.0)),
                nanos(view.max),
                throughput(stats.bytes, view.sum()),
            )?;
            if self.compare_prost {
                let prost = &stats.prost;
                let speedup = prost.percentile(50.0) as f64 / view.percentile(50.0).max(1) as f64;
                write!(
                    out,
                    " | {:>8} {:>8} {:>10} {:>6.1}x",
                    nanos(prost.percentile(50.0)),
                    nanos(prost.percentile(99.0)),
                    throughput(stats.bytes, prost.sum()),
                    speedup,
                )?;
            }
            writeln!(out)?;
        }

        if let Some(error) = self.first_error {
            writeln!(
                out,
                "{} updates rejected by the view; first error: {error}",
                self.parse_errors
            )?;
        }
        Ok(())
    }
}

/// Formats a duration in nanoseconds with a unit suited to its size.
///
/// # Arguments
///
/// * `ns` - The duration in nanoseconds.
///
/// # Returns
///
/// E.g. `850ns`, `12.3µs`, `4.56ms`.
fn nanos(ns: u64) -> String {
    match ns {
        0..1_000 => format!("{ns}ns"),
        1_000..1_000_000 => format!("{:.1}µs", ns as f64 / 1e3),
        1_000_000..1_000_000_000 => format!("{:.2}ms", ns as f64 / 1e6),
        _ => format!("{:.2}s", ns as f64 / 1e9),
    }
}

/// Formats a byte count with a binary unit.
///
/// # Arguments
///
/// * `n` - The byte count; fractional for averages and rates.
///
/// # Returns
///
/// E.g. `512B`, `3.4KiB`, `1.2MiB`.
fn bytes(n: f64) -> String {
    const KIB: f64 = 1024.0;
    if n < KIB {
        format!("{n:.0}B")
    } else if n < KIB * KIB {
        format!("{:.1}KiB", n / KIB)
    } else if n < KIB * KIB * KIB {
        format!("{:.1}MiB", n / (KIB * KIB))
    } else {
        format!("{:.2}GiB", n / (KIB * KIB * KIB))
    }
}

/// Formats how many bytes per second a decoder processes.
///
/// # Arguments
///
/// * `total_bytes` - Bytes decoded.
/// * `total_ns` - Nanoseconds spent decoding them.
///
/// # Returns
///
/// E.g. `1.2GiB/s`, or `-` when no time was recorded.
fn throughput(total_bytes: u64, total_ns: u128) -> String {
    if total_ns == 0 {
        return "-".to_string();
    }
    format!("{}/s", bytes(total_bytes as f64 * 1e9 / total_ns as f64))
}

#[cfg(test)]
mod tests {
    use super::{BUCKETS, Histogram, SUB_BUCKETS};

    #[test]
    fn small_values_are_exact() {
        for value in 0..SUB_BUCKETS {
            let bucket = Histogram::bucket(value);
            assert_eq!(Histogram::lower_bound(bucket), value);
        }
    }

    #[test]
    fn buckets_are_monotonic_and_within_three_percent() {
        let mut values: Vec<u64> = (0..63)
            .flat_map(|e| [1u64 << e, (1 << e) + 1, (3u64 << e) / 2, (1 << (e + 1)) - 1])
            .chain([u64::MAX / 3, u64::MAX - 1, u64::MAX])
            .collect();
        values.sort_unstable();
        values.dedup();

        let mut previous = 0;
        for value in values {
            let bucket = Histogram::bucket(value);
            assert!(bucket < BUCKETS, "{value} -> bucket {bucket}");
            assert!(bucket >= previous, "{value} went backwards");
            previous = bucket;
            let lower = Histogram::lower_bound(bucket);
            assert!(lower <= value, "{value}: lower bound {lower}");
            assert!(
                (value - lower) as f64 <= value as f64 / SUB_BUCKETS as f64,
                "{value}: lower bound {lower} too far"
            );
        }
    }

    #[test]
    fn percentiles_follow_nearest_rank() {
        let mut histogram = Histogram::new();
        for value in 1..=100 {
            histogram.record(value);
        }
        assert_eq!(histogram.percentile(0.0), 1);
        assert_eq!(histogram.percentile(1.0), 1);
        // Above 32 values share buckets, so ranks resolve to bucket lower bounds.
        assert!((48..=50).contains(&histogram.percentile(50.0)));
        assert!((96..=99).contains(&histogram.percentile(99.0)));
        assert_eq!(histogram.percentile(100.0), 100);
        assert_eq!(histogram.sum(), 5050);
    }

    #[test]
    fn empty_histogram_reports_zero() {
        assert_eq!(Histogram::new().percentile(50.0), 0);
    }
}
