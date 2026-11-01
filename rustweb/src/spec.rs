use std::collections::BTreeMap;

use utoipa::openapi::path::{HttpMethod, PathItem, PathsBuilder};
use utoipa::openapi::schema::{ComponentsBuilder, Schema};
use utoipa::openapi::{Info, OpenApi, OpenApiBuilder, OpenApiVersion, RefOr};

use crate::app::RouteMeta;

pub(crate) fn build(title: &str, version: &str, routes: &[RouteMeta]) -> OpenApi {
    let mut paths = PathsBuilder::new();
    let mut schemas: BTreeMap<String, RefOr<Schema>> = BTreeMap::new();

    for meta in routes {
        let method = match meta.method.as_str() {
            "GET" => HttpMethod::Get,
            "POST" => HttpMethod::Post,
            "PUT" => HttpMethod::Put,
            "DELETE" => HttpMethod::Delete,
            "PATCH" => HttpMethod::Patch,
            "HEAD" => HttpMethod::Head,
            "OPTIONS" => HttpMethod::Options,
            "TRACE" => HttpMethod::Trace,
            _ => continue,
        };

        paths = paths.path(meta.path, PathItem::new(method, meta.operation.clone()));

        for (name, schema) in &meta.schemas {
            schemas
                .entry(name.clone())
                .or_insert_with(|| schema.clone());
        }
    }

    let mut components = ComponentsBuilder::new();
    for (name, schema) in schemas {
        components = components.schema(name, schema);
    }

    OpenApiBuilder::new()
        .openapi(OpenApiVersion::Version32)
        .info(Info::new(title, version))
        .paths(paths.build())
        .components(Some(components.build()))
        .build()
}
