use axum::Router;
use utoipa::openapi::{path::Operation, schema::Schema, RefOr};

/// A single HTTP route declared by a rustweb attribute macro.
///
/// Implementations are generated for you by `#[rustweb::get]` and friends.
/// Application code normally only passes the generated unit struct to
/// [`RustWeb::route`](crate::RustWeb::route).
pub trait Route {
    /// HTTP method handled by this route.
    fn method(&self) -> axum::http::Method;

    /// Axum-style path, for example `/users/{id}`.
    fn path(&self) -> &'static str;

    /// OpenAPI operation metadata generated from the handler signature.
    fn operation(&self) -> Operation;

    /// Reusable schemas referenced by the operation.
    fn schemas(&self) -> Vec<(String, RefOr<Schema>)>;

    /// Register the route on an axum router.
    fn register(self, router: Router) -> Router;
}
