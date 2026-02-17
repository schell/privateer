//! Torrent detail view.
use std::ops::Deref;

use futures_lite::FutureExt;
use human_repr::HumanCount;
use iti::components::alert::Alert;
use iti::components::button::Button;
use iti::components::icon::IconGlyph;
use iti::components::Flavor;
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
    back_button: Button<V>,
    status_alert: Alert<V>,
    phase: Proxy<TorrentDetailPhase>,
    detail_form: Option<V::Element>,
    on_click_magnet_link: Option<V::EventListener>,
}

impl<V: View> Default for TorrentDetail<V> {
    fn default() -> Self {
        let phase = Proxy::<TorrentDetailPhase>::default();
        let mut back_button = Button::new("Back", Some(Flavor::Secondary));
        back_button.get_icon_mut().set_glyph(IconGlyph::ArrowLeft);
        let status_alert = Alert::new("", Flavor::Info);
        status_alert.set_is_visible(false);
        rsx! {
            let wrapper = div() {
                div(class = "mb-3") {
                    {&back_button}
                }
                div(class = "mb-3") {
                    {&status_alert}
                }
            }
        }
        Self {
            wrapper,
            back_button,
            status_alert,
            phase,
            detail_form: None,
            on_click_magnet_link: None,
        }
    }
}

impl<V: View> TorrentDetail<V> {
    fn detail_form(info: &TorrentInfo) -> (V::Element, V::EventListener) {
        rsx! {
            let wrapper = div(style:text_align = "left") {
                h5(class = "mb-2") { "Details" }
                div(class = "table-responsive mb-3") {
                    table(class = "table table-bordered") {
                        thead() {
                            tr() {
                                th() { "Name" }
                                th() { "Added" }
                                th() { "Seeders" }
                                th() { "Leechers" }
                                th() { "Files" }
                                th() { "Size" }
                                th() { "Downloads" }
                                th() { "Status" }
                                th() { "User" }
                            }
                        }
                        tbody() {
                            tr() {
                                td() { {&info.name} }
                                td() { {super::format_unix_timestamp_with_locale(info.added)} }
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
                }
                div(class = "description") {
                    div(class = "mb-3", on:click = on_click) {
                        {{info.magnet.as_ref().map(|_| {
                            rsx! {
                                let a = a(
                                    href = "#",
                                    class = "btn btn-outline-primary",
                                ){ "Open the magnet link" }
                            }
                            a
                        })}}
                    }
                    h5(class = "mb-2") { "Description" }
                    pre(class = "bg-light p-3 border rounded", style:text_align = "left") {
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
        match &phase {
            TorrentDetailPhase::Init => {
                self.status_alert.set_is_visible(false);
            }
            TorrentDetailPhase::Getting(t) => {
                self.status_alert
                    .set_text(format!("Retrieving details on '{}'...", t.name));
                self.status_alert.set_flavor(Flavor::Info);
                self.status_alert.set_is_visible(true);
            }
            TorrentDetailPhase::Err(Error { msg }) => {
                self.status_alert.set_text(format!("Error: {msg}"));
                self.status_alert.set_flavor(Flavor::Danger);
                self.status_alert.set_is_visible(true);
            }
            TorrentDetailPhase::Details(info) => {
                self.status_alert.set_is_visible(false);
                let (detail, on_click_magnet) = Self::detail_form(info);
                self.wrapper.append_child(&detail);
                self.detail_form = Some(detail);
                self.on_click_magnet_link = Some(on_click_magnet);
            }
        }
        self.phase.set(phase);
    }

    pub async fn step(&self) {
        loop {
            if let Some(on_click_magnet) = self.on_click_magnet_link.as_ref() {
                log::info!("step details with magnet");
                let clicked_back = self
                    .back_button
                    .step()
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
                self.back_button.step().await;
                break;
            }
        }
    }
}
