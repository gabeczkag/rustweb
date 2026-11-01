#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

PORT="${PORT:-3000}"
CA="${HOME}/.config/byrust/ca.crt"

bash site/build.sh

echo
echo "Lokalny URL: https://127.0.0.1:${PORT}/"
echo "Byrust CA:  ${CA}"
echo
echo "Jesli przegladarka nie ufa certyfikatowi, zaimportuj CA raz:"
echo "  sudo cp \"${CA}\" /usr/local/share/ca-certificates/byrust.crt && sudo update-ca-certificates"
echo
echo "Podglad certyfikatu lokalnego:"
echo "  echo | openssl s_client -connect 127.0.0.1:${PORT} -servername 127.0.0.1 2>/dev/null | openssl x509 -noout -subject -issuer -dates"
echo
echo "Certyfikat GitHub Pages:"
echo "  echo | openssl s_client -connect gabeczkag.github.io:443 -servername gabeczkag.github.io 2>/dev/null | openssl x509 -noout -subject -issuer -dates"
echo

exec cargo run -p rustweb --bin rustweb -- site/site.rweb --port "${PORT}" "$@"
