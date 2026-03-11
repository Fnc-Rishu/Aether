use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "aether")]
#[command(
    about = "Twitter Web Push Receiver (FCM)",
    long_about = "A CLI tool for receiving tweet notifications by emulating a Chrome Web Push client via FCM."
)]
pub struct Cli {
    #[arg(short, long, default_value = "aether.toml")]
    pub config: PathBuf,
    #[arg(short, long)]
    pub verbose: bool,
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    Init {
        #[arg(long)]
        auth_token: Option<String>,
        #[arg(long)]
        ct0: Option<String>,
    },
    Register,
    Listen,
    Status,
    Unregister,
}
