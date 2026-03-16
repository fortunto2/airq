use tracing_subscriber::EnvFilter;

mod app;
mod state;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,airq_dashboard=debug")),
        )
        .init();

    dioxus::LaunchBuilder::desktop()
        .with_cfg(
            dioxus::desktop::Config::new()
                .with_disable_context_menu(true),
        )
        .launch(app::App);
}
