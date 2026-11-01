use std::collections::HashSet;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use bytes::Bytes;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use walkdir::WalkDir;

use crate::parser;
use crate::render;
use crate::Error;

const MAGIC: &[u8; 7] = b"RPACK1\n";
const FORMAT_VERSION: u32 = 1;
const HEADER_LEN: usize = 7 + 4 + 4 + 4;

/// Site metadata stored in `site.json` and in the pack manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiteMeta {
    pub title: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiteConfig {
    pub site: SiteMeta,
    #[serde(default)]
    pub pages: Vec<PageConfig>,
    #[serde(default)]
    pub data: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageConfig {
    pub route: String,
    pub source: String,
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Manifest {
    format: u32,
    site: SiteMeta,
    pages: Vec<PageRecord>,
    assets: Vec<AssetRecord>,
    stats: PackStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PageRecord {
    route: String,
    title: Option<String>,
    offset: usize,
    len: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AssetRecord {
    path: String,
    content_type: String,
    offset: usize,
    len: usize,
}

/// Statistics about a built or loaded site package.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PackStats {
    pub pages: usize,
    pub assets: usize,
    pub source_bytes: usize,
    pub html_bytes: usize,
    pub asset_bytes: usize,
    pub unpacked_bytes: usize,
    pub packed_bytes: usize,
}

/// A pre-rendered page loaded from a `.rpack` file.
#[derive(Debug, Clone)]
pub struct PageEntry {
    pub route: String,
    pub title: Option<String>,
    pub html: Bytes,
}

/// A static asset loaded from a `.rpack` file.
#[derive(Debug, Clone)]
pub struct AssetEntry {
    pub path: String,
    pub content_type: String,
    pub bytes: Bytes,
}

/// A loaded site package: metadata, pages, and assets.
#[derive(Debug, Clone)]
pub struct SitePack {
    site: SiteMeta,
    pages: Vec<PageEntry>,
    assets: Vec<AssetEntry>,
    stats: PackStats,
}

/// Result of building a `.rpack` file.
#[derive(Debug, Clone)]
pub struct BuildReport {
    pub stats: PackStats,
    pub output: PathBuf,
}

impl SitePack {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, Error> {
        let data = fs::read(path)?;
        Self::from_bytes(&data)
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, Error> {
        if data.len() < HEADER_LEN {
            return Err(Error::Pack("file is shorter than the header".to_owned()));
        }

        if &data[..MAGIC.len()] != MAGIC {
            return Err(Error::Pack("invalid magic bytes".to_owned()));
        }

        let version = read_u32(data, MAGIC.len())?;
        if version != FORMAT_VERSION {
            return Err(Error::Pack(format!("unsupported format version {version}")));
        }

        let manifest_len = read_u32(data, MAGIC.len() + 4)? as usize;
        let compressed_len = read_u32(data, MAGIC.len() + 8)? as usize;
        let manifest_start = HEADER_LEN;
        let manifest_end = manifest_start
            .checked_add(manifest_len)
            .ok_or_else(|| Error::Pack("manifest length overflow".to_owned()))?;
        let compressed_end = manifest_end
            .checked_add(compressed_len)
            .ok_or_else(|| Error::Pack("payload length overflow".to_owned()))?;

        if compressed_end > data.len() {
            return Err(Error::Pack("truncated pack".to_owned()));
        }

        let manifest: Manifest = serde_json::from_slice(&data[manifest_start..manifest_end])?;
        if manifest.format != FORMAT_VERSION {
            return Err(Error::Pack(format!(
                "manifest format {} is not supported",
                manifest.format
            )));
        }

        let mut decoder = GzDecoder::new(&data[manifest_end..compressed_end]);
        let mut payload = Vec::new();
        decoder.read_to_end(&mut payload)?;

        let pages = manifest
            .pages
            .into_iter()
            .map(|record| {
                Ok(PageEntry {
                    route: record.route,
                    title: record.title,
                    html: slice_entry(&payload, record.offset, record.len)?,
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;

        let assets = manifest
            .assets
            .into_iter()
            .map(|record| {
                Ok(AssetEntry {
                    path: record.path,
                    content_type: record.content_type,
                    bytes: slice_entry(&payload, record.offset, record.len)?,
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;

        let mut stats = manifest.stats;
        stats.unpacked_bytes = payload.len();
        stats.packed_bytes = data.len();

        Ok(Self {
            site: manifest.site,
            pages,
            assets,
            stats,
        })
    }

    pub fn site(&self) -> &SiteMeta {
        &self.site
    }

    pub fn pages(&self) -> &[PageEntry] {
        &self.pages
    }

    pub fn assets(&self) -> &[AssetEntry] {
        &self.assets
    }

    pub fn stats(&self) -> &PackStats {
        &self.stats
    }
}

/// Build a `.rpack` package from a site directory.
///
/// The directory must contain `site.json`; pages are RTML files listed in
/// it, and every file under `assets/` is packed as a static asset.
pub fn build_dir(site_dir: &Path, output: &Path) -> Result<BuildReport, Error> {
    let root = fs::canonicalize(site_dir)?;
    let config: SiteConfig = serde_json::from_slice(&fs::read(root.join("site.json"))?)?;

    if config.pages.is_empty() {
        return Err(Error::Build("site.json contains no pages".to_owned()));
    }

    let mut payload = Vec::new();
    let mut pages = Vec::new();
    let mut routes = HashSet::new();
    let mut source_bytes = 0usize;
    let mut html_bytes = 0usize;

    for page in &config.pages {
        validate_route(&page.route)?;
        if !routes.insert(page.route.clone()) {
            return Err(Error::Build(format!(
                "duplicate page route `{}`",
                page.route
            )));
        }

        let source_path = safe_join(&root, &page.source)?;
        let source = fs::read_to_string(&source_path)?;
        source_bytes += source.len();

        let nodes = parser::parse(&source)?;
        let context = page_context(&config, page);
        let mut html = String::new();
        render::render(&nodes, &context, &mut html, true);

        let offset = payload.len();
        payload.extend_from_slice(html.as_bytes());
        html_bytes += html.len();

        pages.push(PageRecord {
            route: page.route.clone(),
            title: page.title.clone(),
            offset,
            len: html.len(),
        });
    }

    let mut assets = Vec::new();
    let mut asset_paths = HashSet::new();
    let assets_dir = root.join("assets");

    if assets_dir.exists() {
        for entry in WalkDir::new(&assets_dir) {
            let entry = entry.map_err(|error| Error::Build(error.to_string()))?;
            if !entry.file_type().is_file() {
                continue;
            }

            let path = entry.path();
            let relative = path.strip_prefix(&root).map_err(|error| {
                Error::Build(format!("asset path outside the site directory: {error}"))
            })?;
            let route = format!("/{}", relative.to_string_lossy().replace('\\', "/"));

            if !asset_paths.insert(route.clone()) {
                return Err(Error::Build(format!("duplicate asset path `{route}`")));
            }

            let bytes = fs::read(path)?;
            let content_type = mime_guess::from_path(path)
                .first_or_octet_stream()
                .to_string();
            let offset = payload.len();
            payload.extend_from_slice(&bytes);

            assets.push(AssetRecord {
                path: route,
                content_type,
                offset,
                len: bytes.len(),
            });
        }
    }

    let asset_bytes = assets.iter().map(|asset| asset.len).sum();
    let mut stats = PackStats {
        pages: pages.len(),
        assets: assets.len(),
        source_bytes,
        html_bytes,
        asset_bytes,
        unpacked_bytes: payload.len(),
        packed_bytes: 0,
    };

    let manifest = Manifest {
        format: FORMAT_VERSION,
        site: config.site,
        pages,
        assets,
        stats: stats.clone(),
    };
    let manifest_bytes = serde_json::to_vec(&manifest)?;

    let mut encoder = GzEncoder::new(Vec::new(), Compression::best());
    encoder.write_all(&payload)?;
    let compressed = encoder.finish()?;

    let mut pack = Vec::with_capacity(HEADER_LEN + manifest_bytes.len() + compressed.len());
    pack.extend_from_slice(MAGIC);
    pack.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    pack.extend_from_slice(&(manifest_bytes.len() as u32).to_le_bytes());
    pack.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
    pack.extend_from_slice(&manifest_bytes);
    pack.extend_from_slice(&compressed);

    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::write(output, &pack)?;

    stats.packed_bytes = pack.len();

    Ok(BuildReport {
        stats,
        output: output.to_path_buf(),
    })
}

fn page_context(config: &SiteConfig, page: &PageConfig) -> Value {
    let mut map = match &config.data {
        Value::Object(map) => map.clone(),
        _ => Map::new(),
    };

    let site = serde_json::to_value(&config.site).unwrap_or(Value::Null);
    let title = page
        .title
        .clone()
        .unwrap_or_else(|| config.site.title.clone());

    map.insert("site".to_owned(), site);
    map.insert(
        "page".to_owned(),
        json!({"route": &page.route, "title": &page.title}),
    );
    map.insert("title".to_owned(), Value::String(title));

    Value::Object(map)
}

fn validate_route(route: &str) -> Result<(), Error> {
    if !route.starts_with('/') {
        return Err(Error::Build(format!(
            "page route `{route}` must start with `/`"
        )));
    }
    if route.chars().any(char::is_whitespace) {
        return Err(Error::Build(format!(
            "page route `{route}` must not contain whitespace"
        )));
    }
    if route
        .chars()
        .any(|character| matches!(character, '{' | '}' | '*' | '?'))
    {
        return Err(Error::Build(format!(
            "page route `{route}` must be static; dynamic segments are not supported in .rpack pages"
        )));
    }
    Ok(())
}

fn safe_join(root: &Path, relative: &str) -> Result<PathBuf, Error> {
    let joined = fs::canonicalize(root.join(relative))?;
    if !joined.starts_with(root) {
        return Err(Error::Build(format!(
            "source `{relative}` escapes the site directory"
        )));
    }
    Ok(joined)
}

fn slice_entry(payload: &[u8], offset: usize, len: usize) -> Result<Bytes, Error> {
    let end = offset
        .checked_add(len)
        .ok_or_else(|| Error::Pack("entry length overflow".to_owned()))?;
    let bytes = payload
        .get(offset..end)
        .ok_or_else(|| Error::Pack("entry points outside the payload".to_owned()))?;
    Ok(Bytes::copy_from_slice(bytes))
}

fn read_u32(data: &[u8], offset: usize) -> Result<u32, Error> {
    let bytes = data
        .get(offset..offset + 4)
        .ok_or_else(|| Error::Pack("truncated header".to_owned()))?;
    Ok(u32::from_le_bytes(bytes.try_into().expect("four bytes")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_and_loads_a_pack() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(
            directory.path().join("site.json"),
            r#"{
                "site": {"title": "Test", "version": "1.0.0"},
                "pages": [{"route": "/", "source": "index.rtml"}],
                "data": {"name": "Ada"}
            }"#,
        )
        .unwrap();
        fs::write(directory.path().join("index.rtml"), "<h1>{{ name }}</h1>").unwrap();
        fs::create_dir(directory.path().join("assets")).unwrap();
        fs::write(directory.path().join("assets/app.css"), "body{}").unwrap();

        let output = directory.path().join("site.rpack");
        let report = build_dir(directory.path(), &output).unwrap();
        assert_eq!(report.stats.pages, 1);
        assert_eq!(report.stats.assets, 1);
        assert!(report.stats.packed_bytes > 0);

        let pack = SitePack::load(&output).unwrap();
        assert_eq!(pack.site().title, "Test");
        assert_eq!(pack.pages()[0].route, "/");
        assert_eq!(&pack.pages()[0].html[..], b"<h1>Ada</h1>");
        assert_eq!(pack.assets()[0].content_type, "text/css");
        assert_eq!(pack.assets()[0].path, "/assets/app.css");
    }
}
