use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use rustweb::prelude::*;
use tower::ServiceExt;

#[rustweb::get("/hello")]
async fn hello() -> &'static str {
    "Hello, RustWeb!"
}

#[tokio::test]
async fn hello_route_returns_200() {
    let app = RustWeb::new("test", "0.0.0").route(hello).into_router();
    let response = app
        .oneshot(Request::builder().uri("/hello").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body[..], b"Hello, RustWeb!");
}
