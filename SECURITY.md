# Security

- Never commit a CA private key, an app private key, a TCPs token, a Supabase
  secret key, or a one-time certification password.
- `~/.config/byrust/ca.key`, `.../apps/*/app.key`, `tcps.json`, `tcps-data/`,
  and `certifications.json` are intentionally outside git or ignored.
- The `rustweb` Docker image serves HTTP for a reverse proxy. Terminate TLS in
  Caddy, Traefik, or Nginx before exposing it to the internet.
- `tcps` binds to localhost in the sample config. Change its token before
  exposing the control plane.
- `uc` has no authentication. Keep it on localhost or behind a trusted proxy.
- UC compression is not encryption. Use a real AEAD layer when payloads must
  remain confidential.

Report a vulnerability privately through the repository's security advisory
flow; do not open a public issue with an exploitable proof of concept.
