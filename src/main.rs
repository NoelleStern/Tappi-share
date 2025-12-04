use crate::{app::app_main::App, cli::Cli, logger::init_logger};
use clap::Parser;

pub mod app;
pub mod cli;
pub mod client;
pub mod logger;
pub mod server;
pub mod ui;

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    let result = startup().await;
    if let Err(err) = &result {
        log::error!("{}", err);
    } else {
        log::info!("Application stopped without an error");
    }
    result
}

async fn startup() -> color_eyre::Result<()> {
    color_eyre::install()?; // Init debug

    let args = Cli::parse(); // Parse arguments
    let mut terminal = ratatui::init(); // Create terminal

    init_logger(&args)?; // Init logger

    log::info!("Application started");
    let result = App::new(args.clone())?.run(&args, &mut terminal).await; // Run main loop

    ratatui::restore(); // Restore terminal
    result
}
