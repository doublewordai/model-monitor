use ai_vitals::{Monitor, cli::Config};
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::process::exit;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
#[allow(clippy::large_enum_variant)]
enum Commands {
    /// Run monitoring probes against an endpoint
    Monitor {
        #[command(flatten)]
        config: Config,
    },

    /// Run the web dashboard
    #[cfg(feature = "web")]
    Web {
        /// Database URL (supports PostgreSQL and SQLite)
        #[arg(long, env = "DATABASE_URL")]
        database_url: Option<String>,

        /// Port to run the web server on
        #[arg(long, env = "PORT", default_value = "8080")]
        port: u16,
    },
}

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

    let cli = Cli::parse();

    match cli.command {
        Commands::Monitor { config } => {
            let interval_seconds = config.interval_seconds;
            let monitor = Monitor::new(config).context("Failed to create monitor")?;

            if interval_seconds > 0 {
                // Run continuously on interval
                monitor.run_continuous(interval_seconds).await?;
                // This should never return since run_continuous loops forever
                // If it does return, something went wrong
                exit(1);
            } else {
                // Run once and exit
                let exit_code = monitor.run().await;
                exit(exit_code);
            }
        }

        #[cfg(feature = "web")]
        Commands::Web { database_url, port } => {
            let database_url = database_url
                .or_else(|| std::env::var("DATABASE_URL").ok())
                .context("DATABASE_URL must be set for web mode")?;

            ai_vitals::web::run_server(database_url, port).await?;
            Ok(())
        }
    }
}
