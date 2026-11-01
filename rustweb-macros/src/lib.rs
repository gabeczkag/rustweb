use proc_macro::TokenStream;
use syn::{parse_macro_input, ItemFn, LitStr};

mod route;

use route::Method;

fn expand(method: Method, attr: TokenStream, item: TokenStream) -> TokenStream {
    let path = parse_macro_input!(attr as LitStr);
    let item = parse_macro_input!(item as ItemFn);

    route::expand(method, path, item)
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
