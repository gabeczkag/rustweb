//! RustWeb — a FastAPI-style web framework for Rust with low-latency defaults.
//!
//! The framework is a thin layer over [`axum`]: routes are declared with
//! attribute macros, inputs are validated, and OpenAPI documentation is
//! generated once at startup.

extern crate self as rustweb;

pub use axum;
pub use utoipa;
pub use utoipa::openapi;

mod app;
mod error;
mod extract;
mod route;
mod spec;

pub use app::RustWeb;
pub use error::{ApiError, ApiErrorBody, ApiResult};
pub use extract::{ApiJson, ApiPath, ApiQuery};
pub use route::Route;

pub use rustweb_macros::{delete, get, model, patch, post, put, query};

/// Common imports for application code.
pub mod prelude {
    pub use crate::{ApiError, ApiJson, ApiPath, ApiQuery, ApiResult, RustWeb, Route};
    pub use axum::extract::{Path, Query};
    pub use axum::http::StatusCode;
    pub use axum::Json;
    pub use rustweb_macros::{delete, get, model, patch, post, put, query};
}
