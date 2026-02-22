// use leptos::prelude::*;
use mogwai::web::prelude::*;

mod app;
use app::*;
use wasm_bindgen::UnwrapThrowExt;
use web_sys::{wasm_bindgen::JsCast, HtmlLinkElement};

fn main() {
    console_error_panic_hook::set_once();
    console_log::init_with_level(log::Level::Trace).unwrap_throw();
    log::info!("start");

    iti::assets::embedded::inject_styles();

    {
        // Move the override styles to the end of head
        let head = mogwai::web::document().head().expect("head");
        let children = head.child_nodes();
        for index in 0..children.length() {
            let child = children.get(index).expect("nodes");
            if let Ok(link) = child.dyn_into::<HtmlLinkElement>() {
                let rel = link.get_attribute("rel");
                if rel.as_deref() == Some("stylesheet") {
                    // Append it to the end
                    web_sys::Node::append_child(&head, &link).expect("could not append stylesheet");
                    break;
                }
            }
        }
    }

    let mut app = App::<Web>::default();
    let body = mogwai::web::body();
    body.set_attribute("class", "system-9")
        .expect("can always set class");
    body.append_child(&app);
    wasm_bindgen_futures::spawn_local(async move {
        loop {
            app.step().await;
        }
    });
}
