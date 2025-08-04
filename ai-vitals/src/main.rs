use anyhow::{Context, Result};
use chrono::Utc;
use clap::Parser;
use hostname::get;
use reqwest::Client;
use serde_json::json;
use std::{process::exit, time::Duration};
use tracing::{error, info};

/// State of a Cronitor ping
#[derive(Debug, Clone, Copy)]
enum PingState {
    Run,
    Complete,
    Fail,
}

impl PingState {
    fn as_str(&self) -> &'static str {
        match self {
            PingState::Run => "run",
            PingState::Complete => "complete",
            PingState::Fail => "fail",
        }
    }
}

/// Type of LLM endpoint to probe
#[derive(Debug, Clone, Copy)]
enum EndpointType {
    ChatCompletion,
    Embedding,
}

impl From<&str> for EndpointType {
    fn from(model_type: &str) -> Self {
        match model_type {
            "chat" => EndpointType::ChatCompletion,
            "embedding" => EndpointType::Embedding,
            _ => {
                error!("Unsupported model_type: {model_type}");
                exit(2);
            }
        }
    }
}

/// Probe an LLM endpoint and report status to Cronitor.
#[derive(Parser, Debug, Clone)]
#[command(author, version, about)]
struct Args {
    /// Base URL for Cronitor, e.g. https://cronitor.link
    #[arg(long, env = "CRONITOR_BASE_URL")]
    cronitor_base_url: String,

    /// Monitor name / code in Cronitor
    #[arg(long, env = "MONITOR_NAME")]
    monitor_name: String,

    /// Base URL of the server to probe, e.g. https://my-openai-proxy
    #[arg(long, env = "SERVER_URL")]
    server_url: String,

    /// "chat" or "embedding"
    #[arg(long, env = "ENDPOINT_TYPE")]
    endpoint_type: EndpointType,

    /// Name of the model to query
    #[arg(long, env = "MODEL_NAME")]
    model_name: String,

    /// Environment descriptor (defaults to "production")
    #[arg(long, env = "APP_ENV", default_value = "production")]
    env: String,

    /// Request timeout in seconds (default 10)
    #[arg(long, env = "TIMEOUT_SECONDS", default_value_t = 10)]
    timeout_seconds: u64,
}

/// Cronitor client to send pings.
struct CronitorClient {
    args: Args,
    client: Client,
    host: String,
    series_id: String,
}

impl CronitorClient {
    fn new(args: Args) -> Result<Self> {
        // ---------------- reqwest client ---------------------
        let client = Client::builder()
            .timeout(Duration::from_secs(args.timeout_secs))
            .build()
            .context("building reqwest client")?;

        // ---------------- hostname ---------------------------
        let host = get().unwrap_or_default().to_string_lossy().into_owned();

        // ---------------- series id & hostname ----------------
        let series_id = format!("{}-{}", Utc::now().timestamp(), std::process::id());
        info!("Starting job with series ID: {series_id}");

        Ok(CronitorClient {
            args,
            client,
            host,
            series_id,
        })
    }

    async fn ping(&self, state: PingState, status_code: u16, message: Option<&str>) {
        // Build: <base>/<monitor>?state=...&series=...&status_code=...&env=...&host=...
        let mut url = format!(
            "{}/{}?state={}&series={}&status_code={}&env={}&host={}",
            self.args.cronitor_base_url,
            self.args.monitor_name,
            state.as_str(),
            self.series_id,
            status_code,
            self.args.env,
            self.host
        );
        if let Some(msg) = message {
            url.push_str("&message=");
            url.push_str(&urlencoding::encode(msg));
        }
        // Fire-and-forget (errors are only logged)
        if let Err(e) = self.client.get(url).send().await {
            error!("Failed to send ping to Cronitor: {e}");
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let cronitor_client =
        CronitorClient::new(args.clone()).context("initializing Cronitor client")?;

    // ---------------- "run" ping ---------------------------
    info!("Sending start ping to Cronitor");
    cronitor_client.ping(PingState::Run, 0, None).await;

    // ---------------- build payload -----------------------
    let (endpoint, payload) = match args.endpoint_type {
        EndpointType::ChatCompletion => (
            format!("{}/v1/chat/completions", args.server_url),
            json!({
                "model": args.model_name,
                "messages": [{ "role": "user", "content": "test" }],
                "max_tokens": 1
            }),
        ),
        EndpointType::Embedding => (
            format!("{}/v1/embeddings", args.server_url),
            json!({
                "model": args.model_name,
                "input": "test"
            }),
        )
    };

    // ---------------- send request ------------------------
    info!("Querying {endpoint}");
    let client = Client::builder()
        .timeout(Duration::from_secs(args.timeout_seconds))
        .build()
        .context("building reqwest client")?;

    let result = client.post(&endpoint).json(&payload).send().await;

    match result {
        Ok(resp) => {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            info!("Response body: {body}");

            if status.is_success() {
                info!("Sending success ping to Cronitor");
                cronitor_client.ping(PingState::Complete, 0, None).await;
                info!(
                    "SUCCESS: {endpoint}/{}/ responded successfully",
                    args.model_name
                );
                exit(0);
            } else {
                info!("Sending failure ping to Cronitor");
                cronitor_client.ping(PingState::Fail, status.as_u16(), None).await;
                error!(
                    "FAILURE: {endpoint}/{}/ failed with HTTP {status}",
                    args.model_name
                );
                exit(1);
            }
        }
        Err(e) if e.is_timeout() => {
            info!("Sending timeout ping to Cronitor");
            cronitor_client
                .ping(PingState::Fail, 124, Some("Request timeout"))
                .await;
            error!(
                "TIMEOUT: {endpoint}/{}/ request timed out after {} s",
                args.model_name, args.timeout_seconds
            );
            exit(124);
        }
        Err(e) => {
            info!("Sending failure ping to Cronitor");
            cronitor_client
                .ping(PingState::Fail, 1, Some(&format!("Reqwest error: {e}")))
                .await;
            error!(
                "FAILURE: {endpoint}/{}/ request failed: {e}",
                args.model_name
            );
            exit(1);
        }
    }
}
