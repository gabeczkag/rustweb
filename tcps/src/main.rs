mod config;
mod http;
mod players;
mod store;
mod udp;

use std::error::Error;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use clap::Parser;
use config::Config;
use http::AppState;
use players::Players;
use store::Store;
use tokio::net::{TcpListener, UdpSocket};

#[derive(Debug, Parser)]
#[command(
    name = "tcps",
    version,
    about = "Per-user TCP file sync and UDP player supervisor"
)]
struct Cli {
    /// JSON configuration file.
    #[arg(long, default_value = "tcps.json")]
    config: PathBuf,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();
    let config = Config::load(&cli.config)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    let config = Arc::new(config);

    let store = Store::new(&config.store_dir, &config.user_id)?;
    let players = Players::new();

    let state = AppState {
        config: config.clone(),
        store,
        players: players.clone(),
        started: Instant::now(),
    };

    let tcp = TcpListener::bind(&config.tcp_listen).await?;
    let udp = UdpSocket::bind(&config.udp_listen).await?;

    println!("tcps user: {}", config.user_id);
    println!("tcps mode: {}", config.mode);
    println!("tcps tcp:  http://{}/", config.tcp_listen);
    println!("tcps udp:  {}", config.udp_listen);
    println!("tcps store: {}", config.store_dir.display());
    println!("tcps admin token: configured");

    let udp_players = players.clone();
    let udp_config = config.clone();
    tokio::spawn(async move {
        udp::run(udp, udp_players, udp_config).await;
    });

    let cleanup_players = players.clone();
    let timeout = Duration::from_secs(config.player_timeout_seconds);
    tokio::spawn(async move {
        udp::cleanup(cleanup_players, timeout).await;
    });

    axum::serve(tcp, http::router(state)).await?;
    Ok(())
}
