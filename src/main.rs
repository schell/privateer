// use leptos::prelude::*;
use mogwai::web::prelude::*;

mod app;
use app::*;
use wasm_bindgen::UnwrapThrowExt;

fn main() {
    console_error_panic_hook::set_once();
    console_log::init_with_level(log::Level::Trace).unwrap_throw();
    log::info!("start");

    iti::assets::embedded::inject_styles();

    let mut app = App::<Web>::default();
    mogwai::web::body().append_child(&app);
    wasm_bindgen_futures::spawn_local(async move {
        loop {
            app.step().await;
        }
    });
}
