# TCPs

Per-user file sync and UDP player supervisor. TCPs is separate from the game
system: the game asks TCPs for missing files, and TCPs keeps the per-user
manifest and live player sessions.

## Run

```bash
cp tcps/tcps.json tcps.json
# edit token, addresses, and limits
cargo run -p tcps -- --config tcps.json
```

The configuration in this directory uses `change-me`; change it before exposing
the control port.

## HTTP control plane

All endpoints except `/healthz` require:

```text
X-TCPS-Token: <token>
```

or `Authorization: Bearer <token>`.

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/healthz` | liveness |
| `GET` | `/status` | uptime, mode, file counts, player count |
| `GET` | `/streaminfo` | player byte counters, bandwidth, file stats |
| `GET` | `/manifest` | all files known to TCPs |
| `POST` | `/sync` | body `{"hashes":["..."]}`, returns missing files |
| `GET` | `/files/{sha256}` | download a file |
| `PUT` | `/files/{sha256}` | upload; send `X-File-Name`; body is verified |
| `DELETE` | `/files/{sha256}` | remove a file |
| `GET` | `/players` | live/known player sessions |
| `POST` | `/players` | create a UDP player session |

Example:

```bash
TOKEN='change-me'
curl -H "X-TCPS-Token: $TOKEN" http://127.0.0.1:4100/status
curl -H "X-TCPS-Token: $TOKEN" http://127.0.0.1:4100/manifest
curl -H "X-TCPS-Token: $TOKEN" -H 'content-type: application/json' \
  -d '{"hashes":[]}' http://127.0.0.1:4100/sync
```

Files are stored under `tcps-data/<user_id>/files/<sha256>`. Uploads are
content-addressed and rejected when the SHA-256 does not match the body.

## UDP player plane

`POST /players` returns a `player_id`, a per-player `token`, the UDP address,
and `max_bandwidth` in bytes per second.

Datagrams are JSON:

```json
{"op":"hello","player_id":"...","token":"..."}
{"op":"ping","player_id":"...","token":"..."}
{"op":"state","player_id":"...","token":"...","seq":1}
{"op":"bye","player_id":"...","token":"..."}
```

`state` datagrams are charged against the per-player token bucket. A player
over the limit gets `{"op":"limited","error":"RATE_LIMITED"}` and the datagram
is dropped. `/streaminfo` exposes accepted bytes, dropped datagrams, and the
configured limit.

## Note on security

The control token is a shared secret. Bind TCPs to localhost or a private
network, and use TLS in front of it if it crosses an untrusted network. The
per-player UDP token is a session identifier, not a replacement for game-level
authentication.
