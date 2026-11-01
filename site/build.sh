#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
cargo run -p rtml --bin rpack -- build site -o site/site.rpack
