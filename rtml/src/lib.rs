//! RTML — Rusty Template Markup Language.
//!
//! RTML is an HTML-like template language with `{{ expression }}`
//! interpolation and `@if` / `@for` directives. Templates are parsed,
//! rendered, minified, and packed into a single `.rpack` file by the
//! `rpack` CLI. Runtime serving is just bytes; there is no template work in
//! the request path.

#![forbid(unsafe_code)]

pub mod ast;
pub mod expr;
pub mod pack;
pub mod parser;
pub mod render;

pub use ast::{Attr, BinaryOp, Element, Expr, Node, UnaryOp};
pub use pack::{
    build_dir, AssetEntry, BuildReport, PackStats, PageConfig, PageEntry, SiteConfig, SiteMeta,
    SitePack,
};
pub use parser::parse;
pub use render::render;

/// Errors produced by parsing, rendering, packing, or loading a site pack.
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

    #[error("pack error: {0}")]
    Pack(String),

    #[error("build error: {0}")]
    Build(String),
}
