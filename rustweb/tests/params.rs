use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use rustweb::prelude::*;
use tower::ServiceExt;

#[rustweb::query]
#[derive(Debug)]
struct Pagination {
    #[validate(range(min = 1, max = 100))]
    limit: u32,
}

#[rustweb::get("/users/{id}")]
async fn get_user(ApiPath(id): ApiPath<u64>) -> Json<u64> {
    Json(id)
}

#[rustweb::get("/search")]
async fn search(ApiQuery(q): ApiQuery<Pagination>) -> Json<u32> {
    Json(q.limit)
}

#[tokio::test]
async fn path_param_works() {
    let app = RustWeb::new("test", "0.0.0").route(get_user).into_router();
    let response = app
        .oneshot(Request::builder().uri("/users/42").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&bytes[..], b"42");
}

#[tokio::test]
async fn invalid_path_param_returns_422() {
    let app = RustWeb::new("test", "0.0.0").route(get_user).into_router();
    let response = app
        .oneshot(Request::builder().uri("/users/abc").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn query_validation_returns_422() {
    let app = RustWeb::new("test", "0.0.0").route(search).into_router();
    let response = app
        .oneshot(Request::builder().uri("/search?limit=0").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn valid_query_works() {
    let app = RustWeb::new("test", "0.0.0").route(search).into_router();
    let response = app
        .oneshot(Request::builder().uri("/search?limit=10").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}
