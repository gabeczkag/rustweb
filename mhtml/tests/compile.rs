use std::fs;

use mhtml::{build, inspect, BuildOptions};

#[test]
fn compiles_pages_styles_scripts_and_assets_into_one_html_file() {
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir_all(directory.path().join("pages")).unwrap();
    fs::create_dir_all(directory.path().join("assets")).unwrap();
    fs::create_dir_all(directory.path().join("scripts")).unwrap();

    fs::write(
        directory.path().join("linker.json"),
        r#"{
            "site": {"title": "Test", "version": "1.0.0", "description": "A test"},
            "lang": "pl",
            "styles": ["assets/style.css"],
            "scripts": ["scripts/app.js"],
            "assets": ["assets/logo.svg"],
            "pages": [
                {"route": "/", "file": "pages/index.mhtml"},
                {"route": "/about", "file": "pages/about.mhtml"}
            ],
            "data": {"name": "Ada"}
        }"#,
    )
    .unwrap();
    fs::write(
        directory.path().join("pages/index.mhtml"),
        r#"<h1>{{ name }}</h1><img src="/assets/logo.svg">"#,
    )
    .unwrap();
    fs::write(directory.path().join("pages/about.mhtml"), "<p>About</p>").unwrap();
    fs::write(directory.path().join("assets/style.css"), "body{color:red}").unwrap();
    fs::write(
        directory.path().join("assets/logo.svg"),
        "<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>",
    )
    .unwrap();
    fs::write(
        directory.path().join("scripts/app.js"),
        "console.log('hello');",
    )
    .unwrap();

    let bundle = build(
        directory.path().join("linker.json"),
        BuildOptions::default(),
    )
    .unwrap();

    assert_eq!(bundle.stats.pages, 2);
    assert_eq!(bundle.stats.styles, 1);
    assert_eq!(bundle.stats.scripts, 1);
    assert_eq!(bundle.stats.assets, 1);

    let html = &bundle.html;
    assert!(html.contains("<!doctype html>"));
    assert!(html.contains("<template data-rweb-route=\"/\""));
    assert!(html.contains("<template data-rweb-route=\"/about\""));
    assert!(html.contains("<h1>Ada</h1>"));
    assert!(html.contains("data:image/svg+xml;base64,"));
    assert!(html.contains("body{color:red}"));
    assert!(html.contains("console.log('hello');"));
    assert!(html.contains("rweb:rendered"));

    let info = inspect(html);
    assert_eq!(info.title.as_deref(), Some("Test"));
    assert_eq!(info.version.as_deref(), Some("1.0.0"));
    assert_eq!(info.routes.len(), 2);
    assert_eq!(info.routes[0].0, "/");
}
