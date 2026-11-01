use axum::routing::get;
use axum::Router;

async fn hello() -> &'static str {
    "Hello, World!"
}

#[tokio::main]
async fn main() {
    let app = Router::new().route("/hello", get(hello));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3002")
        .await
        .expect("bind axum benchmark server");
    axum::serve(listener, app)
        .await
        .expect("axum benchmark server failed");
}
