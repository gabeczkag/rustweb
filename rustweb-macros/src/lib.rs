#![forbid(unsafe_code)]

use proc_macro::TokenStream;
use syn::{parse_macro_input, ItemFn};

mod derive;
mod model;
mod openapi_codegen;
mod query;
mod route;

use route::Method;

fn expand(method: Method, attr: TokenStream, item: TokenStream) -> TokenStream {
    let item = parse_macro_input!(item as ItemFn);

    let args = match route::parse_args(attr.into()) {
        Ok(args) => args,
        Err(error) => return error.to_compile_error().into(),
    };

    route::expand(method, args, item)
        .unwrap_or_else(|error| error.to_compile_error())
        .into()
}

#[proc_macro_attribute]
pub fn get(attr: TokenStream, item: TokenStream) -> TokenStream {
    expand(Method::Get, attr, item)
}

#[proc_macro_attribute]
pub fn post(attr: TokenStream, item: TokenStream) -> TokenStream {
    expand(Method::Post, attr, item)
}

#[proc_macro_attribute]
pub fn put(attr: TokenStream, item: TokenStream) -> TokenStream {
    expand(Method::Put, attr, item)
}

#[proc_macro_attribute]
pub fn delete(attr: TokenStream, item: TokenStream) -> TokenStream {
    expand(Method::Delete, attr, item)
}

#[proc_macro_attribute]
pub fn patch(attr: TokenStream, item: TokenStream) -> TokenStream {
    expand(Method::Patch, attr, item)
}

#[proc_macro_attribute]
pub fn model(_attr: TokenStream, item: TokenStream) -> TokenStream {
    model::expand(item.into()).into()
}

#[proc_macro_attribute]
pub fn query(_attr: TokenStream, item: TokenStream) -> TokenStream {
    query::expand(item.into()).into()
}
