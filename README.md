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

## Demo

```bash
cargo run -p rustweb --example demo
```

Then open <http://127.0.0.1:3000/docs>.

## Benchmarks

```bash
bash bench/run.sh
```

The script starts two servers — one on RustWeb, one on plain axum — and runs a
keep-alive load generator that reports throughput and p50/p90/p99/p99.9/max
latency. Results are written to [`bench/RESULTS.md`](bench/RESULTS.md).

Measured on an AMD Ryzen 5 4500U (6 cores), server pinned to 2 cores, client to
4 cores, 64 keep-alive connections, 200k requests:

| Server | Throughput | p50 | p99 |
|---|---:|---:|---:|
| RustWeb | 181–183k req/s | 295 µs | 742–781 µs |
| plain axum | 179k req/s | 295–298 µs | 786–862 µs |

The difference is within run-to-run noise: for a route without validation, the
framework adds no measurable request-path overhead. These are loopback numbers
on one machine, not a TechEmpower claim. Validation cost depends on the model
and payload and is not included in this table.

## Project layout

```
rustweb/          core framework: builder, extractors, errors, OpenAPI
rustweb-macros/   route/model/query procedural macros
bench-load/       small hyper-based load generator
bench/            benchmark script and results
```

## License

MIT OR Apache-2.0
