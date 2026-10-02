//! The generated views parse what `yellowstone-grpc-proto`'s `prost` types encode.

use prost::Message as _;
use yellowstone_grpc_proto::geyser as ys;
use yellowstone_grpc_protoview::geyser::SubscribeUpdate;
use yellowstone_grpc_protoview::geyser::subscribe_update::UpdateOneof;

#[test]
fn slot_update_round_trips() {
    let encoded = ys::SubscribeUpdate {
        filters: vec!["slots".to_string()],
        update_oneof: Some(ys::subscribe_update::UpdateOneof::Slot(
            ys::SubscribeUpdateSlot {
                slot: 42,
                parent: Some(41),
                status: ys::SlotStatus::SlotConfirmed as i32,
                dead_error: None,
                ..Default::default()
            },
        )),
        created_at: None,
    }
    .encode_to_vec();

    let update = SubscribeUpdate::parse(encoded.as_slice()).expect("valid encoding");
    let filters: Vec<_> = update.filters().map(Result::unwrap).collect();
    assert_eq!(filters, ["slots"]);
    match update.update_oneof() {
        Some(UpdateOneof::Slot(slot)) => {
            assert_eq!(slot.slot(), 42);
            assert_eq!(slot.parent(), Some(41));
        }
        _ => panic!("expected a slot update"),
    }
}

#[test]
fn truncated_input_is_rejected() {
    let mut encoded = ys::SubscribeUpdate {
        filters: vec!["slots".to_string()],
        update_oneof: None,
        created_at: None,
    }
    .encode_to_vec();
    encoded.pop();

    assert!(SubscribeUpdate::parse(encoded.as_slice()).is_err());
}
