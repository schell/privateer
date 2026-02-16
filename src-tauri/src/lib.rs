use pb_wire_types::{Error, Torrent, TorrentInfo};
use piratebay::pirateclient::PirateClient;
use tauri::{Manager, State};

struct App {
    client: PirateClient,
}

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

#[tauri::command]
async fn search(state: State<'_, App>, query: &str) -> Result<Vec<Torrent>, Error> {
    log::info!("searching: {query}");
    let torrents = state.client.search(query).await?;
    log::info!("got {} results", torrents.len());
    let torrents = torrents
        .into_iter()
        .map(pb_torrent_to_wire)
        .collect::<Vec<_>>();
    Ok(torrents)
}

#[tauri::command]
async fn info(state: State<'_, App>, id: &str) -> Result<TorrentInfo, Error> {
    log::info!("info: {id}");
    let torrent = state.client.get_info(id).await?;
    Ok(pb_torrent_info_to_wire(torrent))
}

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
            app.manage(App {
                client: PirateClient::new(),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![greet, search, info])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
