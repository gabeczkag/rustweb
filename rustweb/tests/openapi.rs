use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use rustweb::prelude::*;
use tower::ServiceExt;

#[rustweb::model]
struct User {
    id: u64,
    name: String,
}

#[rustweb::get("/users")]
async fn list_users() -> ApiResult<Json<Vec<User>>> {
    Ok(Json(Vec::new()))
}

#[rustweb::get("/users/{id}")]
async fn get_user(ApiPath(id): ApiPath<u64>) -> ApiResult<Json<User>> {
    Ok(Json(User {
        id,
        name: "Ada".into(),
    }))
}

#[rustweb::post("/users", status = 201)]
async fn create_user(ApiJson(body): ApiJson<User>) -> ApiResult<Json<User>> {
    Ok(Json(body))
}

#[rustweb::get("/ping")]
async fn ping() -> Json<String> {
    Json("pong".to_owned())
}

#[tokio::test]
async fn openapi_contains_paths_params_and_schemas() {
    let app = RustWeb::new("Test API", "1.0.0")
        .route(list_users)
        .route(get_user)
        .route(create_user)
        .route(ping)
        .into_router();

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/openapi.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let doc: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

    assert_eq!(doc["openapi"], "3.1.0");
    assert_eq!(doc["info"]["title"], "Test API");
    assert_eq!(
        doc["paths"]["/users/{id}"]["get"]["parameters"][0]["name"],
        "id"
    );
    assert!(doc["paths"]["/users"]["get"].is_object());
    assert_eq!(
        doc["paths"]["/users"]["post"]["responses"]["201"]["description"],
        "Created"
    );
    assert!(doc["components"]["schemas"]["User"].is_object());
    assert_eq!(
        doc["paths"]["/ping"]["get"]["responses"]["200"]["content"]["application/json"]["schema"]
            ["type"],
        "string"
    );

    let list = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/users")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list.status(), StatusCode::OK);

    let created = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/users")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"id":1,"name":"Ada"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);

    let docs = app
        .oneshot(
            Request::builder()
                .uri("/docs/")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(docs.status(), StatusCode::OK);
    assert!(docs
        .headers()
        .get(header::CONTENT_TYPE)
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("text/html"));
}

#[tokio::test]
async fn unknown_route_returns_json_404() {
    let app = RustWeb::new("test", "0.0.0").into_router();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/missing")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["detail"], "Not Found");
}
