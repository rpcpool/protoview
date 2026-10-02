//! Timing one incoming update: indexing it with the generated lens, and optionally
//! decoding it with `prost` for comparison.

use std::hint::black_box;
use std::time::Instant;

use prost::Message as _;
use proto_codec::DecodeError;
use yellowstone_grpc_proto::geyser as ys;

use crate::lens::geyser::SubscribeUpdate;
use crate::lens::geyser::subscribe_update::UpdateOneof;

/// Which member of `SubscribeUpdate`'s `update_oneof` an update carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateKind {
    Account,
    Slot,
    Transaction,
    TransactionStatus,
    Block,
    BlockMeta,
    Entry,
    BlockFooter,
    EntryUpdateParent,
    Ping,
    Pong,
    /// None of the members above: an empty update, or a member newer than the schema.
    Other,
}

impl UpdateKind {
    /// Every kind, in report order.
    pub const ALL: [Self; 12] = [
        Self::Account,
        Self::Slot,
        Self::Transaction,
        Self::TransactionStatus,
        Self::Block,
        Self::BlockMeta,
        Self::Entry,
        Self::BlockFooter,
        Self::EntryUpdateParent,
        Self::Ping,
        Self::Pong,
        Self::Other,
    ];

    /// Returns the name shown in reports.
    ///
    /// # Returns
    ///
    /// The `update_oneof` member name, e.g. `transaction_status`.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Account => "account",
            Self::Slot => "slot",
            Self::Transaction => "transaction",
            Self::TransactionStatus => "transaction_status",
            Self::Block => "block",
            Self::BlockMeta => "block_meta",
            Self::Entry => "entry",
            Self::BlockFooter => "block_footer",
            Self::EntryUpdateParent => "entry_update_parent",
            Self::Ping => "ping",
            Self::Pong => "pong",
            Self::Other => "other",
        }
    }

    /// Returns this kind's position in [`UpdateKind::ALL`].
    ///
    /// # Returns
    ///
    /// An index in `0..ALL.len()`.
    pub const fn index(self) -> usize {
        self as usize
    }

    /// Determines which member an indexed update carries.
    ///
    /// Reading the member builds its lens, which re-indexes that member's own fields, so
    /// this runs outside the timed region.
    ///
    /// # Arguments
    ///
    /// * `update` - The indexed [`SubscribeUpdate`].
    ///
    /// # Returns
    ///
    /// The member's kind, or [`UpdateKind::Other`] if no member the schema knows was set.
    fn of<B: AsRef<[u8]>>(update: &SubscribeUpdate<B>) -> Self {
        match update.update_oneof() {
            Some(UpdateOneof::Account(_)) => Self::Account,
            Some(UpdateOneof::Slot(_)) => Self::Slot,
            Some(UpdateOneof::Transaction(_)) => Self::Transaction,
            Some(UpdateOneof::TransactionStatus(_)) => Self::TransactionStatus,
            Some(UpdateOneof::Block(_)) => Self::Block,
            Some(UpdateOneof::BlockMeta(_)) => Self::BlockMeta,
            Some(UpdateOneof::Entry(_)) => Self::Entry,
            Some(UpdateOneof::BlockFooter(_)) => Self::BlockFooter,
            Some(UpdateOneof::EntryUpdateParent(_)) => Self::EntryUpdateParent,
            Some(UpdateOneof::Ping(_)) => Self::Ping,
            Some(UpdateOneof::Pong(_)) => Self::Pong,
            None => Self::Other,
        }
    }
}

/// The timings taken for one update.
#[derive(Debug, Clone, Copy)]
pub struct Sample {
    /// Which member the update carries.
    pub kind: UpdateKind,
    /// Encoded size in bytes.
    pub size: usize,
    /// Nanoseconds for [`SubscribeUpdate::parse`], averaged over the repeat count.
    pub lens_ns: u64,
    /// Nanoseconds for a `prost` decode plus drop, averaged likewise, when enabled.
    pub prost_ns: Option<u64>,
}

/// Times indexing (and optionally `prost` decoding) of incoming updates.
#[derive(Debug, Clone, Copy)]
pub struct Timer {
    repeat: u32,
    compare_prost: bool,
}

impl Timer {
    /// Creates a timer.
    ///
    /// # Arguments
    ///
    /// * `repeat` - How many times to run each decode per update; the reported time is the
    ///   mean. `1` measures the realistic first touch of freshly received bytes; higher
    ///   values measure a warm cache and amortize clock overhead.
    /// * `compare_prost` - Whether to also time a `prost` decode of the full message.
    ///
    /// # Returns
    ///
    /// A new [`Timer`]; a `repeat` of `0` is treated as `1`.
    pub fn new(repeat: u32, compare_prost: bool) -> Self {
        Self {
            repeat: repeat.max(1),
            compare_prost,
        }
    }

    /// Returns whether `prost` decoding is timed too.
    ///
    /// # Returns
    ///
    /// `true` if [`Sample::prost_ns`] will be populated.
    pub const fn compares_prost(&self) -> bool {
        self.compare_prost
    }

    /// Times one update.
    ///
    /// The lens is timed first, on bytes just received from the network, so with a repeat
    /// count of `1` it pays the cold-cache cost and `prost` then runs on warm bytes — a
    /// bias in `prost`'s favour.
    ///
    /// # Arguments
    ///
    /// * `bytes` - The raw encoding of one `SubscribeUpdate`.
    ///
    /// # Returns
    ///
    /// The [`Sample`] for this update.
    ///
    /// # Errors
    ///
    /// Any [`DecodeError`] from [`SubscribeUpdate::parse`], i.e. the update does not
    /// validate against the schema.
    pub fn measure(&self, bytes: &[u8]) -> Result<Sample, DecodeError> {
        let start = Instant::now();
        let update = black_box(SubscribeUpdate::parse(black_box(bytes))?);
        for _ in 1..self.repeat {
            black_box(SubscribeUpdate::parse(black_box(bytes))?);
        }
        let lens_ns = mean_ns(start, self.repeat);

        let prost_ns = self.compare_prost.then(|| {
            let start = Instant::now();
            for _ in 0..self.repeat {
                // Dropped inside the timed region: freeing is part of prost's cost.
                drop(black_box(ys::SubscribeUpdate::decode(black_box(bytes))));
            }
            mean_ns(start, self.repeat)
        });

        Ok(Sample {
            kind: UpdateKind::of(&update),
            size: bytes.len(),
            lens_ns,
            prost_ns,
        })
    }
}

/// Returns the mean nanoseconds per iteration since `start`.
///
/// # Arguments
///
/// * `start` - When the timed loop began.
/// * `iterations` - How many iterations it ran; at least `1`.
///
/// # Returns
///
/// The elapsed time divided by `iterations`, saturating at [`u64::MAX`].
fn mean_ns(start: Instant, iterations: u32) -> u64 {
    let total = start.elapsed().as_nanos() / u128::from(iterations);
    u64::try_from(total).unwrap_or(u64::MAX)
}
