use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Fields, Meta, parse_macro_input};

#[proc_macro_derive(GraphQLConnection, attributes(cursor))]
pub fn graphql_connection_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;

    let Data::Struct(data_struct) = &input.data else {
        return syn::Error::new_spanned(name, "GraphQLConnection can only be derived for structs")
            .to_compile_error()
            .into();
    };

    let cursor_impl = if let Fields::Named(fields_named) = &data_struct.fields {
        if let Some((cursor, custom_cursor_fn)) = fields_named.named.iter().find_map(|f| {
            f.attrs.iter().find_map(|a| {
                if a.path().is_ident("cursor") {
                    let mut custom_cursor_fn = None;
                    if matches!(a.meta, Meta::List(_)) {
                        let _ = a.parse_nested_meta(|meta| {
                            if meta.path.is_ident("custom_cursor_fn") {
                                let value: syn::LitStr = meta.value()?.parse()?;
                                custom_cursor_fn = Some(value.value());
                            }
                            Ok(())
                        });
                    }
                    Some((f.ident.as_ref().unwrap().clone(), custom_cursor_fn))
                } else {
                    None
                }
            })
        }) {
            let to_string_expr = if let Some(custom_cursor_fn) = custom_cursor_fn {
                quote! { #custom_cursor_fn(&self.#cursor) }
            } else {
                quote! { self.#cursor.to_string() }
            };

            quote! {
                impl ::atb_graphql_ext::Cursor for #name {
                    fn cursor(&self) -> String {
                        #to_string_expr
                    }
                }
            }
        } else {
            quote! {}
        }
    } else {
        quote! {}
    };

    let edge_name = format_ident!("{}Edge", name);
    let connection_name = format_ident!("{}Connection", name);

    let expanded = quote! {
        #[derive(::async_graphql::SimpleObject)]
        pub struct #edge_name {
            pub node: #name,
            pub cursor: String,
        }

        impl From<#name> for #edge_name {
            fn from(item: #name) -> Self {
                #edge_name {
                    cursor: item.cursor(),
                    node: item,
                }
            }
        }

        impl ::atb_graphql_ext::Cursor for #edge_name {
            fn cursor(&self) -> String {
                self.cursor.clone()
            }
        }

        #cursor_impl

        #[derive(::async_graphql::SimpleObject)]
        pub struct #connection_name {
            pub edges: Vec<#edge_name>,
            pub page_info: ::async_graphql::connection::PageInfo,
        }
    };

    TokenStream::from(expanded)
}

#[proc_macro_derive(GraphQLIndexed)]
pub fn graphql_indexed_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;

    let Data::Struct(_data_struct) = &input.data else {
        return syn::Error::new_spanned(name, "GraphQLIndexed can only be derived for structs")
            .to_compile_error()
            .into();
    };

    let indexed_name = format_ident!("{}Indexed", name);

    let expanded = quote! {
        #[derive(SimpleObject)]
        pub struct #indexed_name {
            pub items: Vec<#name>,
            pub total_count: i64,
            pub page: u64,
            pub limit: u64,
        }
    };

    TokenStream::from(expanded)
}
