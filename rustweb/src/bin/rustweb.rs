use std::path::PathBuf;

use clap::Parser;
use rustweb::RustWeb;

#[derive(Debug, Parser)]
#[command(
    name = "rustweb",
    version,
    about = "Serve a compiled .rweb single-file site"
)]
struct Cli {
    /// Compiled .rweb file.
    file: PathBuf,

    /// Host to bind.
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    /// Port to bind.
    #[arg(long, default_value_t = 3000)]
    port: u16,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let html = std::fs::read_to_string(&cli.file)?;
    let info = rustweb::mhtml::inspect(&html);
    let title = info.title.unwrap_or_else(|| "RustWeb".to_owned());
    let version = info
        .version
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_owned());

    println!(
        "serving {} at http://{}:{}/",
        cli.file.display(),
        cli.host,
        cli.port
    );

    RustWeb::new(title, version)
        .no_docs()
        .rweb(html)
        .serve((cli.host.as_str(), cli.port))
        .await?;

    Ok(())
}
