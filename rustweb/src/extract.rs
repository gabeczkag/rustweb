use std::ops::Deref;

use axum::extract::{FromRequest, Request};
use axum::Json;
use serde::de::DeserializeOwned;
use validator::Validate;

use crate::error::ApiError;

/// JSON body extractor with validation.
///
/// Deserialization and validation failures are returned as FastAPI-style 422
/// responses instead of axum's default rejection formats.
pub struct ApiJson<T>(pub T);

impl<T> Deref for ApiJson<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T, S> FromRequest<S> for ApiJson<T>
where
    T: DeserializeOwned + Validate,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        let Json(value) = Json::<T>::from_request(request, state)
            .await
            .map_err(ApiError::from_json_rejection)?;

        value
            .validate()
            .map_err(|errors| ApiError::validation("body", errors))?;

        Ok(ApiJson(value))
    }
}
