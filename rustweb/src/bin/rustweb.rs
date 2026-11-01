use std::env;
use std::error::Error;
use std::fs;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;

use axum::http::{HeaderMap, HeaderName, HeaderValue};
use clap::Parser;
use rustweb::mhtml::{AppIdentity, TrustLevel};
use rustweb::RustWeb;

#[derive(Debug, Parser)]
#[command(
    name = "rustweb",
    version,
    about = "Serve a compiled .rweb single-file site with Byrust identity"
)]
struct Cli {
    /// Compiled .rweb file.
    file: PathBuf,

    /// Host to bind and include in the certificate SAN.
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    /// Port to bind.
    #[arg(long, default_value_t = 3000)]
    port: u16,

    /// Use a production TLS certificate chain instead of the Byrust CA.
    #[arg(long, requires = "tls_key")]
    tls_cert: Option<PathBuf>,

    /// Use a production TLS private key instead of the Byrust CA.
    #[arg(long, requires = "tls_cert")]
    tls_key: Option<PathBuf>,

    /// Allow HTTP even when the Byrust profile requires HTTPS.
    #[arg(long)]
    allow_http: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();
    if cli
        .host
        .chars()
        .any(|character| character.is_whitespace() || matches!(character, '/' | '\\'))
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "host must not contain whitespace, `/`, or `\\`",
        )
        .into());
    }
    let html = fs::read_to_string(&cli.file)?;
    let info = rustweb::mhtml::inspect(&html);
    let title = info.title.clone().unwrap_or_else(|| "RustWeb".to_owned());
    let version = info
        .version
        .clone()
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_owned());

    let manual_tls = match (cli.tls_cert.clone(), cli.tls_key.clone()) {
        (Some(cert), Some(key)) => Some((cert, key)),
        (None, None) => None,
        _ => unreachable!("clap requires both TLS flags"),
    };

    let tls = if cli.allow_http {
        None
    } else if manual_tls.is_some() {
        manual_tls
    } else if let Some(app) = &info.app {
        Some(issue_app_certificate(app, &cli.host)?)
    } else {
        None
    };

    let requires_https = info
        .protocol
        .as_ref()
        .is_some_and(|protocol| protocol.https);
    if requires_https && tls.is_none() && !cli.allow_http {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "byrust requires HTTPS: define app.name/app.uuid in linker.json, pass --tls-cert/--tls-key, or use --allow-http for local development",
        )
        .into());
    }

    let headers = build_headers(info.protocol.as_ref(), tls.is_some())?;
    let app = RustWeb::new(title, version)
        .no_docs()
        .spa_headers(headers)
        .rweb(html)
        .into_router();

    let address = tokio::net::lookup_host((cli.host.as_str(), cli.port))
        .await?
        .next()
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::AddrNotAvailable,
                format!("could not resolve {}:{}", cli.host, cli.port),
            )
        })?;

    if let Some(protocol) = &info.protocol {
        println!(
            "protocol: {} v{} (https={})",
            protocol.name, protocol.version, protocol.https
        );
        if let Some(alias) = &protocol.alias {
            println!("alias: {alias}://{}:{}/", cli.host, cli.port);
        }
    }
    if let Some(app) = &info.app {
        println!(
            "app: {}  uuid={}  trust={}",
            app.name,
            app.uuid,
            trust_label(&app.trust)
        );
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
) -> Result<HeaderMap, Box<dyn Error>> {
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

fn issue_app_certificate(
    app: &AppIdentity,
    host: &str,
) -> Result<(PathBuf, PathBuf), Box<dyn Error>> {
    let base = byrust_config_dir();
    let universal = app.trust == TrustLevel::Universal;
    let ca_key = base.join(if universal {
        "universal-ca.key"
    } else {
        "ca.key"
    });
    let ca_crt = base.join(if universal {
        "universal-ca.crt"
    } else {
        "ca.crt"
    });

    ensure_ca(&base, &ca_key, &ca_crt, universal)?;

    let app_dir = base
        .join("apps")
        .join(format!("{}-{}", app.uuid, slug(host)));
    fs::create_dir_all(&app_dir)?;

    let key = app_dir.join("app.key");
    let leaf = app_dir.join("app.crt");
    let chain = app_dir.join("chain.crt");

    if !key.exists() || !leaf.exists() || !chain.exists() {
        let csr = app_dir.join("app.csr");
        let extension = app_dir.join("ext.cnf");

        run(Command::new("openssl")
            .arg("genrsa")
            .arg("-out")
            .arg(&key)
            .arg("2048"))?;

        let subject = format!(
            "/CN={}/OU=byrust-{}-{}",
            slug(&app.name),
            trust_label(&app.trust),
            app.uuid
        );
        run(Command::new("openssl")
            .args(["req", "-new", "-key"])
            .arg(&key)
            .arg("-subj")
            .arg(subject)
            .arg("-out")
            .arg(&csr))?;

        let mut san = String::from("subjectAltName=DNS:localhost,IP:127.0.0.1");
        if host.parse::<IpAddr>().is_ok() {
            if host != "127.0.0.1" {
                san.push_str(&format!(",IP:{host}"));
            }
        } else if host != "localhost" {
            san.push_str(&format!(",DNS:{host}"));
        }

        fs::write(
            &extension,
            format!(
                "{san}\nbasicConstraints=CA:FALSE\nkeyUsage=digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\n"
            ),
        )?;

        run(Command::new("openssl")
            .args(["x509", "-req", "-in"])
            .arg(&csr)
            .arg("-CA")
            .arg(&ca_crt)
            .arg("-CAkey")
            .arg(&ca_key)
            .arg("-CAcreateserial")
            .arg("-out")
            .arg(&leaf)
            .arg("-days")
            .arg("365")
            .arg("-sha256")
            .arg("-extfile")
            .arg(&extension))?;

        let chain_text = format!(
            "{}{}",
            fs::read_to_string(&leaf)?,
            fs::read_to_string(&ca_crt)?
        );
        fs::write(&chain, chain_text)?;
    }

    Ok((chain, key))
}

fn ensure_ca(
    base: &Path,
    key: &Path,
    certificate: &Path,
    universal: bool,
) -> Result<(), Box<dyn Error>> {
    if key.exists() && certificate.exists() {
        return Ok(());
    }

    fs::create_dir_all(base)?;

    run(Command::new("openssl")
        .arg("genrsa")
        .arg("-out")
        .arg(key)
        .arg("2048"))?;

    let subject = if universal {
        "/CN=Byrust Universal CA (untrusted)"
    } else {
        "/CN=Byrust Local CA"
    };

    run(Command::new("openssl")
        .args(["req", "-x509", "-new", "-nodes", "-key"])
        .arg(key)
        .arg("-sha256")
        .arg("-days")
        .arg("3650")
        .arg("-subj")
        .arg(subject)
        .arg("-out")
        .arg(certificate))?;

    if universal {
        eprintln!(
            "created untrusted Byrust Universal CA at {}",
            certificate.display()
        );
        eprintln!("universal UUID pages show 'Page is not safe' and browsers warn");
    } else {
        eprintln!("created Byrust Local CA at {}", certificate.display());
        eprintln!("trust it once so browsers accept generated app certificates:");
        eprintln!(
            "  sudo cp {} /usr/local/share/ca-certificates/byrust.crt && sudo update-ca-certificates",
            certificate.display()
        );
        eprintln!("  Firefox: Settings -> Certificates -> Import");
    }

    Ok(())
}

fn run(command: &mut Command) -> Result<(), Box<dyn Error>> {
    let program = command.get_program().to_string_lossy().to_string();
    let output = command.output().map_err(|error| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("failed to run {program}: {error}; install OpenSSL"),
        )
    })?;

    if !output.status.success() {
        return Err(std::io::Error::other(format!(
            "{program} failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        ))
        .into());
    }

    Ok(())
}

fn byrust_config_dir() -> PathBuf {
    if let Some(path) = env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(path).join("byrust");
    }
    if let Some(home) = env::var_os("HOME") {
        return PathBuf::from(home).join(".config").join("byrust");
    }
    env::temp_dir().join("byrust")
}

fn trust_label(trust: &TrustLevel) -> &'static str {
    match trust {
        TrustLevel::App => "app",
        TrustLevel::Universal => "universal",
    }
}

fn slug(value: &str) -> String {
    let mut output = String::new();

    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            output.push(character.to_ascii_lowercase());
        } else if matches!(character, '-' | '_' | '.') {
            output.push(character);
        } else if !output.ends_with('-') {
            output.push('-');
        }
    }

    let output = output.trim_matches('-');
    if output.is_empty() {
        "app".to_owned()
    } else {
        output.chars().take(40).collect()
    }
}
