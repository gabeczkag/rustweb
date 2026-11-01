use proc_macro2::TokenStream;

use crate::derive::add_derives;

pub fn expand(item: TokenStream) -> TokenStream {
    add_derives(
        item,
        &[
            ("Deserialize", "::serde::Deserialize"),
            ("IntoParams", "::utoipa::IntoParams"),
            ("Validate", "::validator::Validate"),
        ],
    )
}
