//! Connects to a Yellowstone gRPC endpoint and prints a one-line summary of every account,
//! transaction, entry, block meta and slot status update it receives.
//!
//! Reads `GRPC_ENDPOINT` and `X_TOKEN` from the matching flags or the environment, after
//! loading a dotenv file (`.env` by default) if one exists. Stops on Ctrl-C or when the
//! server closes the stream.

use std::collections::HashMap;
use std::env;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;
use thiserror::Error;
use yellowstone_grpc_proto::geyser::{
    SubscribeRequest, SubscribeRequestFilterAccounts, SubscribeRequestFilterBlocksMeta,
    SubscribeRequestFilterEntry, SubscribeRequestFilterSlots, SubscribeRequestFilterTransactions,
    SubscribeRequestPing,
};
use yellowstone_grpc_protoview::client::{
    ClientConfig, ClientError, GeyserClient, StreamError, SubscribeUpdateView, Subscription,
};
use yellowstone_grpc_protoview::geyser::subscribe_update::UpdateOneof;

/// The filter name every stream is registered under.
const FILTER_NAME: &str = "protoview-client";

/// Command-line options.
#[derive(Debug, Parser)]
#[command(name = "protoview-client", version, about)]
struct Cli {
    /// The endpoint URI, e.g. `https://example.rpcpool.com`. Defaults to `GRPC_ENDPOINT`.
    #[arg(long, value_name = "URI")]
    endpoint: Option<String>,

    /// The `x-token` header value, if the endpoint requires one. Defaults to `X_TOKEN`.
    #[arg(long, value_name = "TOKEN")]
    x_token: Option<String>,

    /// Dotenv file to load before reading the environment. Defaults to `.env`, which may be
    /// absent.
    #[arg(long, value_name = "PATH")]
    env_file: Option<PathBuf>,
}

/// A failure that ends the program.
#[derive(Debug, Error)]
enum AppError {
    /// The dotenv file named with `--env-file` could not be loaded.
    #[error("loading env file {path:?}")]
    DotEnv {
        path: PathBuf,
        #[source]
        source: dotenvy::Error,
    },

    /// No endpoint was given by flag or environment.
    #[error("no endpoint: pass --endpoint or set GRPC_ENDPOINT")]
    MissingEndpoint,

    /// Connecting or subscribing failed.
    #[error(transparent)]
    Client(#[from] ClientError),

    /// The update stream failed or carried an invalid message.
    #[error(transparent)]
    Stream(#[from] StreamError),

    /// The request stream closed while replying to a ping.
    #[error("request stream closed while replying to a ping")]
    RequestStreamClosed,

    /// The Ctrl-C handler could not be installed.
    #[error("listening for Ctrl-C")]
    Signal(#[source] io::Error),

    /// Writing to stdout failed.
    #[error("writing to stdout")]
    Stdout(#[source] io::Error),
}

/// Runs the client and maps failures to a non-zero exit code.
///
/// # Returns
///
/// [`ExitCode::SUCCESS`], or [`ExitCode::FAILURE`] after printing the error chain.
#[tokio::main]
async fn main() -> ExitCode {
    match run(Cli::parse()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(AppError::Stdout(err)) if err.kind() == io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            // Some error types repeat their source in their own message; skip repeats.
            let mut previous = error.to_string();
            let mut source = std::error::Error::source(&error);
            while let Some(cause) = source {
                let message = cause.to_string();
                if message != previous {
                    eprintln!("  caused by: {message}");
                }
                previous = message;
                source = cause.source();
            }
            ExitCode::FAILURE
        }
    }
}

/// Subscribes and prints a summary line per update until stopped.
///
/// # Arguments
///
/// * `cli` - The parsed command line.
///
/// # Returns
///
/// `Ok(())` after Ctrl-C or when the server closes the stream.
///
/// # Errors
///
/// [`AppError::DotEnv`] for an unreadable `--env-file`, [`AppError::MissingEndpoint`] if no
/// endpoint is configured, [`AppError::Client`] if connecting
/// or subscribing fails, [`AppError::Stream`] if the stream breaks,
/// [`AppError::RequestStreamClosed`] if a ping cannot be answered, [`AppError::Signal`] if
/// the Ctrl-C handler cannot be installed and [`AppError::Stdout`] if printing fails.
async fn run(cli: Cli) -> Result<(), AppError> {
    load_env_file(cli.env_file.as_deref())?;
    let config = ClientConfig {
        endpoint: cli
            .endpoint
            .or_else(|| env_var("GRPC_ENDPOINT"))
            .ok_or(AppError::MissingEndpoint)?,
        x_token: cli.x_token.or_else(|| env_var("X_TOKEN")),
    };
    let Subscription {
        requests,
        mut updates,
    } = GeyserClient::connect(&config)
        .await?
        .subscribe(subscribe_request())
        .await?;
    eprintln!("subscribed to {}", config.endpoint);

    // Created once, outside the loop: a fresh listener per iteration would miss a signal
    // that arrives while an update is being printed.
    let interrupt = tokio::signal::ctrl_c();
    tokio::pin!(interrupt);

    let mut out = io::stdout().lock();
    loop {
        let update = tokio::select! {
            result = &mut interrupt => {
                return result.map_err(AppError::Signal);
            }
            update = updates.next() => update,
        };
        let Some(update) = update else {
            return Ok(());
        };
        let update = update?;
        if let Some(UpdateOneof::Ping(_)) = update.update_oneof() {
            requests
                .send(ping_reply())
                .await
                .map_err(|_| AppError::RequestStreamClosed)?;
        }
        summarize(&mut out, &update).map_err(AppError::Stdout)?;
    }
}

/// Loads a dotenv file into the process environment, without overriding variables that
/// are already set.
///
/// # Arguments
///
/// * `path` - The file to load, or [`None`] for `.env`, which may be absent.
///
/// # Returns
///
/// `Ok(())` once loaded, or when the default `.env` does not exist.
///
/// # Errors
///
/// [`AppError::DotEnv`] if the file cannot be read or parsed, including an explicitly
/// given `path` that does not exist.
fn load_env_file(path: Option<&Path>) -> Result<(), AppError> {
    let (path, required) = match path {
        Some(path) => (path, true),
        None => (Path::new(".env"), false),
    };
    if !required && !path.exists() {
        return Ok(());
    }
    dotenvy::from_path(path)
        .map(|_| ())
        .map_err(|source| AppError::DotEnv {
            path: path.to_path_buf(),
            source,
        })
}

/// Reads an environment variable, treating an empty value as unset.
///
/// # Arguments
///
/// * `name` - The variable name.
///
/// # Returns
///
/// The trimmed value, or [`None`] if unset, empty, or not valid Unicode.
fn env_var(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// Builds a request for every account, transaction, entry, block meta and slot update,
/// including slot updates between commitment levels.
///
/// # Returns
///
/// The [`SubscribeRequest`] to open the stream with.
fn subscribe_request() -> SubscribeRequest {
    SubscribeRequest {
        accounts: named(SubscribeRequestFilterAccounts::default()),
        transactions: named(SubscribeRequestFilterTransactions::default()),
        entry: named(SubscribeRequestFilterEntry::default()),
        blocks_meta: named(SubscribeRequestFilterBlocksMeta::default()),
        slots: named(SubscribeRequestFilterSlots {
            interslot_updates: Some(true),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Builds the request that answers a server ping, keeping the stream open.
///
/// # Returns
///
/// A [`SubscribeRequest`] carrying only a [`SubscribeRequestPing`].
fn ping_reply() -> SubscribeRequest {
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
/// A [`HashMap`] holding `filter` under [`FILTER_NAME`].
fn named<F>(filter: F) -> HashMap<String, F> {
    HashMap::from([(FILTER_NAME.to_string(), filter)])
}

/// Writes one summary line for `update`; kinds that are not summarized write nothing.
///
/// # Arguments
///
/// * `out` - Where to write.
/// * `update` - The update to summarize.
///
/// # Returns
///
/// `Ok(())` once the line is written.
///
/// # Errors
///
/// Any [`io::Error`] from `out`.
fn summarize(out: &mut impl Write, update: &SubscribeUpdateView) -> io::Result<()> {
    match update.update_oneof() {
        Some(UpdateOneof::Account(update)) => {
            let Some(account) = update.account() else {
                return Ok(());
            };
            writeln!(
                out,
                "account slot={} pubkey={} owner={} lamports={} data_len={} startup={}",
                update.slot(),
                bs58::encode(account.pubkey()).into_string(),
                bs58::encode(account.owner()).into_string(),
                account.lamports(),
                account.data().len(),
                update.is_startup(),
            )
        }
        Some(UpdateOneof::Transaction(update)) => {
            let Some(txn) = update.transaction() else {
                return Ok(());
            };
            writeln!(
                out,
                "txn slot={} index={} signature={} vote={}",
                update.slot(),
                txn.index(),
                bs58::encode(txn.signature()).into_string(),
                txn.is_vote(),
            )
        }
        Some(UpdateOneof::Entry(entry)) => writeln!(
            out,
            "entry slot={} index={} num_hashes={} txns={} hash={}",
            entry.slot(),
            entry.index(),
            entry.num_hashes(),
            entry.executed_transaction_count(),
            bs58::encode(entry.hash()).into_string(),
        ),
        Some(UpdateOneof::BlockMeta(meta)) => writeln!(
            out,
            "block_meta slot={} blockhash={} parent_slot={} height={} txns={} entries={}",
            meta.slot(),
            meta.blockhash().unwrap_or("<invalid utf-8>"),
            meta.parent_slot(),
            meta.block_height()
                .map_or_else(|| "-".to_string(), |h| h.block_height().to_string()),
            meta.executed_transaction_count(),
            meta.entries_count(),
        ),
        Some(UpdateOneof::Slot(slot)) => {
            write!(out, "slot slot={} status={:?}", slot.slot(), slot.status())?;
            if let Some(parent) = slot.parent() {
                write!(out, " parent={parent}")?;
            }
            if let Some(Ok(error)) = slot.dead_error() {
                write!(out, " dead_error={error:?}")?;
            }
            writeln!(out)
        }
        _ => Ok(()),
    }
}
