use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use serde_json::{json, Value};
use tokio::net::UdpSocket;

use crate::config::Config;
use crate::players::{PlayerError, Players};

#[derive(Debug, Deserialize)]
struct Datagram {
    op: String,
    player_id: String,
    token: String,
}

pub async fn run(socket: UdpSocket, players: Players, config: Arc<Config>) {
    let mut buffer = vec![0u8; 65_535];

    loop {
        let (length, address) = match socket.recv_from(&mut buffer).await {
            Ok(value) => value,
            Err(error) => {
                eprintln!("tcps udp receive failed: {error}");
                continue;
            }
        };

        let response = handle(
            &players,
            &config,
            &buffer[..length],
            address,
            Instant::now(),
        );

        if let Some(player_id) = response.get("player_id").and_then(Value::as_str) {
            if let Ok(encoded) = serde_json::to_vec(&response) {
                players.add_out(player_id, encoded.len() as u64);
            }
        }

        let encoded = serde_json::to_vec(&response)
            .unwrap_or_else(|_| br#"{"op":"error","error":"SERIALIZE_FAILED"}"#.to_vec());

        if let Err(error) = socket.send_to(&encoded, address).await {
            eprintln!("tcps udp send failed: {error}");
        }
    }
}

pub async fn cleanup(players: Players, timeout: Duration) {
    loop {
        tokio::time::sleep(Duration::from_secs(5)).await;
        players.cleanup(timeout);
    }
}

fn handle(
    players: &Players,
    config: &Config,
    buffer: &[u8],
    address: SocketAddr,
    now: Instant,
) -> Value {
    let message: Datagram = match serde_json::from_slice(buffer) {
        Ok(message) => message,
        Err(error) => {
            return json!({ "op": "error", "error": format!("INVALID_JSON: {error}") });
        }
    };

    match message.op.as_str() {
        "hello" | "ping" => match players.touch(&message.player_id, &message.token, address, now) {
            Ok(view) => json!({
                "op": if message.op == "hello" { "welcome" } else { "pong" },
                "player_id": view.player_id,
                "addr": view.addr,
                "max_bandwidth": view.max_bandwidth,
                "udp_addr": config.public_udp_addr,
                "mode": config.mode,
                "server_time": epoch(),
            }),
            Err(error) => player_error(error),
        },
        "state" => match players.consume(
            &message.player_id,
            &message.token,
            address,
            buffer.len() as u64,
            now,
        ) {
            Ok(view) => json!({
                "op": "ack",
                "player_id": view.player_id,
                "bytes_in": view.bytes_in,
                "dropped": view.dropped,
            }),
            Err(PlayerError::Limited) => json!({
                "op": "limited",
                "player_id": message.player_id,
                "error": "RATE_LIMITED",
            }),
            Err(error) => player_error(error),
        },
        "bye" => match players.remove(&message.player_id, &message.token) {
            Ok(()) => json!({ "op": "bye", "player_id": message.player_id }),
            Err(error) => player_error(error),
        },
        other => json!({ "op": "error", "error": format!("UNKNOWN_OP: {other}") }),
    }
}

fn player_error(error: PlayerError) -> Value {
    match error {
        PlayerError::NotFound => json!({ "op": "error", "error": "UNKNOWN_PLAYER" }),
        PlayerError::Unauthorized => json!({ "op": "error", "error": "UNAUTHORIZED" }),
        PlayerError::Limited => json!({ "op": "error", "error": "RATE_LIMITED" }),
    }
}

fn epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}
