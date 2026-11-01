use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

pub const UNIVERSAL_UUID: &str = "00000000-0000-0000-0000-000000000000";

use crate::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiteMeta {
    pub title: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageConfig {
    pub route: String,
    pub file: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub data: Option<Value>,
}

/// Contents of `linker.json`.
///
/// All paths are relative to the directory containing `linker.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub name: String,
    #[serde(default)]
    pub uuid: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TrustLevel {
    App,
    Universal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppIdentity {
    pub name: String,
    pub uuid: String,
    pub trust: TrustLevel,
}

impl AppIdentity {
    pub fn resolve(config: &AppConfig) -> Result<Self, Error> {
        Self::resolve_with_seed(config, "")
    }

    pub fn resolve_with_seed(config: &AppConfig, seed: &str) -> Result<Self, Error> {
        let name = config.name.trim();
        if name.is_empty() {
            return Err(Error::Linker("app.name must not be empty".to_owned()));
        }
        if name
            .chars()
            .any(|character| matches!(character, '\r' | '\n'))
        {
            return Err(Error::Linker(
                "app.name must not contain newlines".to_owned(),
            ));
        }

        let raw = config.uuid.as_deref().unwrap_or("").trim();

        if raw.is_empty() {
            let seed = format!("{name}:{seed}");
            let uuid = Uuid::new_v5(&Uuid::NAMESPACE_OID, seed.as_bytes()).to_string();
            Ok(Self {
                name: name.to_owned(),
                uuid,
                trust: TrustLevel::App,
            })
        } else if raw.eq_ignore_ascii_case("universal") || raw == UNIVERSAL_UUID {
            Ok(Self {
                name: name.to_owned(),
                uuid: UNIVERSAL_UUID.to_owned(),
                trust: TrustLevel::Universal,
            })
        } else {
            let uuid = Uuid::parse_str(raw).map_err(|_| {
                Error::Linker("app.uuid must be a valid UUID, `universal`, or blank".to_owned())
            })?;
            Ok(Self {
                name: name.to_owned(),
                uuid: uuid.to_string(),
                trust: TrustLevel::App,
            })
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolConfig {
    #[serde(default = "default_protocol_name")]
    pub name: String,
    #[serde(default = "default_protocol_version")]
    pub version: u16,
    #[serde(default = "default_true")]
    pub https: bool,
    #[serde(default)]
    pub alias: Option<String>,
    #[serde(default)]
    pub alt_svc: Option<String>,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
}

impl Default for ProtocolConfig {
    fn default() -> Self {
        Self {
            name: default_protocol_name(),
            version: default_protocol_version(),
            https: true,
            alias: None,
            alt_svc: None,
            headers: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkerConfig {
    pub site: SiteMeta,
    #[serde(default = "default_lang")]
    pub lang: String,
    #[serde(default)]
    pub pages: Vec<PageConfig>,
    #[serde(default)]
    pub styles: Vec<String>,
    #[serde(default)]
    pub scripts: Vec<String>,
    #[serde(default)]
    pub assets: Vec<String>,
    #[serde(default)]
    pub data: Value,
    #[serde(default)]
    pub output: Option<String>,
    #[serde(default)]
    pub frame: Option<String>,
    #[serde(default)]
    pub base: String,
    #[serde(default = "default_true")]
    pub ascii: bool,
    #[serde(default)]
    pub protocol: Option<ProtocolConfig>,
    #[serde(default)]
    pub app: Option<AppConfig>,
}

impl LinkerConfig {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, Error> {
        let path = path.as_ref();
        let path = if path.is_dir() {
            path.join("linker.json")
        } else {
            path.to_path_buf()
        };

        let source = fs::read_to_string(&path)?;
        let config: Self = serde_json::from_str(&source)?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), Error> {
        if self.site.title.trim().is_empty() {
            return Err(Error::Linker("site.title must not be empty".to_owned()));
        }
        if self.site.version.trim().is_empty() {
            return Err(Error::Linker("site.version must not be empty".to_owned()));
        }
        if self.lang.trim().is_empty() {
            return Err(Error::Linker("lang must not be empty".to_owned()));
        }
        if self.pages.is_empty() {
            return Err(Error::Linker("linker.json contains no pages".to_owned()));
        }
        if !matches!(self.data, Value::Null | Value::Object(_)) {
            return Err(Error::Linker(
                "`data` must be a JSON object or null".to_owned(),
            ));
        }

        let mut routes = HashSet::new();
        for page in &self.pages {
            if let Some(data) = &page.data {
                if !data.is_object() {
                    return Err(Error::Linker(format!(
                        "page `{}` data must be a JSON object",
                        page.route
                    )));
                }
            }
            if !page.route.starts_with('/') {
                return Err(Error::Linker(format!(
                    "page route `{}` must start with `/`",
                    page.route
                )));
            }
            if page
                .route
                .chars()
                .any(|character| character.is_whitespace())
            {
                return Err(Error::Linker(format!(
                    "page route `{}` must not contain whitespace",
                    page.route
                )));
            }
            if page
                .route
                .chars()
                .any(|character| matches!(character, '{' | '}' | '*' | '?'))
            {
                return Err(Error::Linker(format!(
                    "page route `{}` must be static; SPA routes do not support parameters",
                    page.route
                )));
            }
            if page.file.trim().is_empty() {
                return Err(Error::Linker(format!(
                    "page `{}` has an empty file path",
                    page.route
                )));
            }
            if !routes.insert(page.route.clone()) {
                return Err(Error::Linker(format!(
                    "duplicate page route `{}`",
                    page.route
                )));
            }
        }

        if !routes.contains("/") {
            return Err(Error::Linker(
                "linker.json must contain a `/` page".to_owned(),
            ));
        }

        validate_unique("style", &self.styles)?;
        validate_unique("script", &self.scripts)?;
        validate_unique("asset", &self.assets)?;

        if let Some(output) = &self.output {
            if output.trim().is_empty() {
                return Err(Error::Linker("output must not be empty".to_owned()));
            }
        }
        if let Some(frame) = &self.frame {
            if frame.trim().is_empty() {
                return Err(Error::Linker("frame must not be empty".to_owned()));
            }
        }

        let base = self.base.trim();
        if !base.is_empty()
            && (!base.starts_with('/')
                || base.starts_with("//")
                || base
                    .chars()
                    .any(|character| character.is_whitespace() || matches!(character, '?' | '#')))
        {
            return Err(Error::Linker(
                "base must be empty or a root-relative path like `/rustweb`".to_owned(),
            ));
        }

        if let Some(protocol) = &self.protocol {
            protocol.validate()?;
        }

        if let Some(app) = &self.app {
            AppIdentity::resolve(app)?;
        }

        if self.protocol.is_some() && self.app.is_none() {
            return Err(Error::Linker(
                "a protocol profile requires an app identity (app.name + app.uuid)".to_owned(),
            ));
        }

        Ok(())
    }
}

fn validate_unique(kind: &str, entries: &[String]) -> Result<(), Error> {
    let mut seen = HashSet::new();
    for entry in entries {
        if entry.trim().is_empty() {
            return Err(Error::Linker(format!("empty {kind} path")));
        }
        if !seen.insert(entry) {
            return Err(Error::Linker(format!("duplicate {kind} path `{entry}`")));
        }
    }
    Ok(())
}

impl ProtocolConfig {
    pub fn validate(&self) -> Result<(), Error> {
        if self.name.trim().is_empty()
            || !self.name.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '-' | '+' | '_')
            })
        {
            return Err(Error::Linker(
                "protocol.name must be a non-empty scheme-like identifier".to_owned(),
            ));
        }
        if self.version == 0 {
            return Err(Error::Linker(
                "protocol.version must be greater than zero".to_owned(),
            ));
        }
        if let Some(alias) = &self.alias {
            if alias.trim().is_empty()
                || !alias.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '-' | '+' | '_')
                })
            {
                return Err(Error::Linker(
                    "protocol.alias must be a non-empty scheme-like identifier".to_owned(),
                ));
            }
        }
        if let Some(alt_svc) = &self.alt_svc {
            if alt_svc
                .chars()
                .any(|character| matches!(character, '\r' | '\n'))
            {
                return Err(Error::Linker(
                    "protocol.alt_svc must not contain newlines".to_owned(),
                ));
            }
        }
        for (name, value) in &self.headers {
            if name.trim().is_empty()
                || name
                    .chars()
                    .any(|character| matches!(character, '\r' | '\n' | ':'))
                || value
                    .chars()
                    .any(|character| matches!(character, '\r' | '\n'))
            {
                return Err(Error::Linker(format!("invalid protocol header `{name}`")));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ByrustConfig {
    #[serde(default)]
    pub protocol: Option<ProtocolConfig>,
    #[serde(default)]
    pub app: Option<AppIdentity>,
}

fn default_protocol_name() -> String {
    "byrust".to_owned()
}

fn default_protocol_version() -> u16 {
    1
}

fn default_true() -> bool {
    true
}

fn default_lang() -> String {
    "en".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_uuid_is_stable_and_app_trusted() {
        let config = AppConfig {
            name: "RustWeb".to_owned(),
            uuid: None,
        };
        let first = AppIdentity::resolve(&config).unwrap();
        let second = AppIdentity::resolve(&config).unwrap();

        assert_eq!(first.uuid, second.uuid);
        assert_eq!(first.trust, TrustLevel::App);
        assert!(!first.uuid.is_empty());
    }

    #[test]
    fn universal_uuid_is_low_trust() {
        let identity = AppIdentity::resolve(&AppConfig {
            name: "RustWeb".to_owned(),
            uuid: Some("universal".to_owned()),
        })
        .unwrap();

        assert_eq!(identity.uuid, UNIVERSAL_UUID);
        assert_eq!(identity.trust, TrustLevel::Universal);
    }

    #[test]
    fn invalid_uuid_is_rejected() {
        let error = AppIdentity::resolve(&AppConfig {
            name: "RustWeb".to_owned(),
            uuid: Some("not-a-uuid".to_owned()),
        })
        .unwrap_err();

        assert!(matches!(error, Error::Linker(_)));
    }
}
