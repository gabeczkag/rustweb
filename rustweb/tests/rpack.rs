use std::fs;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use rustweb::prelude::*;
use tower::ServiceExt;

#[tokio::test]
async fn serves_pages_and_assets_from_rpack() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("site.json"),
        r#"{
            "site": {"title": "Test", "version": "1.0.0"},
            "pages": [{"route": "/", "source": "index.rtml"}],
            "data": {"name": "Ada"}
        }"#,
    )
    .unwrap();
    fs::write(directory.path().join("index.rtml"), "<h1>{{ name }}</h1>").unwrap();
    fs::create_dir(directory.path().join("assets")).unwrap();
    fs::write(directory.path().join("assets/app.css"), "body{color:red}").unwrap();

    let output = directory.path().join("site.rpack");
    rustweb::rtml::build_dir(directory.path(), &output).unwrap();

    let app = RustWeb::new("test", "0.0.0")
        .rpack_file(&output)
        .unwrap()
        .into_router();

    let page = app
        .clone()
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(page.status(), StatusCode::OK);
    assert_eq!(
        page.headers().get(header::CONTENT_TYPE).unwrap(),
        "text/html; charset=utf-8"
    );
    let body = page.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body[..], b"<h1>Ada</h1>");

    let asset = app
        .oneshot(
            Request::builder()
                .uri("/assets/app.css")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(asset.status(), StatusCode::OK);
    assert_eq!(
        asset.headers().get(header::CONTENT_TYPE).unwrap(),
        "text/css"
    );
    let body = asset.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body[..], b"body{color:red}");
}
