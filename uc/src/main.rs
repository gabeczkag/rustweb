use std::error::Error;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use axum::body::{Body, Bytes};
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Json, Router};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use serde_json::json;

use uc::{PacketJson, MAX_PACK_BYTES};

#[derive(Debug, Parser)]
#[command(name = "uc", version, about = "UC binary compression packs")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Pack a JSON packet list into a binary UC pack.
    Pack {
        input: PathBuf,
        output: PathBuf,
        #[arg(long, default_value_t = uc::ZSTD_LEVEL)]
        level: i32,
    },
    /// Unpack a binary UC pack into JSON.
    Unpack { input: PathBuf, output: PathBuf },
    /// Print the header and compression stats.
    Inspect { input: PathBuf },
    /// Run the local HTTP pack service.
    Serve {
        #[arg(long, default_value = "127.0.0.1:4200")]
        listen: String,
        #[arg(long, default_value_t = uc::ZSTD_LEVEL)]
        level: i32,
    },
}

#[derive(Debug, Deserialize)]
struct PackRequest {
    packets: Vec<PacketJson>,
}

#[derive(Debug, Serialize)]
struct PackResponse {
    packets: Vec<PacketJson>,
}

#[derive(Clone)]
struct ServiceState {
    level: i32,
    started: Instant,
    packs: Arc<AtomicU64>,
    packets: Arc<AtomicU64>,
    raw_bytes: Arc<AtomicU64>,
    packed_bytes: Arc<AtomicU64>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    match Cli::parse().command {
        Command::Pack {
            input,
            output,
            level,
        } => {
            let request: PackRequest = serde_json::from_slice(&fs::read(&input)?)?;
            let packets = uc::packets_from_json(&request.packets)?;
            let packed = uc::pack_with_level(&packets, level)?;
            fs::write(&output, &packed)?;
            let stats = uc::stats(&packed)?;
            println!("{}", serde_json::to_string_pretty(&stats)?);
        }
        Command::Unpack { input, output } => {
            let packets = uc::unpack(&fs::read(&input)?)?;
            let response = PackResponse {
                packets: uc::packets_to_json(&packets),
            };
            fs::write(&output, serde_json::to_vec_pretty(&response)?)?;
        }
        Command::Inspect { input } => {
            let stats = uc::stats(&fs::read(&input)?)?;
            println!("{}", serde_json::to_string_pretty(&stats)?);
        }
        Command::Serve { listen, level } => {
            serve(&listen, level).await?;
        }
    }

    Ok(())
}

async fn serve(listen: &str, level: i32) -> Result<(), Box<dyn Error>> {
    if !(1..=22).contains(&level) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "zstd level must be between 1 and 22",
        )
        .into());
    }

    let state = ServiceState {
        level,
        started: Instant::now(),
        packs: Arc::new(AtomicU64::new(0)),
        packets: Arc::new(AtomicU64::new(0)),
        raw_bytes: Arc::new(AtomicU64::new(0)),
        packed_bytes: Arc::new(AtomicU64::new(0)),
    };

    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/status", get(status))
        .route("/uc/pack", post(pack_endpoint))
        .route("/uc/unpack", post(unpack_endpoint))
        .layer(DefaultBodyLimit::max(MAX_PACK_BYTES * 2))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(listen).await?;
    println!("uc listening on http://{listen}/");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn healthz() -> Json<serde_json::Value> {
    Json(json!({ "ok": true, "service": "uc", "format": "UC01" }))
}

async fn status(State(state): State<ServiceState>) -> Json<serde_json::Value> {
    let raw = state.raw_bytes.load(Ordering::Relaxed);
    let packed = state.packed_bytes.load(Ordering::Relaxed);

    Json(json!({
        "ok": true,
        "format": "UC01",
        "zstd_level": state.level,
        "uptime_seconds": state.started.elapsed().as_secs(),
        "packs": state.packs.load(Ordering::Relaxed),
        "packets": state.packets.load(Ordering::Relaxed),
        "raw_bytes": raw,
        "packed_bytes": packed,
        "ratio": if raw == 0 { 1.0 } else { packed as f64 / raw as f64 },
    }))
}

async fn pack_endpoint(
    State(state): State<ServiceState>,
    Json(request): Json<PackRequest>,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let packets = uc::packets_from_json(&request.packets)
        .map_err(|error| fail(StatusCode::BAD_REQUEST, &error.to_string()))?;
    let packed = uc::pack_with_level(&packets, state.level)
        .map_err(|error| fail(StatusCode::BAD_REQUEST, &error.to_string()))?;
    let stats = uc::stats(&packed)
        .map_err(|error| fail(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()))?;

    state.packs.fetch_add(1, Ordering::Relaxed);
    state
        .packets
        .fetch_add(stats.packet_count as u64, Ordering::Relaxed);
    state
        .raw_bytes
        .fetch_add(stats.raw_bytes as u64, Ordering::Relaxed);
    state
        .packed_bytes
        .fetch_add(stats.packed_bytes as u64, Ordering::Relaxed);

    let mut response = Response::new(Body::from(packed));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/x-uc-pack"),
    );
    response
        .headers_mut()
        .insert("x-uc-codec", HeaderValue::from_static(stats.codec.as_str()));
    response.headers_mut().insert(
        "x-uc-raw-bytes",
        HeaderValue::from_str(&stats.raw_bytes.to_string())
            .map_err(|error| fail(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()))?,
    );
    response.headers_mut().insert(
        "x-uc-packed-bytes",
        HeaderValue::from_str(&stats.packed_bytes.to_string())
            .map_err(|error| fail(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()))?,
    );
    response.headers_mut().insert(
        "x-uc-packets",
        HeaderValue::from_str(&stats.packet_count.to_string())
            .map_err(|error| fail(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()))?,
    );
    response.headers_mut().insert(
        "x-uc-ratio",
        HeaderValue::from_str(&format!("{:.6}", stats.ratio))
            .map_err(|error| fail(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()))?,
    );

    Ok(response)
}

async fn unpack_endpoint(
    body: Bytes,
) -> Result<Json<PackResponse>, (StatusCode, Json<serde_json::Value>)> {
    let packets =
        uc::unpack(&body).map_err(|error| fail(StatusCode::BAD_REQUEST, &error.to_string()))?;

    Ok(Json(PackResponse {
        packets: uc::packets_to_json(&packets),
    }))
}

fn fail(status: StatusCode, message: &str) -> (StatusCode, Json<serde_json::Value>) {
    (status, Json(json!({ "message": message })))
}
