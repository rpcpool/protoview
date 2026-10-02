//! The README's usage example, compiled and run so it cannot drift from the real API.
//! `print_order` is copied verbatim into README.md; keep the two in sync.

use std::collections::HashMap;

use prost::Message as _;
use protoview::DecodeError;

use crate::shop::orders::order::Delivery;
use crate::shop::orders::{Order, Status};

// --- README: start ---------------------------------------------------------------------
fn print_order(bytes: &[u8]) -> Result<(), DecodeError> {
    // One pass over the bytes validates the whole message tree. Nothing is copied or
    // allocated; `order` is a view over `bytes`.
    let order = Order::parse(bytes)?;

    // Scalars come back by value, with the proto3 default when absent.
    let id: u64 = order.id();
    // A `fixed_bytes` field is a `[u8; N]`, length-checked by `parse`.
    let customer: [u8; 16] = order.customer_id();
    println!("order {id} for customer {customer:02x?}");

    // Enums keep values this schema does not know.
    match order.status() {
        Status::Unspecified => println!("status: not set"),
        Status::Pending => println!("status: pending"),
        Status::Shipped => println!("status: shipped"),
        Status::Unknown(value) => println!("status: {value} (newer than this schema)"),
    }

    // Repeated fields are iterators, in wire order.
    for item in order.items() {
        // Strings are UTF-8-checked when read, so they return a `Result`.
        let sku = item.sku().unwrap_or("<invalid utf-8>");
        // A nested message is an `Option`: there is no default to fall back to.
        let price = item
            .price()
            .map(|money| {
                format!(
                    "{}.{:09} {}",
                    money.units(),
                    money.nanos(),
                    money.currency().unwrap_or("?")
                )
            })
            .unwrap_or_else(|| "no price".to_string());
        println!("  {} x {sku} at {price}", item.quantity());
    }

    // Maps iterate as (key, value) pairs; collect them for last-wins lookups.
    let tags: HashMap<&str, &str> = order
        .tags()
        .filter_map(|(key, value)| Some((key.ok()?, value.ok()?)))
        .collect();
    println!("  tags: {tags:?}");

    // `optional` fields have explicit presence.
    if let Some(Ok(note)) = order.note() {
        println!("  note: {note}");
    }

    // A oneof is an enum of its members, or `None` if none was set.
    match order.delivery() {
        Some(Delivery::Address(address)) => {
            println!(
                "  ship to {}, {}",
                address.city().unwrap_or("?"),
                address.country().unwrap_or("?")
            )
        }
        Some(Delivery::PickupPoint(point)) => println!("  pick up at {}", point.unwrap_or("?")),
        None => println!("  no delivery chosen"),
    }
    Ok(())
}
// --- README: end -----------------------------------------------------------------------

/// Hand-derived `prost` mirrors of `proto/shop/*.proto`, to produce the bytes.
mod pb {
    use std::collections::HashMap;

    #[derive(Clone, PartialEq, prost::Message)]
    pub struct Money {
        #[prost(string, tag = "1")]
        pub currency: String,
        #[prost(int64, tag = "2")]
        pub units: i64,
        #[prost(int32, tag = "3")]
        pub nanos: i32,
    }

    #[derive(Clone, PartialEq, prost::Message)]
    pub struct Address {
        #[prost(string, tag = "1")]
        pub line1: String,
        #[prost(string, tag = "2")]
        pub city: String,
        #[prost(string, tag = "3")]
        pub country: String,
    }

    #[derive(Clone, PartialEq, prost::Message)]
    pub struct LineItem {
        #[prost(string, tag = "1")]
        pub sku: String,
        #[prost(uint32, tag = "2")]
        pub quantity: u32,
        #[prost(message, optional, tag = "3")]
        pub price: Option<Money>,
    }

    #[derive(Clone, PartialEq, prost::Oneof)]
    pub enum Delivery {
        #[prost(message, tag = "7")]
        Address(Address),
        #[prost(string, tag = "8")]
        PickupPoint(String),
    }

    #[derive(Clone, PartialEq, prost::Message)]
    pub struct Order {
        #[prost(uint64, tag = "1")]
        pub id: u64,
        #[prost(bytes = "vec", tag = "2")]
        pub customer_id: Vec<u8>,
        #[prost(int32, tag = "3")]
        pub status: i32,
        #[prost(message, repeated, tag = "4")]
        pub items: Vec<LineItem>,
        #[prost(map = "string, string", tag = "5")]
        pub tags: HashMap<String, String>,
        #[prost(string, optional, tag = "6")]
        pub note: Option<String>,
        #[prost(oneof = "Delivery", tags = "7, 8")]
        pub delivery: Option<Delivery>,
    }
}

fn sample(delivery: pb::Delivery) -> Vec<u8> {
    pb::Order {
        id: 1042,
        customer_id: (0..16).collect(),
        status: 1,
        items: vec![
            pb::LineItem {
                sku: "TEA-01".to_owned(),
                quantity: 2,
                price: Some(pb::Money {
                    currency: "EUR".to_owned(),
                    units: 4,
                    nanos: 500_000_000,
                }),
            },
            pb::LineItem {
                sku: "MUG-07".to_owned(),
                quantity: 1,
                price: None,
            },
        ],
        tags: HashMap::from([("gift".to_owned(), "yes".to_owned())]),
        note: Some("leave at the door".to_owned()),
        delivery: Some(delivery),
    }
    .encode_to_vec()
}

fn address() -> pb::Delivery {
    pb::Delivery::Address(pb::Address {
        line1: "1 Rue de la Paix".to_owned(),
        city: "Montréal".to_owned(),
        country: "CA".to_owned(),
    })
}

#[test]
fn readme_example_runs() {
    print_order(&sample(address())).unwrap();
    print_order(&sample(pb::Delivery::PickupPoint("Locker 12".to_owned()))).unwrap();
}

#[test]
fn readme_example_reads_the_right_values() {
    let bytes = sample(address());
    let order = Order::parse(bytes.as_slice()).unwrap();
    assert_eq!(order.id(), 1042);
    assert_eq!(
        order.customer_id(),
        core::array::from_fn::<u8, 16, _>(|i| i as u8)
    );
    assert_eq!(order.status(), Status::Pending);

    let items: Vec<_> = order.items().collect();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].sku().unwrap(), "TEA-01");
    let price = items[0].price().unwrap();
    assert_eq!(
        (price.units(), price.nanos(), price.currency().unwrap()),
        (4, 500_000_000, "EUR")
    );
    assert!(items[1].price().is_none());

    assert_eq!(order.tags().count(), 1);
    assert_eq!(order.note(), Some(Ok("leave at the door")));
    match order.delivery() {
        Some(Delivery::Address(address)) => assert_eq!(address.city().unwrap(), "Montréal"),
        _ => panic!("expected an address"),
    }
}

#[test]
fn readme_example_rejects_a_bad_customer_id() {
    let mut order = pb::Order::decode(sample(address()).as_slice()).unwrap();
    order.customer_id.truncate(15);
    assert_eq!(
        Order::parse(order.encode_to_vec().as_slice()).err(),
        Some(DecodeError::FixedBytesLenMismatch {
            field: 2,
            expected: 16,
            actual: 15
        })
    );
}

#[test]
fn readme_buffer_types_compile() {
    let bytes = sample(address());
    let borrowed: Order<&[u8]> = Order::parse(bytes.as_slice()).unwrap();
    assert_eq!(borrowed.id(), 1042);
    let owned: Order<Vec<u8>> = Order::parse(bytes.clone()).unwrap();
    let shared: Order<bytes::Bytes> = Order::parse(bytes::Bytes::from(bytes)).unwrap();
    // Owned views are 'static and can move across threads.
    let handle = std::thread::spawn(move || owned.id() + shared.id());
    assert_eq!(handle.join().unwrap(), 2084);
}
