//! Wire types for sending between BE<->FE.

/// Media destination for completed downloads.
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq, Default)]
pub enum Destination {
    #[default]
    Movies,
    Shows,
}

impl Destination {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Movies => "Movies",
            Self::Shows => "Shows",
        }
    }

    /// Auto-detect destination from a Privateer category code.
    ///
    /// Standard video sub-categories:
    /// - 201 Movies, 202 Movies DVDR, 207 HD Movies, 209 3D, 299 Other
    /// - 205 TV Shows, 208 HD TV Shows
    ///
    /// Returns `None` for non-video or unknown categories.
    pub fn from_category_str(cat: &str) -> Option<Self> {
        match cat {
            "201" | "202" | "207" | "209" | "299" => Some(Self::Movies),
            "205" | "208" => Some(Self::Shows),
            _ => None,
        }
    }

    /// Auto-detect destination from a Privateer category code (numeric).
    pub fn from_category(cat: u32) -> Option<Self> {
        match cat {
            201 | 202 | 207 | 209 | 299 => Some(Self::Movies),
            205 | 208 => Some(Self::Shows),
            _ => None,
        }
    }
}

impl std::fmt::Display for Destination {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// Transmission torrent status.
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq, Default)]
pub enum TransmissionStatus {
    #[default]
    Stopped,
    QueuedVerify,
    Verifying,
    QueuedDownload,
    Downloading,
    QueuedSeed,
    Seeding,
}

impl TransmissionStatus {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Stopped => "Stopped",
            Self::QueuedVerify => "Queued (Verify)",
            Self::Verifying => "Verifying",
            Self::QueuedDownload => "Queued",
            Self::Downloading => "Downloading",
            Self::QueuedSeed => "Queued (Seed)",
            Self::Seeding => "Seeding",
        }
    }
}

/// State of the copy operation for a download entry.
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq, Default)]
pub enum CopyState {
    /// Not yet copied (waiting for download to complete or dest to be configured).
    #[default]
    NotCopied,
    /// Copy is currently in progress.
    Copying,
    /// Successfully copied to the destination directory.
    Copied,
    /// Copy failed (will be retried on next cycle).
    Failed,
}

impl CopyState {
    /// Unicode indicator for display in the UI.
    pub fn indicator(&self) -> &'static str {
        match self {
            Self::NotCopied => "",
            Self::Copying => "\u{23F3}", // hourglass
            Self::Copied => "\u{2705}",  // green check
            Self::Failed => "\u{274C}",  // red cross
        }
    }
}

/// A torrent as reported by the Transmission RPC daemon.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct TransmissionTorrent {
    pub id: i64,
    pub name: String,
    pub hash_string: String,
    pub status: TransmissionStatus,
    /// 0.0 to 1.0
    pub percent_done: f64,
    /// Bytes per second
    pub rate_download: i64,
    /// Bytes per second
    pub rate_upload: i64,
    /// Seconds remaining, -1 if unknown, -2 if not applicable
    pub eta: i64,
    /// Total size in bytes when download is complete
    pub size_when_done: i64,
    /// Number of peers connected
    pub peers_connected: i64,
    /// Number of peers sending data to us
    pub peers_sending_to_us: i64,
    /// Number of peers we are sending data to
    pub peers_getting_from_us: i64,
    /// Error code (0 = OK)
    pub error: i64,
    /// Human-readable error string
    pub error_string: String,
    /// Filesystem path where Transmission is storing this torrent's data.
    pub download_dir: Option<String>,
    /// The destination this torrent is assigned to (from our ledger), if any.
    pub destination: Option<Destination>,
    /// Copy state for this torrent's files.
    #[serde(default)]
    pub copy_state: CopyState,
}

/// An entry in the persistent downloads ledger.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct DownloadEntry {
    pub info_hash: String,
    pub name: String,
    pub destination: Destination,
    /// State of the copy operation.
    #[serde(default)]
    pub copy_state: CopyState,
}

/// Configuration for connecting to a Transmission RPC daemon.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct TransmissionConfig {
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    /// Destination directory for completed movie downloads.
    #[serde(default)]
    pub movies_dir: Option<String>,
    /// Destination directory for completed TV show downloads.
    #[serde(default)]
    pub shows_dir: Option<String>,
}

impl Default for TransmissionConfig {
    fn default() -> Self {
        Self {
            host: "localhost".into(),
            port: 9091,
            username: None,
            password: None,
            movies_dir: None,
            shows_dir: None,
        }
    }
}

impl TransmissionConfig {
    /// Get the destination directory for a given destination kind.
    pub fn dir_for(&self, dest: Destination) -> Option<&str> {
        match dest {
            Destination::Movies => self.movies_dir.as_deref(),
            Destination::Shows => self.shows_dir.as_deref(),
        }
    }
}

/// Info about a torrent file.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct Torrent {
    pub added: String,
    pub category: String,
    pub descr: Option<String>,
    pub download_count: Option<String>,
    pub id: String,
    pub info_hash: String,
    pub leechers: String,
    pub name: String,
    pub num_files: Option<String>,
    pub seeders: String,
    pub size: String,
    pub status: String,
    pub username: String,
    pub magnet: Option<String>,
}

impl Torrent {
    pub fn added_i64(&self) -> i64 {
        self.added.parse().unwrap_or_default()
    }

    pub fn seeders_i64(&self) -> i64 {
        self.seeders.parse().unwrap_or_default()
    }

    pub fn leechers_i64(&self) -> i64 {
        self.leechers.parse().unwrap_or_default()
    }

    pub fn size_bytes(&self) -> usize {
        self.size.parse().unwrap_or_default()
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct TorrentInfo {
    pub added: i64,
    pub category: u32,
    pub descr: Option<String>,
    pub download_count: Option<String>,
    pub id: u32,
    pub info_hash: String,
    pub leechers: u32,
    pub name: String,
    pub num_files: Option<u32>,
    pub seeders: u32,
    pub size: u64,
    pub status: String,
    pub username: String,
    pub magnet: Option<String>,
}

/// Categorises errors so the frontend can branch on the kind.
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub enum ErrorKind {
    /// Privateer search/info API errors (network, parsing, etc.).
    PirateSearch,
    /// Could not connect to the Transmission RPC daemon.
    TransmissionConnection,
    /// Transmission RPC returned a non-OK response.
    TransmissionRpc,
    /// Configuration file I/O or serialisation errors.
    Config,
    /// A URL could not be parsed.
    InvalidUrl,
    /// Serialisation / deserialisation errors on the invoke bridge.
    Serialization,
    /// Filesystem copy operation failed.
    Copy,
}

/// Application error sent across the Tauri invoke bridge.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct AppError {
    pub kind: ErrorKind,
    pub message: String,
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl AppError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}
