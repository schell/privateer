use std::borrow::Cow;
use std::ops::Deref;

use detail::{TorrentDetail, TorrentDetailPhase};
use futures_lite::FutureExt;
use human_repr::HumanCount;
use iti::components::alert::Alert;
use iti::components::button::Button;
use iti::components::icon::{Icon, IconGlyph, IconSize};
use iti::components::pane::Panes;
use iti::components::Flavor;
use mogwai::view::AppendArg;
use mogwai::{future::MogwaiFutureExt, web::prelude::*};
use pb_wire_types::*;
use wasm_bindgen::prelude::*;

mod detail;

mod invoke {
    use super::*;

    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], catch)]
        async fn invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;
    }

    fn deserialize_as<T: serde::de::DeserializeOwned>(value: JsValue) -> Result<T, Error> {
        match serde_wasm_bindgen::from_value::<T>(value) {
            Ok(t) => Ok(t),
            Err(e) => {
                log::error!("e: {e:#?}");
                Err(Error {
                    msg: "Could not deserialize".into(),
                })
            }
        }
    }

    pub async fn cmd<T: serde::Serialize, X: serde::de::DeserializeOwned>(
        name: &str,
        args: &T,
    ) -> Result<X, Error> {
        let value = serde_wasm_bindgen::to_value(args)
            .map_err(|e| format!("could not serialize {}: {e}", std::any::type_name::<T>()))?;
        let result = invoke(name, value).await;
        match result {
            Ok(value) => deserialize_as::<X>(value),
            Err(e) => Err(deserialize_as::<Error>(e)?),
        }
    }
}

pub async fn search(query: &str) -> Result<Vec<Torrent>, Error> {
    #[derive(serde::Serialize)]
    struct Query<'a> {
        query: &'a str,
    }

    invoke::cmd("search", &Query { query }).await
}

pub async fn info(id: &str) -> Result<TorrentInfo, Error> {
    #[derive(serde::Serialize)]
    struct Info<'a> {
        id: &'a str,
    }

    invoke::cmd("info", &Info { id }).await
}

#[derive(ViewChild)]
struct TorrentView<V: View> {
    #[child]
    wrapper: V::Element,
    on_click: V::EventListener,
    torrent: Torrent,
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

impl<V: View> TorrentView<V> {
    fn new(torrent: Torrent) -> Self {
        let added = if V::is_view::<Web>() {
            format_unix_timestamp_with_locale(torrent.added_i64())
        } else {
            torrent.added.clone()
        };
        rsx! {
            let wrapper = tr(
                class = "search-result-item",
                on:click = on_click,
                style:cursor = "pointer",
            ) {
                td(class = "torrent-name") { {&torrent.name} }
                td() { {&added} }
                td() { {&torrent.seeders} }
                td() { {&torrent.leechers} }
                td() { {format!("{}", torrent.size_bytes().human_count_bytes())} }
                td(class = "torrent-username") { {&torrent.username} }
            }
        }
        Self {
            wrapper,
            on_click,
            torrent,
        }
    }

    async fn step(&self) -> &Torrent {
        self.on_click.next().await;
        &self.torrent
    }
}

#[derive(Clone, Copy, PartialEq)]
enum SortColumn {
    Name,
    Date,
    Seeders,
    Leechers,
    Size,
    Uploader,
}

impl SortColumn {
    fn header_view<V: View>(&self, current_sorting: &Sort) -> V::Element {
        let name = match self {
            SortColumn::Name => "Name",
            SortColumn::Date => "Date Added",
            SortColumn::Seeders => "Seeders",
            SortColumn::Leechers => "Leechers",
            SortColumn::Size => "Size",
            SortColumn::Uploader => "Uploader",
        };
        let is_selected = Some(self) == current_sorting.column.as_ref();
        let dir = is_selected.then(|| {
            let glyph = match current_sorting.direction {
                Direction::Descending => IconGlyph::ChevronDown,
                Direction::Ascending => IconGlyph::ChevronUp,
            };
            Icon::<V>::new(glyph, IconSize::Sm)
        });
        rsx! {
            let wrapper = span(style:cursor = "pointer") {
                {name.into_text::<V>()}
                span(class = "direction") {{dir}}
            }
        }
        wrapper
    }
}

#[derive(Clone, Copy, Default, PartialEq)]
enum Direction {
    #[default]
    Descending,
    Ascending,
}

#[derive(Clone, Default, PartialEq)]
struct Sort {
    column: Option<SortColumn>,
    direction: Direction,
}

#[derive(ViewChild)]
struct SearchResults<V: View> {
    #[child]
    wrapper: V::Element,
    table: V::Element,
    torrents: Vec<TorrentView<V>>,
    sort: Proxy<Sort>,
    on_click_name: V::EventListener,
    on_click_date: V::EventListener,
    on_click_seeders: V::EventListener,
    on_click_leechers: V::EventListener,
    on_click_size: V::EventListener,
    on_click_uploader: V::EventListener,
}

impl<V: View> Default for SearchResults<V> {
    fn default() -> Self {
        use SortColumn::*;
        let mut sort = Proxy::<Sort>::default();
        rsx! {
            let wrapper = div(class = "search-results mt-3", style:display = "none") {
                h5(class = "mb-2") { "Results" }
                div(class = "table-responsive") {
                    let table = table(class = "table table-striped table-hover") {
                        colgroup() {
                            col(style:width = "35%"){}
                            col(style:width = "20%"){}
                            col(style:width = "9%"){}
                            col(style:width = "9%"){}
                            col(style:width = "9%"){}
                            col(style:width = "9%"){}
                        }
                        thead() {
                            tr() {
                                th(on:click = on_click_name) {{sort(s => Name.header_view::<V>(s))}}
                                th(on:click = on_click_date) {{sort(s => Date.header_view::<V>(s))}}
                                th(on:click = on_click_seeders) {{sort(s => Seeders.header_view::<V>(s))}}
                                th(on:click = on_click_leechers) {{sort(s => Leechers.header_view::<V>(s))}}
                                th(on:click = on_click_size) {{sort(s => Size.header_view::<V>(s))}}
                                th(on:click = on_click_uploader) {{sort(s => Uploader.header_view::<V>(s))}}
                            }
                        }
                    }
                }
            }
        }

        Self {
            wrapper,
            table,
            torrents: vec![],
            on_click_name,
            on_click_date,
            on_click_seeders,
            on_click_leechers,
            on_click_size,
            on_click_uploader,
            sort,
        }
    }
}

enum SearchResultsStep {
    Sort {
        column: SortColumn,
        direction: Direction,
    },
    TorrentSelected(Box<Torrent>),
}

impl<V: View> SearchResults<V> {
    async fn sort_event(&self) -> SearchResultsStep {
        use SortColumn::*;
        let sort_events = vec![
            self.on_click_name.next().map(|_| Name).boxed_local(),
            self.on_click_date.next().map(|_| Date).boxed_local(),
            self.on_click_seeders.next().map(|_| Seeders).boxed_local(),
            self.on_click_leechers
                .next()
                .map(|_| Leechers)
                .boxed_local(),
            self.on_click_size.next().map(|_| Size).boxed_local(),
            self.on_click_uploader
                .next()
                .map(|_| Uploader)
                .boxed_local(),
        ];
        let current_sort = self.sort.as_ref().clone();
        let column = mogwai::future::race_all(sort_events).await;
        let direction = if Some(column) == current_sort.column {
            if current_sort.direction == Direction::Descending {
                Direction::Ascending
            } else {
                Direction::Descending
            }
        } else {
            current_sort.direction
        };
        SearchResultsStep::Sort { column, direction }
    }

    async fn select_event(&self) -> SearchResultsStep {
        let torrent = mogwai::future::race_all(self.torrents.iter().map(|view| view.step())).await;
        SearchResultsStep::TorrentSelected(Box::new(torrent.clone()))
    }

    /// Resolves to the first selected torrent.
    async fn step(&mut self) -> Torrent {
        loop {
            match self.sort_event().or(self.select_event()).await {
                SearchResultsStep::Sort { column, direction } => {
                    let current_sort = self.sort.deref();
                    if Some(column) != current_sort.column || direction != current_sort.direction {
                        self.torrents.sort_by(|a, b| {
                            let a = &a.torrent;
                            let b = &b.torrent;
                            let ord = match column {
                                SortColumn::Name => a.name.cmp(&b.name),
                                SortColumn::Date => a.added_i64().cmp(&b.added_i64()),
                                SortColumn::Seeders => a.seeders_i64().cmp(&b.seeders_i64()),
                                SortColumn::Leechers => a.leechers_i64().cmp(&b.leechers_i64()),
                                SortColumn::Size => a.size_bytes().cmp(&b.size_bytes()),
                                SortColumn::Uploader => a.username.cmp(&b.username),
                            };
                            if direction == Direction::Descending {
                                ord.reverse()
                            } else {
                                ord
                            }
                        });
                    }
                    self.sort.set(Sort {
                        column: Some(column),
                        direction,
                    });

                    // Reorder the search results
                    for view in self.torrents.iter() {
                        self.table.append_child(&view.wrapper);
                    }
                }
                SearchResultsStep::TorrentSelected(t) => return *t,
            }
        }
    }

    fn set_search_results(&mut self, torrents: impl IntoIterator<Item = Torrent>) {
        self.torrents
            .iter()
            .for_each(|view| self.table.remove_child(view));
        let views = torrents
            .into_iter()
            .map(|t| {
                let view = TorrentView::new(t);
                self.table.append_child(&view);
                view
            })
            .collect();
        self.torrents = views;
    }
}

#[derive(ViewChild)]
pub struct SearchView<V: View> {
    #[child]
    wrapper: V::Element,
    input: V::Element,
    on_submit_query: V::EventListener,
    search_button: Button<V>,
    status_alert: Alert<V>,
    search_results: SearchResults<V>,
}

impl<V: View> Default for SearchView<V> {
    fn default() -> Self {
        let status_alert = Alert::new("Enter a search query", Flavor::Info);
        let mut search_button = Button::new("Search", Some(Flavor::Primary));
        search_button.get_icon_mut().set_glyph(IconGlyph::MagnifyingGlass);
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
                        {&search_button}
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

enum Step<V: View> {
    Results(Box<Torrent>),
    Submit(V::Event),
}

impl<V: View> SearchView<V> {
    /// Resolves with a selected torrent.
    pub async fn step(&mut self) -> Torrent {
        log::info!("step");

        loop {
            let submission = self.on_submit_query.next().map(Step::Submit);
            let sorting = self
                .search_results
                .step()
                .map(|t| Step::Results(Box::new(t)));
            let ev: Step<V> = submission.or(sorting).await;
            match ev {
                Step::Results(t) => return *t,
                Step::Submit(ev) => {
                    ev.dyn_ev(|ev: &web_sys::Event| ev.prevent_default());
                    let search_query = self
                        .input
                        .dyn_el(|input: &web_sys::HtmlInputElement| input.value())
                        .unwrap_or_default();
                    self.status_alert.set_text(format!("Searching for '{search_query}'..."));
                    self.status_alert.set_flavor(Flavor::Info);
                    self.search_button.start_spinner();
                    self.search_button.disable();

                    match search(&search_query).await {
                        Ok(torrents) => {
                            self.status_alert
                                .set_text(format!("Found {} results.", torrents.len()));
                            self.status_alert.set_flavor(Flavor::Success);
                            self.search_results.set_search_results(torrents);
                            self.search_results.wrapper.set_style("display", "block");
                        }
                        Err(Error { msg }) => {
                            self.status_alert.set_text(msg);
                            self.status_alert.set_flavor(Flavor::Danger);
                        }
                    }
                    self.search_button.stop_spinner();
                    self.search_button.enable();
                }
            }
        }
    }
}

/// Enum wrapper to allow both SearchView and TorrentDetail in a single Panes<V, T>.
///
/// `Panes<V, T>` requires all panes to be the same type. This enum + manual
/// `ViewChild` impl (using `as_boxed_append_arg` to type-erase the iterator)
/// lets us store both view types in one `Panes` container.
pub enum AppPane<V: View> {
    Search(SearchView<V>),
    Detail(TorrentDetail<V>),
}

impl<V: View> ViewChild<V> for AppPane<V> {
    fn as_append_arg(&self) -> AppendArg<V, impl Iterator<Item = Cow<'_, V::Node>>> {
        match self {
            AppPane::Search(s) => s.as_boxed_append_arg(),
            AppPane::Detail(d) => d.as_boxed_append_arg(),
        }
    }
}

/// Pane indices for `Panes<V, AppPane<V>>`.
const SEARCH_PANE: usize = 0;
const DETAIL_PANE: usize = 1;

#[derive(ViewChild)]
pub struct App<V: View> {
    #[child]
    container: V::Element,
    panes: Panes<V, AppPane<V>>,
    is_in_search: bool,
    is_startup: bool,
}

impl<V: View> Default for App<V> {
    fn default() -> Self {
        rsx! {
            let pane_wrapper = div() {}
        }

        // Both views go in the panes vec so we can switch freely between
        // SEARCH_PANE (0) and DETAIL_PANE (1). The Panes default is a
        // placeholder that gets replaced immediately by select(SEARCH_PANE).
        let placeholder = AppPane::Detail(TorrentDetail::<V>::default());
        let mut panes = Panes::new(pane_wrapper, placeholder);
        panes.add_pane(AppPane::Search(SearchView::<V>::default()));
        panes.add_pane(AppPane::Detail(TorrentDetail::<V>::default()));
        panes.select(SEARCH_PANE);

        rsx! {
            let container = div() {
                nav(class = "navbar navbar-dark bg-dark mb-3") {
                    div(class = "container-fluid") {
                        span(class = "navbar-brand mb-0 h1") { "PirateBay" }
                    }
                }
                div(class = "container") {
                    {&panes}
                }
            }
        }

        Self {
            container,
            panes,
            is_in_search: true,
            is_startup: true,
        }
    }
}

impl<V: View> App<V> {
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

    fn search_view_mut(&mut self) -> &mut SearchView<V> {
        match self
            .panes
            .get_pane_at_mut(SEARCH_PANE)
            .expect("search pane")
        {
            AppPane::Search(s) => s,
            _ => panic!("expected search pane at index {SEARCH_PANE}"),
        }
    }

    fn detail_view(&self) -> &TorrentDetail<V> {
        match self
            .panes
            .get_pane_at(DETAIL_PANE)
            .expect("detail pane")
        {
            AppPane::Detail(d) => d,
            _ => panic!("expected detail pane at index {DETAIL_PANE}"),
        }
    }

    fn detail_view_mut(&mut self) -> &mut TorrentDetail<V> {
        match self
            .panes
            .get_pane_at_mut(DETAIL_PANE)
            .expect("detail pane")
        {
            AppPane::Detail(d) => d,
            _ => panic!("expected detail pane at index {DETAIL_PANE}"),
        }
    }

    fn show_detail(&mut self) {
        self.panes.select(DETAIL_PANE);
    }

    fn show_search(&mut self) {
        self.panes.select(SEARCH_PANE);
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
        if self.is_startup {
            let state = Self::get_state();
            self.set_info(state);
            self.is_startup = false;
        } else if self.is_in_search {
            log::info!("in search");
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
                    self.set_info(Some(info.clone()));
                    Self::store_state(Some(info));
                }
                Err(e) => self
                    .detail_view_mut()
                    .set_phase(TorrentDetailPhase::Err(e)),
            }
        } else {
            log::info!("in detail");
            self.detail_view().step().await;
            self.is_in_search = true;
            log::info!("leaving detail");
        }
    }
}
