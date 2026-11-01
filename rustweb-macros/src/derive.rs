use proc_macro2::TokenStream;
use quote::quote;
use syn::punctuated::Punctuated;
use syn::{parse2, Attribute, ItemStruct, Path, Token};

/// Adds missing derive macros to a struct.
///
/// `derives` is a list of `(short name, full path)` pairs. Derives that are
/// already present (matched by their last path segment) are left untouched.
pub fn add_derives(item: TokenStream, derives: &[(&str, &str)]) -> TokenStream {
    let mut item: ItemStruct = match parse2(item) {
        Ok(item) => item,
        Err(error) => return error.to_compile_error(),
    };

    let existing = existing_derives(&item.attrs);
    let mut missing = Vec::new();

    for (name, path) in derives {
        if !existing.iter().any(|existing| existing == name) {
            match syn::parse_str::<Path>(path) {
                Ok(path) => missing.push(path),
                Err(error) => return error.to_compile_error(),
            }
        }
    }

    if !missing.is_empty() {
        let attribute = match syn::parse::Parser::parse2(
            Attribute::parse_outer,
            quote!(#[derive(#(#missing),*)]),
        ) {
            Ok(mut attributes) => attributes.pop().expect("one attribute"),
            Err(error) => return error.to_compile_error(),
        };
        item.attrs.push(attribute);
    }

    quote!(#item)
}

fn existing_derives(attrs: &[Attribute]) -> Vec<String> {
    let mut output = Vec::new();

    for attr in attrs {
        if !attr.path().is_ident("derive") {
            continue;
        }

        let Ok(paths) = attr.parse_args_with(Punctuated::<Path, Token![,]>::parse_terminated)
        else {
            continue;
        };

        for path in paths {
            if let Some(segment) = path.segments.last() {
                output.push(segment.ident.to_string());
            }
        }
    }

    output
}
