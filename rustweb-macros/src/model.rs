use proc_macro2::TokenStream;
use quote::quote;
use syn::punctuated::Punctuated;
use syn::{parse2, Attribute, ItemStruct, Path, Token};

const DERIVES: [&str; 4] = [
    "Serialize",
    "Deserialize",
    "ToSchema",
    "Validate",
];

pub fn expand(item: TokenStream) -> TokenStream {
    let mut item: ItemStruct = match parse2(item) {
        Ok(item) => item,
        Err(error) => return error.to_compile_error(),
    };

    let existing = existing_derives(&item.attrs);
    let mut missing = Vec::new();

    for name in DERIVES {
        if !existing.iter().any(|existing| existing == name) {
            let path: Path = syn::parse_str(match name {
                "Serialize" => "::serde::Serialize",
                "Deserialize" => "::serde::Deserialize",
                "ToSchema" => "::utoipa::ToSchema",
                "Validate" => "::validator::Validate",
                _ => unreachable!(),
            })
            .expect("valid derive path");
            missing.push(path);
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
