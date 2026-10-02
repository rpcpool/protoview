//! Messages encoded by `yellowstone-grpc-proto` 13.0.0's `prost` types, decoded by the
//! lenses generated from verbatim copies of its protos in `proto/yellowstone/`.
//!
//! Every check destructures the yellowstone struct exhaustively, so a field that is not
//! asserted is a compile error rather than a silent gap.

use std::collections::HashMap;

use prost::Message as _;
use yellowstone_grpc_proto::geyser as ys;
use yellowstone_grpc_proto::solana::storage::confirmed_block as ys_st;

use yellowstone_grpc_proto::geyser::subscribe_update::UpdateOneof;
use yellowstone_grpc_proto::prost_types;

use crate::geyser as lens;
use crate::google::protobuf as lens_pb;
use crate::solana::storage::confirmed_block as lens_st;

/// A lens that can be checked field by field against the `prost` value it was encoded
/// from.
trait Matches<E> {
    fn assert_matches(&self, expected: &E);
}

fn assert_opt<L: Matches<E>, E>(lens: Option<L>, expected: Option<&E>, field: &str) {
    match (lens, expected) {
        (Some(lens), Some(expected)) => lens.assert_matches(expected),
        (None, None) => {}
        (lens, expected) => panic!(
            "{field}: presence mismatch, lens {} vs expected {}",
            lens.is_some(),
            expected.is_some()
        ),
    }
}

fn assert_all<L: Matches<E>, E>(lens: impl Iterator<Item = L>, expected: &[E], field: &str) {
    let lens: Vec<L> = lens.collect();
    assert_eq!(lens.len(), expected.len(), "{field}: element count");
    for (lens, expected) in lens.iter().zip(expected) {
        lens.assert_matches(expected);
    }
}

fn assert_str(lens: Result<&str, core::str::Utf8Error>, expected: &str, field: &str) {
    assert_eq!(lens.unwrap(), expected, "{field}");
}

fn assert_opt_str(
    lens: Option<Result<&str, core::str::Utf8Error>>,
    expected: &Option<String>,
    field: &str,
) {
    assert_eq!(lens.map(Result::unwrap), expected.as_deref(), "{field}");
}

fn assert_strs<'a>(
    lens: impl Iterator<Item = Result<&'a str, core::str::Utf8Error>>,
    expected: &[String],
    field: &str,
) {
    let lens: Vec<&str> = lens.map(Result::unwrap).collect();
    assert_eq!(lens, expected, "{field}");
}

fn assert_bytes_list<'a>(lens: impl Iterator<Item = &'a [u8]>, expected: &[Vec<u8>], field: &str) {
    assert_eq!(lens.collect::<Vec<_>>(), expected, "{field}");
}

/// Encodes `$value` with `prost`, parses it with `$lens`, and checks every field.
macro_rules! round_trip {
    ($lens:ty, $value:expr) => {{
        let expected = $value;
        let bytes = expected.encode_to_vec();
        let lens = <$lens>::parse(bytes.as_slice()).expect("lens must accept prost output");
        lens.assert_matches(&expected);
    }};
}

// ---------------------------------------------------------------------------------------
// solana.storage.ConfirmedBlock
// ---------------------------------------------------------------------------------------

impl<B: AsRef<[u8]>> Matches<ys_st::ConfirmedBlock> for lens_st::ConfirmedBlock<B> {
    fn assert_matches(&self, expected: &ys_st::ConfirmedBlock) {
        let ys_st::ConfirmedBlock {
            previous_blockhash,
            blockhash,
            parent_slot,
            transactions,
            rewards,
            block_time,
            block_height,
            num_partitions,
        } = expected;
        assert_str(
            self.previous_blockhash(),
            previous_blockhash,
            "previous_blockhash",
        );
        assert_str(self.blockhash(), blockhash, "blockhash");
        assert_eq!(self.parent_slot(), *parent_slot, "parent_slot");
        assert_all(self.transactions(), transactions, "transactions");
        assert_all(self.rewards(), rewards, "rewards");
        assert_opt(self.block_time(), block_time.as_ref(), "block_time");
        assert_opt(self.block_height(), block_height.as_ref(), "block_height");
        assert_opt(
            self.num_partitions(),
            num_partitions.as_ref(),
            "num_partitions",
        );
    }
}

impl<B: AsRef<[u8]>> Matches<ys_st::ConfirmedTransaction> for lens_st::ConfirmedTransaction<B> {
    fn assert_matches(&self, expected: &ys_st::ConfirmedTransaction) {
        let ys_st::ConfirmedTransaction { transaction, meta } = expected;
        assert_opt(self.transaction(), transaction.as_ref(), "transaction");
        assert_opt(self.meta(), meta.as_ref(), "meta");
    }
}

impl<B: AsRef<[u8]>> Matches<ys_st::Transaction> for lens_st::Transaction<B> {
    fn assert_matches(&self, expected: &ys_st::Transaction) {
        let ys_st::Transaction {
            signatures,
            message,
        } = expected;
        assert_bytes_list(self.signatures(), signatures, "signatures");
        assert_opt(self.message(), message.as_ref(), "message");
    }
}

impl<B: AsRef<[u8]>> Matches<ys_st::Message> for lens_st::Message<B> {
    fn assert_matches(&self, expected: &ys_st::Message) {
        let ys_st::Message {
            header,
            account_keys,
            recent_blockhash,
            instructions,
            versioned,
            address_table_lookups,
            config,
        } = expected;
        assert_opt(self.header(), header.as_ref(), "header");
        assert_bytes_list(self.account_keys(), account_keys, "account_keys");
        assert_eq!(
            self.recent_blockhash(),
            recent_blockhash.as_slice(),
            "recent_blockhash"
        );
        assert_all(self.instructions(), instructions, "instructions");
        assert_eq!(self.versioned(), *versioned, "versioned");
        assert_all(
            self.address_table_lookups(),
            address_table_lookups,
            "address_table_lookups",
        );
        assert_opt(self.config(), config.as_ref(), "config");
    }
}

impl<B: AsRef<[u8]>> Matches<ys_st::TransactionConfig> for lens_st::TransactionConfig<B> {
    fn assert_matches(&self, expected: &ys_st::TransactionConfig) {
        let ys_st::TransactionConfig {
            priority_fee,
            compute_unit_limit,
            loaded_accounts_data_size_limit,
            heap_size,
        } = expected;
        assert_eq!(self.priority_fee(), *priority_fee, "priority_fee");
        assert_eq!(
            self.compute_unit_limit(),
            *compute_unit_limit,
            "compute_unit_limit"
        );
        assert_eq!(
            self.loaded_accounts_data_size_limit(),
            *loaded_accounts_data_size_limit,
            "loaded_accounts_data_size_limit"
        );
        assert_eq!(self.heap_size(), *heap_size, "heap_size");
    }
}

impl<B: AsRef<[u8]>> Matches<ys_st::MessageHeader> for lens_st::MessageHeader<B> {
    fn assert_matches(&self, expected: &ys_st::MessageHeader) {
        let ys_st::MessageHeader {
            num_required_signatures,
            num_readonly_signed_accounts,
            num_readonly_unsigned_accounts,
        } = expected;
        assert_eq!(self.num_required_signatures(), *num_required_signatures);
        assert_eq!(
            self.num_readonly_signed_accounts(),
            *num_readonly_signed_accounts
        );
        assert_eq!(
            self.num_readonly_unsigned_accounts(),
            *num_readonly_unsigned_accounts
        );
    }
}

impl<B: AsRef<[u8]>> Matches<ys_st::MessageAddressTableLookup>
    for lens_st::MessageAddressTableLookup<B>
{
    fn assert_matches(&self, expected: &ys_st::MessageAddressTableLookup) {
        let ys_st::MessageAddressTableLookup {
            account_key,
            writable_indexes,
            readonly_indexes,
        } = expected;
        assert_eq!(self.account_key(), account_key.as_slice(), "account_key");
        assert_eq!(
            self.writable_indexes(),
            writable_indexes.as_slice(),
            "writable_indexes"
        );
        assert_eq!(
            self.readonly_indexes(),
            readonly_indexes.as_slice(),
            "readonly_indexes"
        );
    }
}

impl<B: AsRef<[u8]>> Matches<ys_st::TransactionStatusMeta> for lens_st::TransactionStatusMeta<B> {
    fn assert_matches(&self, expected: &ys_st::TransactionStatusMeta) {
        let ys_st::TransactionStatusMeta {
            err,
            fee,
            pre_balances,
            post_balances,
            inner_instructions,
            inner_instructions_none,
            log_messages,
            log_messages_none,
            pre_token_balances,
            post_token_balances,
            rewards,
            loaded_writable_addresses,
            loaded_readonly_addresses,
            return_data,
            return_data_none,
            compute_units_consumed,
            cost_units,
        } = expected;
        assert_opt(self.err(), err.as_ref(), "err");
        assert_eq!(self.fee(), *fee, "fee");
        assert_eq!(
            self.pre_balances().collect::<Vec<_>>(),
            *pre_balances,
            "pre_balances"
        );
        assert_eq!(
            self.post_balances().collect::<Vec<_>>(),
            *post_balances,
            "post_balances"
        );
        assert_all(
            self.inner_instructions(),
            inner_instructions,
            "inner_instructions",
        );
        assert_eq!(self.inner_instructions_none(), *inner_instructions_none);
        assert_strs(self.log_messages(), log_messages, "log_messages");
        assert_eq!(self.log_messages_none(), *log_messages_none);
        assert_all(
            self.pre_token_balances(),
            pre_token_balances,
            "pre_token_balances",
        );
        assert_all(
            self.post_token_balances(),
            post_token_balances,
            "post_token_balances",
        );
        assert_all(self.rewards(), rewards, "rewards");
        assert_bytes_list(
            self.loaded_writable_addresses(),
            loaded_writable_addresses,
            "loaded_writable_addresses",
        );
        assert_bytes_list(
            self.loaded_readonly_addresses(),
            loaded_readonly_addresses,
            "loaded_readonly_addresses",
        );
        assert_opt(self.return_data(), return_data.as_ref(), "return_data");
        assert_eq!(self.return_data_none(), *return_data_none);
        assert_eq!(self.compute_units_consumed(), *compute_units_consumed);
        assert_eq!(self.cost_units(), *cost_units, "cost_units");
    }
}

impl<B: AsRef<[u8]>> Matches<ys_st::TransactionError> for lens_st::TransactionError<B> {
    fn assert_matches(&self, expected: &ys_st::TransactionError) {
        let ys_st::TransactionError { err } = expected;
        assert_eq!(self.err(), err.as_slice(), "err");
    }
}

impl<B: AsRef<[u8]>> Matches<ys_st::InnerInstructions> for lens_st::InnerInstructions<B> {
    fn assert_matches(&self, expected: &ys_st::InnerInstructions) {
        let ys_st::InnerInstructions {
            index,
            instructions,
        } = expected;
        assert_eq!(self.index(), *index, "index");
        assert_all(self.instructions(), instructions, "instructions");
    }
}

impl<B: AsRef<[u8]>> Matches<ys_st::InnerInstruction> for lens_st::InnerInstruction<B> {
    fn assert_matches(&self, expected: &ys_st::InnerInstruction) {
        let ys_st::InnerInstruction {
            program_id_index,
            accounts,
            data,
            stack_height,
        } = expected;
        assert_eq!(self.program_id_index(), *program_id_index);
        assert_eq!(self.accounts(), accounts.as_slice(), "accounts");
        assert_eq!(self.data(), data.as_slice(), "data");
        assert_eq!(self.stack_height(), *stack_height, "stack_height");
    }
}

impl<B: AsRef<[u8]>> Matches<ys_st::CompiledInstruction> for lens_st::CompiledInstruction<B> {
    fn assert_matches(&self, expected: &ys_st::CompiledInstruction) {
        let ys_st::CompiledInstruction {
            program_id_index,
            accounts,
            data,
        } = expected;
        assert_eq!(self.program_id_index(), *program_id_index);
        assert_eq!(self.accounts(), accounts.as_slice(), "accounts");
        assert_eq!(self.data(), data.as_slice(), "data");
    }
}

impl<B: AsRef<[u8]>> Matches<ys_st::TokenBalance> for lens_st::TokenBalance<B> {
    fn assert_matches(&self, expected: &ys_st::TokenBalance) {
        let ys_st::TokenBalance {
            account_index,
            mint,
            ui_token_amount,
            owner,
            program_id,
        } = expected;
        assert_eq!(self.account_index(), *account_index, "account_index");
        assert_str(self.mint(), mint, "mint");
        assert_opt(
            self.ui_token_amount(),
            ui_token_amount.as_ref(),
            "ui_token_amount",
        );
        assert_str(self.owner(), owner, "owner");
        assert_str(self.program_id(), program_id, "program_id");
    }
}

impl<B: AsRef<[u8]>> Matches<ys_st::UiTokenAmount> for lens_st::UiTokenAmount<B> {
    fn assert_matches(&self, expected: &ys_st::UiTokenAmount) {
        let ys_st::UiTokenAmount {
            ui_amount,
            decimals,
            amount,
            ui_amount_string,
        } = expected;
        assert_eq!(self.ui_amount(), *ui_amount, "ui_amount");
        assert_eq!(self.decimals(), *decimals, "decimals");
        assert_str(self.amount(), amount, "amount");
        assert_str(
            self.ui_amount_string(),
            ui_amount_string,
            "ui_amount_string",
        );
    }
}

impl<B: AsRef<[u8]>> Matches<ys_st::ReturnData> for lens_st::ReturnData<B> {
    fn assert_matches(&self, expected: &ys_st::ReturnData) {
        let ys_st::ReturnData { program_id, data } = expected;
        assert_eq!(self.program_id(), program_id.as_slice(), "program_id");
        assert_eq!(self.data(), data.as_slice(), "data");
    }
}

impl<B: AsRef<[u8]>> Matches<ys_st::Reward> for lens_st::Reward<B> {
    fn assert_matches(&self, expected: &ys_st::Reward) {
        let ys_st::Reward {
            pubkey,
            lamports,
            post_balance,
            reward_type,
            commission,
            commission_bps,
        } = expected;
        assert_str(self.pubkey(), pubkey, "pubkey");
        assert_eq!(self.lamports(), *lamports, "lamports");
        assert_eq!(self.post_balance(), *post_balance, "post_balance");
        assert_eq!(self.reward_type().to_i32(), *reward_type, "reward_type");
        assert_str(self.commission(), commission, "commission");
        assert_str(self.commission_bps(), commission_bps, "commission_bps");
    }
}

impl<B: AsRef<[u8]>> Matches<ys_st::Rewards> for lens_st::Rewards<B> {
    fn assert_matches(&self, expected: &ys_st::Rewards) {
        let ys_st::Rewards {
            rewards,
            num_partitions,
        } = expected;
        assert_all(self.rewards(), rewards, "rewards");
        assert_opt(
            self.num_partitions(),
            num_partitions.as_ref(),
            "num_partitions",
        );
    }
}

impl<B: AsRef<[u8]>> Matches<ys_st::UnixTimestamp> for lens_st::UnixTimestamp<B> {
    fn assert_matches(&self, expected: &ys_st::UnixTimestamp) {
        let ys_st::UnixTimestamp { timestamp } = expected;
        assert_eq!(self.timestamp(), *timestamp, "timestamp");
    }
}

impl<B: AsRef<[u8]>> Matches<ys_st::BlockHeight> for lens_st::BlockHeight<B> {
    fn assert_matches(&self, expected: &ys_st::BlockHeight) {
        let ys_st::BlockHeight { block_height } = expected;
        assert_eq!(self.block_height(), *block_height, "block_height");
    }
}

impl<B: AsRef<[u8]>> Matches<ys_st::NumPartitions> for lens_st::NumPartitions<B> {
    fn assert_matches(&self, expected: &ys_st::NumPartitions) {
        let ys_st::NumPartitions { num_partitions } = expected;
        assert_eq!(self.num_partitions(), *num_partitions, "num_partitions");
    }
}

// ---------------------------------------------------------------------------------------
// geyser
// ---------------------------------------------------------------------------------------

impl<B: AsRef<[u8]>> Matches<prost_types::Timestamp> for lens_pb::Timestamp<B> {
    fn assert_matches(&self, expected: &prost_types::Timestamp) {
        let prost_types::Timestamp { seconds, nanos } = expected;
        assert_eq!(self.seconds(), *seconds, "seconds");
        assert_eq!(self.nanos(), *nanos, "nanos");
    }
}

/// Asserts that a lens oneof getter and a `prost` oneof field hold the same member with
/// the same contents. Variant names are shared between the two enums.
macro_rules! assert_oneof {
    ($lens:expr, $expected:expr, $lens_enum:ident, $prost_enum:ident, [$($variant:ident),* $(,)?]) => {
        match ($lens, $expected) {
            (None, None) => {}
            $(
                (Some($lens_enum::$variant(lens)), Some($prost_enum::$variant(expected))) => {
                    lens.assert_matches(expected)
                }
            )*
            (lens, expected) => panic!(
                "oneof mismatch: lens has a member: {}, expected has a member: {}",
                lens.is_some(),
                expected.is_some()
            ),
        }
    };
}

impl<B: AsRef<[u8]>> Matches<ys::SubscribeUpdate> for lens::SubscribeUpdate<B> {
    fn assert_matches(&self, expected: &ys::SubscribeUpdate) {
        use lens::subscribe_update::UpdateOneof as LensOneof;
        use ys::subscribe_update::UpdateOneof as ProstOneof;
        let ys::SubscribeUpdate {
            filters,
            update_oneof,
            created_at,
        } = expected;
        assert_strs(self.filters(), filters, "filters");
        assert_opt(self.created_at(), created_at.as_ref(), "created_at");
        assert_oneof!(
            self.update_oneof(),
            update_oneof,
            LensOneof,
            ProstOneof,
            [
                Account,
                Slot,
                Transaction,
                TransactionStatus,
                Block,
                Ping,
                Pong,
                BlockMeta,
                Entry,
                BlockFooter,
                EntryUpdateParent,
            ]
        );
    }
}

impl<B: AsRef<[u8]>> Matches<ys::SubscribeUpdateDeshred> for lens::SubscribeUpdateDeshred<B> {
    fn assert_matches(&self, expected: &ys::SubscribeUpdateDeshred) {
        use lens::subscribe_update_deshred::UpdateOneof as LensOneof;
        use ys::subscribe_update_deshred::UpdateOneof as ProstOneof;
        let ys::SubscribeUpdateDeshred {
            filters,
            update_oneof,
            created_at,
        } = expected;
        assert_strs(self.filters(), filters, "filters");
        assert_opt(self.created_at(), created_at.as_ref(), "created_at");
        assert_oneof!(
            self.update_oneof(),
            update_oneof,
            LensOneof,
            ProstOneof,
            [DeshredTransaction, Ping, Pong, Slot, DeshredUpdateParent]
        );
    }
}

impl<B: AsRef<[u8]>> Matches<ys::SubscribeUpdateGossip> for lens::SubscribeUpdateGossip<B> {
    fn assert_matches(&self, expected: &ys::SubscribeUpdateGossip) {
        use lens::subscribe_update_gossip::UpdateOneof as LensOneof;
        use ys::subscribe_update_gossip::UpdateOneof as ProstOneof;
        let ys::SubscribeUpdateGossip {
            update_oneof,
            created_at,
            seq,
        } = expected;
        assert_opt(self.created_at(), created_at.as_ref(), "created_at");
        assert_eq!(self.seq(), *seq, "seq");
        assert_oneof!(
            self.update_oneof(),
            update_oneof,
            LensOneof,
            ProstOneof,
            [Node, Removed, Ping, Snapshot]
        );
    }
}

/// Asserts that a lens map getter yields exactly the entries of a `prost` `HashMap<String, _>`.
fn assert_map<'a, L: Matches<E>, E>(
    lens: impl Iterator<Item = (Result<&'a str, core::str::Utf8Error>, L)>,
    expected: &HashMap<String, E>,
    field: &str,
) {
    let lens: Vec<(&str, L)> = lens.map(|(key, value)| (key.unwrap(), value)).collect();
    assert_eq!(lens.len(), expected.len(), "{field}: entry count");
    for (key, value) in &lens {
        let expected = expected
            .get(*key)
            .unwrap_or_else(|| panic!("{field}: unexpected key {key:?}"));
        value.assert_matches(expected);
    }
}

impl<B: AsRef<[u8]>> Matches<ys::SubscribeRequest> for lens::SubscribeRequest<B> {
    fn assert_matches(&self, expected: &ys::SubscribeRequest) {
        let ys::SubscribeRequest {
            accounts,
            slots,
            transactions,
            transactions_status,
            blocks,
            blocks_meta,
            entry,
            commitment,
            accounts_data_slice,
            ping,
            from_slot,
            block_footer,
        } = expected;
        assert_map(self.accounts(), accounts, "accounts");
        assert_map(self.slots(), slots, "slots");
        assert_map(self.transactions(), transactions, "transactions");
        assert_map(
            self.transactions_status(),
            transactions_status,
            "transactions_status",
        );
        assert_map(self.blocks(), blocks, "blocks");
        assert_map(self.blocks_meta(), blocks_meta, "blocks_meta");
        assert_map(self.entry(), entry, "entry");
        assert_eq!(
            self.commitment().map(|c| c.to_i32()),
            *commitment,
            "commitment"
        );
        assert_all(
            self.accounts_data_slice(),
            accounts_data_slice,
            "accounts_data_slice",
        );
        assert_opt(self.ping(), ping.as_ref(), "ping");
        assert_eq!(self.from_slot(), *from_slot, "from_slot");
        assert_map(self.block_footer(), block_footer, "block_footer");
    }
}

impl<B: AsRef<[u8]>> Matches<ys::CuckooFilter> for lens::CuckooFilter<B> {
    fn assert_matches(&self, expected: &ys::CuckooFilter) {
        let ys::CuckooFilter {
            data,
            bucket_count,
            entries_per_bucket,
            fingerprint_bits,
            hash_seed,
            hash_algorithm,
        } = expected;
        assert_eq!(self.data(), data.as_slice(), "data");
        assert_eq!(self.bucket_count(), *bucket_count);
        assert_eq!(self.entries_per_bucket(), *entries_per_bucket);
        assert_eq!(self.fingerprint_bits(), *fingerprint_bits);
        assert_eq!(self.hash_seed(), *hash_seed);
        assert_eq!(self.hash_algorithm().to_i32(), *hash_algorithm);
    }
}

impl<B: AsRef<[u8]>> Matches<ys::SubscribeRequestFilterAccounts>
    for lens::SubscribeRequestFilterAccounts<B>
{
    fn assert_matches(&self, expected: &ys::SubscribeRequestFilterAccounts) {
        let ys::SubscribeRequestFilterAccounts {
            account,
            owner,
            filters,
            nonempty_txn_signature,
            cuckoo_accounts_filter,
        } = expected;
        assert_strs(self.account(), account, "account");
        assert_strs(self.owner(), owner, "owner");
        assert_all(self.filters(), filters, "filters");
        assert_eq!(self.nonempty_txn_signature(), *nonempty_txn_signature);
        assert_opt(
            self.cuckoo_accounts_filter(),
            cuckoo_accounts_filter.as_ref(),
            "cuckoo_accounts_filter",
        );
    }
}

impl<B: AsRef<[u8]>> Matches<ys::SubscribeRequestFilterAccountsFilter>
    for lens::SubscribeRequestFilterAccountsFilter<B>
{
    fn assert_matches(&self, expected: &ys::SubscribeRequestFilterAccountsFilter) {
        use lens::subscribe_request_filter_accounts_filter::Filter as L;
        use ys::subscribe_request_filter_accounts_filter::Filter as E;
        let ys::SubscribeRequestFilterAccountsFilter { filter } = expected;
        match (self.filter(), filter) {
            (None, None) => {}
            (Some(L::Memcmp(lens)), Some(E::Memcmp(expected))) => lens.assert_matches(expected),
            (Some(L::Datasize(lens)), Some(E::Datasize(expected))) => assert_eq!(lens, *expected),
            (Some(L::TokenAccountState(lens)), Some(E::TokenAccountState(expected))) => {
                assert_eq!(lens, *expected)
            }
            (Some(L::Lamports(lens)), Some(E::Lamports(expected))) => lens.assert_matches(expected),
            _ => panic!("filter: oneof mismatch"),
        }
    }
}

impl<B: AsRef<[u8]>> Matches<ys::SubscribeRequestFilterAccountsFilterMemcmp>
    for lens::SubscribeRequestFilterAccountsFilterMemcmp<B>
{
    fn assert_matches(&self, expected: &ys::SubscribeRequestFilterAccountsFilterMemcmp) {
        use lens::subscribe_request_filter_accounts_filter_memcmp::Data as L;
        use ys::subscribe_request_filter_accounts_filter_memcmp::Data as E;
        let ys::SubscribeRequestFilterAccountsFilterMemcmp { offset, data } = expected;
        assert_eq!(self.offset(), *offset, "offset");
        match (self.data(), data) {
            (None, None) => {}
            (Some(L::Bytes(lens)), Some(E::Bytes(expected))) => {
                assert_eq!(lens, expected.as_slice())
            }
            (Some(L::Base58(lens)), Some(E::Base58(expected))) => {
                assert_eq!(lens.unwrap(), expected)
            }
            (Some(L::Base64(lens)), Some(E::Base64(expected))) => {
                assert_eq!(lens.unwrap(), expected)
            }
            _ => panic!("data: oneof mismatch"),
        }
    }
}

impl<B: AsRef<[u8]>> Matches<ys::SubscribeRequestFilterAccountsFilterLamports>
    for lens::SubscribeRequestFilterAccountsFilterLamports<B>
{
    fn assert_matches(&self, expected: &ys::SubscribeRequestFilterAccountsFilterLamports) {
        use lens::subscribe_request_filter_accounts_filter_lamports::Cmp as L;
        use ys::subscribe_request_filter_accounts_filter_lamports::Cmp as E;
        let ys::SubscribeRequestFilterAccountsFilterLamports { cmp } = expected;
        match (self.cmp(), cmp) {
            (None, None) => {}
            (Some(L::Eq(lens)), Some(E::Eq(expected)))
            | (Some(L::Ne(lens)), Some(E::Ne(expected)))
            | (Some(L::Lt(lens)), Some(E::Lt(expected)))
            | (Some(L::Gt(lens)), Some(E::Gt(expected))) => assert_eq!(lens, *expected),
            _ => panic!("cmp: oneof mismatch"),
        }
    }
}

impl<B: AsRef<[u8]>> Matches<ys::SubscribeRequestFilterSlots>
    for lens::SubscribeRequestFilterSlots<B>
{
    fn assert_matches(&self, expected: &ys::SubscribeRequestFilterSlots) {
        let ys::SubscribeRequestFilterSlots {
            filter_by_commitment,
            interslot_updates,
        } = expected;
        assert_eq!(self.filter_by_commitment(), *filter_by_commitment);
        assert_eq!(self.interslot_updates(), *interslot_updates);
    }
}

impl<B: AsRef<[u8]>> Matches<ys::SubscribeRequestFilterTransactions>
    for lens::SubscribeRequestFilterTransactions<B>
{
    fn assert_matches(&self, expected: &ys::SubscribeRequestFilterTransactions) {
        let ys::SubscribeRequestFilterTransactions {
            vote,
            failed,
            signature,
            account_include,
            account_exclude,
            account_required,
            cuckoo_account_include,
            token_accounts,
        } = expected;
        assert_eq!(self.vote(), *vote, "vote");
        assert_eq!(self.failed(), *failed, "failed");
        assert_opt_str(self.signature(), signature, "signature");
        assert_strs(self.account_include(), account_include, "account_include");
        assert_strs(self.account_exclude(), account_exclude, "account_exclude");
        assert_strs(
            self.account_required(),
            account_required,
            "account_required",
        );
        assert_opt(
            self.cuckoo_account_include(),
            cuckoo_account_include.as_ref(),
            "cuckoo_account_include",
        );
        assert_eq!(self.token_accounts().map(|t| t.to_i32()), *token_accounts);
    }
}

impl<B: AsRef<[u8]>> Matches<ys::SubscribeRequestFilterBlocks>
    for lens::SubscribeRequestFilterBlocks<B>
{
    fn assert_matches(&self, expected: &ys::SubscribeRequestFilterBlocks) {
        let ys::SubscribeRequestFilterBlocks {
            account_include,
            include_transactions,
            include_accounts,
            include_entries,
            cuckoo_account_include,
        } = expected;
        assert_strs(self.account_include(), account_include, "account_include");
        assert_eq!(self.include_transactions(), *include_transactions);
        assert_eq!(self.include_accounts(), *include_accounts);
        assert_eq!(self.include_entries(), *include_entries);
        assert_opt(
            self.cuckoo_account_include(),
            cuckoo_account_include.as_ref(),
            "cuckoo_account_include",
        );
    }
}

impl<B: AsRef<[u8]>> Matches<ys::SubscribeRequestFilterBlocksMeta>
    for lens::SubscribeRequestFilterBlocksMeta<B>
{
    fn assert_matches(&self, expected: &ys::SubscribeRequestFilterBlocksMeta) {
        let ys::SubscribeRequestFilterBlocksMeta {} = expected;
    }
}

impl<B: AsRef<[u8]>> Matches<ys::SubscribeRequestFilterEntry>
    for lens::SubscribeRequestFilterEntry<B>
{
    fn assert_matches(&self, expected: &ys::SubscribeRequestFilterEntry) {
        let ys::SubscribeRequestFilterEntry {
            include_update_parent,
        } = expected;
        assert_eq!(self.include_update_parent(), *include_update_parent);
    }
}

impl<B: AsRef<[u8]>> Matches<ys::SubscribeRequestFilterBlockFooter>
    for lens::SubscribeRequestFilterBlockFooter<B>
{
    fn assert_matches(&self, expected: &ys::SubscribeRequestFilterBlockFooter) {
        let ys::SubscribeRequestFilterBlockFooter {
            include_certificates,
        } = expected;
        assert_eq!(self.include_certificates(), *include_certificates);
    }
}

impl<B: AsRef<[u8]>> Matches<ys::SubscribeRequestAccountsDataSlice>
    for lens::SubscribeRequestAccountsDataSlice<B>
{
    fn assert_matches(&self, expected: &ys::SubscribeRequestAccountsDataSlice) {
        let ys::SubscribeRequestAccountsDataSlice { offset, length } = expected;
        assert_eq!(self.offset(), *offset, "offset");
        assert_eq!(self.length(), *length, "length");
    }
}

impl<B: AsRef<[u8]>> Matches<ys::SubscribeRequestPing> for lens::SubscribeRequestPing<B> {
    fn assert_matches(&self, expected: &ys::SubscribeRequestPing) {
        let ys::SubscribeRequestPing { id } = expected;
        assert_eq!(self.id(), *id, "id");
    }
}

impl<B: AsRef<[u8]>> Matches<ys::SubscribeUpdateAccount> for lens::SubscribeUpdateAccount<B> {
    fn assert_matches(&self, expected: &ys::SubscribeUpdateAccount) {
        let ys::SubscribeUpdateAccount {
            account,
            slot,
            is_startup,
            bank_id,
        } = expected;
        assert_opt(self.account(), account.as_ref(), "account");
        assert_eq!(self.slot(), *slot, "slot");
        assert_eq!(self.is_startup(), *is_startup, "is_startup");
        assert_eq!(self.bank_id(), *bank_id, "bank_id");
    }
}

impl<B: AsRef<[u8]>> Matches<ys::SubscribeUpdateAccountInfo>
    for lens::SubscribeUpdateAccountInfo<B>
{
    fn assert_matches(&self, expected: &ys::SubscribeUpdateAccountInfo) {
        let ys::SubscribeUpdateAccountInfo {
            pubkey,
            lamports,
            owner,
            executable,
            rent_epoch,
            data,
            write_version,
            txn_signature,
        } = expected;
        assert_eq!(self.pubkey(), pubkey.as_slice(), "pubkey");
        assert_eq!(self.lamports(), *lamports, "lamports");
        assert_eq!(self.owner(), owner.as_slice(), "owner");
        assert_eq!(self.executable(), *executable, "executable");
        assert_eq!(self.rent_epoch(), *rent_epoch, "rent_epoch");
        assert_eq!(self.data(), data.as_slice(), "data");
        assert_eq!(self.write_version(), *write_version, "write_version");
        assert_eq!(
            self.txn_signature(),
            txn_signature.as_deref(),
            "txn_signature"
        );
    }
}

impl<B: AsRef<[u8]>> Matches<ys::SubscribeUpdateSlot> for lens::SubscribeUpdateSlot<B> {
    fn assert_matches(&self, expected: &ys::SubscribeUpdateSlot) {
        let ys::SubscribeUpdateSlot {
            slot,
            parent,
            status,
            dead_error,
            bank_id,
        } = expected;
        assert_eq!(self.slot(), *slot, "slot");
        assert_eq!(self.parent(), *parent, "parent");
        assert_eq!(self.status().to_i32(), *status, "status");
        assert_opt_str(self.dead_error(), dead_error, "dead_error");
        assert_eq!(self.bank_id(), *bank_id, "bank_id");
    }
}

impl<B: AsRef<[u8]>> Matches<ys::SubscribeUpdateTransaction>
    for lens::SubscribeUpdateTransaction<B>
{
    fn assert_matches(&self, expected: &ys::SubscribeUpdateTransaction) {
        let ys::SubscribeUpdateTransaction {
            transaction,
            slot,
            bank_id,
        } = expected;
        assert_opt(self.transaction(), transaction.as_ref(), "transaction");
        assert_eq!(self.slot(), *slot, "slot");
        assert_eq!(self.bank_id(), *bank_id, "bank_id");
    }
}

impl<B: AsRef<[u8]>> Matches<ys::SubscribeUpdateTransactionInfo>
    for lens::SubscribeUpdateTransactionInfo<B>
{
    fn assert_matches(&self, expected: &ys::SubscribeUpdateTransactionInfo) {
        let ys::SubscribeUpdateTransactionInfo {
            signature,
            is_vote,
            transaction,
            meta,
            index,
        } = expected;
        assert_eq!(self.signature(), signature.as_slice(), "signature");
        assert_eq!(self.is_vote(), *is_vote, "is_vote");
        assert_opt(self.transaction(), transaction.as_ref(), "transaction");
        assert_opt(self.meta(), meta.as_ref(), "meta");
        assert_eq!(self.index(), *index, "index");
    }
}

impl<B: AsRef<[u8]>> Matches<ys::SubscribeUpdateTransactionStatus>
    for lens::SubscribeUpdateTransactionStatus<B>
{
    fn assert_matches(&self, expected: &ys::SubscribeUpdateTransactionStatus) {
        let ys::SubscribeUpdateTransactionStatus {
            slot,
            signature,
            is_vote,
            index,
            err,
            bank_id,
        } = expected;
        assert_eq!(self.slot(), *slot, "slot");
        assert_eq!(self.signature(), signature.as_slice(), "signature");
        assert_eq!(self.is_vote(), *is_vote, "is_vote");
        assert_eq!(self.index(), *index, "index");
        assert_opt(self.err(), err.as_ref(), "err");
        assert_eq!(self.bank_id(), *bank_id, "bank_id");
    }
}

impl<B: AsRef<[u8]>> Matches<ys::SubscribeUpdateBlock> for lens::SubscribeUpdateBlock<B> {
    fn assert_matches(&self, expected: &ys::SubscribeUpdateBlock) {
        let ys::SubscribeUpdateBlock {
            slot,
            blockhash,
            rewards,
            block_time,
            block_height,
            parent_slot,
            parent_blockhash,
            executed_transaction_count,
            transactions,
            updated_account_count,
            accounts,
            entries_count,
            entries,
            bank_id,
        } = expected;
        assert_eq!(self.slot(), *slot, "slot");
        assert_str(self.blockhash(), blockhash, "blockhash");
        assert_opt(self.rewards(), rewards.as_ref(), "rewards");
        assert_opt(self.block_time(), block_time.as_ref(), "block_time");
        assert_opt(self.block_height(), block_height.as_ref(), "block_height");
        assert_eq!(self.parent_slot(), *parent_slot, "parent_slot");
        assert_str(
            self.parent_blockhash(),
            parent_blockhash,
            "parent_blockhash",
        );
        assert_eq!(
            self.executed_transaction_count(),
            *executed_transaction_count
        );
        assert_all(self.transactions(), transactions, "transactions");
        assert_eq!(self.updated_account_count(), *updated_account_count);
        assert_all(self.accounts(), accounts, "accounts");
        assert_eq!(self.entries_count(), *entries_count, "entries_count");
        assert_all(self.entries(), entries, "entries");
        assert_eq!(self.bank_id(), *bank_id, "bank_id");
    }
}

impl<B: AsRef<[u8]>> Matches<ys::SubscribeUpdateBlockMeta> for lens::SubscribeUpdateBlockMeta<B> {
    fn assert_matches(&self, expected: &ys::SubscribeUpdateBlockMeta) {
        let ys::SubscribeUpdateBlockMeta {
            slot,
            blockhash,
            rewards,
            block_time,
            block_height,
            parent_slot,
            parent_blockhash,
            executed_transaction_count,
            entries_count,
            bank_id,
        } = expected;
        assert_eq!(self.slot(), *slot, "slot");
        assert_str(self.blockhash(), blockhash, "blockhash");
        assert_opt(self.rewards(), rewards.as_ref(), "rewards");
        assert_opt(self.block_time(), block_time.as_ref(), "block_time");
        assert_opt(self.block_height(), block_height.as_ref(), "block_height");
        assert_eq!(self.parent_slot(), *parent_slot, "parent_slot");
        assert_str(
            self.parent_blockhash(),
            parent_blockhash,
            "parent_blockhash",
        );
        assert_eq!(
            self.executed_transaction_count(),
            *executed_transaction_count
        );
        assert_eq!(self.entries_count(), *entries_count, "entries_count");
        assert_eq!(self.bank_id(), *bank_id, "bank_id");
    }
}

impl<B: AsRef<[u8]>> Matches<ys::SubscribeUpdateBlockFooter>
    for lens::SubscribeUpdateBlockFooter<B>
{
    fn assert_matches(&self, expected: &ys::SubscribeUpdateBlockFooter) {
        let ys::SubscribeUpdateBlockFooter {
            slot,
            bank_id,
            bank_hash,
            block_producer_time_nanos,
            block_user_agent,
            block_final_cert,
            skip_reward_cert,
            notar_reward_cert,
        } = expected;
        assert_eq!(self.slot(), *slot, "slot");
        assert_eq!(self.bank_id(), *bank_id, "bank_id");
        assert_eq!(self.bank_hash(), bank_hash.as_slice(), "bank_hash");
        assert_eq!(self.block_producer_time_nanos(), *block_producer_time_nanos);
        assert_eq!(self.block_user_agent(), block_user_agent.as_slice());
        assert_eq!(self.block_final_cert(), block_final_cert.as_deref());
        assert_eq!(self.skip_reward_cert(), skip_reward_cert.as_deref());
        assert_eq!(self.notar_reward_cert(), notar_reward_cert.as_deref());
    }
}

impl<B: AsRef<[u8]>> Matches<ys::SubscribeUpdateEntry> for lens::SubscribeUpdateEntry<B> {
    fn assert_matches(&self, expected: &ys::SubscribeUpdateEntry) {
        let ys::SubscribeUpdateEntry {
            slot,
            index,
            num_hashes,
            hash,
            executed_transaction_count,
            starting_transaction_index,
            bank_id,
        } = expected;
        assert_eq!(self.slot(), *slot, "slot");
        assert_eq!(self.index(), *index, "index");
        assert_eq!(self.num_hashes(), *num_hashes, "num_hashes");
        assert_eq!(self.hash(), hash.as_slice(), "hash");
        assert_eq!(
            self.executed_transaction_count(),
            *executed_transaction_count
        );
        assert_eq!(
            self.starting_transaction_index(),
            *starting_transaction_index
        );
        assert_eq!(self.bank_id(), *bank_id, "bank_id");
    }
}

impl<B: AsRef<[u8]>> Matches<ys::SubscribeUpdateEntryUpdateParent>
    for lens::SubscribeUpdateEntryUpdateParent<B>
{
    fn assert_matches(&self, expected: &ys::SubscribeUpdateEntryUpdateParent) {
        let ys::SubscribeUpdateEntryUpdateParent {
            slot,
            cleared_bank_id,
            parent_slot,
            parent_block_id,
        } = expected;
        assert_eq!(self.slot(), *slot, "slot");
        assert_eq!(self.cleared_bank_id(), *cleared_bank_id, "cleared_bank_id");
        assert_eq!(self.parent_slot(), *parent_slot, "parent_slot");
        assert_eq!(self.parent_block_id(), parent_block_id.as_slice());
    }
}

impl<B: AsRef<[u8]>> Matches<ys::SubscribeUpdatePing> for lens::SubscribeUpdatePing<B> {
    fn assert_matches(&self, expected: &ys::SubscribeUpdatePing) {
        let ys::SubscribeUpdatePing {} = expected;
    }
}

impl<B: AsRef<[u8]>> Matches<ys::SubscribeUpdatePong> for lens::SubscribeUpdatePong<B> {
    fn assert_matches(&self, expected: &ys::SubscribeUpdatePong) {
        let ys::SubscribeUpdatePong { id } = expected;
        assert_eq!(self.id(), *id, "id");
    }
}

impl<B: AsRef<[u8]>> Matches<ys::SubscribeUpdateDeshredUpdateParent>
    for lens::SubscribeUpdateDeshredUpdateParent<B>
{
    fn assert_matches(&self, expected: &ys::SubscribeUpdateDeshredUpdateParent) {
        let ys::SubscribeUpdateDeshredUpdateParent {
            slot,
            update_parent_fec_set_index,
            parent_slot,
            parent_block_id,
        } = expected;
        assert_eq!(self.slot(), *slot, "slot");
        assert_eq!(
            self.update_parent_fec_set_index(),
            *update_parent_fec_set_index
        );
        assert_eq!(self.parent_slot(), *parent_slot, "parent_slot");
        assert_eq!(self.parent_block_id(), parent_block_id.as_slice());
    }
}

impl<B: AsRef<[u8]>> Matches<ys::SubscribeUpdateDeshredTransaction>
    for lens::SubscribeUpdateDeshredTransaction<B>
{
    fn assert_matches(&self, expected: &ys::SubscribeUpdateDeshredTransaction) {
        let ys::SubscribeUpdateDeshredTransaction { transaction, slot } = expected;
        assert_opt(self.transaction(), transaction.as_ref(), "transaction");
        assert_eq!(self.slot(), *slot, "slot");
    }
}

impl<B: AsRef<[u8]>> Matches<ys::SubscribeUpdateDeshredTransactionInfo>
    for lens::SubscribeUpdateDeshredTransactionInfo<B>
{
    fn assert_matches(&self, expected: &ys::SubscribeUpdateDeshredTransactionInfo) {
        let ys::SubscribeUpdateDeshredTransactionInfo {
            signature,
            is_vote,
            transaction,
            loaded_writable_addresses,
            loaded_readonly_addresses,
            completed_data_set_starting_shred_index,
            completed_data_set_ending_shred_index_exclusive,
        } = expected;
        assert_eq!(self.signature(), signature.as_slice(), "signature");
        assert_eq!(self.is_vote(), *is_vote, "is_vote");
        assert_opt(self.transaction(), transaction.as_ref(), "transaction");
        assert_bytes_list(
            self.loaded_writable_addresses(),
            loaded_writable_addresses,
            "loaded_writable_addresses",
        );
        assert_bytes_list(
            self.loaded_readonly_addresses(),
            loaded_readonly_addresses,
            "loaded_readonly_addresses",
        );
        assert_eq!(
            self.completed_data_set_starting_shred_index(),
            *completed_data_set_starting_shred_index
        );
        assert_eq!(
            self.completed_data_set_ending_shred_index_exclusive(),
            *completed_data_set_ending_shred_index_exclusive
        );
    }
}

impl<B: AsRef<[u8]>> Matches<ys::GossipTopology> for lens::GossipTopology<B> {
    fn assert_matches(&self, expected: &ys::GossipTopology) {
        let ys::GossipTopology { nodes } = expected;
        assert_all(self.nodes(), nodes, "nodes");
    }
}

impl<B: AsRef<[u8]>> Matches<ys::SubscribeUpdateContactInfoNode>
    for lens::SubscribeUpdateContactInfoNode<B>
{
    fn assert_matches(&self, expected: &ys::SubscribeUpdateContactInfoNode) {
        let ys::SubscribeUpdateContactInfoNode {
            pubkey,
            wallclock,
            outset,
            shred_version,
            version_major,
            version_minor,
            version_patch,
            version_commit,
            version_feature_set,
            version_client_id,
            gossip,
            tpu_quic,
            tpu_forwards_quic,
            tpu_vote_udp,
            tpu_vote_quic,
            tvu_udp,
            tvu_quic,
            serve_repair_udp,
            serve_repair_quic,
            rpc,
            rpc_pubsub,
            alpenglow,
        } = expected;
        assert_eq!(self.pubkey(), pubkey.as_slice(), "pubkey");
        assert_eq!(self.wallclock(), *wallclock, "wallclock");
        assert_eq!(self.outset(), *outset, "outset");
        assert_eq!(self.shred_version(), *shred_version, "shred_version");
        assert_eq!(self.version_major(), *version_major, "version_major");
        assert_eq!(self.version_minor(), *version_minor, "version_minor");
        assert_eq!(self.version_patch(), *version_patch, "version_patch");
        assert_eq!(self.version_commit(), *version_commit, "version_commit");
        assert_eq!(self.version_feature_set(), *version_feature_set);
        assert_eq!(self.version_client_id(), *version_client_id);
        assert_opt_str(self.gossip(), gossip, "gossip");
        assert_opt_str(self.tpu_quic(), tpu_quic, "tpu_quic");
        assert_opt_str(
            self.tpu_forwards_quic(),
            tpu_forwards_quic,
            "tpu_forwards_quic",
        );
        assert_opt_str(self.tpu_vote_udp(), tpu_vote_udp, "tpu_vote_udp");
        assert_opt_str(self.tpu_vote_quic(), tpu_vote_quic, "tpu_vote_quic");
        assert_opt_str(self.tvu_udp(), tvu_udp, "tvu_udp");
        assert_opt_str(self.tvu_quic(), tvu_quic, "tvu_quic");
        assert_opt_str(
            self.serve_repair_udp(),
            serve_repair_udp,
            "serve_repair_udp",
        );
        assert_opt_str(
            self.serve_repair_quic(),
            serve_repair_quic,
            "serve_repair_quic",
        );
        assert_opt_str(self.rpc(), rpc, "rpc");
        assert_opt_str(self.rpc_pubsub(), rpc_pubsub, "rpc_pubsub");
        assert_opt_str(self.alpenglow(), alpenglow, "alpenglow");
    }
}

impl<B: AsRef<[u8]>> Matches<ys::SubscribeUpdateContactInfoRemoved>
    for lens::SubscribeUpdateContactInfoRemoved<B>
{
    fn assert_matches(&self, expected: &ys::SubscribeUpdateContactInfoRemoved) {
        let ys::SubscribeUpdateContactInfoRemoved { pubkey } = expected;
        assert_eq!(self.pubkey(), pubkey.as_slice(), "pubkey");
    }
}

// ---------------------------------------------------------------------------------------
// Fixtures: every field set, with optional fields alternating between set and unset
// across elements so both presence states are exercised.
// ---------------------------------------------------------------------------------------

fn pubkey(n: u8) -> Vec<u8> {
    vec![n; 32]
}

fn signature(n: u8) -> Vec<u8> {
    vec![n; 64]
}

fn reward(n: u8) -> ys_st::Reward {
    ys_st::Reward {
        pubkey: format!("reward-pubkey-{n}"),
        lamports: -1_000 * i64::from(n),
        post_balance: 5_000_000_000 + u64::from(n),
        reward_type: ys_st::RewardType::Voting as i32,
        commission: format!("{n}"),
        commission_bps: format!("{}", u32::from(n) * 100),
    }
}

fn num_partitions() -> ys_st::NumPartitions {
    ys_st::NumPartitions { num_partitions: 4 }
}

fn rewards() -> ys_st::Rewards {
    ys_st::Rewards {
        rewards: vec![reward(1), reward(2), reward(3)],
        num_partitions: Some(num_partitions()),
    }
}

fn token_balance(n: u8) -> ys_st::TokenBalance {
    ys_st::TokenBalance {
        account_index: u32::from(n),
        mint: format!("mint-{n}"),
        ui_token_amount: Some(ys_st::UiTokenAmount {
            ui_amount: 1.5 * f64::from(n),
            decimals: 6,
            amount: format!("{}", 1_500_000 * u64::from(n)),
            ui_amount_string: format!("{}.5", n),
        }),
        owner: format!("owner-{n}"),
        program_id: "token-program".to_owned(),
    }
}

fn transaction(n: u8) -> ys_st::Transaction {
    // Odd transactions are V1 with an inline config; even ones are legacy.
    let config = (!n.is_multiple_of(2)).then_some(ys_st::TransactionConfig {
        priority_fee: Some(5_000),
        compute_unit_limit: Some(200_000),
        loaded_accounts_data_size_limit: None,
        heap_size: Some(256 * 1024),
    });
    ys_st::Transaction {
        signatures: vec![signature(n), signature(n.wrapping_add(100))],
        message: Some(ys_st::Message {
            header: Some(ys_st::MessageHeader {
                num_required_signatures: 2,
                num_readonly_signed_accounts: 0,
                num_readonly_unsigned_accounts: 1,
            }),
            account_keys: vec![pubkey(n), pubkey(n + 1), pubkey(n + 2)],
            recent_blockhash: pubkey(0xbb),
            instructions: vec![
                ys_st::CompiledInstruction {
                    program_id_index: 2,
                    accounts: vec![0, 1],
                    data: vec![2, 0, 0, 0, n],
                },
                ys_st::CompiledInstruction {
                    program_id_index: 2,
                    accounts: vec![],
                    data: vec![],
                },
            ],
            versioned: true,
            address_table_lookups: vec![ys_st::MessageAddressTableLookup {
                account_key: pubkey(0xa1),
                writable_indexes: vec![0, 3],
                readonly_indexes: vec![1],
            }],
            config,
        }),
    }
}

fn meta(n: u8) -> ys_st::TransactionStatusMeta {
    let failed = n.is_multiple_of(3);
    ys_st::TransactionStatusMeta {
        err: failed.then(|| ys_st::TransactionError {
            err: vec![8, 0, 0, 0, n],
        }),
        fee: 5_000 + u64::from(n),
        pre_balances: vec![10_000_000, 0, u64::MAX],
        post_balances: vec![9_995_000, 0, u64::MAX],
        inner_instructions: vec![ys_st::InnerInstructions {
            index: 0,
            instructions: vec![
                ys_st::InnerInstruction {
                    program_id_index: 1,
                    accounts: vec![0, 2],
                    data: vec![3; 12],
                    stack_height: Some(2),
                },
                ys_st::InnerInstruction {
                    program_id_index: 1,
                    accounts: vec![1],
                    data: vec![],
                    stack_height: None,
                },
            ],
        }],
        inner_instructions_none: false,
        log_messages: vec![
            format!("Program log: tx {n}"),
            "Program success".to_owned(),
            String::new(),
        ],
        log_messages_none: false,
        pre_token_balances: vec![token_balance(n)],
        post_token_balances: vec![token_balance(n), token_balance(n + 1)],
        rewards: vec![reward(n)],
        loaded_writable_addresses: vec![pubkey(0xc1)],
        loaded_readonly_addresses: vec![pubkey(0xc2), pubkey(0xc3)],
        return_data: (!failed).then(|| ys_st::ReturnData {
            program_id: pubkey(0xd1),
            data: vec![n; 8],
        }),
        return_data_none: failed,
        compute_units_consumed: (!failed).then_some(1_400 + u64::from(n)),
        cost_units: Some(3_000),
    }
}

fn transaction_info(n: u8) -> ys::SubscribeUpdateTransactionInfo {
    ys::SubscribeUpdateTransactionInfo {
        signature: signature(n),
        is_vote: n.is_multiple_of(2),
        transaction: Some(transaction(n)),
        meta: Some(meta(n)),
        index: u64::from(n),
    }
}

fn account_info(n: u8) -> ys::SubscribeUpdateAccountInfo {
    ys::SubscribeUpdateAccountInfo {
        pubkey: pubkey(n),
        lamports: 1_000_000 * u64::from(n),
        owner: pubkey(0xee),
        executable: n % 2 == 1,
        rent_epoch: u64::MAX,
        data: (0..=255).cycle().take(1_000 + usize::from(n)).collect(),
        write_version: 42 + u64::from(n),
        txn_signature: n.is_multiple_of(2).then(|| signature(n)),
    }
}

fn entry(n: u8) -> ys::SubscribeUpdateEntry {
    ys::SubscribeUpdateEntry {
        slot: 300_000_000,
        index: u64::from(n),
        num_hashes: 12_500,
        hash: pubkey(0xf0 + n),
        executed_transaction_count: 2,
        starting_transaction_index: 2 * u64::from(n),
        bank_id: 7,
    }
}

fn contact_info(n: u8, all_sockets: bool) -> ys::SubscribeUpdateContactInfoNode {
    let socket = |port: u16| all_sockets.then(|| format!("10.0.0.{n}:{port}"));
    ys::SubscribeUpdateContactInfoNode {
        pubkey: pubkey(n),
        wallclock: 1_700_000_000_000,
        outset: 1_690_000_000_000_000,
        shred_version: 50_093,
        version_major: 2,
        version_minor: 3,
        version_patch: 4,
        version_commit: 0xdead_beef,
        version_feature_set: 0x1234_5678,
        version_client_id: 3,
        gossip: Some(format!("10.0.0.{n}:8001")),
        tpu_quic: socket(8009),
        tpu_forwards_quic: socket(8010),
        tpu_vote_udp: socket(8005),
        tpu_vote_quic: socket(8011),
        tvu_udp: socket(8000),
        tvu_quic: socket(8012),
        serve_repair_udp: socket(8008),
        serve_repair_quic: socket(8013),
        rpc: socket(8899),
        rpc_pubsub: socket(8900),
        alpenglow: socket(8020),
    }
}

// ---------------------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------------------

#[test]
fn subscribe_update_block() {
    round_trip!(
        lens::SubscribeUpdateBlock<&[u8]>,
        ys::SubscribeUpdateBlock {
            slot: 300_000_000,
            blockhash: "blockhash-300000000".to_owned(),
            rewards: Some(rewards()),
            block_time: Some(ys_st::UnixTimestamp {
                timestamp: 1_700_000_000,
            }),
            block_height: Some(ys_st::BlockHeight {
                block_height: 280_000_000,
            }),
            parent_slot: 299_999_999,
            parent_blockhash: "blockhash-299999999".to_owned(),
            executed_transaction_count: 6,
            transactions: (1..=6).map(transaction_info).collect(),
            updated_account_count: 4,
            accounts: (1..=4).map(account_info).collect(),
            entries_count: 3,
            entries: (0..3).map(entry).collect(),
            bank_id: 7,
        }
    );
}

#[test]
fn subscribe_update_block_meta() {
    round_trip!(
        lens::SubscribeUpdateBlockMeta<&[u8]>,
        ys::SubscribeUpdateBlockMeta {
            slot: 300_000_000,
            blockhash: "blockhash-300000000".to_owned(),
            rewards: Some(rewards()),
            block_time: Some(ys_st::UnixTimestamp { timestamp: -1 }),
            block_height: None,
            parent_slot: 299_999_999,
            parent_blockhash: "blockhash-299999999".to_owned(),
            executed_transaction_count: 1_234,
            entries_count: 64,
            bank_id: 7,
        }
    );
}

#[test]
fn subscribe_update_transaction() {
    for n in 1..=3 {
        round_trip!(
            lens::SubscribeUpdateTransaction<&[u8]>,
            ys::SubscribeUpdateTransaction {
                transaction: Some(transaction_info(n)),
                slot: 300_000_000,
                bank_id: u64::from(n),
            }
        );
    }
}

#[test]
fn subscribe_update_transaction_status() {
    for err in [None, Some(ys_st::TransactionError { err: vec![1, 2, 3] })] {
        round_trip!(
            lens::SubscribeUpdateTransactionStatus<&[u8]>,
            ys::SubscribeUpdateTransactionStatus {
                slot: 300_000_000,
                signature: signature(9),
                is_vote: true,
                index: 17,
                err,
                bank_id: 7,
            }
        );
    }
}

#[test]
fn subscribe_update_account() {
    // A startup account has no bank; a live update does.
    for (n, is_startup) in [(1, true), (2, false)] {
        round_trip!(
            lens::SubscribeUpdateAccount<&[u8]>,
            ys::SubscribeUpdateAccount {
                account: Some(account_info(n)),
                slot: 300_000_000,
                is_startup,
                bank_id: (!is_startup).then_some(7),
            }
        );
    }
}

#[test]
fn subscribe_update_slot() {
    round_trip!(
        lens::SubscribeUpdateSlot<&[u8]>,
        ys::SubscribeUpdateSlot {
            slot: 300_000_000,
            parent: Some(299_999_999),
            status: ys::SlotStatus::SlotDead as i32,
            dead_error: Some("shred insert failed".to_owned()),
            bank_id: Some(7),
        }
    );
    round_trip!(
        lens::SubscribeUpdateSlot<&[u8]>,
        ys::SubscribeUpdateSlot {
            slot: 300_000_001,
            parent: None,
            status: ys::SlotStatus::SlotFirstShredReceived as i32,
            dead_error: None,
            bank_id: None,
        }
    );
}

#[test]
fn subscribe_update_block_footer() {
    round_trip!(
        lens::SubscribeUpdateBlockFooter<&[u8]>,
        ys::SubscribeUpdateBlockFooter {
            slot: 300_000_000,
            bank_id: 7,
            bank_hash: pubkey(0x77),
            block_producer_time_nanos: 1_700_000_000_123_456_789,
            block_user_agent: b"agave/3.0.0".to_vec(),
            block_final_cert: Some(vec![1; 96]),
            skip_reward_cert: None,
            notar_reward_cert: Some(Vec::new()),
        }
    );
}

#[test]
fn subscribe_update_entry_and_update_parent() {
    round_trip!(lens::SubscribeUpdateEntry<&[u8]>, entry(5));
    round_trip!(
        lens::SubscribeUpdateEntryUpdateParent<&[u8]>,
        ys::SubscribeUpdateEntryUpdateParent {
            slot: 300_000_000,
            cleared_bank_id: 6,
            parent_slot: 299_999_998,
            parent_block_id: pubkey(0x42),
        }
    );
}

#[test]
fn subscribe_update_ping_and_pong() {
    round_trip!(lens::SubscribeUpdatePing<&[u8]>, ys::SubscribeUpdatePing {});
    round_trip!(
        lens::SubscribeUpdatePong<&[u8]>,
        ys::SubscribeUpdatePong { id: -3 }
    );
}

#[test]
fn subscribe_update_deshred() {
    round_trip!(
        lens::SubscribeUpdateDeshredTransaction<&[u8]>,
        ys::SubscribeUpdateDeshredTransaction {
            transaction: Some(ys::SubscribeUpdateDeshredTransactionInfo {
                signature: signature(4),
                is_vote: false,
                transaction: Some(transaction(3)),
                loaded_writable_addresses: vec![pubkey(1), pubkey(2)],
                loaded_readonly_addresses: vec![],
                completed_data_set_starting_shred_index: 32,
                completed_data_set_ending_shred_index_exclusive: 64,
            }),
            slot: 300_000_000,
        }
    );
    round_trip!(
        lens::SubscribeUpdateDeshredUpdateParent<&[u8]>,
        ys::SubscribeUpdateDeshredUpdateParent {
            slot: 300_000_000,
            update_parent_fec_set_index: 96,
            parent_slot: 299_999_997,
            parent_block_id: pubkey(0x43),
        }
    );
}

#[test]
fn gossip_topology_and_contact_info() {
    round_trip!(
        lens::GossipTopology<&[u8]>,
        ys::GossipTopology {
            nodes: vec![
                contact_info(1, true),
                contact_info(2, false),
                contact_info(3, true)
            ],
        }
    );
    round_trip!(
        lens::SubscribeUpdateContactInfoRemoved<&[u8]>,
        ys::SubscribeUpdateContactInfoRemoved { pubkey: pubkey(9) }
    );
}

#[test]
fn confirmed_block() {
    round_trip!(
        lens_st::ConfirmedBlock<&[u8]>,
        ys_st::ConfirmedBlock {
            previous_blockhash: "blockhash-299999999".to_owned(),
            blockhash: "blockhash-300000000".to_owned(),
            parent_slot: 299_999_999,
            transactions: (1..=4)
                .map(|n| ys_st::ConfirmedTransaction {
                    transaction: Some(transaction(n)),
                    meta: (n != 4).then(|| meta(n)),
                })
                .collect(),
            rewards: vec![reward(7), reward(8)],
            block_time: Some(ys_st::UnixTimestamp {
                timestamp: 1_700_000_000,
            }),
            block_height: Some(ys_st::BlockHeight {
                block_height: 280_000_000,
            }),
            num_partitions: Some(num_partitions()),
        }
    );
}

#[test]
fn empty_messages_decode_to_defaults() {
    round_trip!(
        lens::SubscribeUpdateBlock<&[u8]>,
        ys::SubscribeUpdateBlock::default()
    );
    round_trip!(
        lens::SubscribeUpdateAccount<&[u8]>,
        ys::SubscribeUpdateAccount::default()
    );
    round_trip!(
        lens_st::ConfirmedBlock<&[u8]>,
        ys_st::ConfirmedBlock::default()
    );
    round_trip!(
        lens::SubscribeUpdateContactInfoNode<&[u8]>,
        ys::SubscribeUpdateContactInfoNode::default()
    );
}

#[test]
fn subscribe_update_envelope_every_variant() {
    let block_meta = ys::SubscribeUpdateBlockMeta {
        slot: 300_000_000,
        blockhash: "blockhash".to_owned(),
        rewards: Some(rewards()),
        block_time: None,
        block_height: None,
        parent_slot: 299_999_999,
        parent_blockhash: String::new(),
        executed_transaction_count: 1,
        entries_count: 1,
        bank_id: 7,
    };
    let variants = [
        UpdateOneof::Account(ys::SubscribeUpdateAccount {
            account: Some(account_info(1)),
            slot: 1,
            is_startup: false,
            bank_id: Some(7),
        }),
        UpdateOneof::Slot(ys::SubscribeUpdateSlot {
            slot: 1,
            parent: Some(0),
            status: ys::SlotStatus::SlotConfirmed as i32,
            dead_error: None,
            bank_id: None,
        }),
        UpdateOneof::Transaction(ys::SubscribeUpdateTransaction {
            transaction: Some(transaction_info(1)),
            slot: 1,
            bank_id: 7,
        }),
        UpdateOneof::TransactionStatus(ys::SubscribeUpdateTransactionStatus {
            slot: 1,
            signature: signature(1),
            is_vote: false,
            index: 3,
            err: None,
            bank_id: 7,
        }),
        UpdateOneof::Block(ys::SubscribeUpdateBlock {
            slot: 1,
            transactions: vec![transaction_info(2)],
            accounts: vec![account_info(2)],
            entries: vec![entry(0)],
            ..Default::default()
        }),
        UpdateOneof::Ping(ys::SubscribeUpdatePing {}),
        UpdateOneof::Pong(ys::SubscribeUpdatePong { id: 1 }),
        UpdateOneof::BlockMeta(block_meta),
        UpdateOneof::Entry(entry(3)),
        UpdateOneof::BlockFooter(ys::SubscribeUpdateBlockFooter {
            slot: 1,
            bank_id: 7,
            bank_hash: pubkey(1),
            block_producer_time_nanos: 1,
            block_user_agent: b"agave".to_vec(),
            block_final_cert: None,
            skip_reward_cert: None,
            notar_reward_cert: None,
        }),
        UpdateOneof::EntryUpdateParent(ys::SubscribeUpdateEntryUpdateParent {
            slot: 1,
            cleared_bank_id: 6,
            parent_slot: 0,
            parent_block_id: pubkey(2),
        }),
    ];
    for update in variants {
        round_trip!(
            lens::SubscribeUpdate<&[u8]>,
            ys::SubscribeUpdate {
                filters: vec!["client-filter".to_owned(), "other".to_owned()],
                update_oneof: Some(update),
                created_at: Some(prost_types::Timestamp {
                    seconds: 1_700_000_000,
                    nanos: 123_456_789,
                }),
            }
        );
    }
    round_trip!(lens::SubscribeUpdate<&[u8]>, ys::SubscribeUpdate::default());
}

fn created_at() -> Option<prost_types::Timestamp> {
    Some(prost_types::Timestamp {
        seconds: 1_700_000_000,
        nanos: 123_456_789,
    })
}

#[test]
fn subscribe_update_deshred_envelope_every_variant() {
    use ys::subscribe_update_deshred::UpdateOneof as Deshred;
    let variants = [
        Deshred::DeshredTransaction(ys::SubscribeUpdateDeshredTransaction {
            transaction: Some(ys::SubscribeUpdateDeshredTransactionInfo {
                signature: signature(1),
                is_vote: false,
                transaction: Some(transaction(1)),
                loaded_writable_addresses: vec![pubkey(1)],
                loaded_readonly_addresses: vec![pubkey(2)],
                completed_data_set_starting_shred_index: 0,
                completed_data_set_ending_shred_index_exclusive: 32,
            }),
            slot: 1,
        }),
        Deshred::Ping(ys::SubscribeUpdatePing {}),
        Deshred::Pong(ys::SubscribeUpdatePong { id: 2 }),
        Deshred::Slot(ys::SubscribeUpdateSlot {
            slot: 1,
            parent: None,
            status: ys::SlotStatus::SlotCreatedBank as i32,
            dead_error: None,
            bank_id: Some(7),
        }),
        Deshred::DeshredUpdateParent(ys::SubscribeUpdateDeshredUpdateParent {
            slot: 1,
            update_parent_fec_set_index: 64,
            parent_slot: 0,
            parent_block_id: pubkey(3),
        }),
    ];
    for update in variants {
        round_trip!(
            lens::SubscribeUpdateDeshred<&[u8]>,
            ys::SubscribeUpdateDeshred {
                filters: vec!["deshred".to_owned()],
                update_oneof: Some(update),
                created_at: created_at(),
            }
        );
    }
    round_trip!(
        lens::SubscribeUpdateDeshred<&[u8]>,
        ys::SubscribeUpdateDeshred::default()
    );
}

#[test]
fn subscribe_update_gossip_envelope_every_variant() {
    use ys::subscribe_update_gossip::UpdateOneof as Gossip;
    let variants = [
        Gossip::Node(contact_info(1, true)),
        Gossip::Removed(ys::SubscribeUpdateContactInfoRemoved { pubkey: pubkey(4) }),
        Gossip::Ping(ys::SubscribeUpdatePing {}),
        Gossip::Snapshot(ys::GossipTopology {
            nodes: vec![contact_info(2, false), contact_info(3, true)],
        }),
    ];
    for (seq, update) in variants.into_iter().enumerate() {
        round_trip!(
            lens::SubscribeUpdateGossip<&[u8]>,
            ys::SubscribeUpdateGossip {
                update_oneof: Some(update),
                created_at: created_at(),
                seq: seq as u64,
            }
        );
    }
    round_trip!(
        lens::SubscribeUpdateGossip<&[u8]>,
        ys::SubscribeUpdateGossip::default()
    );
}

fn cuckoo() -> ys::CuckooFilter {
    ys::CuckooFilter {
        data: vec![0xab; 64],
        bucket_count: 16,
        entries_per_bucket: 4,
        fingerprint_bits: 12,
        hash_seed: 0x5eed,
        hash_algorithm: ys::CuckooHashAlgorithm::SipHash as i32,
    }
}

fn named<T>(entries: impl IntoIterator<Item = (&'static str, T)>) -> HashMap<String, T> {
    entries
        .into_iter()
        .map(|(name, value)| (name.to_owned(), value))
        .collect()
}

#[test]
fn subscribe_request_with_every_filter() {
    use ys::subscribe_request_filter_accounts_filter::Filter;
    use ys::subscribe_request_filter_accounts_filter_lamports::Cmp;
    use ys::subscribe_request_filter_accounts_filter_memcmp::Data;

    let memcmp = |offset, data| ys::SubscribeRequestFilterAccountsFilter {
        filter: Some(Filter::Memcmp(
            ys::SubscribeRequestFilterAccountsFilterMemcmp {
                offset,
                data: Some(data),
            },
        )),
    };
    let lamports = |cmp| ys::SubscribeRequestFilterAccountsFilter {
        filter: Some(Filter::Lamports(
            ys::SubscribeRequestFilterAccountsFilterLamports { cmp: Some(cmp) },
        )),
    };
    let transactions = |vote, token_accounts: Option<ys::TokenAccountExpansionControlFlag>| {
        ys::SubscribeRequestFilterTransactions {
            vote,
            failed: Some(false),
            signature: None,
            account_include: vec!["include".to_owned()],
            account_exclude: vec![],
            account_required: vec!["required-a".to_owned(), "required-b".to_owned()],
            cuckoo_account_include: vote.is_some().then(cuckoo),
            token_accounts: token_accounts.map(|t| t as i32),
        }
    };

    round_trip!(
        lens::SubscribeRequest<&[u8]>,
        ys::SubscribeRequest {
            accounts: named([
                (
                    "tokens",
                    ys::SubscribeRequestFilterAccounts {
                        account: vec![],
                        owner: vec!["TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA".to_owned()],
                        filters: vec![
                            ys::SubscribeRequestFilterAccountsFilter {
                                filter: Some(Filter::Datasize(165)),
                            },
                            ys::SubscribeRequestFilterAccountsFilter {
                                filter: Some(Filter::TokenAccountState(true)),
                            },
                            memcmp(32, Data::Bytes(pubkey(7))),
                            memcmp(
                                0,
                                Data::Base58("11111111111111111111111111111111".to_owned())
                            ),
                            memcmp(8, Data::Base64("AAEC".to_owned())),
                            lamports(Cmp::Gt(1_000_000)),
                            lamports(Cmp::Eq(0)),
                            ys::SubscribeRequestFilterAccountsFilter { filter: None },
                        ],
                        nonempty_txn_signature: Some(true),
                        cuckoo_accounts_filter: Some(cuckoo()),
                    },
                ),
                (
                    "",
                    ys::SubscribeRequestFilterAccounts {
                        account: vec!["a".to_owned(), "b".to_owned()],
                        ..Default::default()
                    },
                ),
            ]),
            slots: named([(
                "slots",
                ys::SubscribeRequestFilterSlots {
                    filter_by_commitment: Some(true),
                    interslot_updates: None,
                },
            )]),
            transactions: named([
                (
                    "non-vote",
                    transactions(
                        Some(false),
                        Some(ys::TokenAccountExpansionControlFlag::BalanceChanged),
                    ),
                ),
                ("all", transactions(None, None)),
            ]),
            transactions_status: named([(
                "status",
                transactions(Some(true), Some(ys::TokenAccountExpansionControlFlag::All)),
            )]),
            blocks: named([(
                "blocks",
                ys::SubscribeRequestFilterBlocks {
                    account_include: vec!["vote111".to_owned()],
                    include_transactions: Some(true),
                    include_accounts: Some(false),
                    include_entries: None,
                    cuckoo_account_include: Some(cuckoo()),
                },
            )]),
            blocks_meta: named([("meta", ys::SubscribeRequestFilterBlocksMeta {})]),
            entry: named([(
                "entries",
                ys::SubscribeRequestFilterEntry {
                    include_update_parent: Some(true),
                },
            )]),
            commitment: Some(ys::CommitmentLevel::Confirmed as i32),
            accounts_data_slice: vec![
                ys::SubscribeRequestAccountsDataSlice {
                    offset: 0,
                    length: 32,
                },
                ys::SubscribeRequestAccountsDataSlice {
                    offset: 64,
                    length: 8,
                },
            ],
            ping: Some(ys::SubscribeRequestPing { id: 3 }),
            from_slot: Some(300_000_000),
            block_footer: named([(
                "footer",
                ys::SubscribeRequestFilterBlockFooter {
                    include_certificates: Some(false),
                },
            )]),
        }
    );
    round_trip!(
        lens::SubscribeRequest<&[u8]>,
        ys::SubscribeRequest::default()
    );
}

#[test]
fn enum_fields_decode_to_their_variants() {
    let bytes = ys::SubscribeUpdateSlot {
        slot: 1,
        status: ys::SlotStatus::SlotDead as i32,
        ..Default::default()
    }
    .encode_to_vec();
    let slot = lens::SubscribeUpdateSlot::parse(bytes.as_slice()).unwrap();
    assert_eq!(slot.status(), lens::SlotStatus::SlotDead);

    let bytes = ys_st::Reward {
        reward_type: ys_st::RewardType::VatDebit as i32,
        ..Default::default()
    }
    .encode_to_vec();
    let reward = lens_st::Reward::parse(bytes.as_slice()).unwrap();
    assert_eq!(reward.reward_type(), lens_st::RewardType::VatDebit);

    // A status newer than this schema is kept, not folded into a known one.
    let bytes = ys::SubscribeUpdateSlot {
        status: 99,
        ..Default::default()
    }
    .encode_to_vec();
    let slot = lens::SubscribeUpdateSlot::parse(bytes.as_slice()).unwrap();
    assert_eq!(slot.status(), lens::SlotStatus::Unknown(99));
}
