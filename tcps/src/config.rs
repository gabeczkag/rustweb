use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub user_id: String,
    #[serde(default = "default_mode")]
    pub mode: String,
    pub tcp_listen: String,
    pub udp_listen: String,
    #[serde(default)]
    pub public_udp_addr: String,
    pub store_dir: PathBuf,
    #[serde(default = "default_max_file_size")]
    pub max_file_size: usize,
    #[serde(default = "default_bandwidth")]
    pub max_bandwidth_per_player: u64,
    #[serde(default = "default_timeout")]
    pub player_timeout_seconds: u64,
    pub token: String,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self, String> {
        let source = fs::read_to_string(path)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        let mut config: Self = serde_json::from_str(&source)
            .map_err(|error| format!("invalid {}: {error}", path.display()))?;

        if config.public_udp_addr.trim().is_empty() {
            config.public_udp_addr = config.udp_listen.clone();
        }
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.user_id.is_empty()
            || !self.user_id.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '-' | '_')
            })
        {
            return Err("user_id must contain only ASCII letters, digits, '-' and '_'".to_owned());
        }
        if self.token.trim().is_empty() {
            return Err("token must not be empty".to_owned());
        }
        if self.max_file_size == 0 {
            return Err("max_file_size must be greater than zero".to_owned());
        }
        if self.max_bandwidth_per_player == 0 {
            return Err("max_bandwidth_per_player must be greater than zero".to_owned());
        }
        if self.player_timeout_seconds == 0 {
            return Err("player_timeout_seconds must be greater than zero".to_owned());
        }
        if !matches!(self.mode.as_str(), "self_host" | "server_host") {
            return Err("mode must be self_host or server_host".to_owned());
        }
        Ok(())
    }
}

fn default_mode() -> String {
    "self_host".to_owned()
}

fn default_max_file_size() -> usize {
    64 * 1024 * 1024
}

fn default_bandwidth() -> u64 {
    1024 * 1024
}

fn default_timeout() -> u64 {
    30
}
