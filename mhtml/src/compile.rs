use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use serde_json::{json, Map, Value};

use crate::linker::{
    AppIdentity, ByrustConfig, LinkerConfig, PageConfig, ProtocolConfig, TrustLevel,
};
use crate::{parser, render, Error};

#[derive(Debug, Clone, Copy)]
pub struct BuildOptions {
    pub minify: bool,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self { minify: true }
    }
}

#[derive(Debug, Clone)]
pub struct CompiledBundle {
    pub html: String,
    pub stats: BundleStats,
}

#[derive(Debug, Clone, Default)]
pub struct BundleStats {
    pub pages: usize,
    pub styles: usize,
    pub scripts: usize,
    pub assets: usize,
    pub source_bytes: usize,
    pub bytes: usize,
}

/// Information extracted from an already compiled `.rweb` file.
#[derive(Debug, Clone)]
pub struct BundleInfo {
    pub title: Option<String>,
    pub version: Option<String>,
    pub protocol: Option<ProtocolConfig>,
    pub app: Option<AppIdentity>,
    pub routes: Vec<(String, Option<String>)>,
    pub bytes: usize,
}

struct Asset {
    paths: Vec<String>,
    uri: String,
}

/// Compile a project described by `linker.json` into one self-contained
/// `.rweb` HTML document.
pub fn build(
    linker_path: impl AsRef<Path>,
    options: BuildOptions,
) -> Result<CompiledBundle, Error> {
    let linker_path = linker_path.as_ref();
    let linker_path = if linker_path.is_dir() {
        linker_path.join("linker.json")
    } else {
        linker_path.to_path_buf()
    };

    let config = LinkerConfig::load(&linker_path)?;
    let root = linker_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    build_from_config(&root, &config, options)
}

pub fn build_from_config(
    root: impl AsRef<Path>,
    config: &LinkerConfig,
    options: BuildOptions,
) -> Result<CompiledBundle, Error> {
    config.validate()?;
    let root = fs::canonicalize(root)?;
    let base = normalize_base(&config.base);
    let app = config
        .app
        .as_ref()
        .map(|app| AppIdentity::resolve_with_seed(app, &root.to_string_lossy()))
        .transpose()?;
    let mut source_bytes = 0usize;

    let mut assets = Vec::new();
    for asset in &config.assets {
        let path = resolve_path(&root, asset)?;
        let bytes = fs::read(&path)?;
        source_bytes += bytes.len();
        let mime = mime_guess::from_path(&path).first_or_octet_stream();
        let uri = format!("data:{mime};base64,{}", BASE64.encode(&bytes));
        let relative = asset.trim_start_matches('/').replace('\\', "/");
        assets.push(Asset {
            paths: vec![format!("/{relative}"), relative],
            uri,
        });
    }

    let frame = if let Some(frame) = &config.frame {
        let path = resolve_path(&root, frame)?;
        let source = fs::read_to_string(&path)?;
        source_bytes += source.len();
        let nodes = parser::parse(&source)?;
        let context = shell_context(config, app.as_ref());
        let mut rendered = String::new();
        render::render(&nodes, &context, &mut rendered, options.minify);
        let rendered = inline_html_assets(&rendered, &assets);
        let rendered = apply_base(&rendered, &base);
        if !rendered.contains("id=\"rweb-app\"") && !rendered.contains("id='rweb-app'") {
            return Err(Error::Compile(
                "frame must contain a container with id=\"rweb-app\"".to_owned(),
            ));
        }
        rendered
    } else {
        "<div id=\"rweb-app\"></div>".to_owned()
    };

    let mut css = String::new();
    for style in &config.styles {
        let path = resolve_path(&root, style)?;
        let source = fs::read_to_string(&path)?;
        source_bytes += source.len();
        css.push_str(&inline_css_assets(&source, &assets));
        css.push('\n');
    }

    css.push_str(
        ".byrust-warning{position:sticky;top:0;z-index:9999;padding:10px 16px;\
         background:#7f1d1d;color:#fff;font:600 13px/1.4 system-ui,sans-serif;\
         text-align:center}\n",
    );
    let css = if options.minify {
        minify_css(&css)
    } else {
        css
    };

    let mut classic_scripts = String::new();
    let mut module_scripts = String::new();
    for (index, script) in config.scripts.iter().enumerate() {
        let path = resolve_path(&root, script)?;
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();

        let source = match extension.as_str() {
            "ts" => transpile_typescript(&path, index)?,
            "js" | "cjs" | "mjs" => fs::read_to_string(&path)?,
            other => {
                return Err(Error::Compile(format!(
                    "unsupported script extension `.{other}`: {}",
                    path.display()
                )));
            }
        };

        source_bytes += source.len();
        let source = source.replace("</script", "<\\/script");

        if extension == "mjs" {
            module_scripts.push_str(&source);
            module_scripts.push('\n');
        } else {
            classic_scripts.push_str(&source);
            classic_scripts.push('\n');
        }
    }

    let mut templates = String::new();
    let mut home_title = config.site.title.clone();

    for page in &config.pages {
        let path = resolve_path(&root, &page.file)?;
        let source = fs::read_to_string(&path)?;
        source_bytes += source.len();

        let nodes = parser::parse(&source)?;
        let context = page_context(config, page, app.as_ref());
        let mut rendered = String::new();
        render::render(&nodes, &context, &mut rendered, options.minify);
        let rendered = inline_html_assets(&rendered, &assets);
        let rendered = apply_base(&rendered, &base);
        let (body, embedded_title) = extract_body_and_title(&rendered);
        let title = page
            .title
            .clone()
            .or(embedded_title)
            .unwrap_or_else(|| config.site.title.clone());

        if page.route == "/" {
            home_title = title.clone();
        }

        templates.push_str("<template data-rweb-route=\"");
        templates.push_str(&escape_html(&page.route));
        templates.push_str("\" data-rweb-title=\"");
        templates.push_str(&escape_html(&title));
        templates.push_str("\">");
        templates.push_str(&body);
        templates.push_str("</template>\n");
    }

    let mut byrust_meta = String::new();
    if let Some(protocol) = &config.protocol {
        byrust_meta.push_str(&format!(
            "<meta name=\"byrust\" content=\"{}\">\n\
<meta name=\"byrust-alias\" content=\"{}\">\n",
            escape_html(&protocol.name),
            escape_html(protocol.alias.as_deref().unwrap_or("")),
        ));
    }
    if config.protocol.is_some() || app.is_some() {
        let config_json = ByrustConfig {
            protocol: config.protocol.clone(),
            app: app.clone(),
        };
        let json = serde_json::to_string(&config_json)?.replace("</", "<\\/");
        byrust_meta.push_str(&format!(
            "<script type=\"application/json\" id=\"byrust-config\">{json}</script>\n"
        ));
    }

    let warning = if app
        .as_ref()
        .is_some_and(|app| app.trust == TrustLevel::Universal)
    {
        "<div class=\"byrust-warning\" role=\"alert\">Page is not safe — universal Byrust UUID</div>\n"
    } else {
        ""
    };

    let mut script_tags = String::new();
    if !classic_scripts.is_empty() {
        script_tags.push_str("<script>");
        script_tags.push_str(&classic_scripts);
        script_tags.push_str("</script>\n");
    }
    if !module_scripts.is_empty() {
        script_tags.push_str("<script type=\"module\">");
        script_tags.push_str(&module_scripts);
        script_tags.push_str("</script>\n");
    }
    script_tags.push_str("<script>");
    script_tags.push_str(ROUTER_JS);
    script_tags.push_str("</script>\n");

    let html = format!(
        "<!doctype html>\n\
<html lang=\"{lang}\">\n\
<head>\n\
<meta charset=\"utf-8\">\n\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
<meta name=\"rweb-version\" content=\"{version}\">\n\
<meta name=\"rweb-base\" content=\"{base}\">\n\
<meta name=\"description\" content=\"{description}\">\n\
{byrust_meta}\
<title>{title}</title>\n\
<style>{css}</style>\n\
</head>\n\
<body>\n\
{warning}{frame}\n\
<noscript>MHTML needs JavaScript.</noscript>\n\
{templates}{script_tags}</body>\n\
</html>\n",
        lang = escape_html(&config.lang),
        base = escape_html(&base),
        byrust_meta = byrust_meta,
        version = escape_html(&config.site.version),
        description = escape_html(&config.site.description),
        title = escape_html(&home_title),
        css = css,
        warning = warning,
        frame = frame,
        templates = templates,
        script_tags = script_tags,
    );

    let html = if config.ascii {
        to_ascii_html(&html)
    } else {
        html
    };

    let stats = BundleStats {
        pages: config.pages.len(),
        styles: config.styles.len(),
        scripts: config.scripts.len(),
        assets: config.assets.len(),
        source_bytes,
        bytes: html.len(),
    };

    Ok(CompiledBundle { html, stats })
}

/// Inspect a compiled `.rweb` HTML file.
pub fn inspect(html: &str) -> BundleInfo {
    let title = extract_tag_text(html, "title");
    let version = extract_meta(html, "rweb-version");
    let mut routes = Vec::new();

    for (start, _) in html.match_indices("<template") {
        let Some(open_end) = html[start..].find('>').map(|offset| start + offset) else {
            continue;
        };
        let tag = &html[start..open_end];
        if let Some(route) = attribute_value(tag, "data-rweb-route") {
            routes.push((route, attribute_value(tag, "data-rweb-title")));
        }
    }

    let byrust: Option<ByrustConfig> = extract_script_json(html, "byrust-config")
        .and_then(|source| serde_json::from_str(&source).ok());

    BundleInfo {
        title,
        version,
        protocol: byrust.as_ref().and_then(|config| config.protocol.clone()),
        app: byrust.and_then(|config| config.app),
        routes,
        bytes: html.len(),
    }
}

fn normalize_base(base: &str) -> String {
    let base = base.trim();
    if base == "/" {
        return String::new();
    }
    base.trim_end_matches('/').to_owned()
}

fn apply_base(source: &str, base: &str) -> String {
    if base.is_empty() {
        return source.to_owned();
    }

    source
        .replace("href=\"/", &format!("href=\"{base}/"))
        .replace("src=\"/", &format!("src=\"{base}/"))
}

fn shell_context(config: &LinkerConfig, app: Option<&AppIdentity>) -> Value {
    let mut map = match &config.data {
        Value::Object(map) => map.clone(),
        _ => Map::new(),
    };

    map.insert(
        "site".to_owned(),
        serde_json::to_value(&config.site).unwrap_or(Value::Null),
    );
    map.insert("title".to_owned(), Value::String(config.site.title.clone()));
    if let Some(app) = app {
        map.insert(
            "app".to_owned(),
            serde_json::to_value(app).unwrap_or(Value::Null),
        );
    }

    Value::Object(map)
}

fn minify_css(source: &str) -> String {
    let mut output = String::with_capacity(source.len());
    let mut characters = source.chars().peekable();
    let mut quote: Option<char> = None;
    let mut in_comment = false;
    let mut pending_space = false;

    while let Some(character) = characters.next() {
        if in_comment {
            if character == '*' && characters.peek() == Some(&'/') {
                characters.next();
                in_comment = false;
            }
            continue;
        }

        if let Some(active_quote) = quote {
            output.push(character);
            if character == '\\' {
                if let Some(next) = characters.next() {
                    output.push(next);
                }
            } else if character == active_quote {
                quote = None;
            }
            continue;
        }

        if character == '/' && characters.peek() == Some(&'*') {
            characters.next();
            in_comment = true;
            continue;
        }

        if matches!(character, '"' | '\'') {
            if pending_space && needs_css_space(output.chars().last(), character) {
                output.push(' ');
            }
            pending_space = false;
            quote = Some(character);
            output.push(character);
            continue;
        }

        if character.is_whitespace() {
            pending_space = true;
            continue;
        }

        if pending_space {
            if needs_css_space(output.chars().last(), character) {
                output.push(' ');
            }
            pending_space = false;
        }

        output.push(character);
    }

    output.replace(";}", "}")
}

fn needs_css_space(previous: Option<char>, next: char) -> bool {
    let Some(previous) = previous else {
        return false;
    };

    if matches!(previous, '{' | '}' | ';' | ',' | ':') {
        return false;
    }
    if matches!(next, '{' | '}' | ';' | ',' | ':') {
        return false;
    }

    true
}

fn to_ascii_html(source: &str) -> String {
    source
        .chars()
        .map(|character| {
            if character.is_ascii() {
                return character;
            }

            match character {
                'ą' => 'a',
                'ć' => 'c',
                'ę' => 'e',
                'ł' => 'l',
                'ń' => 'n',
                'ó' => 'o',
                'ś' => 's',
                'ź' => 'z',
                'ż' => 'z',
                'Ą' => 'A',
                'Ć' => 'C',
                'Ę' => 'E',
                'Ł' => 'L',
                'Ń' => 'N',
                'Ó' => 'O',
                'Ś' => 'S',
                'Ź' => 'Z',
                'Ż' => 'Z',
                '—' | '–' | '·' => '-',
                '„' | '”' | '“' => '"',
                '’' => '\'',
                '…' => '.',
                _ => '?',
            }
        })
        .collect()
}

fn page_context(config: &LinkerConfig, page: &PageConfig, app: Option<&AppIdentity>) -> Value {
    let mut map = match &config.data {
        Value::Object(map) => map.clone(),
        _ => Map::new(),
    };

    if let Some(Value::Object(extra)) = &page.data {
        for (key, value) in extra {
            map.insert(key.clone(), value.clone());
        }
    }

    let title = page
        .title
        .clone()
        .unwrap_or_else(|| config.site.title.clone());

    map.insert(
        "site".to_owned(),
        serde_json::to_value(&config.site).unwrap_or(Value::Null),
    );
    map.insert(
        "page".to_owned(),
        json!({
            "route": &page.route,
            "title": &title,
            "description": &page.description,
        }),
    );
    map.insert("title".to_owned(), Value::String(title));
    if let Some(app) = app {
        map.insert(
            "app".to_owned(),
            serde_json::to_value(app).unwrap_or(Value::Null),
        );
    }

    Value::Object(map)
}

fn resolve_path(root: &Path, relative: &str) -> Result<PathBuf, Error> {
    let path = fs::canonicalize(root.join(relative))?;
    if !path.starts_with(root) {
        return Err(Error::Linker(format!(
            "path `{relative}` escapes the project directory"
        )));
    }
    Ok(path)
}

fn inline_html_assets(source: &str, assets: &[Asset]) -> String {
    let mut output = source.to_owned();

    for asset in assets {
        for path in &asset.paths {
            for attribute in ["src", "href", "poster"] {
                output = output.replace(
                    &format!("{attribute}=\"{path}\""),
                    &format!("{attribute}=\"{}\"", asset.uri),
                );
                output = output.replace(
                    &format!("{attribute}='{path}'"),
                    &format!("{attribute}='{}'", asset.uri),
                );
            }

            output = inline_css_urls(&output, path, &asset.uri);
        }
    }

    output
}

fn inline_css_assets(source: &str, assets: &[Asset]) -> String {
    let mut output = source.to_owned();
    for asset in assets {
        for path in &asset.paths {
            output = inline_css_urls(&output, path, &asset.uri);
        }
    }
    output
}

fn inline_css_urls(source: &str, path: &str, uri: &str) -> String {
    source
        .replace(&format!("url({path})"), &format!("url({uri})"))
        .replace(&format!("url(\"{path}\")"), &format!("url({uri})"))
        .replace(&format!("url('{path}')"), &format!("url({uri})"))
}

fn extract_body_and_title(html: &str) -> (String, Option<String>) {
    let lower = html.to_ascii_lowercase();

    let title = lower
        .find("<title")
        .and_then(|start| {
            let open_end = lower[start..].find('>')? + start;
            let end = lower[open_end + 1..].find("</title>")? + open_end + 1;
            Some(html[open_end + 1..end].trim().to_owned())
        })
        .filter(|title| !title.is_empty());

    let body = lower.find("<body").and_then(|start| {
        let open_end = lower[start..].find('>')? + start;
        let end = lower[open_end + 1..].find("</body>")? + open_end + 1;
        Some(html[open_end + 1..end].to_owned())
    });

    (body.unwrap_or_else(|| html.to_owned()), title)
}

fn transpile_typescript(path: &Path, index: usize) -> Result<String, Error> {
    let output_dir = std::env::temp_dir().join(format!("rweb-ts-{}-{index}", std::process::id()));
    let _ = fs::remove_dir_all(&output_dir);
    fs::create_dir_all(&output_dir)?;

    let status = Command::new("tsc")
        .arg("--target")
        .arg("ES2020")
        .arg("--module")
        .arg("commonjs")
        .arg("--skipLibCheck")
        .arg("--outDir")
        .arg(&output_dir)
        .arg(path)
        .status()
        .map_err(|error| {
            Error::Command(format!(
                "failed to run `tsc`: {error}; install TypeScript or use a .js script"
            ))
        })?;

    if !status.success() {
        let _ = fs::remove_dir_all(&output_dir);
        return Err(Error::Compile(format!(
            "tsc failed for {} (script-style TypeScript only; use .js modules for imports)",
            path.display()
        )));
    }

    let stem = path.file_stem().ok_or_else(|| {
        Error::Compile(format!("TypeScript file has no stem: {}", path.display()))
    })?;
    let output = output_dir.join(stem).with_extension("js");
    let source = fs::read_to_string(&output)?;
    let _ = fs::remove_dir_all(&output_dir);
    Ok(source)
}

fn extract_tag_text(html: &str, tag: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let open_tag = format!("<{tag}");
    let close_tag = format!("</{tag}>");
    let start = lower.find(&open_tag)?;
    let open_end = lower[start..].find('>')? + start;
    let end = lower[open_end + 1..].find(&close_tag)? + open_end + 1;
    Some(html[open_end + 1..end].trim().to_owned())
}

fn extract_meta(html: &str, name: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let needle = format!("<meta name=\"{}\"", name.to_ascii_lowercase());
    let start = lower.find(&needle)?;
    let tag_end = lower[start..].find('>')? + start;
    attribute_value(&html[start..tag_end], "content")
}

fn extract_script_json(html: &str, id: &str) -> Option<String> {
    let needle = format!("<script type=\"application/json\" id=\"{id}\">");
    let start = html.find(&needle)? + needle.len();
    let end = html[start..].find("</script>")? + start;
    Some(html[start..end].to_owned())
}

fn attribute_value(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let needle = format!("{}=\"", name.to_ascii_lowercase());
    let start = lower.find(&needle)? + needle.len();
    let end = tag[start..].find('"')? + start;
    Some(tag[start..end].to_owned())
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

const ROUTER_JS: &str = r#"(()=>{const t=new Map;document.querySelectorAll('template[data-rweb-route]').forEach(e=>t.set(e.dataset.rwebRoute,e));const base=document.querySelector('meta[name="rweb-base"]')?.content||'',a=document.getElementById('rweb-app'),f=t.get('/'),n=e=>{if(!e)e='/';if(base&&e.startsWith(base))e=e.slice(base.length)||'/';return e==='/'?'/':e.replace(/\/+$/,'')||'/'};const has=!!document.querySelector('meta[name="byrust"]'),al=document.querySelector('meta[name="byrust-alias"]')?.content||'byrust',sc=`${al}:`,ws=`web+${al}:`;const render=p=>{const q=n(p||location.pathname),x=t.get(q)||f;if(!x||!a)return;a.replaceChildren(x.content.cloneNode(!0));document.title=x.dataset.rwebTitle||document.title;document.dispatchEvent(new CustomEvent('rweb:rendered',{detail:{path:q}}));scrollTo(0,0)};const parse=u=>{try{const p=new URL(u);if(p.protocol!==sc&&p.protocol!==ws)return null;if(p.hostname&&p.hostname!=='self'&&p.hostname!==location.hostname)return null;return p}catch{return null}};if(has){try{const e=new URLSearchParams(location.search).get(al),p=e?parse(e):null;if(p){const u=base+p.pathname+p.search+p.hash;history.replaceState({},'',u);render(p.pathname)}}catch{}if(navigator.registerProtocolHandler){try{navigator.registerProtocolHandler(ws.slice(0,-1),`${location.origin}${base||''}/?${encodeURIComponent(al)}=%s`)}catch{}}}document.addEventListener('click',e=>{if(e.defaultPrevented||e.button!==0||e.metaKey||e.ctrlKey||e.shiftKey||e.altKey)return;const c=e.target.closest('a');if(!c||c.target||c.hasAttribute('download'))return;const h=c.getAttribute('href')||'';if(has&&(h.startsWith(`${al}://`)||h.startsWith(`web+${al}://`))){const p=parse(h);if(p){e.preventDefault();history.pushState({},'',base+p.pathname+p.search+p.hash);render(p.pathname)}return}const u=new URL(c.href,location.href);if(u.origin!==location.origin||u.hash)return;const q=n(u.pathname);if(q.startsWith('/api/')||q.startsWith('/docs')||q==='/openapi.json')return;e.preventDefault();history.pushState({},'',u);render()});addEventListener('popstate',()=>render());render()})();"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_body_and_title() {
        let (body, title) = extract_body_and_title(
            "<html><head><title>Hello</title></head><body><h1>Hi</h1></body></html>",
        );
        assert_eq!(body, "<h1>Hi</h1>");
        assert_eq!(title.as_deref(), Some("Hello"));
    }

    #[test]
    fn inlines_asset_attributes_but_not_code_text() {
        let assets = vec![Asset {
            paths: vec!["/assets/logo.svg".to_owned(), "assets/logo.svg".to_owned()],
            uri: "data:image/svg+xml;base64,AAA".to_owned(),
        }];
        let output = inline_html_assets(
            r#"<img src="/assets/logo.svg"><code>"assets/logo.svg"</code>"#,
            &assets,
        );
        assert!(output.contains(r#"src="data:image/svg+xml;base64,AAA""#));
        assert!(output.contains(r#""assets/logo.svg""#));
    }

    #[test]
    fn inlines_css_urls() {
        let assets = vec![Asset {
            paths: vec!["/assets/logo.svg".to_owned()],
            uri: "data:image/svg+xml;base64,AAA".to_owned(),
        }];
        assert_eq!(
            inline_css_assets("body{background:url(/assets/logo.svg)}", &assets),
            "body{background:url(data:image/svg+xml;base64,AAA)}"
        );
    }
}
