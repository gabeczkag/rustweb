#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

VENV="${FASTAPI_VENV:-bench/.venv}"
if [ ! -x "$VENV/bin/uvicorn" ]; then
  python3 -m venv "$VENV"
  "$VENV/bin/pip" install -q fastapi "uvicorn[standard]"
fi

cargo build --release -p bench-load

SERVER=()
CLIENT=()
if command -v taskset >/dev/null 2>&1 && [ "$(nproc)" -ge 6 ]; then
  SERVER=(taskset -c 0-1)
  CLIENT=(taskset -c 2-5)
fi

"${SERVER[@]}" "$VENV/bin/uvicorn" --app-dir bench fastapi_app:app \
  --host 127.0.0.1 \
  --port 3003 \
  --workers 2 \
  --log-level warning &
PID=$!

cleanup() {
  kill "$PID" 2>/dev/null || true
  pkill -P "$PID" 2>/dev/null || true
}
trap cleanup EXIT

for _ in $(seq 1 200); do
  if curl -sf http://127.0.0.1:3003/hello >/dev/null; then
    break
  fi
  sleep 0.1
done

"${CLIENT[@]}" ./target/release/bench-load \
  --url http://127.0.0.1:3003/hello \
  --connections 64 \
  --requests 200000 \
  --warmup 10000
