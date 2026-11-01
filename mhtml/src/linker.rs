use std::collections::HashSet;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

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

fn default_lang() -> String {
    "en".to_owned()
}
