use rustweb::prelude::*;
use rustweb::Route;

#[rustweb::model]
struct User {
    id: u64,
    name: String,
}

#[rustweb::model]
struct CreateUser {
    #[validate(length(min = 3))]
    name: String,
}

#[rustweb::get("/users/{id}", summary = "Get one user", tags = ["users"])]
async fn get_user(ApiPath(id): ApiPath<u64>) -> ApiResult<Json<User>> {
    Ok(Json(User {
        id,
        name: "Ada".into(),
    }))
}

#[rustweb::post("/users", status = 201, tags = ["users"])]
async fn create_user(ApiJson(body): ApiJson<CreateUser>) -> ApiResult<Json<User>> {
    Ok(Json(User {
        id: 1,
        name: body.name,
    }))
}

#[test]
fn get_operation_has_param_and_response() {
    let op = get_user.operation();
    assert_eq!(op.operation_id.as_deref(), Some("get_user"));
    assert_eq!(op.summary.as_deref(), Some("Get one user"));

    let params = op.parameters.unwrap();
    let rustweb::openapi::RefOr::T(param) = &params[0] else {
        panic!("expected inline parameter");
    };
    assert_eq!(param.name, "id");
    assert!(matches!(
        param.parameter_in,
        rustweb::openapi::path::ParameterIn::Path
    ));
    assert!(op.responses.responses.contains_key("200"));
    assert!(op.responses.responses.contains_key("422"));
}

#[test]
fn post_operation_has_request_body_and_201() {
    let op = create_user.operation();
    assert!(op.request_body.is_some());
    assert!(op.responses.responses.contains_key("201"));
}

#[test]
fn schemas_include_models_and_error_body() {
    let names: Vec<String> = get_user.schemas().into_iter().map(|(name, _)| name).collect();
    assert!(names.contains(&"User".to_string()));
    assert!(names.contains(&"ApiErrorBody".to_string()));
}
