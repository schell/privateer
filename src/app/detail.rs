//! Torrent detail view.
use std::ops::Deref;

use futures_lite::FutureExt;
use human_repr::HumanCount;
use iti::components::alert::Alert;
use iti::components::button::Button;
use iti::components::icon::IconGlyph;
use iti::components::Flavor;
use mogwai::{future::MogwaiFutureExt, web::prelude::*};
use privateer_wire_types::{AppError, Destination, Torrent, TorrentInfo};
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
    Err(AppError),
    Details(TorrentInfo),
}

/// Event from the detail view magnet/add button area.
enum MagnetAction {
    /// The primary "Add to <dest>" button was clicked.
    AddPrimary,
    /// The dropdown selected an alternative destination.
    AddAlternate(Destination),
}

/// Holds the split button group UI for adding a torrent with a destination.
struct AddButtonGroup<V: View> {
    wrapper: V::Element,
    on_click_primary: V::EventListener,
    on_click_toggle: V::EventListener,
    on_click_movies: V::EventListener,
    on_click_shows: V::EventListener,
    menu_open: Proxy<bool>,
    is_menu_open: bool,
    label_text: V::Text,
    /// The currently selected destination for the primary button.
    selected: Destination,
}

impl<V: View> AddButtonGroup<V> {
    fn new(default_dest: Destination) -> Self {
        let label = format!("Add to {}", default_dest.label());
        let label_text = V::Text::new(&label);
        let mut menu_open = Proxy::new(false);

        rsx! {
            let wrapper = div(class = "btn-group mb-3") {
                button(
                    class = "btn btn-outline-primary",
                    type = "button",
                    on:click = on_click_primary,
                ) {
                    {&label_text}
                }
                button(
                    class = "btn btn-outline-primary dropdown-toggle dropdown-toggle-split",
                    type = "button",
                    on:click = on_click_toggle,
                ) {
                    span(class = "visually-hidden") { "Toggle Dropdown" }
                }
                ul(
                    class = menu_open(is_open => if *is_open {
                        "dropdown-menu show"
                    } else {
                        "dropdown-menu"
                    }),
                ) {
                    li() {
                        a(
                            class = "dropdown-item",
                            href = "#",
                            on:click = on_click_movies,
                        ) { "Movies" }
                    }
                    li() {
                        a(
                            class = "dropdown-item",
                            href = "#",
                            on:click = on_click_shows,
                        ) { "Shows" }
                    }
                }
            }
        }

        Self {
            wrapper,
            on_click_primary,
            on_click_toggle,
            on_click_movies,
            on_click_shows,
            menu_open,
            is_menu_open: false,
            label_text,
            selected: default_dest,
        }
    }

    fn toggle_menu(&mut self) {
        self.is_menu_open = !self.is_menu_open;
        self.menu_open.set(self.is_menu_open);
    }

    fn hide_menu(&mut self) {
        self.is_menu_open = false;
        self.menu_open.set(false);
    }

    fn set_selected(&mut self, dest: Destination) {
        self.selected = dest;
        self.label_text.set_text(format!("Add to {}", dest.label()));
    }

    /// Wait for an action on the split button.
    async fn step(&mut self) -> MagnetAction {
        loop {
            let ev = self
                .on_click_primary
                .next()
                .map(|_| 0usize)
                .or(self.on_click_toggle.next().map(|_| 1usize))
                .or(self.on_click_movies.next().map(|_| 2usize))
                .or(self.on_click_shows.next().map(|_| 3usize))
                .await;

            match ev {
                0 => {
                    self.hide_menu();
                    return MagnetAction::AddPrimary;
                }
                1 => {
                    self.toggle_menu();
                }
                2 => {
                    self.hide_menu();
                    self.set_selected(Destination::Movies);
                    return MagnetAction::AddAlternate(Destination::Movies);
                }
                3 => {
                    self.hide_menu();
                    self.set_selected(Destination::Shows);
                    return MagnetAction::AddAlternate(Destination::Shows);
                }
                _ => unreachable!(),
            }
        }
    }
}

#[derive(ViewChild)]
pub struct TorrentDetail<V: View> {
    #[child]
    wrapper: V::Element,
    back_button: Button<V>,
    status_alert: Alert<V>,
    phase: Proxy<TorrentDetailPhase>,
    detail_form: Option<V::Element>,
    add_button_group: Option<AddButtonGroup<V>>,
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
            add_button_group: None,
        }
    }
}

impl<V: View> TorrentDetail<V> {
    fn detail_form(info: &TorrentInfo) -> (V::Element, Option<AddButtonGroup<V>>) {
        // Auto-detect destination from Privateer category
        let default_dest = Destination::from_category(info.category).unwrap_or_default();

        let add_group = info
            .magnet
            .as_ref()
            .map(|_| AddButtonGroup::<V>::new(default_dest));

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
                    {{add_group.as_ref().map(|g| &g.wrapper)}}
                    h5(class = "mb-2") { "Description" }
                    pre(class = "bg-light p-3 border rounded", style:text_align = "left") {
                        {info.descr.clone().unwrap_or_default()}
                    }
                }
            }
        }
        (wrapper, add_group)
    }

    pub fn set_phase(&mut self, phase: TorrentDetailPhase) {
        self.add_button_group.take();
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
            TorrentDetailPhase::Err(e) => {
                self.status_alert.set_text(format!("Error: {e}"));
                self.status_alert.set_flavor(Flavor::Danger);
                self.status_alert.set_is_visible(true);
            }
            TorrentDetailPhase::Details(info) => {
                self.status_alert.set_is_visible(false);
                let (detail, add_group) = Self::detail_form(info);
                self.wrapper.append_child(&detail);
                self.detail_form = Some(detail);
                self.add_button_group = add_group;
            }
        }
        self.phase.set(phase);
    }

    /// Record the download in the backend ledger.
    async fn record_download(
        info_hash: &str,
        name: &str,
        destination: Destination,
    ) -> Result<(), AppError> {
        log::info!("Recording download '{name}'...");
        super::add_download(info_hash, name, destination).await
    }

    pub async fn step(&mut self) {
        loop {
            if let Some(add_group) = self.add_button_group.as_mut() {
                log::info!("step details with add button");

                let clicked_back = self
                    .back_button
                    .step()
                    .map(|_| None)
                    .or(add_group.step().map(Some))
                    .await;

                match clicked_back {
                    None => break, // back button
                    Some(action) => {
                        let destination = match &action {
                            MagnetAction::AddPrimary => self
                                .add_button_group
                                .as_ref()
                                .map(|g| g.selected)
                                .unwrap_or_default(),
                            MagnetAction::AddAlternate(d) => *d,
                        };

                        if let TorrentDetailPhase::Details(info) = self.phase.deref() {
                            // Record in the ledger first — open::path may
                            // disrupt the WASM context by handing focus to
                            // the OS magnet handler.
                            log::info!("Recording the download...");
                            match Self::record_download(&info.info_hash, &info.name, destination)
                                .await
                            {
                                Ok(()) => {
                                    log::info!("...done.");
                                    // Then open the magnet link via OS handler
                                    if let Some(link) = info.magnet.as_ref() {
                                        log::info!("...opening the magnet link.");
                                        open::path(link).await;
                                    }
                                }
                                Err(e) => log::error!("...recording failed: {e}"),
                            }
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
