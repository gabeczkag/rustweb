#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
mkdir -p certs

if [ -f certs/dev.crt ] && [ -f certs/dev.key ]; then
  echo "certs/dev.crt and certs/dev.key already exist"
  exit 0
fi

openssl req -x509 -newkey rsa:2048 -nodes \
  -keyout certs/dev.key \
  -out certs/dev.crt \
  -days 365 \
  -subj "/CN=localhost" \
  -addext "subjectAltName=DNS:localhost,IP:127.0.0.1" >/dev/null 2>&1

echo "wrote certs/dev.crt and certs/dev.key"
echo "the browser will warn until you trust the certificate (mkcert is easier)"
