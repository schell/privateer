//! Downloads view - shows Transmission torrent progress.
use futures_lite::FutureExt;
use human_repr::HumanCount;
use iti::components::alert::Alert;
use iti::components::progress::Progress;
use iti::components::Flavor;
use mogwai::future::MogwaiFutureExt;
use mogwai::web::prelude::*;
use pb_wire_types::{Destination, ErrorKind, TransmissionStatus, TransmissionTorrent};

use super::invoke;

pub async fn get_torrents() -> Result<Vec<TransmissionTorrent>, pb_wire_types::AppError> {
    #[derive(serde::Serialize)]
    struct Empty {}
    invoke::cmd("get_torrents", &Empty {}).await
}

fn status_flavor(status: &TransmissionStatus) -> Flavor {
    match status {
        TransmissionStatus::Downloading => Flavor::Primary,
        TransmissionStatus::Seeding => Flavor::Success,
        TransmissionStatus::Stopped => Flavor::Secondary,
        TransmissionStatus::QueuedDownload | TransmissionStatus::QueuedSeed => Flavor::Warning,
        TransmissionStatus::Verifying | TransmissionStatus::QueuedVerify => Flavor::Info,
    }
}

fn dest_flavor(dest: &Destination) -> Flavor {
    match dest {
        Destination::Movies => Flavor::Info,
        Destination::Shows => Flavor::Warning,
    }
}

/// Event emitted by an assign button in a torrent row.
struct AssignEvent {
    hash_string: String,
    name: String,
    destination: Destination,
}

/// A single row in the downloads table.
struct TorrentRow<V: View> {
    wrapper: V::Element,
    name_text: V::Text,
    progress: Progress<V>,
    pct_text: V::Text,
    status_badge: Proxy<TransmissionStatus>,
    status_text: V::Text,
    size_text: V::Text,
    dest_text: V::Text,
    dest_badge_class: Proxy<Option<Destination>>,
    /// The indicator text (checkmark, hourglass, etc.) — shown when assigned.
    copied_text: V::Text,
    /// Whether the assign buttons are currently visible.
    has_assign_buttons: Proxy<bool>,
    /// Click listener for the "M" (Movies) button.
    on_click_movies: V::EventListener,
    /// Click listener for the "S" (Shows) button.
    on_click_shows: V::EventListener,
    torrent_id: i64,
    hash_string: String,
    torrent_name: String,
}

impl<V: View> TorrentRow<V> {
    fn new(t: &TransmissionTorrent) -> Self {
        let pct = (t.percent_done * 100.0) as u8;
        let progress = Progress::<V>::new(pct, status_flavor(&t.status));
        let mut status_badge = Proxy::new(t.status);
        let mut dest_badge_class = Proxy::new(t.destination);
        let show_buttons = t.destination.is_none();
        let mut has_assign_buttons = Proxy::new(show_buttons);
        rsx! {
            let wrapper = tr() {
                td(class = "torrent-name", style:text_align = "left") {
                    let name_text = ""
                }
                td() {
                    div(class = "d-flex align-items-center gap-2") {
                        div(style:flex = "1", style:min_width = "80px") {
                            {&progress}
                        }
                        span() { let pct_text = "" }
                    }
                }
                td() {
                    span(
                        class = status_badge(s => {
                            format!("badge text-bg-{}", status_flavor(s))
                        }),
                    ) {
                        let status_text = ""
                    }
                }
                td() { let size_text = "" }
                td() {
                    span(
                        class = dest_badge_class(d => match d {
                            Some(dest) => format!("badge text-bg-{}", dest_flavor(dest)),
                            None => "".into(),
                        }),
                    ) {
                        let dest_text = ""
                    }
                }
                td(style:text_align = "center") {
                    // Indicator text (shown when destination is assigned)
                    span(
                        style:display = has_assign_buttons(show => {
                            if *show { "none" } else { "" }
                        }),
                    ) {
                        let copied_text = ""
                    }
                    // Assign buttons (shown when destination is NOT assigned)
                    div(
                        class = "btn-group btn-group-sm",
                        style:display = has_assign_buttons(show => {
                            if *show { "" } else { "none" }
                        }),
                    ) {
                        button(
                            class = "btn btn-outline-info btn-sm",
                            type = "button",
                            on:click = on_click_movies,
                        ) { "M" }
                        button(
                            class = "btn btn-outline-warning btn-sm",
                            type = "button",
                            on:click = on_click_shows,
                        ) { "S" }
                    }
                }
            }
        }

        // Set initial text values
        name_text.set_text(&t.name);
        pct_text.set_text(format!("{:.1}%", t.percent_done * 100.0));
        status_text.set_text(t.status.label());
        size_text.set_text((t.size_when_done as usize).human_count_bytes().to_string());
        dest_text.set_text(
            t.destination
                .map(|d| d.label().to_string())
                .unwrap_or_default(),
        );
        copied_text.set_text(t.copy_state.indicator());

        Self {
            wrapper,
            name_text,
            progress,
            pct_text,
            status_badge,
            status_text,
            size_text,
            dest_text,
            dest_badge_class,
            copied_text,
            has_assign_buttons,
            on_click_movies,
            on_click_shows,
            torrent_id: t.id,
            hash_string: t.hash_string.clone(),
            torrent_name: t.name.clone(),
        }
    }

    fn update(&mut self, t: &TransmissionTorrent) {
        let pct = (t.percent_done * 100.0) as u8;
        self.name_text.set_text(&t.name);
        self.progress.set_value(pct);
        self.progress.set_flavor(status_flavor(&t.status));
        self.pct_text
            .set_text(format!("{:.1}%", t.percent_done * 100.0));
        self.status_badge.set(t.status);
        self.status_text.set_text(t.status.label());
        self.size_text
            .set_text((t.size_when_done as usize).human_count_bytes().to_string());
        self.dest_badge_class.set(t.destination);
        self.dest_text.set_text(
            t.destination
                .map(|d| d.label().to_string())
                .unwrap_or_default(),
        );
        self.copied_text.set_text(t.copy_state.indicator());
        self.has_assign_buttons.set(t.destination.is_none());
        self.hash_string.clone_from(&t.hash_string);
        self.torrent_name.clone_from(&t.name);
    }
}

/// Downloads tab view.
#[derive(ViewChild)]
pub struct DownloadsView<V: View> {
    #[child]
    wrapper: V::Element,
    status_alert: Alert<V>,
    table_wrapper: V::Element,
    tbody: V::Element,
    rows: Vec<TorrentRow<V>>,
}

impl<V: View> Default for DownloadsView<V> {
    fn default() -> Self {
        let status_alert = Alert::new("Connecting to Transmission...", Flavor::Info);
        rsx! {
            let wrapper = div(class = "container-fluid") {
                div(class = "mb-3") {
                    {&status_alert}
                }
                let table_wrapper = div(class = "table-responsive", style:display = "none") {
                    table(class = "table table-striped table-hover") {
                        colgroup() {
                            col(style:width = "30%"){}
                            col(style:width = "25%"){}
                            col(style:width = "12%"){}
                            col(style:width = "12%"){}
                            col(style:width = "12%"){}
                            col(style:width = "9%"){}
                        }
                        thead() {
                            tr() {
                                th() { "Name" }
                                th() { "Progress" }
                                th() { "Status" }
                                th() { "Size" }
                                th() { "Dest" }
                                th() { "Copied" }
                            }
                        }
                        let tbody = tbody() {}
                    }
                }
            }
        }
        Self {
            wrapper,
            status_alert,
            table_wrapper,
            tbody,
            rows: vec![],
        }
    }
}

impl<V: View> DownloadsView<V> {
    fn update_torrents(&mut self, torrents: &[TransmissionTorrent]) {
        // Check if we need to rebuild (different count or different IDs)
        let needs_rebuild = self.rows.len() != torrents.len()
            || self
                .rows
                .iter()
                .zip(torrents.iter())
                .any(|(r, t)| r.torrent_id != t.id);

        if needs_rebuild {
            // Remove old rows
            for row in self.rows.drain(..) {
                self.tbody.remove_child(&row.wrapper);
            }
            // Build new rows
            for t in torrents {
                let row = TorrentRow::<V>::new(t);
                self.tbody.append_child(&row.wrapper);
                self.rows.push(row);
            }
        } else {
            // Just update existing rows
            for (row, t) in self.rows.iter_mut().zip(torrents.iter()) {
                row.update(t);
            }
        }
    }

    /// Poll once: fetch torrents and update the view.
    pub async fn poll(&mut self) {
        match get_torrents().await {
            Ok(torrents) => {
                if torrents.is_empty() {
                    self.status_alert
                        .set_text("No torrents in Transmission.");
                    self.status_alert.set_flavor(Flavor::Info);
                    self.status_alert.set_is_visible(true);
                    self.table_wrapper.set_style("display", "none");
                } else {
                    self.status_alert.set_is_visible(false);
                    self.table_wrapper.set_style("display", "block");
                    self.update_torrents(&torrents);
                }
            }
            Err(e) => {
                let msg = match e.kind {
                    ErrorKind::TransmissionConnection => format!(
                        "Could not connect to Transmission: {}. \
                         Make sure Transmission is running and remote access \
                         is enabled in Preferences > Remote.",
                        e.message
                    ),
                    _ => e.to_string(),
                };
                self.status_alert.set_text(msg);
                self.status_alert.set_flavor(Flavor::Danger);
                self.status_alert.set_is_visible(true);
                self.table_wrapper.set_style("display", "none");
            }
        }
    }

    /// Build a future that resolves when any assign button is clicked.
    ///
    /// `EventListener::next()` takes `&self` and returns a cloned future,
    /// so we can safely race listeners from multiple rows without borrow
    /// conflicts.
    async fn wait_for_assign(&self) -> AssignEvent {
        if self.rows.is_empty() {
            // No rows — never resolve so the caller's .or() picks the
            // other branch (timeout).
            return std::future::pending().await;
        }

        let futures: Vec<_> = self
            .rows
            .iter()
            .flat_map(|row| {
                let hash = row.hash_string.clone();
                let name = row.torrent_name.clone();
                let hash2 = hash.clone();
                let name2 = name.clone();

                let movies_fut = row.on_click_movies.next().map(move |_| AssignEvent {
                    hash_string: hash,
                    name,
                    destination: Destination::Movies,
                });
                let shows_fut = row.on_click_shows.next().map(move |_| AssignEvent {
                    hash_string: hash2,
                    name: name2,
                    destination: Destination::Shows,
                });

                [movies_fut.boxed_local(), shows_fut.boxed_local()]
            })
            .collect();

        mogwai::future::race_all(futures).await
    }

    /// Run one poll cycle, then wait for the next tick.
    /// While waiting, also listen for assign button clicks. If a button is
    /// clicked, record the download and re-poll immediately.
    /// Returns after one tick so the caller can race with tab switches.
    pub async fn step(&mut self) {
        // Poll first
        self.poll().await;

        // Now race the 3-second timer against assign button clicks
        enum WaitResult {
            Timeout,
            Assign(AssignEvent),
        }

        let result = async {
            mogwai::time::wait_millis(3000).await;
            WaitResult::Timeout
        }
        .or(async { WaitResult::Assign(self.wait_for_assign().await) })
        .await;

        match result {
            WaitResult::Timeout => {}
            WaitResult::Assign(event) => {
                // Call add_download, then re-poll immediately
                match super::add_download(
                    &event.hash_string,
                    &event.name,
                    event.destination,
                )
                .await
                {
                    Ok(()) => {
                        log::info!(
                            "Assigned '{}' to {}",
                            event.name,
                            event.destination.label()
                        );
                    }
                    Err(e) => {
                        log::error!("Failed to assign download: {e}");
                    }
                }
                // Re-poll to update the UI immediately
                self.poll().await;
            }
        }
    }
}
