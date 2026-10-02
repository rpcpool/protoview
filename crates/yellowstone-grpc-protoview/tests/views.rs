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

#[test]
fn into_inner_returns_the_original_container() {
    let slot = ys::SubscribeUpdateSlot {
        slot: 42,
        parent: Some(41),
        status: ys::SlotStatus::SlotConfirmed as i32,
        dead_error: None,
        ..Default::default()
    };
    let encoded = bytes::Bytes::from(
        ys::SubscribeUpdate {
            filters: vec!["slots".to_string()],
            update_oneof: Some(ys::subscribe_update::UpdateOneof::Slot(slot.clone())),
            created_at: None,
        }
        .encode_to_vec(),
    );
    let ptr = encoded.as_ptr();

    let update = SubscribeUpdate::parse(encoded.clone()).expect("valid encoding");

    // A nested view's container is that message's own bytes.
    match update.update_oneof() {
        Some(UpdateOneof::Slot(view)) => assert_eq!(view.into_inner(), slot.encode_to_vec()),
        _ => panic!("expected a slot update"),
    }

    // The root view hands back the very buffer it was built over, uncopied.
    let inner = update.into_inner();
    assert_eq!(inner, encoded);
    assert_eq!(inner.as_ptr(), ptr);
}

#[test]
fn owned_oneof_member_can_move_to_another_thread() {
    use yellowstone_grpc_protoview::geyser::subscribe_update::UpdateOneofOwned;

    let encoded = bytes::Bytes::from(
        ys::SubscribeUpdate {
            filters: vec![],
            update_oneof: Some(ys::subscribe_update::UpdateOneof::Slot(
                ys::SubscribeUpdateSlot {
                    slot: 7,
                    ..Default::default()
                },
            )),
            created_at: None,
        }
        .encode_to_vec(),
    );

    let owned = SubscribeUpdate::parse(encoded)
        .expect("valid encoding")
        .update_oneof_owned();

    // The root view is dropped; the member is `'static` and owns its slice.
    let slot = std::thread::spawn(move || match owned {
        Some(UpdateOneofOwned::Slot(slot)) => slot.slot(),
        _ => panic!("expected a slot update"),
    })
    .join()
    .unwrap();
    assert_eq!(slot, 7);
}
