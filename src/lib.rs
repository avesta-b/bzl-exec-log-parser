pub mod proto;
pub mod cli;
pub mod commands;
pub mod error;

pub use error::{AppError, AppResult};
pub use cli::Cli;

use clap::Parser;

/// Main library entry point
pub fn run() -> AppResult<()> {
    let cli = Cli::parse();
    commands::analyze::run_analyze(cli)
}