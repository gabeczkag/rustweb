use std::net::SocketAddr;
use std::path::PathBuf;
use std::str::FromStr;

use axum::http::{HeaderMap, HeaderName, HeaderValue};
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

    /// TLS certificate chain in PEM format.
    #[arg(long, requires = "tls_key")]
    tls_cert: Option<PathBuf>,

    /// TLS private key in PEM format.
    #[arg(long, requires = "tls_cert")]
    tls_key: Option<PathBuf>,

    /// Allow HTTP even when the Byrust profile requires HTTPS.
    #[arg(long)]
    allow_http: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let html = std::fs::read_to_string(&cli.file)?;
    let info = rustweb::mhtml::inspect(&html);
    let title = info.title.clone().unwrap_or_else(|| "RustWeb".to_owned());
    let version = info
        .version
        .clone()
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_owned());

    let tls = match (cli.tls_cert, cli.tls_key) {
        (Some(cert), Some(key)) => Some((cert, key)),
        (None, None) => None,
        _ => unreachable!("clap requires both TLS flags"),
    };

    let requires_https = info
        .protocol
        .as_ref()
        .is_some_and(|protocol| protocol.https);
    if requires_https && tls.is_none() && !cli.allow_http {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "byrust requires HTTPS: pass --tls-cert and --tls-key, or --allow-http for local development",
        )
        .into());
    }

    let headers = build_headers(info.protocol.as_ref(), tls.is_some())?;
    let app = RustWeb::new(title, version)
        .no_docs()
        .spa_headers(headers)
        .rweb(html)
        .into_router();

    let address: SocketAddr = format!("{}:{}", cli.host, cli.port).parse()?;

    if let Some(protocol) = &info.protocol {
        println!(
            "protocol: {} v{} (https={})",
            protocol.name, protocol.version, protocol.https
        );
        if let Some(alias) = &protocol.alias {
            println!("alias: {alias}://{}:{}/", cli.host, cli.port);
        }
    }

    if let Some((cert, key)) = tls {
        let config = axum_server::tls_rustls::RustlsConfig::from_pem_file(cert, key).await?;
        println!("serving https://{}:{}/", cli.host, cli.port);
        axum_server::bind_rustls(address, config)
            .serve(app.into_make_service())
            .await?;
    } else {
        if requires_https {
            eprintln!("warning: Byrust profile downgraded to HTTP for local development");
        }
        println!("serving http://{}:{}/", cli.host, cli.port);
        axum_server::bind(address)
            .serve(app.into_make_service())
            .await?;
    }

    Ok(())
}

fn build_headers(
    protocol: Option<&rustweb::mhtml::ProtocolConfig>,
    tls: bool,
) -> Result<HeaderMap, Box<dyn std::error::Error>> {
    let mut headers = HeaderMap::new();

    if let Some(protocol) = protocol {
        headers.insert(
            HeaderName::from_static("x-byrust"),
            HeaderValue::from_static("1"),
        );
        headers.insert(
            HeaderName::from_static("x-byrust-version"),
            HeaderValue::from_str(&protocol.version.to_string())?,
        );
        headers.insert(
            HeaderName::from_static("x-byrust-scheme"),
            HeaderValue::from_static(if tls { "https" } else { "http" }),
        );

        if let Some(alt_svc) = &protocol.alt_svc {
            headers.insert(
                HeaderName::from_static("alt-svc"),
                HeaderValue::from_str(alt_svc)?,
            );
        }

        for (name, value) in &protocol.headers {
            headers.insert(HeaderName::from_str(name)?, HeaderValue::from_str(value)?);
        }
    }

    if tls {
        headers.insert(
            HeaderName::from_static("strict-transport-security"),
            HeaderValue::from_static("max-age=31536000"),
        );
    }

    Ok(headers)
}
