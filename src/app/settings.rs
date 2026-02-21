//! Settings view for configuring Transmission connection and copy destinations.
use futures_lite::FutureExt;
use iti::components::alert::Alert;
use iti::components::button::Button;
use iti::components::icon::IconGlyph;
use iti::components::Flavor;
use mogwai::{future::MogwaiFutureExt, web::prelude::*};
use pb_wire_types::{AppError, ErrorKind, TransmissionConfig};

use super::invoke;

async fn get_transmission_config() -> Result<TransmissionConfig, AppError> {
    #[derive(serde::Serialize)]
    struct Empty {}
    invoke::cmd("get_transmission_config", &Empty {}).await
}

async fn set_transmission_config(config: &TransmissionConfig) -> Result<(), AppError> {
    #[derive(serde::Serialize)]
    struct Wrapper {
        config: TransmissionConfig,
    }
    invoke::cmd(
        "set_transmission_config",
        &Wrapper {
            config: config.clone(),
        },
    )
    .await
}

async fn test_transmission_connection() -> Result<String, AppError> {
    #[derive(serde::Serialize)]
    struct Empty {}
    invoke::cmd("test_transmission_connection", &Empty {}).await
}

/// Settings view for configuring Transmission RPC connection and copy destinations.
#[derive(ViewChild)]
pub struct SettingsView<V: View> {
    #[child]
    wrapper: V::Element,
    host_input: V::Element,
    port_input: V::Element,
    username_input: V::Element,
    password_input: V::Element,
    movies_dir_input: V::Element,
    shows_dir_input: V::Element,
    save_button: Button<V>,
    test_button: Button<V>,
    on_click_save: V::EventListener,
    on_click_test: V::EventListener,
    status_alert: Alert<V>,
}

impl<V: View> Default for SettingsView<V> {
    fn default() -> Self {
        let status_alert = Alert::new("", Flavor::Info);
        status_alert.set_is_visible(false);

        let mut save_button = Button::new("Save", Some(Flavor::Primary));
        save_button.get_icon_mut().set_glyph(IconGlyph::Check);

        let mut test_button = Button::new("Test Connection", Some(Flavor::Secondary));
        test_button.get_icon_mut().set_glyph(IconGlyph::Globe);

        rsx! {
            let wrapper = div(class = "container-fluid") {
                h5(class = "mb-3") { "Transmission Settings" }
                div(class = "mb-3") {
                    {&status_alert}
                }
                div(class = "mb-3") {
                    label(class = "form-label") { "Host" }
                    let host_input = input(
                        class = "form-control",
                        type = "text",
                        value = "localhost",
                        placeholder = "localhost",
                    ){}
                }
                div(class = "mb-3") {
                    label(class = "form-label") { "Port" }
                    let port_input = input(
                        class = "form-control",
                        type = "number",
                        value = "9091",
                        placeholder = "9091",
                    ){}
                }
                div(class = "mb-3") {
                    label(class = "form-label") { "Username (optional)" }
                    let username_input = input(
                        class = "form-control",
                        type = "text",
                        placeholder = "Leave blank if no auth",
                    ){}
                }
                div(class = "mb-3") {
                    label(class = "form-label") { "Password (optional)" }
                    let password_input = input(
                        class = "form-control",
                        type = "password",
                        placeholder = "Leave blank if no auth",
                    ){}
                }
                h5(class = "mb-3 mt-4") { "Copy Destinations" }
                div(class = "mb-3") {
                    label(class = "form-label") { "Movies Directory" }
                    let movies_dir_input = input(
                        class = "form-control",
                        type = "text",
                        placeholder = "/Volumes/Media/Movies",
                    ){}
                    div(class = "form-text") {
                        "Completed movie torrents will be copied here."
                    }
                }
                div(class = "mb-3") {
                    label(class = "form-label") { "Shows Directory" }
                    let shows_dir_input = input(
                        class = "form-control",
                        type = "text",
                        placeholder = "/Volumes/Media/TV Shows",
                    ){}
                    div(class = "form-text") {
                        "Completed TV show torrents will be copied here."
                    }
                }
                div(class = "d-flex gap-2") {
                    div(on:click = on_click_save) {
                        {&save_button}
                    }
                    div(on:click = on_click_test) {
                        {&test_button}
                    }
                }
            }
        }
        Self {
            wrapper,
            host_input,
            port_input,
            username_input,
            password_input,
            movies_dir_input,
            shows_dir_input,
            save_button,
            test_button,
            on_click_save,
            on_click_test,
            status_alert,
        }
    }
}

enum SettingsAction {
    Save,
    Test,
}

impl<V: View> SettingsView<V> {
    fn read_config(&self) -> TransmissionConfig {
        let host = self
            .host_input
            .dyn_el(|input: &web_sys::HtmlInputElement| input.value())
            .unwrap_or_else(|| "localhost".into());
        let port_str = self
            .port_input
            .dyn_el(|input: &web_sys::HtmlInputElement| input.value())
            .unwrap_or_else(|| "9091".into());
        let port: u16 = port_str.parse().unwrap_or(9091);
        let username = self
            .username_input
            .dyn_el(|input: &web_sys::HtmlInputElement| input.value())
            .unwrap_or_default();
        let password = self
            .password_input
            .dyn_el(|input: &web_sys::HtmlInputElement| input.value())
            .unwrap_or_default();
        let movies_dir = self
            .movies_dir_input
            .dyn_el(|input: &web_sys::HtmlInputElement| input.value())
            .unwrap_or_default();
        let shows_dir = self
            .shows_dir_input
            .dyn_el(|input: &web_sys::HtmlInputElement| input.value())
            .unwrap_or_default();
        TransmissionConfig {
            host,
            port,
            username: if username.is_empty() {
                None
            } else {
                Some(username)
            },
            password: if password.is_empty() {
                None
            } else {
                Some(password)
            },
            movies_dir: if movies_dir.is_empty() {
                None
            } else {
                Some(movies_dir)
            },
            shows_dir: if shows_dir.is_empty() {
                None
            } else {
                Some(shows_dir)
            },
        }
    }

    fn set_config_values(&self, config: &TransmissionConfig) {
        self.host_input.dyn_el(|input: &web_sys::HtmlInputElement| {
            input.set_value(&config.host);
        });
        self.port_input.dyn_el(|input: &web_sys::HtmlInputElement| {
            input.set_value(&config.port.to_string());
        });
        self.username_input
            .dyn_el(|input: &web_sys::HtmlInputElement| {
                input.set_value(config.username.as_deref().unwrap_or(""));
            });
        self.password_input
            .dyn_el(|input: &web_sys::HtmlInputElement| {
                input.set_value(config.password.as_deref().unwrap_or(""));
            });
        self.movies_dir_input
            .dyn_el(|input: &web_sys::HtmlInputElement| {
                input.set_value(config.movies_dir.as_deref().unwrap_or(""));
            });
        self.shows_dir_input
            .dyn_el(|input: &web_sys::HtmlInputElement| {
                input.set_value(config.shows_dir.as_deref().unwrap_or(""));
            });
    }

    /// Load settings from backend on initial display.
    pub async fn load(&self) {
        match get_transmission_config().await {
            Ok(config) => {
                self.set_config_values(&config);
            }
            Err(e) => {
                log::error!("Failed to load config: {e}");
            }
        }
    }

    pub async fn step(&mut self) {
        let action = self
            .on_click_save
            .next()
            .map(|_| SettingsAction::Save)
            .or(self.on_click_test.next().map(|_| SettingsAction::Test))
            .await;

        match action {
            SettingsAction::Save => {
                let config = self.read_config();
                self.save_button.start_spinner();
                self.save_button.disable();
                match set_transmission_config(&config).await {
                    Ok(()) => {
                        self.status_alert.set_text("Settings saved.");
                        self.status_alert.set_flavor(Flavor::Success);
                        self.status_alert.set_is_visible(true);
                    }
                    Err(e) => {
                        self.status_alert.set_text(format!("Failed to save: {e}"));
                        self.status_alert.set_flavor(Flavor::Danger);
                        self.status_alert.set_is_visible(true);
                    }
                }
                self.save_button.stop_spinner();
                self.save_button.enable();
            }
            SettingsAction::Test => {
                // Save first, then test
                let config = self.read_config();
                self.test_button.start_spinner();
                self.test_button.disable();
                // Save before testing so the backend uses the current values
                let _ = set_transmission_config(&config).await;
                match test_transmission_connection().await {
                    Ok(msg) => {
                        self.status_alert.set_text(msg);
                        self.status_alert.set_flavor(Flavor::Success);
                        self.status_alert.set_is_visible(true);
                    }
                    Err(e) => {
                        let msg = match e.kind {
                            ErrorKind::TransmissionConnection => format!(
                                "Connection failed: {}. \
                                 Make sure Transmission is running and remote \
                                 access is enabled in Preferences \u{203a} Remote.",
                                e.message
                            ),
                            _ => format!("Connection failed: {e}"),
                        };
                        self.status_alert.set_text(msg);
                        self.status_alert.set_flavor(Flavor::Danger);
                        self.status_alert.set_is_visible(true);
                    }
                }
                self.test_button.stop_spinner();
                self.test_button.enable();
            }
        }
    }
}
