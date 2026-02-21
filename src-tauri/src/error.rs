//! Domain-specific error types using `snafu`.
//!
//! Each domain (PirateBay search, Transmission RPC, config I/O) has its own
//! error enum. All variants carry context and the original source error.
//! Every domain enum converts into [`pb_wire_types::AppError`] with the
//! appropriate [`pb_wire_types::ErrorKind`] so the frontend can branch on it.

use std::path::PathBuf;

use pb_wire_types::{AppError, ErrorKind};
use snafu::Snafu;

// ---------------------------------------------------------------------------
// PirateBay search / info
// ---------------------------------------------------------------------------

/// Errors originating from the PirateBay search/info API.
///
/// `surf::Error` is an opaque `anyhow`-style wrapper so we stringify it
/// at the boundary rather than carrying the original source.
#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum PirateError {
    #[snafu(display("Search failed: {message}"))]
    Search { message: String },

    #[snafu(display("Failed to get torrent info: {message}"))]
    Info { message: String },
}

impl From<PirateError> for AppError {
    fn from(e: PirateError) -> Self {
        AppError::new(ErrorKind::PirateSearch, e.to_string())
    }
}

// ---------------------------------------------------------------------------
// Transmission RPC
// ---------------------------------------------------------------------------

/// Errors from interacting with the Transmission RPC daemon.
#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum TransmissionError {
    #[snafu(display("Invalid Transmission URL '{url}': {source}"))]
    InvalidUrl {
        url: String,
        source: url::ParseError,
    },

    #[snafu(display("Failed to connect to Transmission: {message}"))]
    Connection { message: String },

    #[snafu(display("Transmission RPC error: {message}"))]
    Rpc { message: String },
}

impl From<TransmissionError> for AppError {
    fn from(e: TransmissionError) -> Self {
        let kind = match &e {
            TransmissionError::InvalidUrl { .. } => ErrorKind::InvalidUrl,
            TransmissionError::Connection { .. } => ErrorKind::TransmissionConnection,
            TransmissionError::Rpc { .. } => ErrorKind::TransmissionRpc,
        };
        AppError::new(kind, e.to_string())
    }
}

// ---------------------------------------------------------------------------
// Config I/O
// ---------------------------------------------------------------------------

/// Errors from reading/writing the on-disk configuration file.
#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum ConfigError {
    #[snafu(display("Failed to create config directory '{}': {source}", path.display()))]
    CreateDir {
        path: PathBuf,
        source: std::io::Error,
    },

    #[snafu(display("Failed to write config to '{}': {source}", path.display()))]
    WriteFile {
        path: PathBuf,
        source: std::io::Error,
    },

    #[snafu(display("Failed to serialize config: {source}"))]
    Serialize { source: serde_json::Error },
}

impl From<ConfigError> for AppError {
    fn from(e: ConfigError) -> Self {
        AppError::new(ErrorKind::Config, e.to_string())
    }
}

// ---------------------------------------------------------------------------
// Filesystem copy
// ---------------------------------------------------------------------------

/// Errors from copying completed downloads to their destination directory.
///
/// Variant names are prefixed with `Copy` to avoid snafu context-selector
/// collisions with [`ConfigError`] (both have dir-creation / I/O variants).
#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum CopyError {
    #[snafu(display("Source path '{}' does not exist", path.display()))]
    CopySourceMissing { path: PathBuf },

    #[snafu(display("No destination directory configured for {destination}"))]
    CopyNoDestDir {
        destination: pb_wire_types::Destination,
    },

    #[snafu(display("Failed to create directory '{}': {source}", path.display()))]
    CopyCreateDir {
        path: PathBuf,
        source: std::io::Error,
    },

    #[snafu(display("Failed to copy '{}' to '{}': {source}", src.display(), dst.display()))]
    CopyFile {
        src: PathBuf,
        dst: PathBuf,
        source: std::io::Error,
    },

    #[snafu(display("Failed to read directory '{}': {source}", path.display()))]
    CopyReadDir {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl From<CopyError> for AppError {
    fn from(e: CopyError) -> Self {
        AppError::new(ErrorKind::Copy, e.to_string())
    }
}
