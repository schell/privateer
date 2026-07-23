//! Watching view – track movies and shows on a watchlist.
//!
//! Each watched title is displayed as a Card in a responsive grid.  The app
//! polls periodically (every 60 s), searching for each title via the existing
//! `search` Tauri command.  Movies show a total result count; shows auto-parse
//! S##E## patterns and display the latest season grouped by episode.  Movies
//! are auto-removed when they appear in the downloads ledger; shows are never
//! auto-removed.
//!
//! Episode rows on show cards show an existence badge when the episode is found
//! in the downloads ledger or on disk, and mute the row.  Movie cards show a
//! "Downloaded" badge when the movie exists.  Result text is clickable and
//! navigates to the Search tab with the corresponding query.
use futures_lite::FutureExt;
use iti::components::button::{Button, PrimaryButton};
use iti::components::icon::IconGlyph;
use iti::components::title_bar::TitleBarEvent;
use iti::components::Flavor;
use iti::components::{badge::Badge, title_bar::TitleBar};
use mogwai::web::prelude::*;
use privateer_wire_types::{Destination, Torrent, WatchlistEntry};

// ---------------------------------------------------------------------------
// Episode parsing
// ---------------------------------------------------------------------------

/// A group of search results for a single episode.
struct EpisodeGroup {
    season: u32,
    episode: u32,
    count: usize,
}

/// Parse S##E## patterns from torrent names.
///
/// Returns groups for the **latest (highest) season only**, sorted by episode
/// number descending (newest first).
fn parse_episodes(results: &[Torrent]) -> Vec<EpisodeGroup> {
    use std::collections::HashMap;

    let mut groups: HashMap<(u32, u32), usize> = HashMap::new();

    for torrent in results {
        let name = torrent.name.as_bytes();
        let len = name.len();
        let mut i = 0;
        while i + 4 < len {
            // Look for 'S' or 's'
            if (name[i] == b'S' || name[i] == b's') && name[i + 1].is_ascii_digit() {
                let season_start = i + 1;
                let mut j = season_start;
                while j < len && name[j].is_ascii_digit() {
                    j += 1;
                }
                if j < len
                    && (name[j] == b'E' || name[j] == b'e')
                    && j + 1 < len
                    && name[j + 1].is_ascii_digit()
                {
                    let season: u32 = std::str::from_utf8(&name[season_start..j])
                        .ok()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0);
                    let ep_start = j + 1;
                    let mut k = ep_start;
                    while k < len && name[k].is_ascii_digit() {
                        k += 1;
                    }
                    let episode: u32 = std::str::from_utf8(&name[ep_start..k])
                        .ok()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0);
                    if season > 0 && episode > 0 {
                        *groups.entry((season, episode)).or_default() += 1;
                    }
                    i = k;
                    continue;
                }
            }
            i += 1;
        }
    }

    if groups.is_empty() {
        return Vec::new();
    }

    // Find highest season
    let max_season = groups.keys().map(|(s, _)| *s).max().unwrap_or(0);

    // Filter to latest season, sort by episode descending
    let mut episodes: Vec<EpisodeGroup> = groups
        .into_iter()
        .filter(|((s, _), _)| *s == max_season)
        .map(|((season, episode), count)| EpisodeGroup {
            season,
            episode,
            count,
        })
        .collect();
    episodes.sort_by_key(|b| std::cmp::Reverse(b.episode));
    episodes
}

// ---------------------------------------------------------------------------
// WatchCard – a single card in the grid
// ---------------------------------------------------------------------------

/// Per-episode list item element and its click listener, stored so we can
/// race them all in the event loop.
struct EpisodeRow<V: View> {
    li: V::Element,
    on_click: V::EventListener,
    /// The search query this row links to (e.g. "Breaking Bad S05E16").
    search_query: String,
}

#[derive(ViewChild)]
#[allow(dead_code)]
struct WatchCard<V: View> {
    #[child]
    column: V::Element,
    entry_id: u64,
    destination: Destination,
    title: TitleBar<V>,
    body_text: V::Text,
    on_body_text_click: V::EventListener,
    episode_list: V::Element,
    /// Per-episode rows (rebuilt on each poll).  Each holds an event listener
    /// for navigating to Search.
    episode_rows: Vec<EpisodeRow<V>>,
}

impl<V: View> WatchCard<V> {
    fn new(entry: &WatchlistEntry) -> Self {
        let dest_flavor = match entry.destination {
            Destination::Movies => Flavor::Info,
            Destination::Shows => Flavor::Warning,
        };

        let badge = Badge::<V>::new(entry.destination.label(), dest_flavor);

        rsx! {
            let column = div(class = "window col-auto mb-3") {
                let title = {TitleBar::new(&entry.title)}
                div(class = "watching") {
                    div(class = "watching-body") {
                        div(class = "container") {
                            div(
                                class = "row text-primary",
                                style:cursor = "pointer",
                                style:text_decoration = "underline",
                                on:click = on_body_text_click,
                            ) {
                                let body_text = "Searching..."
                            }
                            div( class = "row") {
                                {&badge}
                            }
                        }
                        let episode_list = ul(class = "mt-2", style:display = "none") {}
                    }
                    div(class = "watching-footer") {
                        let remove_button = {{
                            let mut btn = Button::new("Remove", None);
                            btn.get_icon_mut().set_glyph(IconGlyph::Xmark);
                            btn.set_has_icon(true);
                            btn
                        }}
                    }
                }
            }
        }

        let mut title = title;
        title.set_show_close_button(true);
        Self {
            column,
            entry_id: entry.id,
            destination: entry.destination,
            title,
            body_text,
            on_body_text_click,
            episode_list,
            episode_rows: Vec::new(),
        }
    }

    fn set_movie_results(&mut self, count: usize, exists: bool) {
        if count == 0 {
            self.body_text.set_text("No results found");
        } else {
            self.body_text.set_text(format!("{count} results found"));
        }
        self.episode_list.set_style("display", "none");

        // Show/hide downloaded badge
        // if exists {
        //     self.movie_badge.set_text("Downloaded");
        // } else {
        //     self.movie_badge.set_text("");
        // }
    }

    fn set_show_results(
        &mut self,
        groups: &[EpisodeGroup],
        total: usize,
        exists: &[bool],
        title: &str,
    ) {
        if groups.is_empty() {
            if total == 0 {
                self.body_text.set_text("No results found");
            } else {
                self.body_text.set_text(format!(
                    "No episode info found \u{2014} {total} total results"
                ));
            }
            self.episode_list.set_style("display", "none");
            // // Hide movie badge for shows
            // self.movie_badge.set_text("");
            // Clear old episode rows
            for row in self.episode_rows.drain(..) {
                self.episode_list.remove_child(&row.li);
            }
            return;
        }

        let season = groups[0].season;
        self.body_text.set_text(format!(
            "Season {season} \u{2014} {} episodes found",
            groups.len()
        ));
        // // Hide movie badge for shows
        // self.movie_badge.set_text("");

        // Clear and rebuild episode list
        for row in self.episode_rows.drain(..) {
            self.episode_list.remove_child(&row.li);
        }

        for (i, g) in groups.iter().enumerate() {
            let ep_exists = exists.get(i).copied().unwrap_or(false);
            let search_query = format!("{title} S{:02}E{:02}", g.season, g.episode);

            let muted_class = if ep_exists {
                "list-group-item py-1 px-2 d-flex justify-content-between align-items-center text-muted"
            } else {
                "list-group-item py-1 px-2 d-flex justify-content-between align-items-center"
            };

            if ep_exists {
                let exists_badge = Badge::new("Exists", Flavor::Success);
                rsx! {
                    let li = li(class = muted_class) {
                        span(
                            class = "text-primary",
                            style:cursor = "pointer",
                            style:text_decoration = "underline",
                            on:click = on_ep_click,
                        ) {
                            {format!("E{:02} \u{2014} {} results", g.episode, g.count)}
                        }
                        {&exists_badge}
                    }
                }
                self.episode_list.append_child(&li);
                self.episode_rows.push(EpisodeRow {
                    li,
                    on_click: on_ep_click,
                    search_query,
                });
            } else {
                rsx! {
                    let li = li(class = muted_class) {
                        span(
                            class = "text-primary",
                            style:cursor = "pointer",
                            style:text_decoration = "underline",
                            on:click = on_ep_click,
                        ) {
                            {format!("E{:02} \u{2014} {} results", g.episode, g.count)}
                        }
                    }
                }
                self.episode_list.append_child(&li);
                self.episode_rows.push(EpisodeRow {
                    li,
                    on_click: on_ep_click,
                    search_query,
                });
            }
        }
        self.episode_list.remove_style("display");
    }

    fn set_error(&self, message: &str) {
        self.body_text.set_text(format!("\u{26A0} {message}"));
        self.episode_list.set_style("display", "none");
    }
}

// ---------------------------------------------------------------------------
// WatchingView – the top-level tab content
// ---------------------------------------------------------------------------

/// The Watching tab content.
#[derive(ViewChild)]
pub struct WatchingView<V: View> {
    #[child]
    container: V::Element,
    grid: V::Element,
    add_form: V::Element,
    title_input: V::Element,
    dest_movies_btn: V::Element,
    dest_shows_btn: V::Element,
    add_button: PrimaryButton<V>,
    on_add_submit: V::EventListener,
    on_cancel: V::EventListener,
    // State
    watch_cards: Vec<WatchCard<V>>,
    entries: Vec<WatchlistEntry>,
    selected_destination: Destination,
    loaded: bool,
}

impl<V: View> Default for WatchingView<V> {
    fn default() -> Self {
        // Add form (hidden initially)
        rsx! {
            let title_input = input(
                type = "text",
                class = "form-control form-control-sm mb-2",
                placeholder = "Title...",
            ) {}
        }
        rsx! {
            let dest_movies_btn = button(class = "btn btn-sm btn-info active") {
                "Movies"
            }
        }
        rsx! {
            let dest_shows_btn = button(class = "btn btn-sm btn-outline-warning") {
                "Shows"
            }
        }
        rsx! {
            let add_form = div(
                class = "window",
                style:display = "none",
            ) {
                div(class = "watching") {
                    div(class = "watching-body") {
                        {&title_input}
                        div(class = "btn-group btn-group-sm mb-2 w-100") {
                            {&dest_movies_btn}
                            {&dest_shows_btn}
                        }
                        div(class = "d-flex gap-2") {
                            button(class = "btn btn-sm btn-primary", on:click = on_add_submit) { "Add" }
                            button(class = "btn btn-sm btn-secondary", on:click = on_cancel) { "Cancel" }
                        }
                    }
                }
            }
        }

        // The "+" button
        let mut add_button = PrimaryButton::new("Add to your watchlist", None);
        add_button.get_icon_mut().set_glyph(IconGlyph::Plus);
        add_button.set_has_icon(true);

        // Grid container
        rsx! {
            let add_button_row = div(class = "row") {
                div(style:margin_left = "auto") {
                    {&add_button}
                }
            }
        }

        rsx! {
            let container = div(class = "container-fluid") {
                {add_button_row}
                let grid = div(class = "row") {}
            }
        }

        Self {
            container,
            grid,
            add_form,
            title_input,
            dest_movies_btn,
            dest_shows_btn,
            add_button,
            on_add_submit,
            on_cancel,
            watch_cards: Vec::new(),
            entries: Vec::new(),
            selected_destination: Destination::Movies,
            loaded: false,
        }
    }
}

impl<V: View> WatchingView<V> {
    fn show_add_form(&self) {
        self.grid.append_child(&self.add_form);
        self.add_form.remove_style("display");
    }

    fn hide_add_form(&self) {
        self.add_form.set_style("display", "none");
        self.title_input
            .dyn_el(|input: &web_sys::HtmlInputElement| input.set_value(""));
    }

    fn select_movies(&mut self) {
        self.selected_destination = Destination::Movies;
        self.dest_movies_btn
            .set_property("class", "btn btn-sm btn-info active");
        self.dest_shows_btn
            .set_property("class", "btn btn-sm btn-outline-warning");
    }

    fn select_shows(&mut self) {
        self.selected_destination = Destination::Shows;
        self.dest_movies_btn
            .set_property("class", "btn btn-sm btn-outline-info");
        self.dest_shows_btn
            .set_property("class", "btn btn-sm btn-warning active");
    }

    /// Reload watchlist from backend and rebuild all cards.
    async fn reload(&mut self) {
        let entries = match super::get_watchlist().await {
            Ok(e) => e,
            Err(e) => {
                log::error!("Failed to load watchlist: {e}");
                return;
            }
        };

        // Remove old cards from DOM
        for card in self.watch_cards.drain(..) {
            self.grid.remove_child(&card);
        }

        // Build new cards
        self.entries = entries;
        for entry in &self.entries {
            let card = WatchCard::new(entry);
            self.grid.append_child(&card);
            self.watch_cards.push(card);
        }

        // Poll the watchlist
        self.poll().await;
    }

    /// Poll search results for all watched entries, including existence checks.
    async fn poll(&mut self) {
        for (i, entry) in self.entries.iter().enumerate() {
            if i >= self.watch_cards.len() {
                break;
            }
            match super::search(&entry.title).await {
                Ok(results) => match entry.destination {
                    Destination::Movies => {
                        let exists = super::check_movie_exists(&entry.title)
                            .await
                            .unwrap_or(false);
                        self.watch_cards[i].set_movie_results(results.len(), exists);
                    }
                    Destination::Shows => {
                        let groups = parse_episodes(&results);
                        // Build episode pairs for the existence check
                        let ep_pairs: Vec<(u32, u32)> =
                            groups.iter().map(|g| (g.season, g.episode)).collect();
                        let exists = if ep_pairs.is_empty() {
                            Vec::new()
                        } else {
                            super::check_episodes_exist(&entry.title, &ep_pairs)
                                .await
                                .unwrap_or_else(|_| vec![false; ep_pairs.len()])
                        };
                        self.watch_cards[i].set_show_results(
                            &groups,
                            results.len(),
                            &exists,
                            &entry.title,
                        );
                    }
                },
                Err(e) => {
                    self.watch_cards[i].set_error(&e.message);
                }
            }
        }
    }

    /// Auto-remove movies that appear in the downloads ledger.
    async fn auto_remove_movies(&mut self) {
        let ledger = match super::get_downloads_ledger().await {
            Ok(l) => l,
            Err(_) => return,
        };

        let mut removed = false;
        for entry in &self.entries {
            if entry.destination != Destination::Movies {
                continue;
            }
            let title_lower = entry.title.to_lowercase();
            if ledger
                .iter()
                .any(|d| d.name.to_lowercase().contains(&title_lower))
            {
                let _ = super::remove_from_watchlist(entry.id).await;
                removed = true;
            }
        }

        if removed {
            self.reload().await;
        }
    }

    /// One step of the watching view event loop.
    ///
    /// Loads watchlist on first call, polls searches, then waits 60 s or until
    /// the user interacts (add/remove/toggle).
    ///
    /// Returns `query` when the user clicks a search link, indicating
    /// the caller should switch to the Search tab and run that query.
    pub async fn step(&mut self) -> String {
        loop {
            // First load
            if !self.loaded {
                self.reload().await;
                self.loaded = true;
            }

            // // Poll searches
            // self.poll().await;

            // // Auto-remove downloaded movies
            // self.auto_remove_movies().await;

            // Wait for user interaction or timeout
            enum Event {
                Timeout,
                AddClick,
                AddSubmit,
                Cancel,
                DestMovies,
                DestShows,
                Remove(u64),
                Search(String),
            }

            let timeout = async {
                mogwai::time::wait_millis(60_000).await;
                Event::Timeout
            };
            let add_click = async {
                let _ = self.add_button.step().await;
                Event::AddClick
            };
            let add_submit = async {
                self.on_add_submit.next().await;
                Event::AddSubmit
            };
            let cancel = async {
                self.on_cancel.next().await;
                Event::Cancel
            };
            let dest_movies = async {
                self.dest_movies_btn.listen("click").next().await;
                Event::DestMovies
            };
            let dest_shows = async {
                self.dest_shows_btn.listen("click").next().await;
                Event::DestShows
            };

            // Race remove buttons from all cards
            let remove = async {
                if self.watch_cards.is_empty() {
                    std::future::pending::<Event>().await
                } else {
                    let futures: Vec<_> = self
                        .watch_cards
                        .iter()
                        .map(|c| {
                            async move {
                                let TitleBarEvent::CloseClicked = c.title.step().await;
                                Event::Remove(c.entry_id)
                            }
                            .boxed_local()
                        })
                        .collect();
                    mogwai::future::race_all(futures).await
                }
            };

            // Race movie body text clicks (search link for movies)
            let movie_search = async {
                let movie_cards: Vec<_> = self
                    .watch_cards
                    .iter()
                    .filter(|c| c.destination == Destination::Movies)
                    .collect();
                if movie_cards.is_empty() {
                    std::future::pending::<Event>().await
                } else {
                    let futures: Vec<_> = movie_cards
                        .iter()
                        .map(|c| {
                            let query = c.title.get_title();
                            async move {
                                c.on_body_text_click.next().await;
                                Event::Search(query)
                            }
                            .boxed_local()
                        })
                        .collect();
                    mogwai::future::race_all(futures).await
                }
            };

            // Race episode row clicks (search link for shows)
            let episode_search = async {
                let all_rows: Vec<_> = self
                    .watch_cards
                    .iter()
                    .flat_map(|c| c.episode_rows.iter())
                    .collect();
                if all_rows.is_empty() {
                    std::future::pending::<Event>().await
                } else {
                    let futures: Vec<_> = all_rows
                        .iter()
                        .map(|row| {
                            let query = row.search_query.clone();
                            async move {
                                row.on_click.next().await;
                                Event::Search(query)
                            }
                            .boxed_local()
                        })
                        .collect();
                    mogwai::future::race_all(futures).await
                }
            };

            let event = timeout
                .or(add_click)
                .or(add_submit)
                .or(cancel)
                .or(dest_movies)
                .or(dest_shows)
                .or(remove)
                .or(movie_search)
                .or(episode_search)
                .await;

            match event {
                Event::Timeout => {}
                Event::AddClick => {
                    log::info!("adding to watchlist");
                    self.show_add_form();
                }
                Event::AddSubmit => {
                    let title = self
                        .title_input
                        .dyn_el(|input: &web_sys::HtmlInputElement| input.value())
                        .unwrap_or_default();
                    let title = title.trim();
                    if !title.is_empty() {
                        match super::add_to_watchlist(title, self.selected_destination).await {
                            Ok(_entry) => {
                                self.hide_add_form();
                                self.reload().await;
                            }
                            Err(e) => {
                                log::error!("Failed to add to watchlist: {e}");
                            }
                        }
                    }
                }
                Event::Cancel => {
                    self.hide_add_form();
                }
                Event::DestMovies => {
                    self.select_movies();
                }
                Event::DestShows => {
                    self.select_shows();
                }
                Event::Remove(id) => match super::remove_from_watchlist(id).await {
                    Ok(()) => {
                        self.reload().await;
                    }
                    Err(e) => {
                        log::error!("Failed to remove from watchlist: {e}");
                    }
                },
                Event::Search(query) => return query,
            }
        }
    }
}
