//! Subscribes to a Yellowstone gRPC endpoint and measures how long the `protoview`
//! views take to index each incoming `SubscribeUpdate`, optionally next to a `prost`
//! decode of the same bytes.
//!
//! Reads `GRPC_ENDPOINT` (required) and `X_TOKEN` (optional) from the environment, after
//! loading a dotenv file (`.env` by default) if one exists.

mod client;
mod codec;
// Generated API: the binary uses only the getters it needs to classify updates.
mod measure;
mod request;
mod stats;
#[allow(dead_code)]
mod view;

use std::env;
use std::future;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use clap::Parser;
use thiserror::Error;
use tokio::time::MissedTickBehavior;

use crate::client::{ClientConfig, ClientError, Subscription};
use crate::measure::{Timer, UpdateKind};
use crate::request::Stream;
use crate::stats::Stats;

/// Benchmark view indexing of live Yellowstone gRPC `SubscribeUpdate`s.
#[derive(Debug, Parser)]
#[command(name = "geyser-index-bench", version, about)]
struct Cli {
    /// Dotenv file to load `GRPC_ENDPOINT` and `X_TOKEN` from. Defaults to `.env`, which
    /// is skipped silently when absent; an explicitly given file must exist. Variables
    /// already set in the environment take precedence.
    #[arg(long, value_name = "PATH")]
    env_file: Option<PathBuf>,

    /// Update streams to subscribe to, comma-separated or repeated. Each is unfiltered.
    #[arg(
        long,
        value_enum,
        value_delimiter = ',',
        required = true,
        value_name = "STREAM"
    )]
    subscribe: Vec<Stream>,

    /// Stop after this many seconds.
    #[arg(long, value_name = "SECS")]
    duration: Option<u64>,

    /// Stop after this many updates.
    #[arg(long, value_name = "N")]
    limit: Option<u64>,

    /// Print a cumulative report every this many seconds.
    #[arg(long, value_name = "SECS", default_value_t = 10, value_parser = clap::value_parser!(u64).range(1..))]
    report_every: u64,

    /// Decode each update this many times and report the mean. `1` measures the first,
    /// cold-cache touch of freshly received bytes; higher values measure a warm cache.
    #[arg(long, value_name = "N", default_value_t = 1, value_parser = clap::value_parser!(u32).range(1..))]
    repeat: u32,

    /// Skip the `prost` comparison decode.
    #[arg(long)]
    no_prost: bool,
}

/// Every failure the benchmark can exit with.
#[derive(Debug, Error)]
enum AppError {
    /// The dotenv file could not be read or parsed.
    #[error("loading {}", path.display())]
    DotEnv {
        path: PathBuf,
        #[source]
        source: dotenvy::Error,
    },

    /// A required environment variable is unset or empty.
    #[error("{0} is not set (in the environment or the dotenv file)")]
    MissingEnv(&'static str),

    /// Connecting or subscribing failed.
    #[error(transparent)]
    Client(#[from] ClientError),

    /// The update stream failed mid-run.
    #[error("update stream failed")]
    Stream(#[source] tonic::Status),

    /// The request stream closed while replying to a ping.
    #[error("request stream closed while replying to a ping")]
    RequestStreamClosed,
}

/// Why the measurement loop stopped.
#[derive(Debug, Clone, Copy)]
enum StopReason {
    Interrupted,
    Duration,
    Limit,
    StreamClosed,
}

impl StopReason {
    /// Describes the reason for the final report header.
    ///
    /// # Returns
    ///
    /// A short human-readable description.
    const fn describe(self) -> &'static str {
        match self {
            Self::Interrupted => "interrupted",
            Self::Duration => "duration reached",
            Self::Limit => "update limit reached",
            Self::StreamClosed => "server closed the stream",
        }
    }
}

/// Runs the benchmark and maps failures to a non-zero exit code.
///
/// # Returns
///
/// [`ExitCode::SUCCESS`], or [`ExitCode::FAILURE`] after printing the error chain.
#[tokio::main]
async fn main() -> ExitCode {
    match run(Cli::parse()).await {
        Ok(()) => ExitCode::SUCCESS,
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

/// Loads configuration, subscribes, and measures until a stop condition is met.
///
/// # Arguments
///
/// * `cli` - The parsed [`Cli`] arguments.
///
/// # Returns
///
/// `Ok(())` once the final report is printed.
///
/// # Errors
///
/// [`AppError::DotEnv`] or [`AppError::MissingEnv`] for configuration problems,
/// [`AppError::Client`] if subscribing fails, [`AppError::Stream`] if the stream breaks,
/// and [`AppError::RequestStreamClosed`] if a ping cannot be answered.
async fn run(cli: Cli) -> Result<(), AppError> {
    load_env_file(cli.env_file.as_deref())?;
    let config = ClientConfig {
        endpoint: env_var("GRPC_ENDPOINT").ok_or(AppError::MissingEnv("GRPC_ENDPOINT"))?,
        x_token: env_var("X_TOKEN"),
    };

    let timer = Timer::new(cli.repeat, !cli.no_prost);
    let request = request::subscribe_request(&cli.subscribe);
    println!(
        "subscribing to {:?} on {} (x-token {}), repeat {}, prost comparison {}",
        cli.subscribe,
        config.endpoint,
        if config.x_token.is_some() {
            "set"
        } else {
            "unset"
        },
        cli.repeat,
        if timer.compares_prost() { "on" } else { "off" },
    );

    let Subscription {
        requests,
        mut updates,
    } = client::subscribe(&config, request).await?;
    let started = Instant::now();
    let mut stats = Stats::new(timer.compares_prost());

    let mut report = tokio::time::interval(Duration::from_secs(cli.report_every));
    report.set_missed_tick_behavior(MissedTickBehavior::Skip);
    report.tick().await; // The first tick completes immediately.

    let deadline = async {
        match cli.duration {
            Some(secs) => tokio::time::sleep(Duration::from_secs(secs)).await,
            None => future::pending().await,
        }
    };
    tokio::pin!(deadline);
    let interrupt = tokio::signal::ctrl_c();
    tokio::pin!(interrupt);

    let reason = loop {
        tokio::select! {
            biased;
            _ = &mut interrupt => break StopReason::Interrupted,
            () = &mut deadline => break StopReason::Duration,
            _ = report.tick() => print!("{}", stats.render(started.elapsed())),
            message = updates.message() => {
                let Some(bytes) = message.map_err(AppError::Stream)? else {
                    break StopReason::StreamClosed;
                };
                match timer.measure(&bytes) {
                    Ok(sample) => {
                        stats.record(&sample);
                        if sample.kind == UpdateKind::Ping {
                            requests
                                .send(request::ping_reply())
                                .await
                                .map_err(|_| AppError::RequestStreamClosed)?;
                        }
                    }
                    Err(error) => stats.record_error(error),
                }
                if cli.limit.is_some_and(|limit| stats.total() >= limit) {
                    break StopReason::Limit;
                }
            }
        }
    };

    println!("\nstopped: {}", reason.describe());
    print!("{}", stats.render(started.elapsed()));
    Ok(())
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
