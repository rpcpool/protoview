//! Turning the selected update streams into a [`SubscribeRequest`].

use std::collections::HashMap;

use clap::ValueEnum;
use yellowstone_grpc_proto::geyser::{
    SubscribeRequest, SubscribeRequestFilterAccounts, SubscribeRequestFilterBlocks,
    SubscribeRequestFilterEntry, SubscribeRequestFilterTransactions, SubscribeRequestPing,
};

/// The filter name every stream is registered under; it comes back in each update's
/// `filters` list.
const FILTER_NAME: &str = "bench";

/// An update stream to subscribe to. Each one is unfiltered: every update of that kind
/// the endpoint produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Stream {
    /// Every account update.
    Account,
    /// Every transaction, votes and failures included.
    Txn,
    /// Every entry.
    Entries,
    /// Every full block, with its transactions, accounts and entries.
    Block,
}

/// Builds the subscription request for `streams`, at the default (processed) commitment.
///
/// # Arguments
///
/// * `streams` - The [`Stream`]s to subscribe to; duplicates are harmless.
///
/// # Returns
///
/// A [`SubscribeRequest`] registering one unfiltered filter per stream under the name
/// `bench`.
pub fn subscribe_request(streams: &[Stream]) -> SubscribeRequest {
    let mut request = SubscribeRequest::default();
    for stream in streams {
        match stream {
            Stream::Account => {
                request.accounts = named(SubscribeRequestFilterAccounts::default());
            }
            Stream::Txn => {
                request.transactions = named(SubscribeRequestFilterTransactions::default());
            }
            Stream::Entries => request.entry = named(SubscribeRequestFilterEntry::default()),
            Stream::Block => {
                request.blocks = named(SubscribeRequestFilterBlocks {
                    include_transactions: Some(true),
                    include_accounts: Some(true),
                    include_entries: Some(true),
                    ..Default::default()
                });
            }
        }
    }
    request
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

#[cfg(test)]
mod tests {
    use super::{Stream, subscribe_request};

    #[test]
    fn each_stream_sets_only_its_filter() {
        let request = subscribe_request(&[Stream::Txn]);
        assert!(request.transactions.contains_key("bench"));
        assert!(
            request.accounts.is_empty() && request.blocks.is_empty() && request.entry.is_empty()
        );

        let request = subscribe_request(&[Stream::Account, Stream::Entries, Stream::Block]);
        assert!(request.accounts.contains_key("bench"));
        assert!(request.entry.contains_key("bench"));
        assert!(request.transactions.is_empty());
        let block = &request.blocks["bench"];
        assert_eq!(block.include_transactions, Some(true));
        assert_eq!(block.include_accounts, Some(true));
        assert_eq!(block.include_entries, Some(true));
        assert_eq!(request.commitment, None);
    }
}
