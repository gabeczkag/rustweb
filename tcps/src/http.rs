use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

use axum::body::{Body, Bytes};
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::config::Config;
use crate::players::Players;
use crate::store::Store;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub store: Store,
    pub players: Players,
    pub started: Instant,
}

type ApiError = (StatusCode, Json<Value>);
type ApiResult<T> = Result<T, ApiError>;

pub fn router(state: AppState) -> Router {
    let limit = state.config.max_file_size;

    Router::new()
        .route("/healthz", get(healthz))
        .route("/status", get(status))
        .route("/streaminfo", get(streaminfo))
        .route("/manifest", get(manifest))
        .route("/sync", post(sync))
        .route(
            "/files/{sha}",
            get(get_file).put(put_file).delete(delete_file),
        )
        .route("/players", get(list_players).post(create_player))
        .layer(DefaultBodyLimit::max(limit))
        .with_state(state)
}

async fn healthz() -> Json<Value> {
    Json(json!({ "ok": true, "service": "tcps" }))
}

async fn status(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<Json<Value>> {
    authorize(&state.config, &headers)?;
    let (files, bytes) = state.store.stats();
    let players = state.players.list();

    Ok(Json(json!({
        "ok": true,
        "user_id": state.config.user_id,
        "mode": state.config.mode,
        "uptime_seconds": state.started.elapsed().as_secs(),
        "tcp_listen": state.config.tcp_listen,
        "udp_listen": state.config.udp_listen,
        "players": players.len(),
        "files": files,
        "file_bytes": bytes,
        "max_bandwidth_per_player": state.config.max_bandwidth_per_player,
    })))
}

async fn streaminfo(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<Json<Value>> {
    authorize(&state.config, &headers)?;
    let (files, bytes) = state.store.stats();

    Ok(Json(json!({
        "players": state.players.list(),
        "files": { "count": files, "bytes": bytes },
        "max_bandwidth_per_player": state.config.max_bandwidth_per_player,
    })))
}

async fn manifest(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<Json<Value>> {
    authorize(&state.config, &headers)?;
    Ok(Json(
        serde_json::to_value(state.store.manifest()).unwrap_or(Value::Array(Vec::new())),
    ))
}

#[derive(Debug, Deserialize)]
struct SyncRequest {
    #[serde(default)]
    hashes: Vec<String>,
}

async fn sync(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<SyncRequest>,
) -> ApiResult<Json<Value>> {
    authorize(&state.config, &headers)?;
    let have: HashSet<String> = request.hashes.into_iter().collect();
    let missing: Vec<_> = state
        .store
        .manifest()
        .into_iter()
        .filter(|file| !have.contains(&file.sha256))
        .collect();

    Ok(Json(json!({
        "missing": missing,
        "count": missing.len(),
    })))
}

async fn get_file(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(sha): Path<String>,
) -> ApiResult<Response> {
    authorize(&state.config, &headers)?;
    let bytes = state
        .store
        .read(&sha)
        .await
        .map_err(|error| fail(StatusCode::NOT_FOUND, &format!("FILE_NOT_FOUND: {error}")))?;

    let mut response = Body::from(bytes).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );

    if let Some(meta) = state
        .store
        .manifest()
        .into_iter()
        .find(|file| file.sha256 == sha)
    {
        if let Ok(value) = HeaderValue::from_str(&format!(
            "attachment; filename=\"{}\"",
            meta.name.replace('"', "")
        )) {
            response
                .headers_mut()
                .insert(header::CONTENT_DISPOSITION, value);
        }
    }

    Ok(response)
}

async fn put_file(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(sha): Path<String>,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    authorize(&state.config, &headers)?;
    let name = headers
        .get("x-file-name")
        .and_then(|value| value.to_str().ok())
        .unwrap_or(&sha);

    let meta = state
        .store
        .put(&sha, name, &body)
        .await
        .map_err(|error| fail(StatusCode::BAD_REQUEST, &error))?;

    Ok(Json(json!(meta)))
}

async fn delete_file(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(sha): Path<String>,
) -> ApiResult<StatusCode> {
    authorize(&state.config, &headers)?;
    let existed = state
        .store
        .delete(&sha)
        .await
        .map_err(|error| fail(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()))?;

    if existed {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(fail(StatusCode::NOT_FOUND, "FILE_NOT_FOUND"))
    }
}

async fn create_player(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    authorize(&state.config, &headers)?;
    let session = state.players.create(state.config.max_bandwidth_per_player);

    Ok(Json(json!({
        "player_id": session.player_id,
        "token": session.token,
        "max_bandwidth": session.max_bandwidth,
        "udp_addr": state.config.public_udp_addr,
        "mode": state.config.mode,
    })))
}

async fn list_players(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<Json<Value>> {
    authorize(&state.config, &headers)?;
    Ok(Json(
        serde_json::to_value(state.players.list()).unwrap_or(Value::Array(Vec::new())),
    ))
}

fn authorize(config: &Config, headers: &HeaderMap) -> ApiResult<()> {
    let supplied = headers
        .get("x-tcps-token")
        .and_then(|value| value.to_str().ok())
        .or_else(|| {
            headers
                .get(header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.strip_prefix("Bearer "))
        });

    if supplied.is_some_and(|token| token == config.token) {
        Ok(())
    } else {
        Err(fail(StatusCode::UNAUTHORIZED, "UNAUTHORIZED"))
    }
}

fn fail(status: StatusCode, message: &str) -> ApiError {
    (status, Json(json!({ "message": message })))
}
