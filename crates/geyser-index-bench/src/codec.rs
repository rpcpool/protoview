//! A tonic [`Codec`] that encodes `SubscribeRequest`s with `prost` but hands incoming
//! messages back as the raw [`Bytes`] of each gRPC frame, so the benchmark can time
//! decoding itself instead of receiving an already-decoded message.

use bytes::{Buf, Bytes};
use prost::Message as _;
use tonic::Status;
use tonic::codec::{Codec, DecodeBuf, Decoder, EncodeBuf, Encoder};
use yellowstone_grpc_proto::geyser::SubscribeRequest;

/// Encodes [`SubscribeRequest`]s and decodes each incoming frame to its raw [`Bytes`].
#[derive(Debug, Clone, Copy, Default)]
pub struct RawUpdateCodec;

impl Codec for RawUpdateCodec {
    type Encode = SubscribeRequest;
    type Decode = Bytes;
    type Encoder = RequestEncoder;
    type Decoder = RawDecoder;

    /// Returns the request encoder.
    ///
    /// # Returns
    ///
    /// A [`RequestEncoder`].
    fn encoder(&mut self) -> Self::Encoder {
        RequestEncoder
    }

    /// Returns the raw frame decoder.
    ///
    /// # Returns
    ///
    /// A [`RawDecoder`].
    fn decoder(&mut self) -> Self::Decoder {
        RawDecoder
    }
}

/// Encodes a [`SubscribeRequest`] with `prost`.
#[derive(Debug, Clone, Copy, Default)]
pub struct RequestEncoder;

impl Encoder for RequestEncoder {
    type Item = SubscribeRequest;
    type Error = Status;

    /// Encodes `item` into the outgoing frame buffer.
    ///
    /// # Arguments
    ///
    /// * `item` - The request to send.
    /// * `dst` - The frame buffer tonic provides.
    ///
    /// # Returns
    ///
    /// `Ok(())` once `item` is written.
    ///
    /// # Errors
    ///
    /// [`Status::internal`] if `prost` reports insufficient buffer capacity, which the
    /// growable [`EncodeBuf`] does not produce in practice.
    fn encode(&mut self, item: Self::Item, dst: &mut EncodeBuf<'_>) -> Result<(), Self::Error> {
        item.encode(dst)
            .map_err(|err| Status::internal(format!("encoding SubscribeRequest: {err}")))
    }
}

/// Hands each incoming frame back undecoded.
#[derive(Debug, Clone, Copy, Default)]
pub struct RawDecoder;

impl Decoder for RawDecoder {
    type Item = Bytes;
    type Error = Status;

    /// Takes the whole frame out of the receive buffer.
    ///
    /// Tonic's [`DecodeBuf`] is backed by a `BytesMut`, so this splits the buffer rather
    /// than copying it: the benchmark then times decoding on the same allocation tonic
    /// received the frame into.
    ///
    /// # Arguments
    ///
    /// * `src` - Exactly one message's bytes, gRPC framing already removed.
    ///
    /// # Returns
    ///
    /// The frame's raw [`Bytes`].
    ///
    /// # Errors
    ///
    /// Never; the signature is dictated by [`Decoder`].
    fn decode(&mut self, src: &mut DecodeBuf<'_>) -> Result<Option<Self::Item>, Self::Error> {
        Ok(Some(src.copy_to_bytes(src.remaining())))
    }
}
