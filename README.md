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
- MHTML templates and `.rweb` single-file sites: pages, CSS, TypeScript, and
  assets compiled into one standalone HTML document.

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
`@for`. The `rweb` compiler reads `linker.json`, renders every page, inlines
CSS, transpiles TypeScript with `tsc`, turns images and fonts into data URIs,
and writes one self-contained `.rweb` HTML file. RustWeb serves that file as an
SPA fallback: one cached document, client-side routing, no template work in the
request path.

```bash
bash site/build.sh
rustweb site/site.rweb --allow-http   # local development
# or, for real HTTPS:
bash site/dev-cert.sh
rustweb site/site.rweb --tls-cert certs/dev.crt --tls-key certs/dev.key
# open https://127.0.0.1:3000
```

`linker.json` links every page, stylesheet, script, and asset:

```json
{
  "site": { "title": "RustWeb", "version": "0.1.0" },
  "lang": "pl",
  "output": "site.rweb",
  "styles": ["assets/style.css"],
  "scripts": ["scripts/app.ts"],
  "assets": ["assets/logo.svg"],
  "pages": [
    { "route": "/", "file": "pages/index.mhtml", "title": "Start" },
    { "route": "/about", "file": "pages/about.mhtml", "title": "MHTML" }
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

The compiled `.rweb` is valid HTML. It contains a `<template>` per route, the
styles, the scripts, and a tiny router that swaps templates on navigation. It
can be opened as a static file by an SPA-capable host or served directly by
RustWeb. Dynamic APIs belong in normal RustWeb handlers; the SPA fallback only
handles unmatched routes.

Startup RSS in this repository's measurement was ~6.6 MiB for the
`.rweb` server versus ~153 MiB for the FastAPI + uvicorn process tree
(master, two workers, and resource tracker), with no requests in flight. The
server holds one HTML document and does no runtime templating. Details and
caveats are in [`bench/RESULTS.md`](bench/RESULTS.md).

Scripts can be `.js`, `.mjs`, or `.ts`; `.ts` is transpiled with `tsc` at build
time (install TypeScript or use `.js`). Styles and assets are inlined, so the
compiled page makes no external requests for its own CSS, JS, or images. Pages
can be `.mhtml` or plain `.html`; a full document is reduced to its `<body>`.

```bash
rweb inspect site/site.rweb
# RustWeb — 0.1.0, 3 routes: /, /about, /bench
```

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

## Benchmarks

```bash
bash bench/run.sh      # RustWeb vs plain axum
bash bench/fastapi.sh  # FastAPI + uvicorn comparison (creates bench/.venv)
```

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
bench-load/       small hyper-based load generator
bench/            benchmark script and results
```

## License

MIT OR Apache-2.0
