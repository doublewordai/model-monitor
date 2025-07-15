# Model Monitor

A Helm chart for monitoring OpenAI-compatible API endpoints using Cronitor.

## Overview

This chart deploys CronJobs that periodically test your OpenAI-compatible endpoints and report the results to Cronitor. It supports both chat and embedding models.

## Installation

1. Add your endpoints to `values.yaml` or use `--set` flags
2. Create a secret with your Cronitor URL or set it directly in values
3. Install the chart:

```bash
helm install model-monitor .
```

## Configuration

### Endpoints

Configure your endpoints in `values.yaml`:

```yaml
endpoints:
  - name: "my-service"
    url: "http://my-service"
    models: 
      - name: "embed"
        type: "embedding"
        monitor: "my-embedding-model"  # Optional: cronitor monitor name
      - name: "generate"
        type: "chat"
        monitor: "my-chat-model"  # Optional: cronitor monitor name
```

### Telemetry Configuration

You can configure the telemetry URL in two ways:

#### Option 1: Using a Kubernetes Secret (Recommended)

Create a secret with your Cronitor URL:

```bash
kubectl create secret generic cronitor-secret \
  --from-literal=cronitor-url="https://cronitor.link/p/your-key/your-group"
```

Then use the default configuration in `values.yaml`:

```yaml
telemetry:
  url:
    valueFrom:
      secretKeyRef:
        name: "cronitor-secret"
        key: "cronitor-url"
```

#### Option 2: Direct Configuration

Set the telemetry URL directly in `values.yaml`:

```yaml
telemetry:
  url:
    value: "https://cronitor.link/p/your-key/your-group"
```

### Model Types

- **chat**: Sends a test message and expects a chat completion response
- **embedding**: Sends a test string and expects an embedding response

### Schedule

The default schedule runs every 5 minutes. Customize in `values.yaml`:

```yaml
cronJob:
  schedule: "*/5 * * * *"
```

## Values

| Parameter | Description | Default |
|-----------|-------------|---------|
| `endpoints` | List of OpenAI-compatible endpoints to monitor | `[]` |
| `cronJob.schedule` | Cron schedule for monitoring jobs | `"*/5 * * * *"` |
| `cronJob.image.repository` | Container image repository | `"curlimages/curl"` |
| `cronJob.image.tag` | Container image tag | `"latest"` |
| `cronJob.resources` | Resource limits and requests | `{}` |
| `telemetry.url` | Telemetry URL configuration (value or valueFrom) | `valueFrom.secretKeyRef` |

## Monitoring

Each endpoint/model combination creates a separate CronJob that:

1. Makes a test request to the endpoint
2. Measures response time
3. Reports success/failure to Cronitor with duration
4. Logs the result

Monitor names default to `{endpoint-name}-{model-name}` but can be customized using the `monitor` field.