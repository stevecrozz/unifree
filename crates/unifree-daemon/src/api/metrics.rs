use std::sync::Arc;

use axum::{
    body::Body,
    extract::State,
    http::{header, StatusCode},
    response::Response,
    Json,
};

use crate::SharedState;

pub async fn metrics_handler() -> Response {
    let body = crate::telemetry::TelemetryStore::gather_prometheus();
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/plain; version=0.0.4")
        .body(Body::from(body))
        .unwrap()
}

pub async fn telemetry_json(State(shared): State<Arc<SharedState>>) -> Json<serde_json::Value> {
    let snapshot = shared.telemetry.export_json().await;
    Json(snapshot)
}
