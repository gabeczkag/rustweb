use std::path::Path;

use axum::http::{header, HeaderValue};
use axum::routing::get;
use rtml::SitePack;

use crate::RustWeb;

impl RustWeb {
    /// Mount a loaded `.rpack` site: every page becomes a GET route and every
    /// asset is served with its content type.
    pub fn rpack(mut self, pack: SitePack) -> Self {
        self.pack_stats = Some(pack.stats().clone());

        for page in pack.pages() {
            let route = page.route.clone();
            let html = page.html.clone();

            self.router = self.router.route(
                &route,
                get(move || {
                    let html = html.clone();
                    async move {
                        (
                            [(
                                header::CONTENT_TYPE,
                                HeaderValue::from_static("text/html; charset=utf-8"),
                            )],
                            html,
                        )
                    }
                }),
            );
        }

        for asset in pack.assets() {
            let route = asset.path.clone();
            let bytes = asset.bytes.clone();
            let content_type = HeaderValue::from_str(&asset.content_type)
                .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream"));

            self.router = self.router.route(
                &route,
                get(move || {
                    let bytes = bytes.clone();
                    let content_type = content_type.clone();
                    async move { ([(header::CONTENT_TYPE, content_type)], bytes) }
                }),
            );
        }

        self
    }

    /// Load a `.rpack` file and mount it.
    pub fn rpack_file(self, path: impl AsRef<Path>) -> Result<Self, rtml::Error> {
        Ok(self.rpack(SitePack::load(path)?))
    }

    /// Statistics of the mounted pack, if any.
    pub fn pack_stats(&self) -> Option<&rtml::PackStats> {
        self.pack_stats.as_ref()
    }
}
