use privateer_wire_types::{
    AppError, CopyState, Destination, DownloadEntry, Torrent, TorrentInfo, TransmissionConfig,
    TransmissionStatus, TransmissionTorrent,
};
use piratebay::pirateclient::PirateClient;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{Manager, State};
use tokio::sync::{Mutex, Notify};
use transmission_rpc::types::{BasicAuth, TorrentGetField};
use transmission_rpc::TransClient;

mod error;
use error::*;
use snafu::ResultExt;

// ---------------------------------------------------------------------------
// App state
// ---------------------------------------------------------------------------

struct App {
    client: PirateClient,
    transmission_config: Mutex<TransmissionConfig>,
    config_path: PathBuf,
    downloads_ledger: Mutex<Vec<DownloadEntry>>,
    ledger_path: PathBuf,
    /// Signal the background copy task to wake up immediately.
    copy_notify: Arc<Notify>,
}

impl App {
    fn new(config_path: PathBuf, ledger_path: PathBuf) -> Self {
        let config = Self::load_config(&config_path);
        let ledger = Self::load_ledger(&ledger_path);
        Self {
            client: PirateClient::new(),
            transmission_config: Mutex::new(config),
            config_path,
            downloads_ledger: Mutex::new(ledger),
            ledger_path,
            copy_notify: Arc::new(Notify::new()),
        }
    }

    fn load_config(path: &PathBuf) -> TransmissionConfig {
        if path.exists() {
            match std::fs::read_to_string(path) {
                Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
                Err(_) => TransmissionConfig::default(),
            }
        } else {
            TransmissionConfig::default()
        }
    }

    fn save_config(path: &PathBuf, config: &TransmissionConfig) -> Result<(), ConfigError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).context(CreateDirSnafu {
                path: parent.to_path_buf(),
            })?;
        }
        let json = serde_json::to_string_pretty(config).context(SerializeSnafu)?;
        std::fs::write(path, json).context(WriteFileSnafu {
            path: path.to_path_buf(),
        })?;
        Ok(())
    }

    fn load_ledger(path: &PathBuf) -> Vec<DownloadEntry> {
        if path.exists() {
            match std::fs::read_to_string(path) {
                Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
                Err(_) => Vec::new(),
            }
        } else {
            Vec::new()
        }
    }

    fn save_ledger(path: &PathBuf, ledger: &[DownloadEntry]) -> Result<(), ConfigError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).context(CreateDirSnafu {
                path: parent.to_path_buf(),
            })?;
        }
        let json = serde_json::to_string_pretty(ledger).context(SerializeSnafu)?;
        std::fs::write(path, json).context(WriteFileSnafu {
            path: path.to_path_buf(),
        })?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Transmission helpers
// ---------------------------------------------------------------------------

fn make_trans_client(config: &TransmissionConfig) -> Result<TransClient, TransmissionError> {
    let url_str = format!("http://{}:{}/transmission/rpc", config.host, config.port);
    let url: url::Url = url_str.parse().context(InvalidUrlSnafu {
        url: url_str.clone(),
    })?;

    let client = if let (Some(user), Some(password)) = (&config.username, &config.password) {
        if !user.is_empty() {
            TransClient::with_auth(
                url,
                BasicAuth {
                    user: user.clone(),
                    password: password.clone(),
                },
            )
        } else {
            TransClient::new(url)
        }
    } else {
        TransClient::new(url)
    };

    Ok(client)
}

fn transmission_status(status: i64) -> TransmissionStatus {
    match status {
        0 => TransmissionStatus::Stopped,
        1 => TransmissionStatus::QueuedVerify,
        2 => TransmissionStatus::Verifying,
        3 => TransmissionStatus::QueuedDownload,
        4 => TransmissionStatus::Downloading,
        5 => TransmissionStatus::QueuedSeed,
        6 => TransmissionStatus::Seeding,
        _ => TransmissionStatus::Stopped,
    }
}

// ---------------------------------------------------------------------------
// Wire-type conversions
// ---------------------------------------------------------------------------

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

fn pb_torrent_to_wire(pb_t: piratebay::types::Torrent) -> Torrent {
    let piratebay::types::Torrent {
        added,
        category,
        descr,
        download_count,
        id,
        info_hash,
        leechers,
        name,
        num_files,
        seeders,
        size,
        status,
        username,
        magnet,
    } = pb_t;

    Torrent {
        added,
        category,
        descr,
        download_count,
        id,
        info_hash,
        leechers,
        name,
        num_files,
        seeders,
        size,
        status,
        username,
        magnet,
    }
}

fn pb_torrent_info_to_wire(pb_ti: piratebay::types::TorrentInfo) -> TorrentInfo {
    let piratebay::types::TorrentInfo {
        added,
        category,
        descr,
        download_count,
        id,
        info_hash,
        leechers,
        name,
        num_files,
        seeders,
        size,
        status,
        username,
        magnet,
    } = pb_ti;
    TorrentInfo {
        added,
        category,
        descr,
        download_count,
        id,
        info_hash,
        leechers,
        name,
        num_files,
        seeders,
        size,
        status,
        username,
        magnet,
    }
}

// ---------------------------------------------------------------------------
// Tauri commands – Privateer
// ---------------------------------------------------------------------------

#[tauri::command]
async fn search(state: State<'_, App>, query: &str) -> Result<Vec<Torrent>, AppError> {
    log::info!("searching: {query}");
    let torrents = state
        .client
        .search(query)
        .await
        .map_err(|e| PirateError::Search {
            message: e.to_string(),
        })?;
    log::info!("got {} results", torrents.len());
    let torrents = torrents
        .into_iter()
        .map(pb_torrent_to_wire)
        .collect::<Vec<_>>();
    Ok(torrents)
}

#[tauri::command]
async fn info(state: State<'_, App>, id: &str) -> Result<TorrentInfo, AppError> {
    log::info!("info: {id}");
    let torrent = state
        .client
        .get_info(id)
        .await
        .map_err(|e| PirateError::Info {
            message: e.to_string(),
        })?;
    Ok(pb_torrent_info_to_wire(torrent))
}

// ---------------------------------------------------------------------------
// Tauri commands – Transmission config
// ---------------------------------------------------------------------------

#[tauri::command]
async fn get_transmission_config(state: State<'_, App>) -> Result<TransmissionConfig, AppError> {
    let config = state.transmission_config.lock().await;
    Ok(config.clone())
}

#[tauri::command]
async fn set_transmission_config(
    state: State<'_, App>,
    config: TransmissionConfig,
) -> Result<(), AppError> {
    App::save_config(&state.config_path, &config)?;
    let mut current = state.transmission_config.lock().await;
    *current = config;
    Ok(())
}

#[tauri::command]
async fn test_transmission_connection(state: State<'_, App>) -> Result<String, AppError> {
    let config = state.transmission_config.lock().await;
    let mut client = make_trans_client(&config)?;
    let response = client
        .session_get()
        .await
        .map_err(|e| TransmissionError::Connection {
            message: e.to_string(),
        })?;
    if response.is_ok() {
        let version = if response.arguments.version.is_empty() {
            "unknown".to_string()
        } else {
            response.arguments.version
        };
        Ok(format!("Connected to Transmission {version}"))
    } else {
        Err(AppError::from(TransmissionError::Rpc {
            message: response.result,
        }))
    }
}

// ---------------------------------------------------------------------------
// Tauri commands – Torrents & ledger
// ---------------------------------------------------------------------------

#[tauri::command]
async fn get_torrents(state: State<'_, App>) -> Result<Vec<TransmissionTorrent>, AppError> {
    let config = state.transmission_config.lock().await;
    let mut client = make_trans_client(&config)?;

    let fields = vec![
        TorrentGetField::Id,
        TorrentGetField::Name,
        TorrentGetField::HashString,
        TorrentGetField::Status,
        TorrentGetField::PercentDone,
        TorrentGetField::RateDownload,
        TorrentGetField::RateUpload,
        TorrentGetField::Eta,
        TorrentGetField::SizeWhenDone,
        TorrentGetField::PeersConnected,
        TorrentGetField::PeersSendingToUs,
        TorrentGetField::PeersGettingFromUs,
        TorrentGetField::Error,
        TorrentGetField::ErrorString,
        TorrentGetField::DownloadDir,
    ];

    let response = client.torrent_get(Some(fields), None).await.map_err(|e| {
        TransmissionError::Connection {
            message: e.to_string(),
        }
    })?;

    if !response.is_ok() {
        return Err(AppError::from(TransmissionError::Rpc {
            message: response.result,
        }));
    }

    let ledger = state.downloads_ledger.lock().await;

    let torrents = response
        .arguments
        .torrents
        .into_iter()
        .map(|t| {
            let hash_string = t.hash_string.clone().unwrap_or_default();
            let download_dir = t.download_dir.clone();
            let name = t.name.clone().unwrap_or_default();

            // Cross-reference with the ledger
            let ledger_entry = ledger
                .iter()
                .find(|e| e.info_hash.eq_ignore_ascii_case(&hash_string));

            let (destination, copy_state) = match ledger_entry {
                Some(entry) => {
                    let state = match entry.copy_state {
                        // If not yet copied, check whether it already exists
                        // at the destination (e.g. manually copied).
                        CopyState::NotCopied | CopyState::Failed => {
                            if check_already_copied(&config, entry.destination, &name) {
                                CopyState::Copied
                            } else {
                                entry.copy_state
                            }
                        }
                        other => other,
                    };
                    (Some(entry.destination), state)
                }
                None => {
                    // Not in ledger — check whether the torrent's files
                    // already exist at either destination directory.
                    match detect_destination(&config, &name) {
                        Some((dest, state)) => (Some(dest), state),
                        None => (None, CopyState::default()),
                    }
                }
            };

            TransmissionTorrent {
                id: t.id.unwrap_or(-1),
                name,
                hash_string,
                status: transmission_status(t.status.map(|s| s as i64).unwrap_or(0)),
                percent_done: t.percent_done.unwrap_or(0.0) as f64,
                rate_download: t.rate_download.unwrap_or(0),
                rate_upload: t.rate_upload.unwrap_or(0),
                eta: t.eta.unwrap_or(-1),
                size_when_done: t.size_when_done.unwrap_or(0),
                peers_connected: t.peers_connected.unwrap_or(0),
                peers_sending_to_us: t.peers_sending_to_us.unwrap_or(0),
                peers_getting_from_us: t.peers_getting_from_us.unwrap_or(0),
                error: t.error.map(|e| e as i64).unwrap_or(0),
                error_string: t.error_string.unwrap_or_default(),
                download_dir,
                destination,
                copy_state,
            }
        })
        .collect();

    Ok(torrents)
}

/// Check whether a torrent's files already exist at the destination.
fn check_already_copied(config: &TransmissionConfig, dest: Destination, name: &str) -> bool {
    if let Some(dir) = config.dir_for(dest) {
        let dest_path = PathBuf::from(dir).join(name);
        dest_path.exists()
    } else {
        false
    }
}

/// Detect whether a torrent already exists at either destination directory.
///
/// Checks `movies_dir` first, then `shows_dir`. Returns the destination
/// and `CopyState::Copied` if the torrent's files are found on disk,
/// or `None` if the torrent doesn't exist at either location.
fn detect_destination(
    config: &TransmissionConfig,
    name: &str,
) -> Option<(Destination, CopyState)> {
    for dest in [Destination::Movies, Destination::Shows] {
        if let Some(dir) = config.dir_for(dest) {
            if !dir.is_empty() {
                let path = PathBuf::from(dir).join(name);
                if path.exists() {
                    return Some((dest, CopyState::Copied));
                }
            }
        }
    }
    None
}

#[tauri::command]
async fn add_download(
    state: State<'_, App>,
    info_hash: String,
    name: String,
    destination: Destination,
) -> Result<(), AppError> {
    log::info!("adding download '{name}' to downloads.json...");
    let mut ledger = state.downloads_ledger.lock().await;

    // Check if already tracked
    if let Some(entry) = ledger
        .iter_mut()
        .find(|e| e.info_hash.eq_ignore_ascii_case(&info_hash))
    {
        // Update destination if changed
        entry.destination = destination;
        entry.copy_state = CopyState::NotCopied;
    } else {
        ledger.push(DownloadEntry {
            info_hash,
            name,
            destination,
            copy_state: CopyState::NotCopied,
        });
    }

    App::save_ledger(&state.ledger_path, &ledger)?;
    // Wake the background copy task so it picks up this entry immediately
    // instead of waiting for the next 30-second cycle.
    state.copy_notify.notify_one();
    log::info!("...done.");
    Ok(())
}

#[tauri::command]
async fn get_downloads_ledger(state: State<'_, App>) -> Result<Vec<DownloadEntry>, AppError> {
    let ledger = state.downloads_ledger.lock().await;
    Ok(ledger.clone())
}

// ---------------------------------------------------------------------------
// Background copy task
// ---------------------------------------------------------------------------

/// Recursively copy `src` to `dst` using async I/O (tokio::fs).
///
/// This avoids blocking the tokio runtime when copying large files to slow
/// destinations (e.g. a NAS with spinning disks).
async fn copy_recursive_async(src: &std::path::Path, dst: &std::path::Path) -> Result<(), CopyError> {
    if src.is_dir() {
        tokio::fs::create_dir_all(dst).await.context(CopyCreateDirSnafu {
            path: dst.to_path_buf(),
        })?;
        let mut read_dir = tokio::fs::read_dir(src).await.context(CopyReadDirSnafu {
            path: src.to_path_buf(),
        })?;
        while let Some(entry) = read_dir.next_entry().await.context(CopyReadDirSnafu {
            path: src.to_path_buf(),
        })? {
            let child_src = entry.path();
            let child_dst = dst.join(entry.file_name());
            Box::pin(copy_recursive_async(&child_src, &child_dst)).await?;
        }
    } else {
        // Single file
        if let Some(parent) = dst.parent() {
            tokio::fs::create_dir_all(parent).await.context(CopyCreateDirSnafu {
                path: parent.to_path_buf(),
            })?;
        }
        tokio::fs::copy(src, dst).await.context(CopyFileSnafu {
            src: src.to_path_buf(),
            dst: dst.to_path_buf(),
        })?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// App entry point
// ---------------------------------------------------------------------------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::builder().init();
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            #[cfg(debug_assertions)]
            {
                let window = app.get_webview_window("main").unwrap();
                window.open_devtools();
                // window.close_devtools();
            }

            let app_data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| PathBuf::from("."));
            let config_path = app_data_dir.join("transmission_config.json");
            let ledger_path = app_data_dir.join("downloads.json");

            let app_state = App::new(config_path, ledger_path);

            // Spawn the background copy task.
            // The task reads config and ledger from disk each cycle so it
            // always sees the latest saved state without sharing Mutex refs.
            let copy_config_path = app_state.config_path.clone();
            let copy_ledger_path = app_state.ledger_path.clone();
            let copy_notify = app_state.copy_notify.clone();

            app.manage(app_state);

            tauri::async_runtime::spawn(async move {
                copy_task_from_disk(copy_config_path, copy_ledger_path, copy_notify).await;
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            search,
            info,
            get_transmission_config,
            set_transmission_config,
            test_transmission_connection,
            get_torrents,
            add_download,
            get_downloads_ledger,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Background copy task that reads config/ledger from disk each cycle.
///
/// Uses async I/O (`tokio::fs`) so large copies to slow NAS drives don't
/// block the tokio runtime.  State transitions are persisted to the ledger
/// file so the frontend can show real-time progress:
///
///   NotCopied/Failed  →  Copying  →  Copied | Failed
async fn copy_task_from_disk(config_path: PathBuf, ledger_path: PathBuf, notify: Arc<Notify>) {
    loop {
        // Wait for either the 30-second interval or an explicit wake-up
        // from `add_download`.
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_secs(30)) => {}
            _ = notify.notified() => {
                log::info!("Copy task: woken up by add_download");
            }
        }

        let config = App::load_config(&config_path);
        let mut ledger = App::load_ledger(&ledger_path);

        // Connect to Transmission to get torrent statuses.
        // We need the torrent list for both reconciliation and copying.
        let mut client = match make_trans_client(&config) {
            Ok(c) => c,
            Err(e) => {
                log::warn!("Copy task: cannot connect to Transmission: {e}");
                continue;
            }
        };

        let fields = vec![
            TorrentGetField::HashString,
            TorrentGetField::Name,
            TorrentGetField::Status,
            TorrentGetField::PercentDone,
            TorrentGetField::DownloadDir,
        ];

        let response = match client.torrent_get(Some(fields), None).await {
            Ok(r) => r,
            Err(e) => {
                log::warn!("Copy task: torrent_get failed: {e}");
                continue;
            }
        };

        if !response.is_ok() {
            log::warn!("Copy task: RPC error: {}", response.result);
            continue;
        }

        let transmission_torrents = response.arguments.torrents;

        // -----------------------------------------------------------------
        // Reconciliation: scan Transmission torrents and update the ledger.
        //
        // 1. Untracked torrents whose files exist at a destination dir
        //    → auto-add to ledger as Copied.
        // 2. Stale states (NotCopied/Failed but files exist at dest)
        //    → update to Copied.
        // -----------------------------------------------------------------
        let mut ledger_changed = false;

        for tt in &transmission_torrents {
            let hash = match tt.hash_string.as_deref() {
                Some(h) => h,
                None => continue,
            };
            let name = match tt.name.as_deref() {
                Some(n) => n,
                None => continue,
            };

            let existing = ledger
                .iter_mut()
                .find(|e| e.info_hash.eq_ignore_ascii_case(hash));

            match existing {
                Some(entry) => {
                    // Fix stale states: ledger says NotCopied/Failed but
                    // files already exist at the destination.
                    if matches!(entry.copy_state, CopyState::NotCopied | CopyState::Failed) {
                        if check_already_copied(&config, entry.destination, name) {
                            log::info!(
                                "Reconcile: '{name}' already at {}, marking Copied",
                                entry.destination
                            );
                            entry.copy_state = CopyState::Copied;
                            ledger_changed = true;
                        }
                    }
                }
                None => {
                    // Not in ledger — check whether files exist at either
                    // destination. If so, auto-add as Copied.
                    if let Some((dest, state)) = detect_destination(&config, name) {
                        log::info!(
                            "Reconcile: auto-adding '{name}' to ledger as {dest} ({:?})",
                            state
                        );
                        ledger.push(DownloadEntry {
                            info_hash: hash.to_string(),
                            name: name.to_string(),
                            destination: dest,
                            copy_state: state,
                        });
                        ledger_changed = true;
                    }
                }
            }
        }

        if ledger_changed {
            if let Err(e) = App::save_ledger(&ledger_path, &ledger) {
                log::error!("Copy task: failed to save ledger after reconciliation: {e}");
            }
        }

        // -----------------------------------------------------------------
        // Copy pending entries
        // -----------------------------------------------------------------

        // Find entries eligible for copying (not yet copied, not currently copying)
        let pending: Vec<usize> = ledger
            .iter()
            .enumerate()
            .filter(|(_, e)| matches!(e.copy_state, CopyState::NotCopied | CopyState::Failed))
            .map(|(i, _)| i)
            .collect();

        if pending.is_empty() {
            continue;
        }

        for idx in pending {
            // Gather all needed values upfront so we don't hold a borrow on
            // `ledger` across the mutation points below.
            let info_hash = ledger[idx].info_hash.clone();
            let entry_name = ledger[idx].name.clone();
            let destination = ledger[idx].destination;

            // Find the matching torrent in Transmission
            let trans_torrent = transmission_torrents.iter().find(|t| {
                t.hash_string
                    .as_deref()
                    .map(|h| h.eq_ignore_ascii_case(&info_hash))
                    .unwrap_or(false)
            });

            let trans_torrent = match trans_torrent {
                Some(t) => t,
                None => continue,
            };

            let percent = trans_torrent.percent_done.unwrap_or(0.0);
            if percent < 1.0 {
                continue;
            }

            let torrent_name = trans_torrent
                .name
                .clone()
                .unwrap_or_else(|| entry_name.clone());
            let download_dir = match trans_torrent.download_dir.as_deref() {
                Some(d) => d.to_string(),
                None => {
                    log::warn!("Copy task: no download_dir for torrent '{entry_name}'");
                    continue;
                }
            };

            let dest_dir = match config.dir_for(destination) {
                Some(d) if !d.is_empty() => d.to_string(),
                _ => {
                    log::debug!(
                        "Copy task: no destination dir configured for {destination} (torrent '{entry_name}')",
                    );
                    continue;
                }
            };

            let src_path = PathBuf::from(&download_dir).join(&torrent_name);
            let dst_path = PathBuf::from(&dest_dir).join(&torrent_name);

            // Already at destination — mark Copied without re-copying
            if dst_path.exists() {
                log::info!(
                    "Copy task: '{}' already exists at destination, marking copied",
                    torrent_name
                );
                ledger[idx].copy_state = CopyState::Copied;
                let _ = App::save_ledger(&ledger_path, &ledger);
                continue;
            }

            if !src_path.exists() {
                log::warn!(
                    "Copy task: source '{}' does not exist, skipping",
                    src_path.display()
                );
                continue;
            }

            // Transition: → Copying  (persist immediately so the UI updates)
            ledger[idx].copy_state = CopyState::Copying;
            if let Err(e) = App::save_ledger(&ledger_path, &ledger) {
                log::error!("Copy task: failed to save ledger (Copying): {e}");
            }

            log::info!(
                "Copy task: copying '{}' -> '{}'",
                src_path.display(),
                dst_path.display()
            );

            match copy_recursive_async(&src_path, &dst_path).await {
                Ok(()) => {
                    log::info!("Copy task: successfully copied '{}'", torrent_name);
                    ledger[idx].copy_state = CopyState::Copied;
                }
                Err(e) => {
                    log::error!("Copy task: failed to copy '{}': {e}", torrent_name);
                    ledger[idx].copy_state = CopyState::Failed;
                    // Clean up partial copy on failure
                    if dst_path.exists() {
                        let _ = if dst_path.is_dir() {
                            tokio::fs::remove_dir_all(&dst_path).await
                        } else {
                            tokio::fs::remove_file(&dst_path).await
                        };
                    }
                }
            }

            // Persist Copied/Failed state
            if let Err(e) = App::save_ledger(&ledger_path, &ledger) {
                log::error!("Copy task: failed to save ledger: {e}");
            }
        }
    }
}
