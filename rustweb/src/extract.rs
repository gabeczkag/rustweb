use std::ops::Deref;

use axum::extract::{FromRequest, FromRequestParts, Path, Query, Request};
use axum::http::request::Parts;
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

/// Path extractor that turns extraction failures into FastAPI-style 422
/// responses.
pub struct ApiPath<T>(pub T);

impl<T> Deref for ApiPath<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T, S> FromRequestParts<S> for ApiPath<T>
where
    T: DeserializeOwned + Send,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Path(value) = Path::<T>::from_request_parts(parts, state)
            .await
            .map_err(|rejection| ApiError::from_path_rejection(rejection.body_text()))?;

        Ok(ApiPath(value))
    }
}

/// Query string extractor with validation.
pub struct ApiQuery<T>(pub T);

impl<T> Deref for ApiQuery<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T, S> FromRequestParts<S> for ApiQuery<T>
where
    T: DeserializeOwned + Validate + Send,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Query(value) = Query::<T>::from_request_parts(parts, state)
            .await
            .map_err(|rejection| ApiError::from_query_rejection(rejection.body_text()))?;

        value
            .validate()
            .map_err(|errors| ApiError::validation("query", errors))?;

        Ok(ApiQuery(value))
    }
}
