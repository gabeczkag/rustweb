use std::convert::Infallible;

use axum::body::{Body, Bytes};
use axum::http::{header, HeaderValue};
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use futures_util::{stream, StreamExt};

const CHUNK_SIZE: usize = 64 * 1024;
const CHUNKS: usize = 1024;

async fn hello() -> &'static str {
    "Hello, World!"
}

async fn stream_bench() -> Response {
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
    let app = Router::new()
        .route("/hello", get(hello))
        .route("/stream", get(stream_bench));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3002")
        .await
        .expect("bind axum benchmark server");
    axum::serve(listener, app)
        .await
        .expect("axum benchmark server failed");
}
