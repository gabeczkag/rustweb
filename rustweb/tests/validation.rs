use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use rustweb::prelude::*;
use tower::ServiceExt;

#[rustweb::model]
#[derive(Debug)]
struct CreateUser {
    #[validate(length(min = 3, max = 20))]
    name: String,
}

#[rustweb::post("/users", status = 201)]
async fn create_user(ApiJson(body): ApiJson<CreateUser>) -> ApiResult<Json<CreateUser>> {
    Ok(Json(body))
}

#[rustweb::delete("/users/current", status = 204)]
async fn delete_user() -> ApiResult<()> {
    Ok(())
}

fn post(body: &'static str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/users")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .unwrap()
}

#[tokio::test]
async fn invalid_body_returns_fastapi_style_422() {
    let app = RustWeb::new("test", "0.0.0").route(create_user).into_router();
    let response = app.oneshot(post(r#"{"name":"ab"}"#)).await.unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/json"
    );

    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["detail"][0]["loc"][0], "body");
    assert_eq!(json["detail"][0]["loc"][1], "name");
    assert!(json["detail"][0]["msg"].is_string());
    assert!(json["detail"][0]["type"].is_string());
}

#[tokio::test]
async fn malformed_json_returns_422() {
    let app = RustWeb::new("test", "0.0.0").route(create_user).into_router();
    let response = app.oneshot(post("{not json")).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn valid_body_returns_created() {
    let app = RustWeb::new("test", "0.0.0").route(create_user).into_router();
    let response = app.oneshot(post(r#"{"name":"Ada"}"#)).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn no_content_status_has_empty_body() {
    let app = RustWeb::new("test", "0.0.0").route(delete_user).into_router();
    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/users/current")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    assert!(bytes.is_empty());
}
