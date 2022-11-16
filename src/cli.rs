use clap::Parser;

#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// URL to start scan from
    pub url: String,

    /// Turn debugging information on
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Specifies the maximum number of concurrent connections used
    #[arg(short, long)]
    pub concurrency: Option<u32>,

    /// Specifies the timeout for each request in seconds
    #[arg(long)]
    pub timeout: Option<u8>,

    /// Specifies the connection timeout for each request in seconds
    #[arg(long)]
    pub connect_timeout: Option<u8>,
}
