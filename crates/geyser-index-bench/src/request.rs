//! Turning command-line stream selections into a [`SubscribeRequest`].

use std::collections::HashMap;

use clap::{Args, ValueEnum};
use yellowstone_grpc_proto::geyser::{
    CommitmentLevel, SubscribeRequest, SubscribeRequestFilterAccounts,
    SubscribeRequestFilterBlocks, SubscribeRequestFilterBlocksMeta, SubscribeRequestFilterEntry,
    SubscribeRequestFilterSlots, SubscribeRequestFilterTransactions, SubscribeRequestPing,
};

/// The filter name every stream is registered under; it comes back in each update's
/// `filters` list.
const FILTER_NAME: &str = "bench";

/// The commitment level to subscribe at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Commitment {
    Processed,
    Confirmed,
    Finalized,
}

impl From<Commitment> for CommitmentLevel {
    /// Maps the CLI value onto the protocol enum.
    ///
    /// # Arguments
    ///
    /// * `commitment` - The selected [`Commitment`].
    ///
    /// # Returns
    ///
    /// The matching [`CommitmentLevel`].
    fn from(commitment: Commitment) -> Self {
        match commitment {
            Commitment::Processed => Self::Processed,
            Commitment::Confirmed => Self::Confirmed,
            Commitment::Finalized => Self::Finalized,
        }
    }
}

/// Which update streams to subscribe to. With no stream selected, slots and non-vote
/// transactions are subscribed.
#[derive(Debug, Clone, Args)]
pub struct StreamArgs {
    /// Subscribe to slot updates.
    #[arg(long)]
    pub slots: bool,

    /// Subscribe to transactions (non-vote, successful and failed, unless narrowed).
    #[arg(long)]
    pub transactions: bool,

    /// Include vote transactions in the transaction stream.
    #[arg(long, requires = "transactions")]
    pub include_votes: bool,

    /// Exclude failed transactions from the transaction stream.
    #[arg(long, requires = "transactions")]
    pub exclude_failed: bool,

    /// Only transactions touching any of these accounts (base58, repeatable).
    #[arg(long = "tx-account-include", value_name = "PUBKEY", requires = "transactions")]
    pub tx_account_include: Vec<String>,

    /// Subscribe to accounts owned by any of these programs (base58, repeatable).
    #[arg(long = "account-owner", value_name = "PUBKEY")]
    pub account_owner: Vec<String>,

    /// Subscribe to these specific accounts (base58, repeatable).
    #[arg(long = "account", value_name = "PUBKEY")]
    pub account: Vec<String>,

    /// Subscribe to block metadata.
    #[arg(long)]
    pub blocks_meta: bool,

    /// Subscribe to full blocks, including transactions, accounts and entries. Heavy.
    #[arg(long)]
    pub blocks: bool,

    /// Subscribe to entries.
    #[arg(long)]
    pub entries: bool,

    /// Commitment level for every stream.
    #[arg(long, value_enum, default_value_t = Commitment::Processed)]
    pub commitment: Commitment,
}

impl StreamArgs {
    /// Builds the initial subscription request.
    ///
    /// # Returns
    ///
    /// A [`SubscribeRequest`] registering one filter per selected stream under the name
    /// `bench`, falling back to slots plus non-vote transactions when nothing is selected.
    pub fn to_request(&self) -> SubscribeRequest {
        let nothing_selected = !(self.slots
            || self.transactions
            || self.blocks_meta
            || self.blocks
            || self.entries
            || !self.account_owner.is_empty()
            || !self.account.is_empty());
        let slots = self.slots || nothing_selected;
        let transactions = self.transactions || nothing_selected;

        let mut request = SubscribeRequest {
            commitment: Some(CommitmentLevel::from(self.commitment) as i32),
            ..Default::default()
        };
        if slots {
            request.slots = named(SubscribeRequestFilterSlots {
                filter_by_commitment: Some(true),
                ..Default::default()
            });
        }
        if transactions {
            request.transactions = named(SubscribeRequestFilterTransactions {
                vote: (!self.include_votes).then_some(false),
                failed: self.exclude_failed.then_some(false),
                account_include: self.tx_account_include.clone(),
                ..Default::default()
            });
        }
        if !self.account_owner.is_empty() || !self.account.is_empty() {
            request.accounts = named(SubscribeRequestFilterAccounts {
                account: self.account.clone(),
                owner: self.account_owner.clone(),
                ..Default::default()
            });
        }
        if self.blocks_meta {
            request.blocks_meta = named(SubscribeRequestFilterBlocksMeta::default());
        }
        if self.blocks {
            request.blocks = named(SubscribeRequestFilterBlocks {
                include_transactions: Some(true),
                include_accounts: Some(true),
                include_entries: Some(true),
                ..Default::default()
            });
        }
        if self.entries {
            request.entry = named(SubscribeRequestFilterEntry::default());
        }
        request
    }
}

/// Builds the request that answers a server ping, keeping load balancers from closing an
/// otherwise quiet stream.
///
/// # Returns
///
/// A [`SubscribeRequest`] carrying only a [`SubscribeRequestPing`].
pub fn ping_reply() -> SubscribeRequest {
    SubscribeRequest {
        ping: Some(SubscribeRequestPing { id: 1 }),
        ..Default::default()
    }
}

/// Wraps one filter in the single-entry map the request fields expect.
///
/// # Arguments
///
/// * `filter` - The filter to register.
///
/// # Returns
///
/// A [`HashMap`] holding `filter` under the `bench` filter name.
fn named<F>(filter: F) -> HashMap<String, F> {
    HashMap::from([(FILTER_NAME.to_string(), filter)])
}
