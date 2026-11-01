use rustweb::prelude::*;

#[rustweb::get("/api/health")]
async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "framework": "rustweb",
        "site": "site.rweb"
    }))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let site = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../site/site.rweb");

    RustWeb::new("RustWeb site", "0.1.0")
        .route(health)
        .rweb_file(site)?
        .serve("127.0.0.1:3000")
        .await?;

    Ok(())
}
