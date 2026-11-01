use std::fmt;

use axum::extract::rejection::JsonRejection;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::{json, Map, Value};
use validator::{ValidationError, ValidationErrors, ValidationErrorsKind};

/// Result type used by rustweb handlers.
pub type ApiResult<T> = Result<T, ApiError>;

/// HTTP error with a FastAPI-style JSON body.
#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    detail: Value,
}

impl ApiError {
    pub fn custom(status: StatusCode, detail: Value) -> Self {
        Self { status, detail }
    }

    pub fn bad_request(detail: impl Into<String>) -> Self {
        Self::custom(StatusCode::BAD_REQUEST, json!(detail.into()))
    }

    pub fn not_found(detail: impl Into<String>) -> Self {
        Self::custom(StatusCode::NOT_FOUND, json!(detail.into()))
    }

    pub fn internal(detail: impl Into<String>) -> Self {
        Self::custom(StatusCode::INTERNAL_SERVER_ERROR, json!(detail.into()))
    }

    pub fn unprocessable(detail: Value) -> Self {
        Self::custom(StatusCode::UNPROCESSABLE_ENTITY, detail)
    }

    pub fn validation(location: &str, errors: ValidationErrors) -> Self {
        Self::unprocessable(Value::Array(flatten_errors(location, &errors)))
    }

    pub(crate) fn from_json_rejection(rejection: JsonRejection) -> Self {
        Self::unprocessable(json!([{
            "type": "json_invalid",
            "loc": ["body"],
            "msg": rejection.body_text(),
        }]))
    }

    pub(crate) fn from_path_rejection(message: impl Into<String>) -> Self {
        Self::unprocessable(json!([{
            "type": "path_invalid",
            "loc": ["path"],
            "msg": message.into(),
        }]))
    }

    pub(crate) fn from_query_rejection(message: impl Into<String>) -> Self {
        Self::unprocessable(json!([{
            "type": "query_invalid",
            "loc": ["query"],
            "msg": message.into(),
        }]))
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.status, self.detail)
    }
}

impl std::error::Error for ApiError {}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({ "detail": self.detail }))).into_response()
    }
}

fn flatten_errors(location: &str, errors: &ValidationErrors) -> Vec<Value> {
    let mut output = Vec::new();
    let mut path = vec![Value::String(location.to_owned())];
    collect_errors(&mut path, errors, &mut output);
    output
}

fn collect_errors(path: &mut Vec<Value>, errors: &ValidationErrors, output: &mut Vec<Value>) {
    for (field, kind) in errors.errors() {
        match kind {
            ValidationErrorsKind::Field(field_errors) => {
                for error in field_errors {
                    let mut location = path.clone();
                    location.push(Value::String(field.to_string()));
                    output.push(validation_item(location, error));
                }
            }
            ValidationErrorsKind::Struct(nested) => {
                path.push(Value::String(field.to_string()));
                collect_errors(path, nested, output);
                path.pop();
            }
            ValidationErrorsKind::List(list) => {
                for (index, nested) in list {
                    path.push(Value::String(field.to_string()));
                    path.push(json!(index));
                    collect_errors(path, nested, output);
                    path.pop();
                    path.pop();
                }
            }
        }
    }
}

fn validation_item(location: Vec<Value>, error: &ValidationError) -> Value {
    let mut item = Map::new();
    item.insert("type".into(), json!(validation_type(error)));
    item.insert("loc".into(), Value::Array(location));
    item.insert("msg".into(), json!(validation_message(error)));
    if !error.params.is_empty() {
        item.insert("ctx".into(), json!(error.params));
    }
    Value::Object(item)
}

fn validation_message(error: &ValidationError) -> String {
    if let Some(message) = error.message.as_deref() {
        return message.to_owned();
    }

    match error.code.as_ref() {
        "length" => match (number_param(error, "min"), number_param(error, "max")) {
            (Some(min), Some(max)) => format!("length must be between {min} and {max}"),
            (Some(min), None) => format!("length must be at least {min}"),
            (None, Some(max)) => format!("length must be at most {max}"),
            _ => "invalid length".to_owned(),
        },
        "range" => match (number_param(error, "min"), number_param(error, "max")) {
            (Some(min), Some(max)) => format!("value must be between {min} and {max}"),
            (Some(min), None) => format!("value must be at least {min}"),
            (None, Some(max)) => format!("value must be at most {max}"),
            _ => "value out of range".to_owned(),
        },
        "email" => "value is not a valid email".to_owned(),
        "url" => "value is not a valid URL".to_owned(),
        other => other.to_owned(),
    }
}

fn validation_type(error: &ValidationError) -> String {
    match error.code.as_ref() {
        "length" => match (number_param(error, "min"), number_param(error, "max")) {
            (Some(_), None) => "string_too_short",
            (None, Some(_)) => "string_too_long",
            _ => "string_length",
        }
        .to_owned(),
        "range" => "value_out_of_range".to_owned(),
        "email" | "url" => "value_error".to_owned(),
        other => other.to_owned(),
    }
}

fn number_param(error: &ValidationError, name: &str) -> Option<u64> {
    error.params.get(name).and_then(Value::as_u64)
}

/// Schema of the error body used in generated OpenAPI documents.
#[derive(utoipa::ToSchema)]
pub struct ApiErrorBody {
    pub detail: Value,
}
