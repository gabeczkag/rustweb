use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

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

static USERS: LazyLock<Mutex<HashMap<u64, User>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[rustweb::get("/")]
async fn root() -> &'static str {
    "RustWeb is running. Try /docs or /openapi.json"
}

#[rustweb::get("/users", tags = ["users"])]
async fn list_users() -> ApiResult<Json<Vec<User>>> {
    let users = USERS.lock().unwrap().values().cloned().collect();
    Ok(Json(users))
}

#[rustweb::get("/users/{id}", tags = ["users"])]
async fn get_user(ApiPath(id): ApiPath<u64>) -> ApiResult<Json<User>> {
    USERS
        .lock()
        .unwrap()
        .get(&id)
        .cloned()
        .map(Json)
        .ok_or_else(|| ApiError::not_found("User not found"))
}

#[rustweb::post("/users", status = 201, tags = ["users"])]
async fn create_user(ApiJson(body): ApiJson<CreateUser>) -> ApiResult<Json<User>> {
    let mut users = USERS.lock().unwrap();
    let id = users.len() as u64 + 1;
    let user = User {
        id,
        name: body.name,
        age: body.age,
    };
    users.insert(id, user.clone());
    Ok(Json(user))
}

#[tokio::main]
async fn main() {
    RustWeb::new("RustWeb Demo", "0.1.0")
        .route(root)
        .route(list_users)
        .route(get_user)
        .route(create_user)
        .serve("127.0.0.1:3000")
        .await
        .expect("server failed");
}
