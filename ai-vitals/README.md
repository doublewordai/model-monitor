# AI Vitals

AI Vitals is a tool designed to monitor LLM (Large Language Model) endpoints and report their status to Cronitor, a service for monitoring and alerting on the health of applications. It actively monitors OpenAI-compatible API endpoints, ensuring they are functioning correctly and reporting any issues to Cronitor for further triage.

## Usage

You can use the library interface or the command-line interface (CLI) to monitor your endpoints. The CLI is particularly useful for quick checks or running in automated scripts. We have an example [helm chart](https://github.com/doublewordai/model-monitor/tree/main/helm/README.md) that uses the CLI to monitor endpoints with kubernetes CronJobs.

```rust
use ai_vitals::{Config, Monitor};
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::parse(); // Load configuration from environment or command line
    let monitor = Monitor::new(config)?;
    let exit_code = monitor.run().await;
    std::process::exit(exit_code);
}
```

You can also run the CLI directly:

```bash
cargo run --bin ai-vitals -- --cronitor-base-url "https://cronitor.link/p/your-key/your-group" \
  --server-url "http://my-service" \
  --model-name "embed" \
  --endpoint-type "embedding" \
  --monitor-name "my-embedding-model"
```
