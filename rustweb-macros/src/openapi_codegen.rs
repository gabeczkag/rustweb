use proc_macro2::TokenStream;
use quote::quote;
use syn::{Error, FnArg, GenericArgument, PathArguments, Signature, Type};

pub struct Metadata {
    params: Vec<Param>,
    body: Option<Type>,
    response: Option<Type>,
}

enum Param {
    Path { name: String, ty: Type },
    Query(Type),
}

pub fn analyze(signature: &Signature, path: &str) -> syn::Result<Metadata> {
    let template_names = path_param_names(path);
    let mut path_types = Vec::new();
    let mut query_types = Vec::new();
    let mut body = None;

    for argument in &signature.inputs {
        let FnArg::Typed(pat_type) = argument else {
            continue;
        };
        let ty = &*pat_type.ty;

        if let Some(inner) = wrapper_inner(ty, &["Path", "ApiPath"]) {
            if !path_types.is_empty() {
                return Err(Error::new_spanned(
                    ty,
                    "rustweb supports only one Path extractor per handler",
                ));
            }

            match inner {
                Type::Tuple(tuple) => path_types.extend(tuple.elems.iter().cloned()),
                other => path_types.push(other),
            }
        } else if let Some(inner) = wrapper_inner(ty, &["Query", "ApiQuery"]) {
            query_types.push(inner);
        } else if let Some(inner) = wrapper_inner(ty, &["Json", "ApiJson"]) {
            if body.replace(inner).is_some() {
                return Err(Error::new_spanned(
                    ty,
                    "rustweb supports only one JSON body extractor per handler",
                ));
            }
        }
    }

    if !path_types.is_empty() && path_types.len() != template_names.len() {
        return Err(Error::new_spanned(
            &signature.inputs,
            format!(
                "path template has {} parameter(s), but the Path extractor provides {} type(s); \
                 use a tuple type for multiple parameters",
                template_names.len(),
                path_types.len()
            ),
        ));
    }

    let mut params = Vec::new();
    for (name, ty) in template_names.into_iter().zip(path_types) {
        params.push(Param::Path { name, ty });
    }
    params.extend(query_types.into_iter().map(Param::Query));

    Ok(Metadata {
        params,
        body,
        response: response_type(&signature.output),
    })
}

pub fn operation_tokens(metadata: &Metadata, status: u16) -> TokenStream {
    let mut statements = Vec::new();

    for param in &metadata.params {
        match param {
            Param::Path { name, ty } => statements.push(quote! {
                __params.push(
                    ::rustweb::openapi::path::ParameterBuilder::new()
                        .name(#name)
                        .parameter_in(::rustweb::openapi::path::ParameterIn::Path)
                        .required(::rustweb::openapi::Required::True)
                        .schema(Some(<#ty as ::rustweb::utoipa::PartialSchema>::schema()))
                        .build(),
                );
            }),
            Param::Query(ty) => statements.push(quote! {
                __params.extend(<#ty as ::rustweb::utoipa::IntoParams>::into_params(|| {
                    Some(::rustweb::openapi::path::ParameterIn::Query)
                }));
            }),
        }
    }

    if let Some(ty) = &metadata.body {
        let body_schema = schema_tokens(ty);
        statements.push(quote! {
            __builder = __builder.request_body(Some(
                ::rustweb::openapi::request_body::RequestBodyBuilder::new()
                    .required(Some(::rustweb::openapi::Required::True))
                    .content(
                        "application/json",
                        ::rustweb::openapi::ContentBuilder::new().schema(Some(#body_schema)),
                    )
                    .build(),
            ));
        });
    }

    let status = status.to_string();
    let description = status_description(status.as_str());

    if let Some(ty) = &metadata.response {
        let response_schema = schema_tokens(ty);
        statements.push(quote! {
            __builder = __builder.response(
                #status,
                ::rustweb::openapi::ResponseBuilder::new()
                    .description(#description)
                    .content(
                        "application/json",
                        ::rustweb::openapi::ContentBuilder::new()
                            .schema(Some(#response_schema)),
                    ),
            );
        });
    } else {
        statements.push(quote! {
            __builder = __builder.response(
                #status,
                ::rustweb::openapi::ResponseBuilder::new().description(#description),
            );
        });
    }

    if needs_validation(metadata) {
        statements.push(quote! {
            __builder = __builder.response(
                "422",
                ::rustweb::openapi::ResponseBuilder::new()
                    .description("Validation Error")
                    .content(
                        "application/json",
                        ::rustweb::openapi::ContentBuilder::new().schema(Some(
                            ::rustweb::openapi::Ref::from_schema_name(
                                <::rustweb::ApiErrorBody as ::rustweb::utoipa::ToSchema>::name(),
                            ),
                        )),
                    ),
            );
        });
    }

    quote! {
        let mut __params: Vec<::rustweb::openapi::path::Parameter> = Vec::new();
        #(#statements)*
        if !__params.is_empty() {
            __builder = __builder.parameters(Some(__params));
        }
    }
}

pub fn schemas_tokens(metadata: &Metadata) -> TokenStream {
    let mut statements = Vec::new();

    for ty in metadata.body.iter().chain(metadata.response.iter()) {
        collect_type_schemas(ty, &mut statements);
    }

    if needs_validation(metadata) {
        statements.push(quote! {
            <::rustweb::ApiErrorBody as ::rustweb::utoipa::ToSchema>::schemas(&mut __schemas);
            __schemas.push((
                <::rustweb::ApiErrorBody as ::rustweb::utoipa::ToSchema>::name().into_owned(),
                <::rustweb::ApiErrorBody as ::rustweb::utoipa::PartialSchema>::schema(),
            ));
        });
    }

    quote! {
        let mut __schemas: Vec<(
            String,
            ::rustweb::openapi::RefOr<::rustweb::openapi::schema::Schema>,
        )> = Vec::new();
        #(#statements)*
        __schemas
    }
}

fn needs_validation(metadata: &Metadata) -> bool {
    !metadata.params.is_empty() || metadata.body.is_some()
}

fn schema_tokens(ty: &Type) -> TokenStream {
    if collection_inner_types(ty).is_some() {
        quote!(<#ty as ::rustweb::utoipa::PartialSchema>::schema())
    } else {
        quote!(::rustweb::openapi::Ref::from_schema_name(
            <#ty as ::rustweb::utoipa::ToSchema>::name()
        ))
    }
}

fn collect_type_schemas(ty: &Type, statements: &mut Vec<TokenStream>) {
    if let Some(inner_types) = collection_inner_types(ty) {
        for inner in inner_types {
            collect_type_schemas(&inner, statements);
        }
        return;
    }

    if is_primitive(ty) {
        return;
    }

    statements.push(quote! {
        <#ty as ::rustweb::utoipa::ToSchema>::schemas(&mut __schemas);
        __schemas.push((
            <#ty as ::rustweb::utoipa::ToSchema>::name().into_owned(),
            <#ty as ::rustweb::utoipa::PartialSchema>::schema(),
        ));
    });
}

fn collection_inner_types(ty: &Type) -> Option<Vec<Type>> {
    match ty {
        Type::Array(array) => Some(vec![(*array.elem).clone()]),
        Type::Slice(slice) => Some(vec![(*slice.elem).clone()]),
        Type::Path(path) => {
            let segment = path.path.segments.last()?;
            let name = segment.ident.to_string();

            if !matches!(
                name.as_str(),
                "Vec" | "Option" | "HashMap" | "BTreeMap" | "HashSet" | "BTreeSet"
            ) {
                return None;
            }

            let PathArguments::AngleBracketed(arguments) = &segment.arguments else {
                return Some(Vec::new());
            };

            Some(
                arguments
                    .args
                    .iter()
                    .filter_map(|argument| match argument {
                        GenericArgument::Type(ty) => Some(ty.clone()),
                        _ => None,
                    })
                    .collect(),
            )
        }
        _ => None,
    }
}

fn is_primitive(ty: &Type) -> bool {
    let Type::Path(path) = ty else {
        return false;
    };
    let Some(segment) = path.path.segments.last() else {
        return false;
    };
    let name = segment.ident.to_string();

    matches!(
        name.as_str(),
        "i8" | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "isize"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
            | "f32"
            | "f64"
            | "bool"
            | "String"
            | "str"
            | "char"
            | "Value"
    )
}

fn path_param_names(path: &str) -> Vec<String> {
    path.split('/')
        .filter_map(|segment| {
            let segment = segment.trim();
            if !segment.starts_with('{') || !segment.ends_with('}') {
                return None;
            }

            let name = segment[1..segment.len() - 1].trim_start_matches('*');
            if name.is_empty() {
                None
            } else {
                Some(name.to_owned())
            }
        })
        .collect()
}

fn wrapper_inner(ty: &Type, names: &[&str]) -> Option<Type> {
    let Type::Path(path) = ty else {
        return None;
    };
    let segment = path.path.segments.last()?;

    if !names.iter().any(|name| segment.ident == *name) {
        return None;
    }

    first_type_argument(&segment.arguments)
}

fn first_type_argument(arguments: &PathArguments) -> Option<Type> {
    let PathArguments::AngleBracketed(arguments) = arguments else {
        return None;
    };

    arguments.args.iter().find_map(|argument| match argument {
        GenericArgument::Type(ty) => Some(ty.clone()),
        _ => None,
    })
}

fn response_type(output: &syn::ReturnType) -> Option<Type> {
    let syn::ReturnType::Type(_, ty) = output else {
        return None;
    };

    response_type_inner(ty)
}

fn response_type_inner(ty: &Type) -> Option<Type> {
    match ty {
        Type::Path(path) => {
            let segment = path.path.segments.last()?;

            if segment.ident == "Result" || segment.ident == "ApiResult" {
                return first_type_argument(&segment.arguments)
                    .and_then(|inner| response_type_inner(&inner));
            }

            if segment.ident == "Json" || segment.ident == "ApiJson" {
                return first_type_argument(&segment.arguments);
            }

            None
        }
        Type::Tuple(tuple) => tuple.elems.iter().find_map(response_type_inner),
        _ => None,
    }
}

fn status_description(status: &str) -> &'static str {
    match status {
        "200" => "OK",
        "201" => "Created",
        "202" => "Accepted",
        "204" => "No Content",
        "400" => "Bad Request",
        "401" => "Unauthorized",
        "403" => "Forbidden",
        "404" => "Not Found",
        "409" => "Conflict",
        "422" => "Unprocessable Entity",
        "500" => "Internal Server Error",
        _ => "Response",
    }
}
