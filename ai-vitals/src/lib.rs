//! # ai-vitals
//!
//! A monitoring tool for LLM endpoints that reports status to Cronitor.
//! 
//! The library is split into a few main components:
//! 
//! * monitor: Entrypoint for running the monitoring process. It orchestrates the probing of endpoints and exporting results.
//! * cli: Handles command-line argument parsing and configuration setup.
//! * probes: Contains implementations for probing different types of endpoints, such as OpenAI chat completions and embeddings.
//! * exporters: Contains implementations for exporting monitoring results to different services, currently only Cronitor.
//! 
//! ## Running Tests
//! 
//! ```bash
//! cargo test
//! ```
use anyhow::Result;
use tracing::{error, info};

/// Result of an LLM endpoint probe
#[derive(Debug, PartialEq)]
pub enum ProbeResult {
    Success,
    HttpError(u16),
    Timeout,
    NetworkError(String),
}

#[async_trait::async_trait]
pub trait Probe {
    fn new(config: cli::Config) -> Result<Self> where Self: std::marker::Sized;
    async fn probe(&self) -> ProbeResult;
}

/// State of a Export ping
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PingState {
    Run,
    Complete,
    Fail,
}

impl PingState {
    pub fn as_str(&self) -> &'static str {
        match self {
            PingState::Run => "run",
            PingState::Complete => "complete",
            PingState::Fail => "fail",
        }
    }
}

#[async_trait::async_trait]
pub trait Export {
    fn new(config: cli::Config) -> Result<Self> where Self: std::marker::Sized;
    async fn ping(&self, state: PingState, status_code: u16, message: Option<&str>);
}

/// Main monitoring orchestrator.
/// 
/// It holds the exporter and probe implementations and runs the monitoring process.
pub struct Monitor {
    exporter: Box<dyn Export>,
    probe: Box<dyn Probe>,
}

impl Monitor {
    pub fn new(config: cli::Config) -> Result<Self> {
        let exporter = exporters::Cronitor::new(config.clone())?;
        let llm_probe = match config.endpoint_type {
            probes::Type::OpenAIChatCompletion | probes::Type::OpenAIEmbedding => {
                probes::OpenAI::new(config.clone())?
            }
            _ => {
                return Err(anyhow::anyhow!(
                    "Unsupported endpoint type: {:?}",
                    config.endpoint_type
                ))
            }
        };

        Ok(Monitor {
            exporter: Box::new(exporter),
            probe: Box::new(llm_probe),
        })
    }

    pub async fn run(&self) -> i32 {
        // Send start ping
        info!("Sending start ping to Cronitor");
        self.exporter.ping(PingState::Run, 0, None).await;

        // Probe the endpoint
        match self.probe.probe().await {
            ProbeResult::Success => {
                info!("Sending success ping to Cronitor");
                self.exporter
                    .ping(PingState::Complete, 0, None)
                    .await;
                info!("SUCCESS: Endpoint responded successfully");
                0
            }
            ProbeResult::HttpError(status_code) => {
                info!("Sending failure ping to Cronitor");
                self.exporter
                    .ping(PingState::Fail, status_code, None)
                    .await;
                error!("FAILURE: Endpoint failed with HTTP {status_code}");
                1
            }
            ProbeResult::Timeout => {
                info!("Sending timeout ping to Cronitor");
                self.exporter
                    .ping(PingState::Fail, 124, Some("Request timeout"))
                    .await;
                error!("TIMEOUT: Request timed out");
                124
            }
            ProbeResult::NetworkError(error) => {
                info!("Sending failure ping to Cronitor");
                self.exporter
                    .ping(PingState::Fail, 1, Some(&format!("Network error: {error}")))
                    .await;
                error!("FAILURE: Network error: {error}");
                1
            }
        }
    }
}

pub mod cli {
    use clap::Parser;

    use super::probes::Type as ProbeType;

    /// Configuration for the monitoring tool
    #[derive(Parser, Debug, Clone, PartialEq)]
    #[command(
        author,
        version,
        about,
        long_about = "Probe an LLM endpoint and report status to Cronitor."
    )]
    pub struct Config {
        /// Base URL for Cronitor, e.g. https://cronitor.link
        #[arg(long, env = "CRONITOR_BASE_URL")]
        pub cronitor_base_url: String,

        /// Base URL for Cronitor, e.g. https://cronitor.link
        #[arg(long, env = "CRONITOR_API_KEY")]
        pub cronitor_api_key: Option<String>,

        /// Monitor name / code in Cronitor
        #[arg(long, env = "MONITOR_NAME")]
        pub monitor_name: String,

        /// Base URL of the server to probe, e.g. https://my-openai-proxy
        #[arg(long, env = "SERVER_URL")]
        pub server_url: String,

        /// Optional: Probe type to use for the probe. Currently only "llm" is supported.
        #[arg(long, env = "ENDPOINT_TYPE", default_value = ProbeType::OpenAIChatCompletion)]
        pub endpoint_type: ProbeType,

        /// Name of the model to query
        #[arg(long, env = "MODEL_NAME")]
        pub model_name: String,

        /// Environment descriptor (defaults to "production")
        #[arg(long, env = "APP_ENV", default_value = "production")]
        pub env: String,

        /// Request timeout in seconds (default 10)
        #[arg(long, env = "TIMEOUT_SECONDS", default_value_t = 10)]
        pub timeout_seconds: u64,
        
        /// The below all require an API key to be set to take effect.

        /// minFreqRequiredMins catches inactive alerts - if an alert starts but never completes, 
        /// it'll be marked as inactive by Cronitor. To force this into raising an alert,
        /// we require a successful ping once per any minFreqRequiredMins period. 
        #[arg(long, env = "MIN_SUCCESS_FREQ")]
        pub min_success_freq: Option<u8>,
        
        /// Which schedule to display in the frontend and to guide CONSECUTIVE_FAILURES_FOR_ALERT.
        #[arg(long, env = "SCHEDULE")]
        pub schedule: Option<String>,
        
        /// Optional: how many failed pings are needed to trigger an alert. Cronitor assumes 1 if unset.
        #[arg(long, env = "CONSECUTIVE_FAILURES_FOR_ALERT")]
        pub consecutive_failures: Option<u8>,

        /// Optional: how many missing pings are needed to trigger an alert. Cronitor disables this
        /// unless specified here as > 0. Requires schedule to be set.
        #[arg(long, env = "CONSECUTIVE_MISSING_FOR_ALERT")]
        pub consecutive_missing: Option<u8>,

        /// Optional: Group to put monitor in, mostly for frontend viewing.
        #[arg(long, env = "MONITOR_GROUP")]
        pub monitor_group: Option<String>,
    }

    impl Default for Config {
        fn default() -> Self {
            Config {
                cronitor_base_url: "https://cronitor.link".to_string(),
                cronitor_api_key: None,
                monitor_name: "test-monitor".to_string(),
                server_url: "https://api.openai.com".to_string(),
                endpoint_type: ProbeType::OpenAIChatCompletion,
                model_name: "gpt-4".to_string(),
                env: "test".to_string(),
                timeout_seconds: 10,
                schedule: Option::from("*/5 * * * *".to_string()),
                consecutive_failures: Some(1),
                min_success_freq: Some(60),
                monitor_group: None,
                consecutive_missing: Some(1),
            }
        }
    }
}

pub mod exporters {
    use anyhow::{Context, Result};
    use chrono::Utc;
    use hostname::get;
    use reqwest::Client;
    use serde_json::json;
    use std::time::Duration;
    use tracing::{error, info};

    use crate::Export;

    use super::{cli::Config, PingState};

    /// Cronitor client to send pings
    pub struct Cronitor {
        config: Config,
        client: Client,
        host: String,
        series_id: String,
    }

    /// Cronitor exporter implementation
    #[async_trait::async_trait]
    impl Export for Cronitor {
        fn new(config: Config) -> Result<Self> {
            let client = Client::builder()
                .timeout(Duration::from_secs(config.timeout_seconds))
                .build()
                .context("building reqwest client")?;

            let host = get().unwrap_or_default().to_string_lossy().into_owned();
            let series_id = format!("{}-{}", Utc::now().timestamp(), std::process::id());

            info!("Starting job with series ID: {series_id}");

            Ok(Cronitor {
                config,
                client,
                host,
                series_id,
            })
        }

        async fn ping(&self, state: PingState, status_code: u16, message: Option<&str>) {
            let url = self.build_ping_url(state, status_code, message);

            match self.client.get(&url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    // success: optionally peek at body for debugging
                    info!("Cronitor ping OK");
                }
                Ok(resp) => {
                    // non-2xx: log status + response body (often has the reason)
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default(); // consumes resp
                    error!("Cronitor ping non-2xx {status}: {body}");
                }
                Err(e) => {
                    // request failed before a response was received
                    error!("Failed to send ping to Cronitor: {e}");
                }
            }

            if state == PingState::Run {
                // The above handles the ping. We also want to update the created monitor if we can.

                let Some(api_key) = self.config.cronitor_api_key.as_deref() else {
                    info!("No api key, skipping monitor enrichment");
                    return; // no key => skip update
                };

                match self
                    .client
                    .put("https://cronitor.io/api/monitors")
                    .basic_auth(api_key, Some("")) // username = API key, blank password
                    .json(&self.get_monitor_update_payload())
                    .send()
                    .await
                {
                    Ok(resp) if resp.status().is_success() => {
                        info!("Monitor enriched successful");
                    }
                    Ok(resp) => {
                        if !resp.status().is_success() {
                            error!(
                                "Monitor enrichment failed {}: {}",
                                resp.status(),
                                resp.text().await.unwrap_or_default()
                            );
                        }
                    }
                    Err(err) => {
                        error!("Failed to enrich Cronitor monitor: {err}");
                    }
                }
            }
        }
    }

    /// Internal methods for Cronitor
    impl Cronitor {
        pub fn build_ping_url(
            &self,
            state: PingState,
            status_code: u16,
            message: Option<&str>,
        ) -> String {
            let mut url = format!(
                "{}/{}?state={}&series={}&status_code={}&env={}&host={}",
                self.config.cronitor_base_url,
                self.config.monitor_name,
                state.as_str(),
                self.series_id,
                status_code,
                self.config.env,
                self.host
            );
            if let Some(msg) = message {
                url.push_str("&message=");
                url.push_str(&urlencoding::encode(msg));
            }
            url
        }

        pub fn get_monitor_update_payload(&self) -> serde_json::Value {
            let mut monitor = serde_json::Map::new();
            monitor.insert("type".into(), json!("job"));
            monitor.insert("key".into(), json!(self.config.monitor_name));

            if let Some(consecutive_failures) = self.config.consecutive_failures {
                monitor.insert("failure_tolerance".into(), json!(consecutive_failures));
            }

            if let Some(schedule) = self.config.schedule.clone() {
                monitor.insert("schedule".into(), json!(schedule));
            }

            
            if let (Some(consecutive_missing), Some(_)) = (self.config.consecutive_missing, self.config.schedule.clone()) {
                monitor.insert("schedule_tolerance".into(), json!(consecutive_missing));
            }
            
            if let Some(group) = self.config.monitor_group.clone() {
                monitor.insert("group".into(), json!(group));
            }

            // always include the duration assertion
            let mut assertions: Vec<String> = vec![format!(
                "metric.duration < {}s",
                self.config.timeout_seconds * 2
            )];

            if let Some(min_success_freq) = self.config.min_success_freq {
                assertions.push(format!("job.completes < {min_success_freq} minute"));
            }
            monitor.insert("assertions".into(), json!(assertions));

            json!({ "monitors": [serde_json::Value::Object(monitor)] })
        }
    }
}

pub mod probes {
    use anyhow::{Context, Result};
    use reqwest::Client;
    use serde_json::json;
    use std::time::Duration;
    use tracing::info;

    use super::{cli::Config, ProbeResult};

    // Type of LLM endpoint to probe
    #[derive(Debug, Clone, Copy, PartialEq, clap::ValueEnum)]
    pub enum Type {
        #[value(name = "openai-chat-completion")]
        OpenAIChatCompletion,
        #[value(name = "openai-embedding")]
        OpenAIEmbedding,
        #[value(name = "newman")]
        Newman,
    }

    impl From<Type> for clap::builder::OsStr {
        fn from(value: Type) -> Self {
            match value {
                Type::OpenAIChatCompletion => "openai-chat-completion".into(),
                Type::OpenAIEmbedding => "openai-embedding".into(),
                Type::Newman => "newman".into(),
            }
        }
    }

    /// LLM endpoint probe functionality
    pub struct OpenAI {
        client: Client,
        config: Config,
    }

    /// LLM probe implementation
    #[async_trait::async_trait]
    impl super::Probe for OpenAI {
        fn new(config: Config) -> Result<Self> {
            let client = Client::builder()
                .timeout(Duration::from_secs(config.timeout_seconds))
                .build()
                .context("building reqwest client")?;

            Ok(OpenAI { client, config })
        }

        async fn probe(&self) -> ProbeResult {
            let endpoint = self.build_endpoint_url();
            let payload = self.build_payload();

            info!("Querying {endpoint}");

            match self.client.post(&endpoint).json(&payload).send().await {
                Ok(resp) => {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    info!("Response body: {body}");

                    if status.is_success() {
                        ProbeResult::Success
                    } else {
                        ProbeResult::HttpError(status.as_u16())
                    }
                }
                Err(e) if e.is_timeout() => ProbeResult::Timeout,
                Err(e) => ProbeResult::NetworkError(e.to_string()),
            }
        }
    }

    /// Internal methods for OpenAI probe
    impl OpenAI {
        pub fn build_endpoint_url(&self) -> String {
            match self.config.endpoint_type {
                Type::OpenAIChatCompletion => {
                    format!("{}/v1/chat/completions", self.config.server_url)
                }
                Type::OpenAIEmbedding => format!("{}/v1/embeddings", self.config.server_url),
                _ => panic!("Unsupported endpoint type"),
            }
        }

        pub fn build_payload(&self) -> serde_json::Value {
            match self.config.endpoint_type {
                Type::OpenAIChatCompletion => json!({
                    "model": self.config.model_name,
                    "messages": [{ "role": "user", "content": "test" }],
                    "max_tokens": 1,
                    "priority": -100
                }),
                Type::OpenAIEmbedding => json!({
                    "model": self.config.model_name,
                    "input": "test",
                    "priority": -100
                }),
                _ => panic!("Unsupported endpoint type"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{exporters::Cronitor, probes::{OpenAI, Type as EndpointType}, cli::Config, Export, Probe, Monitor, PingState, ProbeResult};
    use httpmock::prelude::*;
    use serde_json::json;

    #[test]
    fn test_cronitor_client_creation() {
        let config = Config::default();
        let client = Cronitor::new(config);
        assert!(client.is_ok());
    }

    #[test]
    fn test_cronitor_ping_url_construction_without_message() {
        let config = Config::default();
        let client = Cronitor::new(config).unwrap();

        let url = client.build_ping_url(PingState::Run, 0, None);

        assert!(url.contains("https://cronitor.link/test-monitor"));
        assert!(url.contains("state=run"));
        assert!(url.contains("status_code=0"));
        assert!(url.contains("env=test"));
        assert!(url.contains("series="));
        assert!(url.contains("host="));
        assert!(!url.contains("message="));
    }

    #[test]
    fn test_cronitor_ping_url_construction_with_message() {
        let config = Config::default();
        let client = Cronitor::new(config).unwrap();

        let url = client.build_ping_url(PingState::Fail, 500, Some("Test error"));

        assert!(url.contains("https://cronitor.link/test-monitor"));
        assert!(url.contains("state=fail"));
        assert!(url.contains("status_code=500"));
        assert!(url.contains("env=test"));
        assert!(url.contains("message=Test%20error")); // URL encoded
    }

    #[test]
    fn test_cronitor_ping_url_special_characters() {
        let config = Config::default();
        let client = Cronitor::new(config).unwrap();

        let url = client.build_ping_url(PingState::Fail, 500, Some("Error: 500 & timeout!"));

        assert!(url.contains("message=Error%3A%20500%20%26%20timeout%21"));
    }

    #[test]
    fn test_llm_probe_creation() {
        let config = Config::default();
        let probe = OpenAI::new(config);
        assert!(probe.is_ok());
    }

    #[test]
    fn test_llm_probe_chat_endpoint_url() {
        let config = Config {
            endpoint_type: EndpointType::OpenAIChatCompletion,
            server_url: "https://api.openai.com".to_string(),
            ..Default::default()
        };
        let probe = OpenAI::new(config).unwrap();

        let url = probe.build_endpoint_url();
        assert_eq!(url, "https://api.openai.com/v1/chat/completions");
    }

    #[test]
    fn test_llm_probe_embedding_endpoint_url() {
        let config = Config {
            endpoint_type: EndpointType::OpenAIEmbedding,
            server_url: "https://api.example.com".to_string(),
            ..Default::default()
        };
        let probe = OpenAI::new(config).unwrap();

        let url = probe.build_endpoint_url();
        assert_eq!(url, "https://api.example.com/v1/embeddings");
    }

    #[test]
    fn test_llm_probe_chat_payload() {
        let config = Config {
            endpoint_type: EndpointType::OpenAIChatCompletion,
            model_name: "a-piece-of-cheese".to_string(),
            ..Default::default()
        };
        let probe = OpenAI::new(config).unwrap();

        let payload = probe.build_payload();
        let expected = json!({
            "model": "a-piece-of-cheese",
            "messages": [{ "role": "user", "content": "test" }],
            "max_tokens": 1,
            "priority": -100
        });

        assert_eq!(payload, expected);
    }

    #[test]
    fn test_llm_probe_embedding_payload() {
        let config = Config {
            endpoint_type: EndpointType::OpenAIEmbedding,
            model_name: "text-embedding-ada-002".to_string(),
            ..Default::default()
        };
        let probe = OpenAI::new(config).unwrap();

        let payload = probe.build_payload();
        let expected = json!({
            "model": "text-embedding-ada-002",
            "input": "test",
            "priority": -100
        });

        assert_eq!(payload, expected);
    }

    #[tokio::test]
    async fn test_llm_probe_successful_response() {
        let server = MockServer::start();

        // Mock successful LLM response
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/v1/chat/completions")
                .json_body(json!({
                    "model": "gpt-4",
                    "messages": [{ "role": "user", "content": "test" }],
                    "max_tokens": 1,
                    "priority": -100
                }));
            then.status(200).json_body(json!({
                "choices": [{"message": {"role": "assistant", "content": "Hello"}}]
            }));
        });

        let config = Config {
            server_url: server.base_url(),
            endpoint_type: EndpointType::OpenAIChatCompletion,
            model_name: "gpt-4".to_string(),
            ..Default::default()
        };

        let probe = OpenAI::new(config).unwrap();
        let result = probe.probe().await;

        assert_eq!(result, ProbeResult::Success);

        mock.assert();
    }

    #[tokio::test]
    async fn test_llm_probe_http_error_response() {
        let server = MockServer::start();

        // Mock failed LLM response
        let mock = server.mock(|when, then| {
            when.method(POST).path("/v1/embeddings");
            then.status(420).json_body(json!({
                "error": {"message": "Internal server error"}
            }));
        });

        let config = Config {
            server_url: server.base_url(),
            endpoint_type: EndpointType::OpenAIEmbedding,
            model_name: "text-embedding-ada-002".to_string(),
            ..Default::default()
        };

        let probe = OpenAI::new(config).unwrap();
        let result = probe.probe().await;

        match result {
            ProbeResult::HttpError(status_code) => {
                assert_eq!(status_code, 420);
            }
            _ => panic!("Expected HTTP error probe result"),
        }

        mock.assert();
    }

    #[tokio::test]
    async fn test_llm_probe_timeout() {
        let config = Config {
            server_url: "http://10.255.255.1:12345".to_string(), // Non-routable IP for timeout
            timeout_seconds: 1,                                  // Very short timeout
            ..Default::default()
        };

        let probe = OpenAI::new(config).unwrap();
        let result = probe.probe().await;

        assert!(matches!(result, ProbeResult::Timeout));
    }

    #[tokio::test]
    async fn test_llm_probe_network_error() {
        let config = Config {
            server_url: "http://localhost:99999".to_string(), // Invalid port
            ..Default::default()
        };

        let probe = OpenAI::new(config).unwrap();
        let result = probe.probe().await;

        match result {
            ProbeResult::NetworkError(error) => {
                assert!(!error.is_empty());
            }
            _ => panic!("Expected network error probe result"),
        }
    }

    #[tokio::test]
    async fn test_monitor_creation() {
        let config = Config::default();
        let monitor = Monitor::new(config);
        assert!(monitor.is_ok());
    }

    #[tokio::test]
    async fn test_monitor_run_success() {
        let server = MockServer::start();

        // Mock successful LLM response
        let llm_mock = server.mock(|when, then| {
            when.method(POST).path("/v1/chat/completions");
            then.status(200).json_body(
                json!({"choices": [{"message": {"role": "assistant", "content": "OK"}}]}),
            );
        });

        // Mock Cronitor pings
        let cronitor_run_mock = server.mock(|when, then| {
            when.method(GET)
                .path("/test-monitor")
                .query_param("state", "run");
            then.status(200);
        });

        let cronitor_complete_mock = server.mock(|when, then| {
            when.method(GET)
                .path("/test-monitor")
                .query_param("state", "complete");
            then.status(200);
        });

        let config = Config {
            cronitor_base_url: server.base_url(),
            server_url: server.base_url(),
            ..Default::default()
        };

        let monitor = Monitor::new(config).unwrap();
        let exit_code = monitor.run().await;

        assert_eq!(exit_code, 0);
        llm_mock.assert();
        cronitor_run_mock.assert();
        cronitor_complete_mock.assert();
    }

    #[tokio::test]
    async fn test_monitor_run_http_error() {
        let server = MockServer::start();

        // Mock failed LLM response
        let llm_mock = server.mock(|when, then| {
            when.method(POST).path("/v1/chat/completions");
            then.status(500)
                .json_body(json!({"error": {"message": "Server error"}}));
        });

        // Mock Cronitor pings
        let cronitor_run_mock = server.mock(|when, then| {
            when.method(GET)
                .path("/test-monitor")
                .query_param("state", "run");
            then.status(200);
        });

        let cronitor_fail_mock = server.mock(|when, then| {
            when.method(GET)
                .path("/test-monitor")
                .query_param("state", "fail")
                .query_param("status_code", "500");
            then.status(200);
        });

        let config = Config {
            cronitor_base_url: server.base_url(),
            server_url: server.base_url(),
            ..Default::default()
        };

        let monitor = Monitor::new(config).unwrap();
        let exit_code = monitor.run().await;

        assert_eq!(exit_code, 1);
        llm_mock.assert();
        cronitor_run_mock.assert();
        cronitor_fail_mock.assert();
    }

    #[tokio::test]
    async fn test_monitor_run_timeout() {
        let server = MockServer::start();

        // Mock Cronitor pings
        let cronitor_run_mock = server.mock(|when, then| {
            when.method(GET)
                .path("/test-monitor")
                .query_param("state", "run");
            then.status(200);
        });

        let cronitor_fail_mock = server.mock(|when, then| {
            when.method(GET)
                .path("/test-monitor")
                .query_param("state", "fail")
                .query_param("status_code", "124")
                .query_param("message", "Request timeout");
            then.status(200);
        });

        let config = Config {
            cronitor_base_url: server.base_url(),
            server_url: "http://10.255.255.1:12345".to_string(), // Non-routable for timeout
            timeout_seconds: 1,
            ..Default::default()
        };

        let monitor = Monitor::new(config).unwrap();
        let exit_code = monitor.run().await;

        assert_eq!(exit_code, 124); // Timeout exit code
        cronitor_run_mock.assert();
        cronitor_fail_mock.assert();
    }

    #[tokio::test]
    async fn test_monitor_run_network_error() {
        let server = MockServer::start();

        // Mock Cronitor pings
        let cronitor_run_mock = server.mock(|when, then| {
            when.method(GET)
                .path("/test-monitor")
                .query_param("state", "run");
            then.status(200);
        });

        let cronitor_fail_mock = server.mock(|when, then| {
            when.method(GET)
                .path("/test-monitor")
                .query_param("state", "fail")
                .query_param("status_code", "1")
                .query_param_exists("message"); // Just check that message parameter exists
            then.status(200);
        });

        let config = Config {
            cronitor_base_url: server.base_url(),
            server_url: "http://localhost:99999".to_string(), // Invalid port
            ..Default::default()
        };

        let monitor = Monitor::new(config).unwrap();
        let exit_code = monitor.run().await;

        assert_eq!(exit_code, 1); // Network error exit code
        cronitor_run_mock.assert();
        cronitor_fail_mock.assert();
    }

    #[tokio::test]
    async fn test_monitor_cronitor_message_validation() {
        let server = MockServer::start();

        // Mock specific Cronitor ping calls with exact message validation
        let cronitor_run_mock = server.mock(|when, then| {
            when.method(GET)
                .path("/test-monitor")
                .query_param("state", "run")
                .query_param("status_code", "0")
                .query_param("env", "test");
            then.status(200);
        });

        let cronitor_timeout_mock = server.mock(|when, then| {
            when.method(GET)
                .path("/test-monitor")
                .query_param("state", "fail")
                .query_param("status_code", "124")
                .query_param("message", "Request timeout")
                .query_param("env", "test");
            then.status(200);
        });

        let config = Config {
            cronitor_base_url: server.base_url(),
            server_url: "http://10.255.255.1:12345".to_string(), // Non-routable for timeout
            timeout_seconds: 1,
            ..Default::default()
        };

        let monitor = Monitor::new(config).unwrap();
        let exit_code = monitor.run().await;

        assert_eq!(exit_code, 124);
        cronitor_run_mock.assert();
        cronitor_timeout_mock.assert();
    }
}
