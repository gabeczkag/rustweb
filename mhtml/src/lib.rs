//! MHTML — mini-HTML for RustWeb.
//!
//! MHTML is an HTML-like template language with `{{ expression }}`
//! interpolation and `@if` / `@for` directives. The `rweb` compiler reads a
//! `linker.json`, renders every page, inlines CSS, TypeScript/JavaScript, and
//! assets, and writes a single self-contained `.rweb` HTML file. RustWeb serves
//! that file with zero template work in the request path.

#![forbid(unsafe_code)]

pub mod ast;
pub mod compile;
pub mod expr;
pub mod linker;
pub mod mcss;
pub mod mscript;
pub mod parser;
pub mod render;

pub use ast::{Attr, BinaryOp, Element, Expr, Node, UnaryOp};
pub use compile::{
    build, build_from_config, inspect, BuildOptions, BundleInfo, BundleStats, CompiledBundle,
};
pub use linker::{
    AppConfig, AppIdentity, ByrustConfig, LinkerConfig, PageConfig, ProtocolConfig, SiteMeta,
    TrustLevel, UNIVERSAL_UUID,
};
pub use parser::parse;
pub use render::render;

/// Errors produced by parsing, rendering, compiling, or inspecting a bundle.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("parse error at byte {offset}: {message}")]
    Parse { offset: usize, message: String },

    #[error("expression error at byte {offset}: {message}")]
    Expr { offset: usize, message: String },

    #[error("linker error: {0}")]
    Linker(String),

    #[error("compile error: {0}")]
    Compile(String),

    #[error("external command failed: {0}")]
    Command(String),
}
