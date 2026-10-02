//! Connecting to a Yellowstone gRPC endpoint and opening a `Subscribe` stream whose
//! updates arrive as raw [`Bytes`].

use std::time::Duration;

use bytes::Bytes;
use thiserror::Error;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::codegen::http::uri::PathAndQuery;
use tonic::metadata::AsciiMetadataValue;
use tonic::metadata::errors::InvalidMetadataValue;
use tonic::transport::{Channel, ClientTlsConfig, Endpoint};
use tonic::{Request, Status, Streaming};
use yellowstone_grpc_proto::geyser::SubscribeRequest;

use crate::codec::RawUpdateCodec;

/// The fully-qualified gRPC method for `Geyser.Subscribe`.
const SUBSCRIBE_PATH: &str = "/geyser.Geyser/Subscribe";

/// How many outgoing requests (the initial subscription, then ping replies) may queue
/// before the sender waits.
const REQUEST_QUEUE: usize = 16;

/// A failure while connecting or subscribing.
#[derive(Debug, Error)]
pub enum ClientError {
    /// `GRPC_ENDPOINT` is not a valid URI.
    #[error("invalid gRPC endpoint {endpoint:?}")]
    InvalidEndpoint {
        endpoint: String,
        #[source]
        source: tonic::transport::Error,
    },

    /// TLS could not be configured for an `https` endpoint.
    #[error("configuring TLS for {endpoint}")]
    Tls {
        endpoint: String,
        #[source]
        source: tonic::transport::Error,
    },

    /// The connection could not be established.
    #[error("connecting to {endpoint}")]
    Connect {
        endpoint: String,
        #[source]
        source: tonic::transport::Error,
    },

    /// `X_TOKEN` contains characters not allowed in a gRPC metadata value.
    #[error("X_TOKEN is not a valid gRPC metadata value")]
    InvalidToken(#[from] InvalidMetadataValue),

    /// The server rejected the `Subscribe` call.
    #[error("Subscribe call failed")]
    Subscribe(#[source] Status),

    /// The request stream closed before the initial subscription was queued.
    #[error("request stream closed before subscribing")]
    RequestStreamClosed,
}

/// A local `Result` alias defaulting to [`ClientError`].
pub type Result<T> = std::result::Result<T, ClientError>;

/// Where and how to connect.
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// The endpoint URI, e.g. `https://example.rpcpool.com`.
    pub endpoint: String,
    /// The `x-token` header value, if the endpoint requires one.
    pub x_token: Option<String>,
}

/// An open `Subscribe` stream.
pub struct Subscription {
    /// Sends further requests on the bidirectional stream, such as ping replies.
    pub requests: mpsc::Sender<SubscribeRequest>,
    /// Incoming updates, each the raw encoding of one `SubscribeUpdate`.
    pub updates: Streaming<Bytes>,
}

/// Connects to `config.endpoint` and opens a `Subscribe` stream with `initial` as its
/// first request.
///
/// TLS is enabled for `https` endpoints using both the platform's native roots and the
/// bundled WebPKI roots. Decoding size limits are lifted, as full blocks routinely exceed
/// tonic's 4 MiB default.
///
/// # Arguments
///
/// * `config` - The endpoint and optional token, see [`ClientConfig`].
/// * `initial` - The [`SubscribeRequest`] describing what to subscribe to.
///
/// # Returns
///
/// The open [`Subscription`].
///
/// # Errors
///
/// [`ClientError::InvalidEndpoint`], [`ClientError::Tls`] or [`ClientError::Connect`] if
/// the connection cannot be set up; [`ClientError::InvalidToken`] if the token is not a
/// valid header value; [`ClientError::Subscribe`] if the server refuses the call.
pub async fn subscribe(config: &ClientConfig, initial: SubscribeRequest) -> Result<Subscription> {
    // Validated before connecting, so a bad token is reported as such.
    let x_token: Option<AsciiMetadataValue> =
        config.x_token.as_deref().map(str::parse).transpose()?;
    let channel = connect(&config.endpoint).await?;
    let mut grpc = tonic::client::Grpc::new(channel).max_decoding_message_size(usize::MAX);

    let (requests, receiver) = mpsc::channel(REQUEST_QUEUE);
    requests
        .send(initial)
        .await
        .map_err(|_| ClientError::RequestStreamClosed)?;

    let mut request = Request::new(ReceiverStream::new(receiver));
    if let Some(token) = x_token {
        request.metadata_mut().insert("x-token", token);
    }

    grpc.ready().await.map_err(|source| ClientError::Connect {
        endpoint: config.endpoint.clone(),
        source,
    })?;
    let updates = grpc
        .streaming(
            request,
            PathAndQuery::from_static(SUBSCRIBE_PATH),
            RawUpdateCodec,
        )
        .await
        .map_err(ClientError::Subscribe)?
        .into_inner();

    Ok(Subscription { requests, updates })
}

/// Opens a channel to `endpoint`, with TLS when its scheme is `https`.
///
/// # Arguments
///
/// * `endpoint` - The endpoint URI.
///
/// # Returns
///
/// A connected [`Channel`].
///
/// # Errors
///
/// [`ClientError::InvalidEndpoint`] if `endpoint` does not parse, [`ClientError::Tls`] if
/// TLS configuration fails, and [`ClientError::Connect`] if the connection fails.
async fn connect(endpoint: &str) -> Result<Channel> {
    let mut builder = Endpoint::from_shared(endpoint.to_string())
        .map_err(|source| ClientError::InvalidEndpoint {
            endpoint: endpoint.to_string(),
            source,
        })?
        .connect_timeout(Duration::from_secs(10))
        .tcp_nodelay(true)
        .http2_adaptive_window(true);

    if endpoint.starts_with("https") {
        builder = builder
            .tls_config(ClientTlsConfig::new().with_enabled_roots())
            .map_err(|source| ClientError::Tls {
                endpoint: endpoint.to_string(),
                source,
            })?;
    }

    builder
        .connect()
        .await
        .map_err(|source| ClientError::Connect {
            endpoint: endpoint.to_string(),
            source,
        })
}
