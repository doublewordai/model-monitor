use ai_vitals::{Config, Monitor};
use anyhow::{Context, Result};
use clap::Parser;
use std::process::exit;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

/// Setup tracing/logging for the application
fn setup_logging() {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        // Default to info level if no RUST_LOG env var is set
        EnvFilter::new("ai_vitals=info")
    });

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(env_filter)
        .init();
}

#[tokio::main]
async fn main() -> Result<()> {
    setup_logging();

    let config = Config::parse();

    let monitor = Monitor::new(config).context("Failed to create monitor")?;
    let exit_code = monitor.run().await;

    exit(exit_code);
}
