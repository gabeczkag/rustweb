use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use rustweb::prelude::*;
use tower::ServiceExt;

#[rustweb::get("/api/health")]
async fn health() -> Json<&'static str> {
    Json("ok")
}

fn spa_html() -> &'static str {
    r#"<!doctype html>
<html lang="pl">
<head><title>Test</title></head>
<body>
<div id="rweb-app"></div>
<template data-rweb-route="/" data-rweb-title="Home">HOME</template>
<template data-rweb-route="/about" data-rweb-title="About">ABOUT</template>
</body>
</html>"#
}

#[tokio::test]
async fn serves_spa_for_unknown_routes_and_keeps_explicit_routes() {
    let app = RustWeb::new("test", "0.0.0")
        .no_docs()
        .route(health)
        .rweb(spa_html())
        .into_router();

    let home = app
        .clone()
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(home.status(), StatusCode::OK);
    assert_eq!(
        home.headers().get(header::CONTENT_TYPE).unwrap(),
        "text/html; charset=utf-8"
    );
    let body = home.into_body().collect().await.unwrap().to_bytes();
    let body = String::from_utf8_lossy(&body);
    assert!(body.contains("data-rweb-route=\"/\""));
    assert!(body.contains("data-rweb-route=\"/about\""));

    let about = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/about")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(about.status(), StatusCode::OK);
    let body = about.into_body().collect().await.unwrap().to_bytes();
    assert!(String::from_utf8_lossy(&body).contains("ABOUT"));

    let api = app
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(api.status(), StatusCode::OK);
    assert_eq!(
        api.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/json"
    );
    let body = api.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body[..], b"\"ok\"");
}
