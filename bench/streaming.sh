#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

CONNECTIONS="${CONNECTIONS:-4}"
STREAMS="${STREAMS:-8}"
STREAM_BYTES=67108864

PIDS=()
cleanup() {
  for pid in "${PIDS[@]:-}"; do
    kill "$pid" 2>/dev/null || true
  done
}
trap cleanup EXIT

cargo build --release -p rustweb --example bench_rustweb --example bench_axum
cargo build --release -p bench-load --bin bench-stream

CLIENT=(./target/release/bench-stream)
SERVER_PREFIX=()
if command -v taskset >/dev/null 2>&1 && [ "$(nproc)" -ge 6 ]; then
  SERVER_PREFIX=(taskset -c 0-1)
  CLIENT=(taskset -c 2-5 "${CLIENT[@]}")
fi

wait_for_server() {
  local port="$1"
  for _ in $(seq 1 100); do
    if curl -sf "http://127.0.0.1:${port}/hello" >/dev/null; then
      return 0
    fi
    sleep 0.1
  done
  echo "server on port ${port} did not start" >&2
  return 1
}

run_case() {
  local name="$1"
  local binary="$2"
  local port="$3"

  "${SERVER_PREFIX[@]}" "$binary" &
  local pid=$!
  PIDS+=("$pid")
  wait_for_server "$port"

  printf '== %s ==\n' "$name"
  "${CLIENT[@]}" \
    --url "http://127.0.0.1:${port}/stream" \
    --connections "$CONNECTIONS" \
    --streams "$STREAMS" \
    --expected-bytes "$STREAM_BYTES"

  kill "$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
  sleep 1
}

echo "Streaming bandwidth: ${STREAMS} streams x ${STREAM_BYTES} bytes, ${CONNECTIONS} connections"
run_case "rustweb" ./target/release/examples/bench_rustweb 3001
run_case "plain axum" ./target/release/examples/bench_axum 3002
