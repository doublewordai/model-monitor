# Model Monitor

A helm chart and rust library for **actively** monitoring API endpoints and reporting results to an exporter.

Currently supported Probes:

* [OpenAI-compatible](https://platform.openai.com/docs/api-reference/introduction) API: Chat Completion, Embedding
* [Newman](https://www.npmjs.com/package/newman): Run Postman collections

Currently supported exporters:

* [Cronitor](https://cronitor.io/)

Make an [issue](https://github.com/doublewordai/model-monitor/issues) if you have requests for additional probes or exporters.

## Overview

The repository consists of two main components:

1. **[AI Vitals](./ai-vitals/README.md)**: A Rust library that provides a CLI and programmatic interface to probe endpoints and report their status to exporters.
2. **[Helm Chart](./helm/README.md)**: A Helm chart that deploys CronJobs to periodically test the health of these endpoints.
