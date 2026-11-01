use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use rustweb::prelude::*;
use tower::ServiceExt;

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

static STORE: LazyLock<Mutex<HashMap<u64, User>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

#[rustweb::post("/users", status = 201)]
async fn create_user(ApiJson(body): ApiJson<CreateUser>) -> ApiResult<Json<User>> {
    let mut store = STORE.lock().unwrap();
    let id = store.len() as u64 + 1;
    let user = User {
        id,
        name: body.name,
        age: body.age,
    };
    store.insert(id, user.clone());
    Ok(Json(user))
}

#[rustweb::get("/users/{id}")]
async fn get_user(ApiPath(id): ApiPath<u64>) -> ApiResult<Json<User>> {
    STORE
        .lock()
        .unwrap()
        .get(&id)
        .cloned()
        .map(Json)
        .ok_or_else(|| ApiError::not_found("User not found"))
}

#[tokio::test]
async fn create_then_read_then_docs() {
    let app = RustWeb::new("Demo", "0.1.0")
        .route(create_user)
        .route(get_user)
        .into_router();

    let created = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/users")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"name":"Ada","age":36}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);

    let fetched = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/users/1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(fetched.status(), StatusCode::OK);
    let bytes = fetched.into_body().collect().await.unwrap().to_bytes();
    let user: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(user["name"], "Ada");

    let spec = app
        .oneshot(
            Request::builder()
                .uri("/openapi.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(spec.status(), StatusCode::OK);
}
