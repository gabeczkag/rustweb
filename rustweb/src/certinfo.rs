use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use axum::extract::{Path as UrlPath, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::get;
use axum::{Json, Router};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

const MAX_APPS: usize = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicRecord {
    pub app_id: String,
    pub app: String,
    pub uuid_commitment: String,
    pub encrypted_key: String,
    pub public_key: String,
    pub key_fingerprint: String,
    pub salt: String,
    pub iv: String,
    pub iterations: u64,
    pub status: String,
    pub verified: bool,
    pub created: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredRecord {
    #[serde(flatten)]
    public: PublicRecord,
    identity_hash: String,
    ip_hash: Option<String>,
    hwid_hash: String,
    fingerprint_hash: String,
}

#[derive(Debug, Deserialize)]
struct RegisterRequest {
    payload: Value,
    hwid: String,
    fingerprint: String,
}

#[derive(Clone)]
struct CertRegistry {
    store: Arc<Mutex<Store>>,
}

struct Store {
    path: PathBuf,
    records: Vec<StoredRecord>,
}

type ApiError = (StatusCode, Json<Value>);
type ApiResult<T> = Result<T, ApiError>;

/// Local `/certinfo/` registry.
///
/// The registry is a JSON file seeded from the compiled `.rweb` page. It is
/// intentionally local-only: GitHub Pages cannot accept writes, so the public
/// deployment uses Supabase instead. The same request/response shape is used
/// by both backends.
pub fn router(path: impl Into<PathBuf>, html: &str) -> Router {
    let path = path.into();
    let mut records = load(&path).unwrap_or_default();

    if records.is_empty() {
        records = seed_from_html(html);
        if !records.is_empty() {
            let _ = save(&path, &records);
        }
    }

    let registry = CertRegistry {
        store: Arc::new(Mutex::new(Store { path, records })),
    };

    Router::new()
        .route("/certinfo", get(list).post(register))
        .route("/certinfo/", get(list).post(register))
        .route("/certinfo/{app_id}", get(get_one))
        .route("/rustweb/certinfo", get(list).post(register))
        .route("/rustweb/certinfo/", get(list).post(register))
        .route("/rustweb/certinfo/{app_id}", get(get_one))
        .with_state(registry)
}

async fn list(State(registry): State<CertRegistry>) -> Json<Vec<PublicRecord>> {
    let store = lock(&registry);
    Json(
        store
            .records
            .iter()
            .rev()
            .map(|record| record.public.clone())
            .collect(),
    )
}

async fn get_one(
    State(registry): State<CertRegistry>,
    UrlPath(app_id): UrlPath<String>,
) -> ApiResult<Json<PublicRecord>> {
    let store = lock(&registry);
    store
        .records
        .iter()
        .find(|record| record.public.app_id == app_id)
        .map(|record| Json(record.public.clone()))
        .ok_or_else(|| fail(StatusCode::NOT_FOUND, "NOT_FOUND"))
}

async fn register(
    State(registry): State<CertRegistry>,
    headers: HeaderMap,
    Json(body): Json<RegisterRequest>,
) -> ApiResult<(StatusCode, Json<PublicRecord>)> {
    let payload = body
        .payload
        .as_object()
        .ok_or_else(|| fail(StatusCode::BAD_REQUEST, "INVALID_ENTRY"))?;

    let app = field(payload, "app").ok_or_else(|| fail(StatusCode::BAD_REQUEST, "INVALID_APP"))?;
    if app.len() > 64 {
        return Err(fail(StatusCode::BAD_REQUEST, "INVALID_APP"));
    }

    let app_id = field(payload, "app_id")
        .and_then(|value| Uuid::parse_str(value).ok())
        .ok_or_else(|| fail(StatusCode::BAD_REQUEST, "INVALID_APP_ID"))?
        .to_string();

    let uuid_commitment = field(payload, "uuid_commitment")
        .map(str::to_ascii_lowercase)
        .filter(|value| is_hex_64(value))
        .ok_or_else(|| fail(StatusCode::BAD_REQUEST, "INVALID_UUID_COMMITMENT"))?;

    let key_fingerprint = field(payload, "key_fingerprint")
        .map(str::to_ascii_lowercase)
        .filter(|value| is_hex_64(value))
        .ok_or_else(|| fail(StatusCode::BAD_REQUEST, "INVALID_FINGERPRINT"))?;

    let encrypted_key = field(payload, "encrypted_key")
        .or_else(|| field(payload, "ciphertext"))
        .filter(|value| value.len() >= 32)
        .ok_or_else(|| fail(StatusCode::BAD_REQUEST, "INVALID_ENCRYPTED_KEY"))?;

    let public_key = field(payload, "public_key")
        .ok_or_else(|| fail(StatusCode::BAD_REQUEST, "INVALID_PUBLIC_KEY"))?;
    let public_key_bytes = BASE64
        .decode(public_key)
        .map_err(|_| fail(StatusCode::BAD_REQUEST, "INVALID_PUBLIC_KEY"))?;
    if hex(&Sha256::digest(public_key_bytes)) != key_fingerprint {
        return Err(fail(StatusCode::BAD_REQUEST, "FINGERPRINT_MISMATCH"));
    }

    let salt = field(payload, "salt")
        .filter(|value| value.len() >= 8)
        .ok_or_else(|| fail(StatusCode::BAD_REQUEST, "INVALID_ENVELOPE"))?;
    let iv = field(payload, "iv")
        .filter(|value| value.len() >= 8)
        .ok_or_else(|| fail(StatusCode::BAD_REQUEST, "INVALID_ENVELOPE"))?;

    let iterations = payload
        .get("iterations")
        .and_then(Value::as_u64)
        .unwrap_or(210_000);
    if !(100_000..=1_000_000).contains(&iterations) {
        return Err(fail(StatusCode::BAD_REQUEST, "INVALID_ITERATIONS"));
    }

    let hwid = body.hwid.trim();
    let fingerprint = body.fingerprint.trim();
    if !(8..=200).contains(&hwid.len()) {
        return Err(fail(StatusCode::BAD_REQUEST, "INVALID_HWID"));
    }
    if !(16..=200).contains(&fingerprint.len()) {
        return Err(fail(StatusCode::BAD_REQUEST, "INVALID_DEVICE_FINGERPRINT"));
    }

    let ip = client_ip(&headers);
    let ip_hash = hash("byrust-ip-v1:", &ip);
    let hwid_hash = hash("byrust-hwid-v1:", hwid);
    let fingerprint_hash = hash("byrust-fingerprint-v1:", fingerprint);
    let identity_hash = hash("byrust-identity-v1:", &format!("{ip}:{hwid}:{fingerprint}"));

    let mut store = lock(&registry);

    if store
        .records
        .iter()
        .any(|record| record.public.app_id == app_id)
    {
        return Err(fail(StatusCode::CONFLICT, "DUPLICATE_APP_ID"));
    }
    if store
        .records
        .iter()
        .any(|record| record.public.key_fingerprint == key_fingerprint)
    {
        return Err(fail(StatusCode::CONFLICT, "DUPLICATE_FINGERPRINT"));
    }

    let limit = |hash: &str, field: &str| {
        let count = store
            .records
            .iter()
            .filter(|record| match field {
                "identity" => record.identity_hash == hash,
                "ip" => record.ip_hash.as_deref() == Some(hash),
                "hwid" => record.hwid_hash == hash,
                _ => record.fingerprint_hash == hash,
            })
            .count();
        if count >= MAX_APPS {
            Some(count)
        } else {
            None
        }
    };

    if limit(&identity_hash, "identity").is_some() {
        return Err(fail(
            StatusCode::TOO_MANY_REQUESTS,
            "LIMIT_REACHED_IDENTITY",
        ));
    }
    if limit(&hwid_hash, "hwid").is_some() {
        return Err(fail(StatusCode::TOO_MANY_REQUESTS, "LIMIT_REACHED_HWID"));
    }
    if limit(&fingerprint_hash, "fingerprint").is_some() {
        return Err(fail(
            StatusCode::TOO_MANY_REQUESTS,
            "LIMIT_REACHED_FINGERPRINT",
        ));
    }
    if limit(&ip_hash, "ip").is_some() {
        return Err(fail(StatusCode::TOO_MANY_REQUESTS, "LIMIT_REACHED_IP"));
    }

    let public = PublicRecord {
        app_id,
        app: app.to_owned(),
        uuid_commitment,
        encrypted_key: encrypted_key.to_owned(),
        public_key: public_key.to_owned(),
        key_fingerprint,
        salt: salt.to_owned(),
        iv: iv.to_owned(),
        iterations,
        status: "SEALED".to_owned(),
        verified: true,
        created: Utc::now().to_rfc3339(),
    };

    store.records.push(StoredRecord {
        public: public.clone(),
        identity_hash,
        ip_hash: Some(ip_hash),
        hwid_hash,
        fingerprint_hash,
    });

    if let Err(error) = save(&store.path, &store.records) {
        store.records.pop();
        return Err(fail(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("WRITE_FAILED: {error}"),
        ));
    }

    Ok((StatusCode::CREATED, Json(public)))
}

fn lock(registry: &CertRegistry) -> std::sync::MutexGuard<'_, Store> {
    registry
        .store
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn field<'a>(payload: &'a serde_json::Map<String, Value>, key: &str) -> Option<&'a str> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn is_hex_64(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

fn hash(prefix: &str, value: &str) -> String {
    hex(&Sha256::digest(format!("{prefix}{value}").as_bytes()))
}

fn client_ip(headers: &HeaderMap) -> String {
    for name in ["x-forwarded-for", "x-real-ip"] {
        if let Some(value) = headers.get(name).and_then(|value| value.to_str().ok()) {
            let first = value.split(',').next().unwrap_or("").trim();
            if !first.is_empty() {
                return first.to_owned();
            }
        }
    }

    "127.0.0.1".to_owned()
}

fn fail(status: StatusCode, message: &str) -> ApiError {
    (status, Json(json!({ "message": message })))
}

fn load(path: &Path) -> io::Result<Vec<StoredRecord>> {
    match fs::read_to_string(path) {
        Ok(source) => serde_json::from_str(&source)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error),
    }
}

fn save(path: &Path, records: &[StoredRecord]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    let temporary = path.with_extension("tmp");
    let data = serde_json::to_vec_pretty(records).map_err(io::Error::other)?;
    fs::write(&temporary, data)?;
    fs::rename(temporary, path)?;
    Ok(())
}

fn seed_from_html(html: &str) -> Vec<StoredRecord> {
    let Some(marker) = html.find("id=\"cert-seed\"") else {
        return Vec::new();
    };
    let Some(open) = html[marker..].find('>').map(|offset| marker + offset + 1) else {
        return Vec::new();
    };
    let Some(close) = html[open..].find("</textarea>").map(|offset| open + offset) else {
        return Vec::new();
    };

    let Ok(values) = serde_json::from_str::<Vec<Value>>(&html[open..close]) else {
        return Vec::new();
    };

    values.iter().filter_map(seed_record).collect()
}

fn seed_record(value: &Value) -> Option<StoredRecord> {
    let app_id = field_object(value, "app_id")?;
    let app = field_object(value, "app")?;
    let uuid_commitment = field_object(value, "uuid_commitment")?;
    let encrypted_key =
        field_object(value, "encrypted_key").or_else(|| field_object(value, "ciphertext"))?;
    let public_key = field_object(value, "public_key")?;
    let key_fingerprint = field_object(value, "key_fingerprint")?;
    let salt = field_object(value, "salt")?;
    let iv = field_object(value, "iv")?;
    let created = field_object(value, "created")
        .map(str::to_owned)
        .unwrap_or_else(|| Utc::now().to_rfc3339());

    Some(StoredRecord {
        public: PublicRecord {
            app_id: app_id.to_owned(),
            app: app.to_owned(),
            uuid_commitment: uuid_commitment.to_owned(),
            encrypted_key: encrypted_key.to_owned(),
            public_key: public_key.to_owned(),
            key_fingerprint: key_fingerprint.to_owned(),
            salt: salt.to_owned(),
            iv: iv.to_owned(),
            iterations: value
                .get("iterations")
                .and_then(Value::as_u64)
                .unwrap_or(210_000),
            status: "SEALED".to_owned(),
            verified: true,
            created,
        },
        identity_hash: "seed".to_owned(),
        ip_hash: None,
        hwid_hash: format!("seed-{app_id}"),
        fingerprint_hash: format!("seed-{key_fingerprint}"),
    })
}

fn field_object<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_seed_and_skips_private_fields() {
        let html = r#"<textarea id="cert-seed" hidden>[{"app_id":"9f1c2d3e-4b5a-4c6d-8e7f-0123456789ab","app":"Seed","uuid_commitment":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","ciphertext":"ciphertext-ciphertext-ciphertext","public_key":"cHVibGlj","key_fingerprint":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","salt":"c2FsdA==","iv":"aXY=","iterations":210000,"created":"2026-01-01T00:00:00Z"}]</textarea>"#;
        let records = seed_from_html(html);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].public.app, "Seed");
        assert_eq!(
            records[0].public.encrypted_key,
            "ciphertext-ciphertext-ciphertext"
        );
    }

    #[test]
    fn hashes_are_stable_and_distinct() {
        let first = hash("byrust-ip-v1:", "127.0.0.1");
        let second = hash("byrust-ip-v1:", "127.0.0.1");
        let other = hash("byrust-ip-v1:", "127.0.0.2");
        assert_eq!(first, second);
        assert_ne!(first, other);
        assert_eq!(first.len(), 64);
    }
}
