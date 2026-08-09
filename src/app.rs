use std::borrow::Cow;

use downloads::DownloadsView;
use futures_lite::FutureExt;
use iti::components::tab::{TabListEvent, TabPanel, TabPanelEvent};
use mogwai::view::AppendArg;
use mogwai::web::prelude::*;
use privateer_wire_types::*;
use settings::SettingsView;
use wasm_bindgen::prelude::*;

use crate::app::search::SearchTabContent;

mod detail;
mod downloads;
mod search;
mod settings;
pub mod watching;

pub mod invoke {
    use super::*;

    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], catch)]
        async fn invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;
    }

    fn deserialize_as<T: serde::de::DeserializeOwned>(value: JsValue) -> Result<T, AppError> {
        match serde_wasm_bindgen::from_value::<T>(value) {
            Ok(t) => Ok(t),
            Err(e) => {
                log::error!("e: {e:#?}");
                Err(AppError::new(
                    ErrorKind::Serialization,
                    "Could not deserialize",
                ))
            }
        }
    }

    pub async fn cmd<T: serde::Serialize, X: serde::de::DeserializeOwned>(
        name: &str,
        args: &T,
    ) -> Result<X, AppError> {
        let value = serde_wasm_bindgen::to_value(args).map_err(|e| {
            AppError::new(
                ErrorKind::Serialization,
                format!("could not serialize {}: {e}", std::any::type_name::<T>()),
            )
        })?;
        let result = invoke(name, value).await;
        match result {
            Ok(value) => deserialize_as::<X>(value),
            Err(e) => Err(deserialize_as::<AppError>(e)?),
        }
    }
}

pub async fn search(query: &str) -> Result<Vec<Torrent>, AppError> {
    #[derive(serde::Serialize)]
    struct Query<'a> {
        query: &'a str,
    }

    invoke::cmd("search", &Query { query }).await
}

pub async fn info(id: &str) -> Result<TorrentInfo, AppError> {
    #[derive(serde::Serialize)]
    struct Info<'a> {
        id: &'a str,
    }

    invoke::cmd("info", &Info { id }).await
}

pub async fn add_download(
    info_hash: &str,
    name: &str,
    destination: Destination,
) -> Result<(), AppError> {
    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct AddDownloadArgs<'a> {
        info_hash: &'a str,
        name: &'a str,
        destination: Destination,
    }

    invoke::cmd(
        "add_download",
        &AddDownloadArgs {
            info_hash,
            name,
            destination,
        },
    )
    .await
}

pub async fn get_watchlist() -> Result<Vec<WatchlistEntry>, AppError> {
    #[derive(serde::Serialize)]
    struct Empty {}
    invoke::cmd("get_watchlist", &Empty {}).await
}

pub async fn add_to_watchlist(
    title: &str,
    destination: Destination,
) -> Result<WatchlistEntry, AppError> {
    #[derive(serde::Serialize)]
    struct Args<'a> {
        title: &'a str,
        destination: Destination,
    }
    invoke::cmd("add_to_watchlist", &Args { title, destination }).await
}

pub async fn remove_from_watchlist(id: u64) -> Result<(), AppError> {
    #[derive(serde::Serialize)]
    struct Args {
        id: u64,
    }
    invoke::cmd("remove_from_watchlist", &Args { id }).await
}

#[allow(dead_code)]
pub async fn get_downloads_ledger() -> Result<Vec<DownloadEntry>, AppError> {
    #[derive(serde::Serialize)]
    struct Empty {}
    invoke::cmd("get_downloads_ledger", &Empty {}).await
}

pub async fn check_movie_exists(title: &str) -> Result<bool, AppError> {
    #[derive(serde::Serialize)]
    struct Args<'a> {
        title: &'a str,
    }
    invoke::cmd("check_movie_exists", &Args { title }).await
}

pub async fn check_episodes_exist(
    title: &str,
    episodes: &[(u32, u32)],
) -> Result<Vec<bool>, AppError> {
    #[derive(serde::Serialize)]
    struct Args<'a> {
        title: &'a str,
        episodes: &'a [(u32, u32)],
    }
    invoke::cmd("check_episodes_exist", &Args { title, episodes }).await
}

pub fn format_unix_timestamp_with_locale(seconds: i64) -> String {
    // Convert seconds to milliseconds
    let milliseconds = seconds as f64 * 1000.0;
    // Create a new Date object
    let date = web_sys::js_sys::Date::new(&milliseconds.into());
    // Get the user's locale
    let user_locale =
        web_sys::js_sys::Reflect::get(&web_sys::js_sys::global(), &"navigator".into())
            .and_then(|navigator| web_sys::js_sys::Reflect::get(&navigator, &"language".into()))
            .unwrap_or_else(|_| JsValue::from_str("en-US"))
            .as_string()
            .unwrap_or_else(|| "en-US".to_string());
    // Format the date using the user's locale
    date.to_locale_string(&user_locale, &JsValue::undefined())
        .into()
}

/// Enum of all top-level tab content panes.
pub enum TabContent<V: View> {
    Default(V::Element),
    Search(Box<SearchTabContent<V>>),
    Downloads(DownloadsView<V>),
    Watching(watching::WatchingView<V>),
    Settings(SettingsView<V>),
}

impl<V: View> Default for TabContent<V> {
    fn default() -> Self {
        TabContent::Default({
            rsx! {
                let view = p() {"Awaiting panes"}
            }
            view
        })
    }
}

impl<V: View> ViewChild<V> for TabContent<V> {
    fn as_append_arg(&self) -> AppendArg<V, impl Iterator<Item = Cow<'_, V::Node>>> {
        match self {
            TabContent::Search(s) => s.as_boxed_append_arg(),
            TabContent::Downloads(d) => d.as_boxed_append_arg(),
            TabContent::Watching(w) => w.as_boxed_append_arg(),
            TabContent::Settings(s) => s.as_boxed_append_arg(),
            TabContent::Default(d) => d.as_boxed_append_arg(),
        }
    }
}

impl<V: View> StepMut for TabContent<V> {
    type Output = ();
    async fn step_mut(&mut self) {
        match self {
            TabContent::Default(_) => {
                futures_lite::future::pending::<()>().await;
            }
            TabContent::Search(search_tab_content) => {
                search_tab_content.step_mut().await;
            }
            TabContent::Downloads(downloads_view) => {
                downloads_view.step_mut().await;
            }
            TabContent::Watching(watching_view) => {
                let watching_result = watching_view.step_mut().await;
                log::info!("watching result: {watching_result:?}");
            }
            TabContent::Settings(settings_view) => {
                settings_view.step_mut().await;
            }
        }
    }
}

/// Top-level application.
#[derive(ViewChild)]
pub struct App<V: View> {
    #[child]
    container: V::Element,
    tab_panel: TabPanel<V, V::Element, TabContent<V>>,
}

impl<V: View> Default for App<V> {
    fn default() -> Self {
        let mut tab_panel = TabPanel::new(TabContent::default());
        tab_panel.set_property("id", "tab-panel");
        // The panels will align right with a spacer on the left
        tab_panel.push_spacer();

        // Create tab labels
        rsx! {
            let search_label = span() { "Search" }
        }
        rsx! {
            let downloads_label = span() { "Downloads" }
        }
        rsx! {
            let watching_label = span() { "Watching" }
        }
        rsx! {
            let settings_label = span() { "Settings" }
        }

        let tab_ids = [
            tab_panel.push(search_label, TabContent::Search(Box::default())),
            tab_panel.push(
                downloads_label,
                TabContent::Downloads(DownloadsView::default()),
            ),
            tab_panel.push(
                watching_label,
                TabContent::Watching(watching::WatchingView::default()),
            ),
            tab_panel.push(
                settings_label,
                TabContent::Settings(SettingsView::default()),
            ),
        ];

        // Read the last selected index from a previous save state
        let index = iti::storage::get_item::<usize>(STORAGE_ITEM_FOR_TAB_PANEL_INDEX)
            .ok()
            .flatten()
            .unwrap_or_default();
        tab_panel.select(&tab_ids[index]);

        rsx! {
            let container = div(
                class = "container-fluid",
                style:height = "100vh",
                data_tauri_drag_region = ""
            ) {
                // Drag bar
                div(
                    style:width = "100%",
                    style:height = "35px",
                    data_tauri_drag_region = ""
                ) { }
                div(
                    class = "row",
                    data_tauri_drag_region = "",
                ) {
                    h1(
                        id = "title",
                        class = "editorial row",
                        style:color = iti::color::PURPLE,
                        style:font_weight = "lighter",
                        data_tauri_drag_region = "",
                    ) {
                        img(
                            src = "public/logo.png",
                            alt = "Privateer",
                            style:height = "28px",
                            style:color = "rgb(123, 97, 255)",
                            data_tauri_drag_region = "",
                        ){}
                        "Privateer"
                    }
                    {&tab_panel}
                }
            }
        }

        Self {
            container,
            tab_panel,
        }
    }
}

const STORAGE_ITEM_FOR_TAB_PANEL_INDEX: &str = "last-index-of-app-tab-panel";

impl<V: View> StepMut for App<V> {
    type Output = ();
    async fn step_mut(&mut self) {
        let ev = self
            .tab_panel
            .step_with_mut(|content| content.step_mut().boxed_local())
            .await;
        match ev {
            TabPanelEvent::Tabs(TabListEvent::ItemClicked {
                id: _,
                index,
                event: _,
            }) => {
                let _ = iti::storage::set_item(STORAGE_ITEM_FOR_TAB_PANEL_INDEX, &index);
            }
            TabPanelEvent::Panes(_) => {}
        }
    }
}
