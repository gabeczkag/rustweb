# RustWeb

FastAPI-style web framework for Rust, built on axum, designed for low latency.

You declare types, RustWeb gives you parsing, validation, FastAPI-shaped 422
errors, and generated OpenAPI/Swagger UI. The framework adds no middleware to
the hot path: macros expand at compile time and the OpenAPI document is built
once at startup.

## Quickstart

```toml
# Cargo.toml
[dependencies]
rustweb = { path = "../rustweb" }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
serde = { version = "1", features = ["derive"] }
utoipa = "6"
validator = { version = "0.21", features = ["derive"] }
```

```rust
use rustweb::prelude::*;

#[rustweb::model]
#[derive(Clone)]
struct CreateUser {
    #[validate(length(min = 3, max = 50))]
    name: String,
    #[validate(range(min = 0, max = 150))]
    age: u8,
}

#[rustweb::model]
#[derive(Clone)]
struct User {
    id: u64,
    name: String,
    age: u8,
}

#[rustweb::get("/users/{id}")]
async fn get_user(ApiPath(id): ApiPath<u64>) -> ApiResult<Json<User>> {
    Ok(Json(User { id, name: "Ada".into(), age: 36 }))
}

#[rustweb::post("/users", status = 201, tags = ["users"])]
async fn create_user(ApiJson(body): ApiJson<CreateUser>) -> ApiResult<Json<User>> {
    Ok(Json(User { id: 1, name: body.name, age: body.age }))
}

#[tokio::main]
async fn main() {
    RustWeb::new("RustWeb Demo", "0.1.0")
        .route(get_user)
        .route(create_user)
        .serve("127.0.0.1:3000")
        .await
        .unwrap();
}
```

Run it and open <http://127.0.0.1:3000/docs>.

```bash
curl -X POST localhost:3000/users \
  -H 'content-type: application/json' \
  -d '{"name":"Ada","age":36}'

curl localhost:3000/users/1
curl localhost:3000/openapi.json
```

Invalid input returns the FastAPI shape:

```json
{
  "detail": [
    {
      "type": "string_length",
      "loc": ["body", "name"],
      "msg": "length must be between 3 and 50",
      "ctx": { "min": 3, "max": 50, "value": "ab" }
    }
  ]
}
```

## Features

- `#[get]`, `#[post]`, `#[put]`, `#[delete]`, `#[patch]` route macros.
- `ApiPath<T>`, `ApiQuery<T>`, `ApiJson<T>` extractors with FastAPI-style 422
  errors.
- `#[model]` adds `Serialize`, `Deserialize`, `ToSchema`, and `Validate` in one
  attribute.
- Generated OpenAPI 3.1 document and vendored Swagger UI, no manual
  `#[utoipa::path]`.
- JSON 404 fallback.
- No per-request framework overhead; routes use axum's `matchit` router
  directly.
- MHTML templates and `.rweb` single-file sites: full HTML, MCSS variables,
  MScript modules, TypeScript, and assets compiled into one standalone HTML
  document.
- `/viewer`: drag-and-drop `.rweb` inspector with metadata, size, gzip, source,
  and a sandboxed preview.
- `/cert`: Supabase-backed public certification registry. Each entry seals its
  UUID and ECDSA P-256 private key with AES-256-GCM and publishes a separate
  public `app_id`, public key, fingerprint, and UUID commitment.
- Local `/certinfo/` JSON registry when the site is served by RustWeb; it uses
  the same request shape and max-2 limit as the Supabase backend.
- `tcps`: a separate per-user TCP/HTTP file sync and UDP player supervisor with
  manifests, missing-file sync, debug paths, and bandwidth limits.
- `/game`: a browser packet lab with 4096-byte packet limits, nine pre-made
  systems, channel functions, packet overrides, and a live log.
- `uc`: binary compression packs for many small packets, with zstd, CRC32,
  a 4 KiB packet limit, a 4 MiB pack limit, CLI tools, and a local HTTP service.

## How it works

Rust has no runtime reflection, so FastAPI's magic becomes procedural macros:

```rust
#[rustweb::get("/users/{id}")]
async fn get_user(ApiPath(id): ApiPath<u64>) -> ApiResult<Json<User>> { ... }
```

The macro turns the function into a unit struct implementing `rustweb::Route`.
Registration is explicit:

```rust
RustWeb::new("Demo", "0.1.0")
    .route(get_user)
    .route(create_user)
```

The wrapper sets the declared status, converts `ApiResult` errors into
`{"detail": ...}` responses, and the macro records path parameters, request
body, response schema, tags, and summary for OpenAPI.

## Differences from FastAPI

- No runtime type coercion: `"36"` is not accepted as `u8`. Serde is strict.
- Handlers become unit structs, so they are not directly callable. Put business
  logic in normal functions and keep handlers thin.
- `#[model]` expects `serde`, `utoipa`, and `validator` as direct dependencies,
  because it emits their derive macros.
- No `Depends`/per-request dependency injection and no application state in
  v0.1. State can be kept in `OnceLock`/`LazyLock` for now.
- Schema names are global; two types with the same name in different modules
  collide (use `#[schema(as = ...)]` on the utoipa derive if needed).
- OpenAPI path docs support a single `Path<T>` or a tuple `Path<(A, B)>` whose
  element count matches the template. Struct/map extractors covering multiple
  path segments are rejected at compile time.
- Operation IDs are function names; avoid duplicate handler names across
  modules if you consume the spec programmatically.
- Publishing note: the `rustweb` name is already taken on crates.io. This is a
  working name; pick a different package name before publishing.

## Demo

```bash
cargo run -p rustweb --example demo
```

Then open <http://127.0.0.1:3000/docs>.

## MHTML and .rweb

MHTML is a mini-HTML: ordinary markup plus `{{ expression }}`, `@if`, and
`@for`. The `rweb` compiler reads `linker.json`, renders every page, resolves
MCSS variables, bundles MScript modules, transpiles TypeScript with `tsc`,
minifies JavaScript with `esbuild` when it is installed, turns images and fonts
into data URIs, and writes one self-contained `.rweb` HTML file. RustWeb serves
that file as an SPA fallback: one cached document, client-side routing, no
template work in the request path.

```bash
# one command: build the site and serve it over local HTTPS
./run.sh
# PORT=3460 ./run.sh

# or manually:
bash site/build.sh
rustweb site/site.rweb
# first run creates ~/.config/byrust/ca.crt and a CA-signed app certificate
# trust the CA once:
sudo cp ~/.config/byrust/ca.crt /usr/local/share/ca-certificates/byrust.crt
sudo update-ca-certificates
# Firefox: Settings -> Certificates -> Import ~/.config/byrust/ca.crt
# local dev without TLS: --allow-http
```

If the browser says **“SSL received a record that exceeded the maximum
permissible length”**, it is sending TLS to an HTTP server. Use the exact URL
printed by `rustweb`: `https://` by default, `http://` only with
`--allow-http`. A stale server on the same port is the usual cause; the CLI now
refuses to start if the port is already taken. Stop the old process or use
`--port <PORT>`.


`linker.json` links every page, stylesheet, script, and asset:

```json
{
  "site": { "title": "RustWeb", "version": "0.1.0" },
  "app": { "name": "RustWeb", "uuid": "" },
  "lang": "pl",
  "output": "site.rweb",
  "frame": "frame.mhtml",
  "base": "/rustweb",
  "ascii": true,
  "styles": ["assets/style.mcss"],
  "scripts": [
    "scripts/app.ts",
    "scripts/viewer.mscript",
    "scripts/cert.mscript"
  ],
  "assets": ["assets/logo.svg"],
  "pages": [
    { "route": "/", "file": "pages/index.mhtml", "title": "Start" },
    { "route": "/about", "file": "pages/about.mhtml", "title": "MHTML" },
    { "route": "/bench", "file": "pages/bench.mhtml", "title": "Benchmark" },
    { "route": "/viewer", "file": "pages/viewer.mhtml", "title": "Viewer" },
    { "route": "/cert", "file": "pages/cert.mhtml", "title": "Certifications" }
  ],
  "data": { "headline": "API w Rustcie..." }
}
```

A page can use `{{ headline }}`, `{{ site.title }}`, `{{ page.route }}`,
`@if show { ... }`, and `@for item in items { ... }` with `loop.index`,
`loop.first`, and `loop.last`. Literal template markup is trusted; interpolated
values are escaped for text and attributes. To show a literal `@if` or `@for`
in prose, write `&#64;if` / `&#64;for` so the parser does not treat it as a
directive.

The compiled `.rweb` is valid HTML. It contains a shared frame, a `<template>`
per route, the styles, the scripts, and a tiny router that swaps templates on
navigation. The blocky redesign with the viewer and certification database
compiles to about **72 KiB** (~22 KB gzipped on Pages) and contains ASCII only
(`ascii: true` transliterates Polish diacritics). The earlier compact build was
19.5 KiB (21% smaller), but it had a much plainer visual layer; the zero-image
blocky UI, viewer, and certification tools trade some bytes for the features.

**MHTML syntax.** Full HTML documents are supported; a full document is reduced
to its `<body>` when embedded. Use `{{{ trusted_html }}}` for raw, unescaped
HTML, and `{{ value }}` for escaped interpolation.

**MCSS.** `.mcss` styles add `$variables`; they are resolved before CSS
minification:

```mcss
$sky: #7dd3fc;
.button { background: $sky; }
```

**MScript.** `.mscript` files are a small Node-flavoured module dialect:
relative `import`/`export` and `require()` are bundled at build time into one
classic script. It targets browser globals; Node built-ins are not emulated:

```js
import { bytes } from "./format.mscript";
export function size(value) { return bytes(value); }
```

The compiled file can be opened as a static file by an SPA-capable host or
served directly by RustWeb. Dynamic APIs belong in normal RustWeb handlers; the
SPA fallback only handles unmatched routes.

### Certification Database

`/cert` is a public registry backed by Supabase when configured. Registering
an app generates an ECDSA P-256 keypair and a 128-character one-time password
(shown once, never stored). The UUID, private key, and a marker are sealed with
AES-256-GCM. The public entry contains `app_id` (a separate public UUID),
`uuid_commitment`, `encrypted_key`, `public_key`, `key_fingerprint`, salt, IV,
iterations, and the encrypted payload. The private UUID stays inside the
ciphertext.

The browser calls two Postgres RPCs with the publishable key:

- `POST /rest/v1/rpc/get_certinfo` returns the public list.
- `POST /rest/v1/rpc/register_certification` stores one entry. Body:
  `{"payload": entry, "hwid": "...", "fingerprint": "..."}`.

The RPC hashes IP (from `x-forwarded-for`), HWID, and the device fingerprint,
and allows at most **2 applications per identity, HWID, fingerprint, and IP**.
Raw identifiers are never stored. Direct table writes are blocked; `anon` can
only execute the two security-definer functions. The limit is best-effort:
VPN/proxy changes IP, and browser HWID/fingerprint values can be spoofed.

The checker decrypts the payload and verifies the marker (`POPRAWNE` or
`as10sign edi5nbaseApro3ved`), the UUID commitment, key fingerprint, and ECDSA
signature. A successful check prints `POPRAWNE`, the UUID, and the fingerprint.
The decrypted private key is used only for that signature check and is not
returned to the page or persisted. A wrong password or tampered entry fails
without revealing the UUID. The bundled `RustWeb Site` seed is public; its
one-time password is not in this repository.

#### Supabase setup

1. Open the Supabase SQL Editor for the project and run
   [`supabase/schema.sql`](supabase/schema.sql).
2. Put the project URL and **publishable** key in `site/linker.json` under
   `data.registry_url` / `data.registry_key`.
3. Never put `sb_secret_...` in the browser, `site.rweb`, or git. The secret key
   is not needed for registration; rotate it if it was exposed.
4. Run `bash site/build.sh` and deploy.

Without Supabase config, or when the RPC is unavailable, `/cert` falls back to
`localStorage`; those entries are not public. An exact `/certinfo/` path on
GitHub Pages is not available because Pages serves only static files. The
Supabase RPC URL is the public endpoint; a RustWeb/Worker proxy is required for
an exact `/certinfo/` path on your own domain.

### Local `/certinfo/`

When RustWeb serves the page, it exposes a local JSON registry:

- `GET /certinfo/` returns the public list.
- `GET /certinfo/{app_id}` returns one entry.
- `POST /certinfo/` accepts `{"payload": entry, "hwid": "...",
  "fingerprint": "..."}` and enforces the same max-2 limit.

The local store is `certifications.json` (ignored by git) and is seeded from
the compiled `.rweb` on first run. It is local-only; GitHub Pages has no
writable endpoint and uses Supabase instead. `./run.sh` starts both the site
and the local API.

```bash
node site/scripts/cert-crypto.test.mjs
# CERT CRYPTO OK <uuid> <marker> <fingerprint prefix>
```

Startup RSS in this repository's measurement was ~6.6 MiB for the
`.rweb` server versus ~153 MiB for the FastAPI + uvicorn process tree
(master, two workers, and resource tracker), with no requests in flight. The
server holds one HTML document and does no runtime templating. Details and
caveats are in [`bench/RESULTS.md`](bench/RESULTS.md).

Scripts can be `.js`, `.mjs`, `.ts`, or `.mscript`; `.ts` is transpiled with
`tsc` and `.mscript` modules are bundled by `rweb`. Styles can be `.css` or
`.mcss`; assets are inlined, so the compiled page makes no external requests
for its own CSS, JS, or images. Pages can be `.mhtml` or plain `.html`; a full
document is reduced to its `<body>`.

```bash
rweb inspect site/site.rweb
# RustWeb — 0.1.0, 6 routes: /, /about, /bench, /viewer, /cert, /game
```

### Game Lab

`/game` is a browser test bench for the packet/channel model. It has a small
canvas arena, WASD movement, left/right clicks, chat, packet logs, and a live
registry of nine systems:

```text
/game_chat  /emotes  /player_packets  /game_state
/inventory  /trade   /file_sync       /leaderboard
/metrics
```

The default packet set includes `move`, `look`, `left_click`, `right_click`,
`chat`, `ping`, `snapshot`, `sync`, `inventory`, `score`, `emote`, `trade`,
`respawn`, and `metrics`. Every packet is limited to **4096 bytes** at encode
and decode time. A channel is a function; a system can import the default
packets and override one by defining a function named after the packet inside
its channel function.

The game is a local browser simulation. TCPs remains a separate service; this
page tests the packet/channel logic without requiring the UDP server.

### UC compression packs

`uc` packs many small packets into one binary `UC01` blob, optionally
compressing the whole pack with zstd. The decoder verifies lengths and a CRC32,
then returns the original packets. Per-packet limit is **4096 B**; per-pack
limit is **4 MiB**.

```bash
cargo run -p uc -- pack packets.json packets.uc --level 9
cargo run -p uc -- inspect packets.uc
cargo run -p uc -- unpack packets.uc packets.out.json

cargo run -p uc -- serve --listen 127.0.0.1:4200
```

The service exposes `/status`, `/uc/pack`, and `/uc/unpack`. This is framing and
compression, not encryption; a human sees binary, but a decoder can read it.

### Byrust protocol

`linker.json` can declare a protocol profile:

```json
"protocol": {
  "name": "byrust",
  "version": 1,
  "https": true,
  "alias": "byrust",
  "headers": { "X-Byrust-Site": "rustweb" }
}
```

`rweb` embeds the profile in the `.rweb` as `<meta name="byrust">` and a
`byrust-config` JSON block. `rustweb` reads it, refuses plain HTTP when
`https` is true (`--allow-http` is a dev-only override), adds `X-Byrust-*` and
HSTS headers over TLS, and serves a real HTTPS endpoint. The SPA router maps
`byrust://self/path` and `web+byrust://...` links to the current origin, and
can register `web+byrust` as a browser protocol handler.

The browser still sees `https://`: only TLS with a trusted certificate creates
a secure context. `byrust://` is an in-app alias and protocol profile, not a
replacement for TLS. `--allow-http` exists for local development and does not
make the browser treat the page as HTTPS.

### App identity and certificates

When a protocol profile is present, `linker.json` must also define an app:

```json
"app": { "name": "RustWeb", "uuid": "" }
```

- A blank `uuid` generates a stable UUID v5 from the app name and project path.
- An explicit UUID is used as-is and treated as app-specific.
- `"uuid": "universal"` (or the all-zero UUID) is the low-trust Universal UUID.

`rustweb` creates a **Byrust Local CA** at `~/.config/byrust/ca.crt` and issues
a CA-signed leaf certificate per app/host under `~/.config/byrust/apps/`. There
are no per-app self-signed certificates; the leaf is signed by the CA. Trust
the CA once (system and/or Firefox) and all app certificates become valid.

Universal UUIDs use a separate, deliberately untrusted Universal CA. The
browser warns, and the compiled page includes a sticky **“Page is not safe”**
banner. The UUID alone does not create TLS trust — the CA does. For production,
pass a real certificate with `--tls-cert` and `--tls-key` (for example from
Let’s Encrypt).

## Deployment

[`.github/workflows/pages.yml`](.github/workflows/pages.yml) builds `site.rweb`
with the Rust `rweb` compiler and deploys it to GitHub Pages. The live URL is
<https://gabeczkag.github.io/rustweb/> (after the Pages workflow completes).

GitHub Pages serves the static `.rweb` over GitHub's TLS certificate. No Byrust
CA, leaf certificate, or private key is uploaded — the local `~/.config/byrust`
directory and the `certs/` folder are outside the repository. GitHub Actions is
CI, not a long-running host: it builds and deploys the single-file site. To run
the Rust server itself, use `rustweb site/site.rweb` on a VPS/container.

## TCPs

`tcps` is a separate service, not part of the `.rweb` site. It keeps a
per-user content-addressed file store and a UDP player session table:

```bash
cp tcps/tcps.json tcps.json   # change token first
cargo run -p tcps -- --config tcps.json
```

The HTTP control plane exposes `/status`, `/streaminfo`, `/manifest`, `/sync`,
`/files/{sha256}`, and `/players`. The UDP plane accepts `hello`, `ping`,
`state`, and `bye` datagrams and enforces the per-player bandwidth limit.
Details and examples are in [`tcps/README.md`](tcps/README.md).

## Benchmarks

```bash
bash bench/run.sh        # request throughput and latency
bash bench/streaming.sh  # streaming bandwidth in MiB/s, MB/s, and Gbit/s
bash bench/fastapi.sh    # FastAPI + uvicorn comparison (creates bench/.venv)
```

The streaming benchmark downloads a 64 MiB body per stream in 64 KiB
chunks, with 8 streams over 4 HTTP/1.1 connections by default. The client
counts body frames without buffering the whole response, so the number reflects
transfer bandwidth rather than allocation speed. Override the concurrency with `CONNECTIONS` and `STREAMS`.

The scripts start the servers and run a keep-alive load generator that reports
throughput and p50/p90/p99/p99.9/max latency. Results are written to
[`bench/RESULTS.md`](bench/RESULTS.md).

Measured on an AMD Ryzen 5 4500U (6 cores), server pinned to 2 cores, client to
4 cores, 64 keep-alive connections, 200k requests. Five runs, 0.97–1.02× the
throughput of plain axum (no consistent direction):

| Server | Throughput | p50 | p99 |
|---|---:|---:|---:|
| RustWeb | 117–211k req/s | 258–441 µs | 0.70–1.98 ms |
| plain axum | 120–208k req/s | 258–449 µs | 0.74–1.94 ms |

The range comes from machine load, not from the framework: the same code
measured 117k req/s with a browser and editor running and 211k on a quiet
machine. The comparative result is what matters: for a route without
validation, RustWeb stays within a few percent of plain axum in either
direction. These are loopback numbers on one machine, not a TechEmpower claim.
Validation cost depends on the model and payload and is not included in this
table.

In an adjacent run on the same machine, FastAPI 0.162 + uvicorn 0.54 (two
workers, same two pinned cores) served the same route at 13.8–14.3k req/s with
p99 4.5–5.0 ms, versus 202–211k req/s and p99 0.70–0.72 ms for RustWeb. That is
roughly 14–15× throughput and 6–7× lower p99 in this setup — a runtime
comparison, not just an API-shape comparison. The detailed runs are in
[`bench/RESULTS.md`](bench/RESULTS.md).

## Project layout

```
rustweb/          core framework: builder, extractors, errors, OpenAPI
rustweb-macros/   route/model/query procedural macros
mhtml/            MHTML parser, page compiler, .rweb bundler, rweb CLI
site/             mini site sources and the built site/site.rweb
supabase/         SQL schema and RPCs for the public certification registry
tcps/             per-user TCP file sync + UDP player sessions (separate service)
uc/               binary compression packs for small packets
bench-load/       small hyper-based load generator
bench/            benchmark script and results
```

## License

MIT OR Apache-2.0
