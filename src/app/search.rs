use std::borrow::Cow;

use futures_lite::FutureExt;
use human_repr::HumanCount;
use iti::{
    components::{
        alert::Alert,
        button::PrimaryButton,
        icon::IconGlyph,
        pane::Panes,
        table::{Table, TableBuilder, TableEvent},
        Flavor,
    },
    id::Id,
};
use mogwai::{future::MogwaiFutureExt, web::prelude::*};
use privateer_wire_types::{Torrent, TorrentInfo};
use web_sys::wasm_bindgen::UnwrapThrowExt;

use crate::app::{
    detail::{TorrentDetail, TorrentDetailPhase},
    info, search,
};

#[derive(ViewChild)]
pub(crate) struct SearchResults<V: View> {
    #[child]
    pub(crate) wrapper: V::Element,
    pub(crate) table: Table<V, TorrentRow<V>>,
}

impl<V: View> Default for SearchResults<V> {
    fn default() -> Self {
        let table = TableBuilder::default()
            .column("Name", TorrentRow::create_cell_fn, |a, b| {
                a.torrent.name.cmp(&b.torrent.name)
            })
            .column("Date", TorrentRow::create_cell_fn, |a, b| {
                a.torrent.added_i64().cmp(&b.torrent.added_i64())
            })
            .column("Seeders", TorrentRow::create_cell_fn, |a, b| {
                a.torrent.seeders_i64().cmp(&b.torrent.seeders_i64())
            })
            .column("Leechers", TorrentRow::create_cell_fn, |a, b| {
                a.torrent.leechers_i64().cmp(&b.torrent.leechers_i64())
            })
            .column("Size", TorrentRow::create_cell_fn, |a, b| {
                a.torrent.size_bytes().cmp(&b.torrent.size_bytes())
            })
            .column("Uploader", TorrentRow::create_cell_fn, |a, b| {
                a.torrent.username.cmp(&b.torrent.username)
            })
            .use_scrollbar(true)
            .width_auto()
            .build();
        table.set_style("max-height", "calc(100vh - 286px - 2em)");
        rsx! {
            let wrapper = div(class = "search-results mt-3", style:display = "none") {
                h5(class = "mb-2") { "Results" }
                {&table}
            }
        }

        Self { wrapper, table }
    }
}

impl<V: View> SearchResults<V> {
    /// Resolves to the first selected torrent.
    pub(crate) async fn step(&mut self) -> Torrent {
        loop {
            let ev = self.table.step_with(|row| row.step().boxed_local()).await;
            if let TableEvent::User(torrent) = ev {
                return torrent;
            }
        }
    }

    pub(crate) fn set_search_results(&mut self, torrents: impl IntoIterator<Item = Torrent>) {
        let rows = torrents.into_iter().map(TorrentRow::from);
        while !self.table.is_empty() {
            self.table.remove(0);
        }
        for row in rows {
            self.table.push(row);
        }
    }
}

#[derive(ViewChild)]
pub struct SearchView<V: View> {
    #[child]
    pub(crate) wrapper: V::Element,
    pub(crate) input: V::Element,
    pub(crate) on_submit_query: V::EventListener,
    pub(crate) search_button: PrimaryButton<V>,
    pub(crate) status_alert: Alert<V>,
    pub(crate) search_results: SearchResults<V>,
}

impl<V: View> Default for SearchView<V> {
    fn default() -> Self {
        let status_alert = Alert::new("Enter a search query", Flavor::Info);
        let mut search_button = PrimaryButton::new("Search", None);
        search_button
            .get_icon_mut()
            .set_glyph(IconGlyph::MagnifyingGlass);
        rsx! {
            let wrapper = div(class = "container-fluid") {
                div(class = "mb-3") {
                    {&status_alert}
                }
                form(on:submit = on_submit_query) {
                    div(class = "input-group mb-3") {
                        let input = input(
                            class = "form-control",
                            placeholder = "Search for torrents...",
                        ){}
                    }
                    div(class = "row") {
                        div(class = "col") {}
                        div(class = "col-auto") {{&search_button}}
                    }
                }
                let search_results = {SearchResults::default()}
            }
        }
        Self {
            wrapper,
            input,
            on_submit_query,
            search_button,
            status_alert,
            search_results,
        }
    }
}

pub(crate) enum Step<V: View> {
    Results(Box<Torrent>),
    Submit(V::Event),
}

impl<V: View> SearchView<V> {
    /// Resolves with a selected torrent.
    pub async fn step(&mut self) -> Torrent {
        loop {
            let submission = self.on_submit_query.next().map(Step::Submit);
            let search_button = self.search_button.step().map(Step::Submit);
            let sorting = self
                .search_results
                .step()
                .map(|t| Step::Results(Box::new(t)));
            let ev: Step<V> = submission.or(search_button).or(sorting).await;
            match ev {
                Step::Results(t) => {
                    log::info!("showing results for {}", t.name);
                    return *t;
                }
                Step::Submit(ev) => {
                    ev.dyn_ev(|ev: &web_sys::Event| ev.prevent_default());
                    let search_query = self
                        .input
                        .dyn_el(|input: &web_sys::HtmlInputElement| input.value())
                        .unwrap_or_default();
                    log::info!("submitting search query '{search_query}'");
                    self.status_alert
                        .set_text(format!("Searching for '{search_query}'..."));
                    self.status_alert.set_flavor(Flavor::Info);
                    self.search_button.start_spinner();
                    self.search_button.disable();

                    match search(&search_query).await {
                        Ok(torrents) => {
                            log::info!("got {} search results", torrents.len());
                            self.status_alert
                                .set_text(format!("Found {} results.", torrents.len()));
                            self.status_alert.set_flavor(Flavor::Success);
                            self.search_results.set_search_results(torrents);
                            self.search_results.wrapper.set_style("display", "block");
                        }
                        Err(e) => {
                            self.status_alert.set_text(e.to_string());
                            self.status_alert.set_flavor(Flavor::Danger);
                        }
                    }
                    self.search_button.stop_spinner();
                    self.search_button.enable();
                }
            }
        }
    }

    /// Programmatically run a search query.  Sets the input value, executes the
    /// search, and populates results — the same as if the user had typed the
    /// query and pressed Enter.
    pub async fn run_search(&mut self, query: &str) {
        self.input
            .dyn_el(|input: &web_sys::HtmlInputElement| input.set_value(query));
        self.status_alert
            .set_text(format!("Searching for '{query}'..."));
        self.status_alert.set_flavor(Flavor::Info);
        self.search_button.start_spinner();
        self.search_button.disable();

        match search(query).await {
            Ok(torrents) => {
                self.status_alert
                    .set_text(format!("Found {} results.", torrents.len()));
                self.status_alert.set_flavor(Flavor::Success);
                self.search_results.set_search_results(torrents);
                self.search_results.wrapper.set_style("display", "block");
            }
            Err(e) => {
                self.status_alert.set_text(e.to_string());
                self.status_alert.set_flavor(Flavor::Danger);
            }
        }
        self.search_button.stop_spinner();
        self.search_button.enable();
    }
}

#[derive(Clone, Copy, PartialEq)]
enum TorrentColumn {
    Name,
    Date,
    Seeders,
    Leechers,
    Size,
    Uploader,
}

#[derive(ViewChild)]
struct TorrentView<V: View> {
    #[child]
    wrapper: V::Element,
    on_click: V::EventListener,
}

impl<V: View> TorrentView<V> {
    fn new(torrent: &Torrent, col: TorrentColumn) -> Self {
        fn slot<V: View>(s: &str) -> V::Element {
            rsx! {
                let slot = slot() { {s.into_text::<V>()} }
            }
            slot
        }
        rsx! {
            let wrapper = div(on:click = on_click) {
                {{
                     match col {
                        TorrentColumn::Name => {
                            slot::<V>(&torrent.name)
                        }
                        TorrentColumn::Date => {
                            let added = if V::is_view::<Web>() {
                                super::format_unix_timestamp_with_locale(torrent.added_i64())
                            } else {
                                torrent.added.clone()
                            };
                            slot::<V>(&added)
                        }
                        TorrentColumn::Seeders => {
                            slot::<V>(&torrent.seeders)
                        }
                        TorrentColumn::Leechers => slot::<V>(&torrent.leechers),
                        TorrentColumn::Size => slot::<V>(&format!("{}", torrent.size_bytes().human_count_bytes())),
                        TorrentColumn::Uploader => slot::<V>(&torrent.username),
                    }
                }}
            }
        };
        Self { wrapper, on_click }
    }
}

pub(crate) struct TorrentRow<V: View> {
    torrent: Torrent,
    cells: [TorrentView<V>; 6],
}

impl<V: View> From<Torrent> for TorrentRow<V> {
    fn from(torrent: Torrent) -> Self {
        let cols = [
            TorrentColumn::Name,
            TorrentColumn::Date,
            TorrentColumn::Seeders,
            TorrentColumn::Leechers,
            TorrentColumn::Size,
            TorrentColumn::Uploader,
        ];
        let cells = cols.map(|col| TorrentView::new(&torrent, col));
        Self { torrent, cells }
    }
}

impl<V: View> TorrentRow<V> {
    fn create_cell_fn(row: &Self, index: usize) -> V::Element {
        row.cells[index].wrapper.clone()
    }

    async fn step(&mut self) -> Torrent {
        let _ev = mogwai::future::race_all(
            self.cells
                .iter()
                .map(|cell| cell.on_click.next().boxed_local()),
        )
        .await;
        self.torrent.clone()
    }
}

/// Enum wrapper to allow both SearchView and TorrentDetail in a single Panes<V, T>.
///
/// `Panes<V, T>` requires all panes to be the same type. This enum + manual
/// `ViewChild` impl (using `as_boxed_append_arg` to type-erase the iterator)
/// lets us store both view types in one `Panes` container.
pub enum SearchPane<V: View> {
    Search(search::SearchView<V>),
    Detail(TorrentDetail<V>),
}

impl<V: View> ViewChild<V> for SearchPane<V> {
    fn as_append_arg(&self) -> AppendArg<V, impl Iterator<Item = Cow<'_, V::Node>>> {
        match self {
            SearchPane::Search(s) => s.as_boxed_append_arg(),
            SearchPane::Detail(d) => d.as_boxed_append_arg(),
        }
    }
}

/// The Search tab content: contains the search form and detail view with pane switching.
#[derive(ViewChild)]
pub struct SearchTabContent<V: View> {
    #[child]
    container: V::Element,
    panes: Panes<V, SearchPane<V>>,
    is_in_search: bool,
    is_startup: bool,
    /// When set, the next `step()` call will auto-run this search query
    /// instead of waiting for user input.
    pending_search: Option<String>,

    search_id: Id<SearchPane<V>>,
    detail_id: Id<SearchPane<V>>,
}

impl<V: View> Default for SearchTabContent<V> {
    fn default() -> Self {
        rsx! {
            let pane_wrapper = div() {}
        }

        let placeholder = SearchPane::Detail(TorrentDetail::<V>::default());
        let mut panes = Panes::new(pane_wrapper, placeholder);
        let search_id = panes.add_pane(SearchPane::Search(search::SearchView::<V>::default()));
        let detail_id = panes.add_pane(SearchPane::Detail(TorrentDetail::<V>::default()));
        panes.select(&search_id);

        rsx! {
            let container = div() {
                {&panes}
            }
        }

        Self {
            container,
            panes,
            is_in_search: true,
            is_startup: true,
            pending_search: None,

            search_id,
            detail_id,
        }
    }
}

impl<V: View> SearchTabContent<V> {
    fn store_state(info: Option<TorrentInfo>) {
        if V::is_view::<Web>() {
            let storage = mogwai::web::window()
                .local_storage()
                .unwrap_throw()
                .unwrap_throw();
            storage
                .set_item("store-state", &serde_json::to_string(&info).unwrap_throw())
                .unwrap_throw();
        }
    }

    fn get_state() -> Option<TorrentInfo> {
        let storage = mogwai::web::window()
            .local_storage()
            .unwrap_throw()
            .unwrap_throw();
        let s = storage.get_item("store-state").unwrap_throw()?;
        serde_json::from_str(&s).unwrap_throw()
    }

    fn search_view_mut(&mut self) -> &mut search::SearchView<V> {
        match self
            .panes
            .get_pane_mut(&self.search_id)
            .expect("search pane")
        {
            SearchPane::Search(s) => s,
            _ => panic!("expected search pane"),
        }
    }

    fn detail_view_mut(&mut self) -> &mut TorrentDetail<V> {
        match self
            .panes
            .get_pane_mut(&self.detail_id)
            .expect("detail pane")
        {
            SearchPane::Detail(d) => d,
            _ => panic!("expected detail pane"),
        }
    }

    fn show_detail(&mut self) {
        self.panes.select(&self.detail_id);
    }

    fn show_search(&mut self) {
        self.panes.select(&self.search_id);
    }

    /// Queue a search query to be executed on the next `step()`.  The search
    /// tab will switch to its search pane, populate the input, run the query,
    /// and display results.
    pub fn set_pending_search(&mut self, query: String) {
        self.pending_search = Some(query);
        self.is_in_search = true;
    }

    fn set_info(&mut self, state: Option<TorrentInfo>) {
        self.is_in_search = state.is_none();
        if let Some(info) = state {
            self.detail_view_mut()
                .set_phase(TorrentDetailPhase::Details(info));
            self.show_detail();
        } else {
            self.show_search();
            self.detail_view_mut().set_phase(TorrentDetailPhase::Init);
        }
    }

    pub async fn step(&mut self) {
        loop {
            if self.is_startup {
                log::info!("search view startup");
                let state = Self::get_state();
                self.set_info(state);
                self.is_startup = false;
            } else if let Some(query) = self.pending_search.take() {
                // A cross-tab search was requested (e.g. from the Watching tab).
                log::info!("running pending search: {query}");
                Self::store_state(None);
                self.show_search();
                self.search_view_mut().run_search(&query).await;
                // Don't wait for result click — just show results and return.
                // The next step() will be a normal `is_in_search` step.
            } else if self.is_in_search {
                log::info!("showing search view");
                Self::store_state(None);
                self.show_search();
                let torrent = self.search_view_mut().step().await;
                log::info!("getting info");
                let id = torrent.id.clone();
                self.detail_view_mut()
                    .set_phase(TorrentDetailPhase::Getting(torrent));
                self.show_detail();
                match info(&id).await {
                    Ok(info) => {
                        log::info!("got torrent info for {}", info.name);
                        self.set_info(Some(info.clone()));
                        Self::store_state(Some(info));
                    }
                    Err(e) => {
                        log::error!("could not fetch torrent info: {e}");
                        self.detail_view_mut().set_phase(TorrentDetailPhase::Err(e));
                    }
                }
            } else {
                log::info!("in detail");
                self.detail_view_mut().step().await;
                self.is_in_search = true;
                log::info!("leaving detail");
            }
        }
    }
}
