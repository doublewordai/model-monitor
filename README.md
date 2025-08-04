# Model Monitor

A helm chart and rust library for **actively** monitoring [OpenAI-compatible API](https://platform.openai.com/docs/api-reference/introduction) endpoints using [Cronitor](https://cronitor.io).

## Overview

The repository consists of two main components:

1. **[AI Vitals](./ai-vitals/README.md)**: A Rust library that provides a CLI and programmatic interface to monitor LLM endpoints and report their status to Cronitor.
2. **[Helm Chart](./helm/README.md)**: A Helm chart that deploys CronJobs to periodically test the health of these endpoints.
