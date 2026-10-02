//! [`GeyserClient`]: connecting to a Yellowstone gRPC endpoint and opening a `Subscribe`
//! stream whose updates arrive as raw [`Bytes`], ready to be parsed into views.

use std::pin::Pin;
use std::task::{Context, Poll, ready};
use std::time::Duration;

use bytes::Bytes;
use thiserror::Error;
use tokio::sync::mpsc;
use tokio_stream::Stream;
use tokio_stream::wrappers::ReceiverStream;
use tonic::client::Grpc;
use tonic::codegen::http::uri::PathAndQuery;
use tonic::metadata::AsciiMetadataValue;
use tonic::metadata::errors::InvalidMetadataValue;
use tonic::transport::{Channel, ClientTlsConfig, Endpoint};
use tonic::{Request, Status, Streaming};
use yellowstone_grpc_proto::geyser::SubscribeRequest;

use crate::codec::RawUpdateCodec;
use crate::geyser::SubscribeUpdate;
use protoview::DecodeError;

/// The fully-qualified gRPC method for `Geyser.Subscribe`.
const SUBSCRIBE_PATH: &str = "/geyser.Geyser/Subscribe";

/// How many outgoing requests (the initial subscription, then ping replies) may queue
/// before the sender waits.
const REQUEST_QUEUE: usize = 16;

/// A failure while connecting or subscribing.
#[derive(Debug, Error)]
pub enum ClientError {
    /// The endpoint is not a valid URI.
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

    /// The token contains characters not allowed in a gRPC metadata value.
    #[error("x-token is not a valid gRPC metadata value")]
    InvalidToken(#[from] InvalidMetadataValue),

    /// The channel stopped accepting calls.
    #[error("gRPC channel is not ready")]
    NotReady(#[source] tonic::transport::Error),

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

/// A parsed `SubscribeUpdate` that owns the [`Bytes`] it was received in.
pub type SubscribeUpdateView = SubscribeUpdate<Bytes>;

/// A failure while reading the update stream.
#[derive(Debug, Error)]
pub enum StreamError {
    /// The server ended the stream with an error status, or the transport broke.
    #[error("Subscribe stream failed")]
    Status(#[source] Status),

    /// A frame was not a valid `SubscribeUpdate`.
    #[error("decoding SubscribeUpdate: {0}")]
    Decode(DecodeError),
}

/// An open `Subscribe` stream.
///
/// `S` is the type of the incoming stream: [`GeyserStream`] for parsed updates, or
/// [`Streaming<Bytes>`] for raw frames.
pub struct Subscription<S = GeyserStream> {
    /// Sends further requests on the bidirectional stream, such as ping replies or an
    /// updated subscription.
    pub requests: mpsc::Sender<SubscribeRequest>,
    /// Incoming updates.
    pub updates: S,
}

/// The incoming side of a `Subscribe` call: a [`Stream`] of [`SubscribeUpdateView`]s.
///
/// Each frame is validated once as it arrives, so every item is safe to read with no
/// further error handling. After an `Err` the stream may still yield later items for
/// [`StreamError::Decode`], but ends after a [`StreamError::Status`].
#[derive(Debug)]
pub struct GeyserStream {
    frames: Streaming<Bytes>,
}

impl GeyserStream {
    /// Waits for the next update.
    ///
    /// # Returns
    ///
    /// The next [`SubscribeUpdateView`], or `None` once the server closes the stream.
    ///
    /// # Errors
    ///
    /// [`StreamError::Status`] if the stream fails; [`StreamError::Decode`] if a frame is not
    /// a valid `SubscribeUpdate`.
    pub async fn next(&mut self) -> Option<std::result::Result<SubscribeUpdateView, StreamError>> {
        std::future::poll_fn(|cx| Pin::new(&mut *self).poll_next(cx)).await
    }
}

impl Stream for GeyserStream {
    type Item = std::result::Result<SubscribeUpdateView, StreamError>;

    /// Polls for the next frame and parses it.
    ///
    /// # Arguments
    ///
    /// * `cx` - The task context.
    ///
    /// # Returns
    ///
    /// [`Poll::Pending`] while no frame is available, otherwise the next item or `None` at
    /// the end of the stream.
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let frame = ready!(Pin::new(&mut self.frames).poll_next(cx));
        Poll::Ready(frame.map(|frame| {
            let frame = frame.map_err(StreamError::Status)?;
            SubscribeUpdate::parse(frame).map_err(StreamError::Decode)
        }))
    }
}

/// A client for the Yellowstone `Geyser` service whose streams yield raw message bytes.
///
/// Cloning is cheap: clones share the underlying connection.
#[derive(Debug, Clone)]
pub struct GeyserClient {
    grpc: Grpc<Channel>,
    x_token: Option<AsciiMetadataValue>,
}

impl GeyserClient {
    /// Connects to `config.endpoint`.
    ///
    /// TLS is enabled for `https` endpoints using both the platform's native roots and the
    /// bundled WebPKI roots. Decoding size limits are lifted, as full blocks routinely exceed
    /// tonic's 4 MiB default.
    ///
    /// # Arguments
    ///
    /// * `config` - The endpoint and optional token, see [`ClientConfig`].
    ///
    /// # Returns
    ///
    /// A connected [`GeyserClient`].
    ///
    /// # Errors
    ///
    /// [`ClientError::InvalidToken`] if the token is not a valid header value;
    /// [`ClientError::InvalidEndpoint`], [`ClientError::Tls`] or [`ClientError::Connect`] if
    /// the connection cannot be set up.
    pub async fn connect(config: &ClientConfig) -> Result<Self> {
        // Validated before connecting, so a bad token is reported as such.
        let x_token = config.x_token.as_deref().map(str::parse).transpose()?;
        let channel = connect(&config.endpoint).await?;
        Ok(Self::from_channel(channel, x_token))
    }

    /// Wraps an already-established `channel`, for callers that need their own transport
    /// settings.
    ///
    /// # Arguments
    ///
    /// * `channel` - The channel to the endpoint.
    /// * `x_token` - The `x-token` header value to send with each call, if any.
    ///
    /// # Returns
    ///
    /// A [`GeyserClient`] using `channel`.
    pub fn from_channel(channel: Channel, x_token: Option<AsciiMetadataValue>) -> Self {
        Self {
            grpc: Grpc::new(channel).max_decoding_message_size(usize::MAX),
            x_token,
        }
    }

    /// Opens a `Subscribe` stream with `initial` as its first request, parsing each update
    /// into a view.
    ///
    /// # Arguments
    ///
    /// * `initial` - The [`SubscribeRequest`] describing what to subscribe to.
    ///
    /// # Returns
    ///
    /// The open [`Subscription`].
    ///
    /// # Errors
    ///
    /// As [`GeyserClient::subscribe_raw`].
    pub async fn subscribe(&self, initial: SubscribeRequest) -> Result<Subscription> {
        let Subscription { requests, updates } = self.subscribe_raw(initial).await?;
        Ok(Subscription {
            requests,
            updates: GeyserStream { frames: updates },
        })
    }

    /// Opens a `Subscribe` stream with `initial` as its first request, yielding each update
    /// as the raw bytes of one `SubscribeUpdate`, unparsed.
    ///
    /// # Arguments
    ///
    /// * `initial` - The [`SubscribeRequest`] describing what to subscribe to.
    ///
    /// # Returns
    ///
    /// The open [`Subscription`] over raw frames.
    ///
    /// # Errors
    ///
    /// [`ClientError::NotReady`] if the channel cannot take a call,
    /// [`ClientError::Subscribe`] if the server refuses it, and
    /// [`ClientError::RequestStreamClosed`] if `initial` cannot be queued.
    pub async fn subscribe_raw(
        &self,
        initial: SubscribeRequest,
    ) -> Result<Subscription<Streaming<Bytes>>> {
        let mut grpc = self.grpc.clone();

        let (requests, receiver) = mpsc::channel(REQUEST_QUEUE);
        requests
            .send(initial)
            .await
            .map_err(|_| ClientError::RequestStreamClosed)?;

        let mut request = Request::new(ReceiverStream::new(receiver));
        if let Some(token) = &self.x_token {
            request.metadata_mut().insert("x-token", token.clone());
        }

        grpc.ready().await.map_err(ClientError::NotReady)?;
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
