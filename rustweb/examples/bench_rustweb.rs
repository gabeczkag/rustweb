use rustweb::prelude::*;

#[rustweb::get("/hello")]
async fn hello() -> &'static str {
    "Hello, World!"
}

#[tokio::main]
async fn main() {
    RustWeb::new("rustweb-bench", "0.0.0")
        .route(hello)
        .no_docs()
        .serve("127.0.0.1:3001")
        .await
        .expect("rustweb benchmark server failed");
}
