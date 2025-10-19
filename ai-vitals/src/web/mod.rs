use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{Html, Json},
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, FromRow};
use std::net::SocketAddr;
use tracing::{info, error};

#[derive(Clone)]
pub struct AppState {
    pool: PgPool,
}

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct MonitoringResult {
    id: i32,
    timestamp: chrono::DateTime<chrono::Utc>,
    monitor_name: String,
    endpoint_url: String,
    model_name: String,
    state: String,
    status_code: Option<i32>,
    message: Option<String>,
    series_id: String,
    environment: String,
    duration_ms: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct ResultsQuery {
    monitor_name: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Serialize, FromRow)]
pub struct Stats {
    total_probes: i64,
    successful_probes: i64,
    failed_probes: i64,
    uptime_pct: f64,
}

async fn index() -> Html<&'static str> {
    Html(include_str!("ui.html"))
}

async fn get_results(
    State(state): State<AppState>,
    Query(params): Query<ResultsQuery>,
) -> Result<Json<Vec<MonitoringResult>>, (StatusCode, String)> {
    let limit = params.limit.unwrap_or(100).min(1000);
    let offset = params.offset.unwrap_or(0);

    // Get the latest event for each series_id (which represents the final state of each probe)
    // Also calculate duration from run event to final event
    let results = if let Some(monitor_name) = params.monitor_name {
        sqlx::query_as::<_, MonitoringResult>(
            r#"
            WITH latest_events AS (
                SELECT DISTINCT ON (series_id)
                    id, timestamp, monitor_name, endpoint_url, model_name,
                    state, status_code, message, series_id, environment
                FROM monitoring_results
                WHERE monitor_name = $1
                ORDER BY series_id, timestamp DESC
            ),
            probe_durations AS (
                SELECT
                    le.id,
                    le.timestamp,
                    le.monitor_name,
                    le.endpoint_url,
                    le.model_name,
                    le.state,
                    le.status_code,
                    le.message,
                    le.series_id,
                    le.environment,
                    CAST(EXTRACT(EPOCH FROM (le.timestamp - run_event.timestamp)) * 1000 AS BIGINT) AS duration_ms
                FROM latest_events le
                LEFT JOIN LATERAL (
                    SELECT timestamp
                    FROM monitoring_results
                    WHERE series_id = le.series_id AND state = 'run'
                    ORDER BY timestamp ASC
                    LIMIT 1
                ) run_event ON true
            )
            SELECT id, timestamp, monitor_name, endpoint_url, model_name, state, status_code, message, series_id, environment, duration_ms FROM probe_durations
            ORDER BY timestamp DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(monitor_name)
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.pool)
        .await
    } else {
        sqlx::query_as::<_, MonitoringResult>(
            r#"
            WITH latest_events AS (
                SELECT DISTINCT ON (series_id)
                    id, timestamp, monitor_name, endpoint_url, model_name,
                    state, status_code, message, series_id, environment
                FROM monitoring_results
                ORDER BY series_id, timestamp DESC
            ),
            probe_durations AS (
                SELECT
                    le.id,
                    le.timestamp,
                    le.monitor_name,
                    le.endpoint_url,
                    le.model_name,
                    le.state,
                    le.status_code,
                    le.message,
                    le.series_id,
                    le.environment,
                    CAST(EXTRACT(EPOCH FROM (le.timestamp - run_event.timestamp)) * 1000 AS BIGINT) AS duration_ms
                FROM latest_events le
                LEFT JOIN LATERAL (
                    SELECT timestamp
                    FROM monitoring_results
                    WHERE series_id = le.series_id AND state = 'run'
                    ORDER BY timestamp ASC
                    LIMIT 1
                ) run_event ON true
            )
            SELECT id, timestamp, monitor_name, endpoint_url, model_name, state, status_code, message, series_id, environment, duration_ms FROM probe_durations
            ORDER BY timestamp DESC
            LIMIT $1 OFFSET $2
            "#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.pool)
        .await
    };

    match results {
        Ok(results) => Ok(Json(results)),
        Err(e) => {
            error!("Database error: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Database error: {}", e),
            ))
        }
    }
}

async fn get_stats(
    State(state): State<AppState>,
    Query(params): Query<ResultsQuery>,
) -> Result<Json<Stats>, (StatusCode, String)> {
    // Count unique probes (series_id) instead of individual events
    let stats = if let Some(monitor_name) = params.monitor_name {
        sqlx::query_as::<_, Stats>(
            r#"
            WITH latest_probe_states AS (
                SELECT DISTINCT ON (series_id)
                    series_id, state
                FROM monitoring_results
                WHERE monitor_name = $1
                    AND timestamp > NOW() - INTERVAL '24 hours'
                ORDER BY series_id, timestamp DESC
            )
            SELECT
                COUNT(*) as total_probes,
                COUNT(*) FILTER (WHERE state = 'complete') as successful_probes,
                COUNT(*) FILTER (WHERE state = 'fail') as failed_probes,
                CAST(CASE
                    WHEN COUNT(*) > 0 THEN
                        COUNT(*) FILTER (WHERE state = 'complete') * 100.0 / COUNT(*)
                    ELSE 0.0
                END AS DOUBLE PRECISION) as uptime_pct
            FROM latest_probe_states
            "#,
        )
        .bind(monitor_name)
        .fetch_one(&state.pool)
        .await
    } else {
        sqlx::query_as::<_, Stats>(
            r#"
            WITH latest_probe_states AS (
                SELECT DISTINCT ON (series_id)
                    series_id, state
                FROM monitoring_results
                WHERE timestamp > NOW() - INTERVAL '24 hours'
                ORDER BY series_id, timestamp DESC
            )
            SELECT
                COUNT(*) as total_probes,
                COUNT(*) FILTER (WHERE state = 'complete') as successful_probes,
                COUNT(*) FILTER (WHERE state = 'fail') as failed_probes,
                CAST(CASE
                    WHEN COUNT(*) > 0 THEN
                        COUNT(*) FILTER (WHERE state = 'complete') * 100.0 / COUNT(*)
                    ELSE 0.0
                END AS DOUBLE PRECISION) as uptime_pct
            FROM latest_probe_states
            "#,
        )
        .fetch_one(&state.pool)
        .await
    };

    match stats {
        Ok(stats) => Ok(Json(stats)),
        Err(e) => {
            error!("Database error: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Database error: {}", e),
            ))
        }
    }
}

async fn get_monitors(
    State(state): State<AppState>,
) -> Result<Json<Vec<String>>, (StatusCode, String)> {
    let monitors = sqlx::query_scalar::<_, String>(
        r#"
        SELECT DISTINCT monitor_name
        FROM monitoring_results
        ORDER BY monitor_name
        "#
    )
    .fetch_all(&state.pool)
    .await;

    match monitors {
        Ok(monitors) => Ok(Json(monitors)),
        Err(e) => {
            error!("Database error: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Database error: {}", e),
            ))
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ProbeDetailsQuery {
    series_id: String,
}

async fn get_probe_details(
    State(state): State<AppState>,
    Query(params): Query<ProbeDetailsQuery>,
) -> Result<Json<Vec<MonitoringResult>>, (StatusCode, String)> {
    let results = sqlx::query_as::<_, MonitoringResult>(
        r#"
        SELECT id, timestamp, monitor_name, endpoint_url, model_name,
               state, status_code, message, series_id, environment,
               NULL::BIGINT as duration_ms
        FROM monitoring_results
        WHERE series_id = $1
        ORDER BY timestamp ASC
        "#,
    )
    .bind(&params.series_id)
    .fetch_all(&state.pool)
    .await;

    match results {
        Ok(results) => Ok(Json(results)),
        Err(e) => {
            error!("Database error: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Database error: {}", e),
            ))
        }
    }
}

async fn run_migrations(pool: &PgPool) -> anyhow::Result<()> {
    info!("Running database migrations...");

    let migration_sql = include_str!("../../migrations/001_create_monitoring_results.sql");

    sqlx::raw_sql(migration_sql).execute(pool).await?;
    info!("Database migrations completed successfully");
    Ok(())
}

pub async fn run_server(database_url: String, port: u16) -> anyhow::Result<()> {
    info!("Connecting to database...");
    let pool = PgPool::connect(&database_url).await?;
    info!("Database connection established");

    // Run migrations
    run_migrations(&pool).await?;

    let state = AppState { pool };

    let app = Router::new()
        .route("/", get(index))
        .route("/api/results", get(get_results))
        .route("/api/stats", get(get_stats))
        .route("/api/monitors", get(get_monitors))
        .route("/api/probe-details", get(get_probe_details))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("Starting web server on {}", addr);
    info!("Open http://localhost:{} in your browser", port);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
