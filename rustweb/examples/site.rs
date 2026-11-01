use rustweb::prelude::*;

#[rustweb::get("/api/health")]
async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "framework": "rustweb",
        "site": "site.rpack"
    }))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pack = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../site/site.rpack");

    RustWeb::new("RustWeb site", "0.1.0")
        .route(health)
        .rpack_file(pack)?
        .serve("127.0.0.1:3000")
        .await?;

    Ok(())
}
