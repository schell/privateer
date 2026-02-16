//! Wire types for sending between BE<->FE.

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

/// Any error.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct Error {
    pub msg: String,
}

impl<T: ToString> From<T> for Error {
    fn from(value: T) -> Self {
        let msg = value.to_string();
        Self { msg }
    }
}
