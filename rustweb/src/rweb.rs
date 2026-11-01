use std::path::Path;

use axum::body::Bytes;
use axum::http::HeaderMap;

use crate::RustWeb;

impl RustWeb {
    /// Set extra headers on the SPA fallback response.
    pub fn spa_headers(mut self, headers: HeaderMap) -> Self {
        self.spa_headers = headers;
        self
    }

    /// Mount a compiled `.rweb` HTML document as a single-page application.
    ///
    /// The document is served as the fallback for every route that is not
    /// handled by an explicit RustWeb route, so `/about` and `/bench` reach
    /// the same bundle and its client-side router.
    pub fn rweb(mut self, html: impl Into<Bytes>) -> Self {
        self.spa = Some(html.into());
        self
    }

    /// Read and mount a compiled `.rweb` file.
    pub fn rweb_file(self, path: impl AsRef<Path>) -> std::io::Result<Self> {
        Ok(self.rweb(Bytes::from(std::fs::read(path)?)))
    }
}
