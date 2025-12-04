use log::LevelFilter;
use simplelog::{CombinedLogger, Config, WriteLogger};
use std::fs::File;

use crate::cli::Cli;

pub fn init_logger(cli: &Cli) -> color_eyre::Result<()> {
    if cli.log_level != LevelFilter::Off {
        CombinedLogger::init(vec![WriteLogger::new(
            cli.log_level,
            Config::default(),
            File::create(cli.log_file.clone())?,
        )])?;
    }

    Ok(())
}
