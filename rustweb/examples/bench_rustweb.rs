use std::convert::Infallible;

use futures_util::{stream, StreamExt};
use rustweb::axum::body::{Body, Bytes};
use rustweb::axum::http::{header, HeaderValue};
use rustweb::axum::response::Response;
use rustweb::prelude::*;

const CHUNK_SIZE: usize = 64 * 1024;
const CHUNKS: usize = 1024;

#[rustweb::get("/hello")]
async fn hello() -> &'static str {
    "Hello, World!"
}

#[rustweb::get("/stream")]
async fn stream_bench() -> Response {
    stream_response()
}

fn stream_response() -> Response {
    let chunk = Bytes::from(vec![0xAB; CHUNK_SIZE]);
    let total = CHUNK_SIZE * CHUNKS;
    let body = Body::from_stream(
        stream::repeat_with(move || Ok::<Bytes, Infallible>(chunk.clone())).take(CHUNKS),
    );

    Response::builder()
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(
            header::CONTENT_LENGTH,
            HeaderValue::from_str(&total.to_string()).expect("valid content length"),
        )
        .body(body)
        .expect("valid stream response")
}

#[tokio::main]
async fn main() {
    RustWeb::new("rustweb-bench", "0.0.0")
        .route(hello)
        .route(stream_bench)
        .no_docs()
        .serve("127.0.0.1:3001")
        .await
        .expect("rustweb benchmark server failed");
}
