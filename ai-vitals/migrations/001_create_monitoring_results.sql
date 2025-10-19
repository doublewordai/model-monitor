-- Migration: Create monitoring_results table
-- This table stores the results of endpoint monitoring probes

CREATE TABLE IF NOT EXISTS monitoring_results (
    id SERIAL PRIMARY KEY,
    timestamp TIMESTAMPTZ NOT NULL,
    monitor_name TEXT NOT NULL,
    endpoint_url TEXT NOT NULL,
    model_name TEXT NOT NULL,
    state TEXT NOT NULL,
    status_code INTEGER,
    message TEXT,
    series_id TEXT NOT NULL,
    environment TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Create indexes for common query patterns
CREATE INDEX IF NOT EXISTS idx_monitoring_results_timestamp
    ON monitoring_results(timestamp DESC);

CREATE INDEX IF NOT EXISTS idx_monitoring_results_monitor_name
    ON monitoring_results(monitor_name);

CREATE INDEX IF NOT EXISTS idx_monitoring_results_series_id
    ON monitoring_results(series_id);

CREATE INDEX IF NOT EXISTS idx_monitoring_results_state
    ON monitoring_results(state);

-- Create a composite index for common queries by monitor and time
CREATE INDEX IF NOT EXISTS idx_monitoring_results_monitor_timestamp
    ON monitoring_results(monitor_name, timestamp DESC);
