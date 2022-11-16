use std::time::Duration;

use clap::Parser;
use log::LevelFilter;
use pretty_env_logger::env_logger::Builder;
use reqwest::Url;

use cli::Cli;
use scanner::Scanner;

mod cli;
mod scanner;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let log_level = match cli.verbose {
        0 => LevelFilter::Error,
        1 => LevelFilter::Info,
        2 => LevelFilter::Debug,
        _ => LevelFilter::Trace,
    };

    Builder::new().filter(None, log_level).init();

    let start_url = Url::parse(cli.url.as_str())?;
    start_url.host().expect("start_url must be absolute");

    let timeout = match cli.timeout {
        Some(timeout) => Duration::from_secs(timeout as u64),
        None => Duration::from_secs(5),
    };

    let connect_timeout = match cli.connect_timeout {
        Some(timeout) => Duration::from_secs(timeout as u64),
        None => Duration::from_secs(5),
    };

    let concurrency = cli.concurrency.unwrap_or(10);

    let mut scanner = Scanner::new(timeout, connect_timeout, concurrency)?;
    scanner.run(start_url).await?;

    Ok(())
}
