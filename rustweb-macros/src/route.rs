use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{Error, FnArg, ItemFn, LitStr, ReturnType, Type};

#[derive(Clone, Copy)]
pub enum Method {
    Get,
    Post,
    Put,
    Delete,
    Patch,
}

impl Method {
    fn http_method(self) -> TokenStream2 {
        match self {
            Method::Get => quote!(::rustweb::axum::http::Method::GET),
            Method::Post => quote!(::rustweb::axum::http::Method::POST),
            Method::Put => quote!(::rustweb::axum::http::Method::PUT),
            Method::Delete => quote!(::rustweb::axum::http::Method::DELETE),
            Method::Patch => quote!(::rustweb::axum::http::Method::PATCH),
        }
    }

    fn axum_fn(self) -> TokenStream2 {
        match self {
            Method::Get => quote!(get),
            Method::Post => quote!(post),
            Method::Put => quote!(put),
            Method::Delete => quote!(delete),
            Method::Patch => quote!(patch),
        }
    }
}

pub fn expand(method: Method, path: LitStr, item: ItemFn) -> syn::Result<TokenStream2> {
    if item.sig.asyncness.is_none() {
        return Err(Error::new_spanned(
            &item.sig.ident,
            "rustweb handlers must be async",
        ));
    }
    if !item.sig.generics.params.is_empty() {
        return Err(Error::new_spanned(
            &item.sig.generics,
            "rustweb handlers cannot be generic",
        ));
    }

    let ItemFn {
        attrs,
        vis,
        sig,
        block,
    } = item;
    let fn_ident = sig.ident;
    let output = sig.output;
    let status = status_expr(200);
    let is_result = is_result(&output);

    let mut wrapper_inputs = Vec::new();
    let mut arg_names = Vec::new();
    let mut prologue = Vec::new();

    for (index, arg) in sig.inputs.iter().enumerate() {
        match arg {
            FnArg::Receiver(receiver) => {
                return Err(Error::new_spanned(
                    receiver,
                    "rustweb handlers cannot take self",
                ));
            }
            FnArg::Typed(pat_type) => {
                let ty = &pat_type.ty;
                let pat = &pat_type.pat;
                let ident = format_ident!("__rustweb_arg_{}", index);
                wrapper_inputs.push(quote! { #ident: #ty });
                arg_names.push(quote! { #ident });
                prologue.push(quote! { let #pat = #ident; });
            }
        }
    }

    let struct_attrs = attrs.iter().filter(|attr| {
        attr.path().is_ident("doc")
            || attr.path().is_ident("cfg")
            || attr.path().is_ident("cfg_attr")
    });

    let body_call = quote! { Self::__rustweb_body(#(#arg_names),*).await };
    let wrapper_body = if is_result {
        quote! {
            match #body_call {
                Ok(value) => (#status, value).into_response(),
                Err(error) => error.into_response(),
            }
        }
    } else {
        quote! {
            (#status, #body_call).into_response()
        }
    };

    let axum_fn = method.axum_fn();
    let http_method = method.http_method();

    Ok(quote! {
        #(#struct_attrs)*
        #[allow(non_camel_case_types)]
        #[derive(Clone, Copy)]
        #vis struct #fn_ident;

        impl #fn_ident {
            async fn __rustweb_body(#(#wrapper_inputs),*) #output {
                #(#prologue)*
                #block
            }

            async fn __rustweb_handler(#(#wrapper_inputs),*) -> ::rustweb::axum::response::Response {
                use ::rustweb::axum::response::IntoResponse as _;
                #wrapper_body
            }
        }

        impl ::rustweb::Route for #fn_ident {
            fn method(&self) -> ::rustweb::axum::http::Method {
                #http_method
            }

            fn path(&self) -> &'static str {
                #path
            }

            fn operation(&self) -> ::rustweb::openapi::path::Operation {
                ::rustweb::openapi::path::Operation::new()
            }

            fn schemas(&self) -> Vec<(String, ::rustweb::openapi::RefOr<::rustweb::openapi::schema::Schema>)> {
                Vec::new()
            }

            fn register(self, router: ::rustweb::axum::Router) -> ::rustweb::axum::Router {
                router.route(
                    #path,
                    ::rustweb::axum::routing::#axum_fn(#fn_ident::__rustweb_handler),
                )
            }
        }
    })
}

fn status_expr(code: u16) -> TokenStream2 {
    let known = match code {
        100 => Some("CONTINUE"),
        200 => Some("OK"),
        201 => Some("CREATED"),
        204 => Some("NO_CONTENT"),
        400 => Some("BAD_REQUEST"),
        401 => Some("UNAUTHORIZED"),
        403 => Some("FORBIDDEN"),
        404 => Some("NOT_FOUND"),
        409 => Some("CONFLICT"),
        422 => Some("UNPROCESSABLE_ENTITY"),
        500 => Some("INTERNAL_SERVER_ERROR"),
        _ => None,
    };

    if let Some(ident) = known {
        let ident = format_ident!("{}", ident);
        quote!(::rustweb::axum::http::StatusCode::#ident)
    } else {
        quote!(::rustweb::axum::http::StatusCode::from_u16(#code).expect("invalid status code"))
    }
}

fn is_result(output: &ReturnType) -> bool {
    let ReturnType::Type(_, ty) = output else {
        return false;
    };

    match &**ty {
        Type::Path(path) => path
            .path
            .segments
            .last()
            .is_some_and(|segment| {
                segment.ident == "Result" || segment.ident == "ApiResult"
            }),
        _ => false,
    }
}
