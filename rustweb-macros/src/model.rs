use proc_macro2::TokenStream;

use crate::derive::add_derives;

pub fn expand(item: TokenStream) -> TokenStream {
    add_derives(
        item,
        &[
            ("Serialize", "::serde::Serialize"),
            ("Deserialize", "::serde::Deserialize"),
            ("ToSchema", "::utoipa::ToSchema"),
            ("Validate", "::validator::Validate"),
        ],
    )
}
