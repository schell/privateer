//! Torrent detail view.
use std::ops::Deref;

use futures_lite::FutureExt;
use human_repr::HumanCount;
use mogwai::{future::MogwaiFutureExt, web::prelude::*};
use pb_wire_types::{Error, Torrent, TorrentInfo};
use wasm_bindgen::prelude::*;

mod open {
    use super::*;

    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "opener"])]
        async fn openUrl(path: &str);
    }

    pub async fn path(path: &str) {
        log::info!("opening path: {path}");
        openUrl(path).await
    }
}

#[derive(Clone, Default, Debug, PartialEq)]
pub enum TorrentDetailPhase {
    #[default]
    Init,
    Getting(Torrent),
    Err(Error),
    Details(TorrentInfo),
}

#[derive(ViewChild)]
pub struct TorrentDetail<V: View> {
    #[child]
    wrapper: V::Element,
    on_click_back: V::EventListener,
    phase: Proxy<TorrentDetailPhase>,
    detail_form: Option<V::Element>,
    on_click_magnet_link: Option<V::EventListener>,
}

impl<V: View> Default for TorrentDetail<V> {
    fn default() -> Self {
        let mut phase = Proxy::<TorrentDetailPhase>::default();
        rsx! {
            let wrapper = div() {
                div(class = "row") {
                    span(
                        on:click = on_click_back,
                        style:cursor = "pointer"
                    ) {"🔙"}
                }
                p() {
                    {phase(p => match p {
                        TorrentDetailPhase::Init => {
                            "No details".into_text::<V>()
                        }
                        TorrentDetailPhase::Getting(t) => {
                            format!("Retrieving details on '{}'", t.name).into_text::<V>()
                        }
                        TorrentDetailPhase::Err(Error{msg}) => {
                            format!("Error: {msg}").into_text::<V>()
                        }
                        TorrentDetailPhase::Details(_) => V::Text::new(""),
                    })}
                }
            }
        }
        Self {
            wrapper,
            on_click_back,
            phase,
            detail_form: None,
            on_click_magnet_link: None,
        }
    }
}

impl<V: View> TorrentDetail<V> {
    fn detail_form(info: &TorrentInfo) -> (V::Element, V::EventListener) {
        const HEADERS: &[usize] = &[4, 2, 1, 1, 1, 1, 1, 1, 1, 1];
        fn width_at(n: usize) -> String {
            let total: usize = HEADERS.iter().sum();
            let w = HEADERS.get(n).copied().unwrap_or_default();
            format!("{}%", 100.0 * (w as f32 / total as f32))
        }

        rsx! {
            let wrapper = div(style:text_align = "left") {
                fieldset() {
                    legend(){ "Details" }
                    table() {
                        colgroup() {
                            {(0..HEADERS.len()).map(|i| {
                                rsx!{
                                    let col = col(style:width = width_at(i)){}
                                }
                                col
                            }).collect::<Vec<_>>()}
                        }
                        tr() {
                            th() { "Name"}
                            th() { "Added" }
                            th() { "Seeders" }
                            th() { "Leechers" }
                            th() { "Files"}
                            th() { "Size" }
                            th() { "Downloads" }
                            th() { "Status" }
                            th() { "User" }
                        }
                        tr() {
                            td() { {&info.name} }
                            td() { {super::format_unix_timestamp_with_locale(info.added)}}
                            td() { {info.seeders.to_string()} }
                            td() { {info.leechers.to_string()} }
                            td() { {info.num_files.map(|i| i.to_string()).unwrap_or("unknown".to_string())} }
                            td() { {info.size.human_count_bytes().to_string()} }
                            td() { {info.download_count.clone().unwrap_or("?".into())} }
                            td() { {&info.status} }
                            td() { {&info.username} }
                        }
                    }
                }
                fieldset(class = "description") {
                    legend() { "Description" }
                    div(on:click = on_click) {
                        {{info.magnet.as_ref().map(|_| {
                            rsx! {
                                let a = a(
                                    href = "#",
                                ){ "Open the magnet link" }
                            }
                            a
                        })}}
                    }
                    pre(style:text_align = "left") {
                        {info.descr.clone().unwrap_or_default()}
                    }
                }
            }
        }
        (wrapper, on_click)
    }

    pub fn set_phase(&mut self, phase: TorrentDetailPhase) {
        self.on_click_magnet_link.take();
        if let Some(detail) = self.detail_form.take() {
            self.wrapper.remove_child(&detail);
        }
        if let TorrentDetailPhase::Details(info) = &phase {
            let (detail, on_click_magnet) = Self::detail_form(info);
            self.wrapper.append_child(&detail);
            self.detail_form = Some(detail);
            self.on_click_magnet_link = Some(on_click_magnet);
        }
        self.phase.set(phase);
    }

    pub async fn step(&self) {
        loop {
            if let Some(on_click_magnet) = self.on_click_magnet_link.as_ref() {
                log::info!("step details with magnet");
                let clicked_back = self
                    .on_click_back
                    .next()
                    .map(|_| true)
                    .or(on_click_magnet.next().map(|_| false))
                    .await;
                if clicked_back {
                    break;
                } else {
                    // clicked the magnet link
                    log::info!("clicked the magnet");
                    if let TorrentDetailPhase::Details(info) = self.phase.deref() {
                        if let Some(link) = info.magnet.as_ref() {
                            open::path(link).await;
                        }
                    }
                }
            } else {
                self.on_click_back.next().await;
            }
        }
    }
}
