use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use uuid::Uuid;

#[derive(Clone)]
pub struct Players {
    inner: Arc<Mutex<HashMap<String, Player>>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Session {
    pub player_id: String,
    pub token: String,
    pub max_bandwidth: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlayerView {
    pub player_id: String,
    pub addr: Option<SocketAddr>,
    pub created: u64,
    pub last_seen: u64,
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub dropped: u64,
    pub max_bandwidth: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerError {
    NotFound,
    Unauthorized,
    Limited,
}

struct Player {
    id: String,
    token: String,
    addr: Option<SocketAddr>,
    created: u64,
    last_seen: u64,
    last_seen_instant: Instant,
    bytes_in: u64,
    bytes_out: u64,
    dropped: u64,
    max_bandwidth: u64,
    tokens: f64,
    last_refill: Instant,
}

impl Players {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn create(&self, max_bandwidth: u64) -> Session {
        let player_id = Uuid::new_v4().to_string();
        let token = Uuid::new_v4().simple().to_string();
        let now = Instant::now();

        lock(&self.inner).insert(
            player_id.clone(),
            Player {
                id: player_id.clone(),
                token: token.clone(),
                addr: None,
                created: epoch(),
                last_seen: epoch(),
                last_seen_instant: now,
                bytes_in: 0,
                bytes_out: 0,
                dropped: 0,
                max_bandwidth,
                tokens: max_bandwidth as f64,
                last_refill: now,
            },
        );

        Session {
            player_id,
            token,
            max_bandwidth,
        }
    }

    pub fn touch(
        &self,
        player_id: &str,
        token: &str,
        addr: SocketAddr,
        now: Instant,
    ) -> Result<PlayerView, PlayerError> {
        let mut players = lock(&self.inner);
        let player = players.get_mut(player_id).ok_or(PlayerError::NotFound)?;
        if player.token != token {
            return Err(PlayerError::Unauthorized);
        }

        player.addr = Some(addr);
        player.last_seen = epoch();
        player.last_seen_instant = now;
        Ok(player.view())
    }

    pub fn consume(
        &self,
        player_id: &str,
        token: &str,
        addr: SocketAddr,
        bytes: u64,
        now: Instant,
    ) -> Result<PlayerView, PlayerError> {
        let mut players = lock(&self.inner);
        let player = players.get_mut(player_id).ok_or(PlayerError::NotFound)?;
        if player.token != token {
            return Err(PlayerError::Unauthorized);
        }

        player.addr = Some(addr);
        player.last_seen = epoch();
        player.last_seen_instant = now;

        let elapsed = now.duration_since(player.last_refill).as_secs_f64();
        let maximum = player.max_bandwidth as f64;
        player.tokens = (player.tokens + elapsed * maximum).min(maximum);
        player.last_refill = now;

        if player.tokens >= bytes as f64 {
            player.tokens -= bytes as f64;
            player.bytes_in = player.bytes_in.saturating_add(bytes);
            Ok(player.view())
        } else {
            player.dropped = player.dropped.saturating_add(1);
            Err(PlayerError::Limited)
        }
    }

    pub fn add_out(&self, player_id: &str, bytes: u64) {
        let mut players = lock(&self.inner);
        if let Some(player) = players.get_mut(player_id) {
            player.bytes_out = player.bytes_out.saturating_add(bytes);
        }
    }

    pub fn remove(&self, player_id: &str, token: &str) -> Result<(), PlayerError> {
        let mut players = lock(&self.inner);
        let player = players.get(player_id).ok_or(PlayerError::NotFound)?;
        if player.token != token {
            return Err(PlayerError::Unauthorized);
        }
        players.remove(player_id);
        Ok(())
    }

    pub fn list(&self) -> Vec<PlayerView> {
        let mut views: Vec<_> = lock(&self.inner).values().map(Player::view).collect();
        views.sort_by(|left, right| left.player_id.cmp(&right.player_id));
        views
    }

    pub fn cleanup(&self, timeout: Duration) {
        let now = Instant::now();
        lock(&self.inner)
            .retain(|_, player| now.duration_since(player.last_seen_instant) < timeout);
    }
}

impl Player {
    fn view(&self) -> PlayerView {
        PlayerView {
            player_id: self.id.clone(),
            addr: self.addr,
            created: self.created,
            last_seen: self.last_seen,
            bytes_in: self.bytes_in,
            bytes_out: self.bytes_out,
            dropped: self.dropped,
            max_bandwidth: self.max_bandwidth,
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    fn address() -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 9000)
    }

    #[test]
    fn enforces_bandwidth_per_player() {
        let players = Players::new();
        let session = players.create(100);
        let now = Instant::now();

        assert!(players
            .consume(&session.player_id, &session.token, address(), 80, now)
            .is_ok());
        assert_eq!(
            players.consume(&session.player_id, &session.token, address(), 80, now),
            Err(PlayerError::Limited)
        );
        assert_eq!(players.list()[0].dropped, 1);
    }

    #[test]
    fn rejects_a_wrong_token() {
        let players = Players::new();
        let session = players.create(100);
        assert_eq!(
            players.touch(&session.player_id, "wrong", address(), Instant::now()),
            Err(PlayerError::Unauthorized)
        );
    }
}
