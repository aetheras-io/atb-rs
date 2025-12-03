use heck::ToUpperCamelCase;
use quote::{format_ident, quote};
use syn::{Attribute, Error, FnArg, GenericArgument, ItemFn, Pat, PathArguments, ReturnType, Type};

#[derive(Copy, Clone)]
pub enum ActivityKind {
    Remote,
    Local,
}

impl ActivityKind {
    fn options_ty(&self) -> syn::Result<Type> {
        syn::parse_str(match self {
            ActivityKind::Remote => "temporalio_sdk::ActivityOptions",
            ActivityKind::Local => "temporalio_sdk::LocalActivityOptions",
        })
    }

    fn ctx_method(&self) -> proc_macro2::Ident {
        match self {
            ActivityKind::Remote => format_ident!("activity"),
            ActivityKind::Local => format_ident!("local_activity"),
        }
    }
}

pub fn expand_activity(
    input_fn: ItemFn,
    kind: ActivityKind,
) -> syn::Result<proc_macro2::TokenStream> {
    // 1. Validate async
    if input_fn.sig.asyncness.is_none() {
        return Err(Error::new_spanned(
            input_fn.sig.fn_token,
            "#[activity] functions must be async",
        ));
    }

    // 2. Params: ctx required, req optional
    let mut inputs = input_fn.sig.inputs.iter();

    let (ctx_pat, _ctx_ty) = match inputs.next() {
        Some(arg) => extract_arg(arg)?,
        None => {
            return Err(Error::new_spanned(
                &input_fn.sig,
                "#[activity] requires a context parameter",
            ));
        }
    };

    let ctx_ident = match ctx_pat {
        Pat::Ident(ref pat_ident) => pat_ident.ident.clone(),
        other => {
            return Err(Error::new_spanned(
                other,
                "#[activity] context parameter must be an identifier",
            ));
        }
    };

    let request_arg = match inputs.next() {
        Some(arg) => {
            let (pat, ty) = extract_arg(arg)?;
            Some(RequestArg::new(pat, ty))
        }
        None => None,
    };

    if inputs.next().is_some() {
        return Err(Error::new_spanned(
            &input_fn.sig,
            "#[activity] supports at most two parameters: (ctx) or (ctx, request)",
        ));
    }

    // 3. Return type: Result-like (e.g. Result<T, E> or ActivityResult<T>)
    let return_ty = match &input_fn.sig.output {
        ReturnType::Type(_, ty) => ty.as_ref().clone(),
        ReturnType::Default => {
            return Err(Error::new_spanned(
                &input_fn.sig,
                "#[activity] functions must return a Result-like type (e.g. Result<T, E> or ActivityResult<T>)",
            ));
        }
    };

    let (ok_ty, _err_ty) = parse_result_ok_type(&return_ty)?;
    let inner_ty = extract_act_exit_inner(&ok_ty)?;
    let is_void = is_unit_type(&inner_ty);

    // 4. Strip our own attributes
    let attrs: Vec<Attribute> = input_fn
        .attrs
        .into_iter()
        .filter(|attr| {
            let p = attr.path();
            !(p.is_ident("activity")
                || p.is_ident("local_activity")
                || p.is_ident("activity2")
                || p.is_ident("local_activity2"))
        })
        .collect();

    // 5. Names etc.
    let vis = input_fn.vis.clone();
    let fn_name = input_fn.sig.ident.clone();

    let struct_name = format_ident!("{}Activity", fn_name.to_string().to_upper_camel_case());
    let act_id_literal = syn::LitStr::new(&fn_name.to_string(), fn_name.span());

    let user_block = input_fn.block;
    let generics = input_fn.sig.generics.clone();

    let options_ty = kind.options_ty()?;
    let ctx_method = kind.ctx_method();

    // 6. Builder struct
    let struct_def = quote! {
        /// Builder for the `#fn_name` activity.
        pub struct #struct_name<'a> {
            ctx: &'a temporalio_sdk::WfContext,
            options: #options_ty,
        }
    };

    // 7. impl<'a> – constructor, with_options, resolution, run

    let new_fn = if let Some(req) = &request_arg {
        let req_ty = &req.ty;
        quote! {
            /// Internal constructor – prefer calling the generated `#fn_name` function.
            fn new(
                ctx: &'a temporalio_sdk::WfContext,
                request: &'a #req_ty,
            ) -> ::atb_temporal_ext::activities::ActivityResult<Self> {
                use temporalio_common::protos::coresdk::AsJsonPayloadExt;
                let input = request
                    .as_json_payload()
                    .map_err(::atb_temporal_ext::activities::ActivityRunError::PayloadEncode)?;
                Ok(Self {
                    ctx,
                    options: #options_ty {
                        activity_type: #act_id_literal.to_string(),
                        input,
                        ..#options_ty::default()
                    },
                })
            }
        }
    } else {
        quote! {
            /// Internal constructor – prefer calling the generated `#fn_name` function.
            ///
            /// This activity has no request payload, so we don't serialize any input.
            fn new(
                ctx: &'a temporalio_sdk::WfContext,
            ) -> ::atb_temporal_ext::activities::ActivityResult<Self> {
                Ok(Self {
                    ctx,
                    options: #options_ty {
                        activity_type: #act_id_literal.to_string(),
                        ..#options_ty::default()
                    },
                })
            }
        }
    };

    let run_methods = if is_void {
        quote! {
            /// Return the raw Temporal ActivityResolution without mapping or deserialization.
            pub fn resolution(
                self,
            ) -> impl std::future::Future<
                Output = temporalio_common::protos::coresdk::activity_result::ActivityResolution
            > + 'a {
                async move {
                    let ctx = self.ctx;
                    let options = self.options;
                    ctx.#ctx_method(options).await
                }
            }

            /// Run this activity and ignore any payload, returning `()`.
            pub fn run(
                self,
            ) -> impl std::future::Future<
                Output = ::atb_temporal_ext::activities::ActivityResult<()>
            > + 'a {
                async move {
                    let resolution = self.resolution().await;
                    ::atb_temporal_ext::activities::resolution_unit(resolution)
                }
            }
        }
    } else {
        quote! {
            /// Return the raw Temporal ActivityResolution without mapping or deserialization.
            pub fn resolution(
                self,
            ) -> impl std::future::Future<
                Output = temporalio_common::protos::coresdk::activity_result::ActivityResolution
            > + 'a {
                async move {
                    let ctx = self.ctx;
                    let options = self.options;
                    ctx.#ctx_method(options).await
                }
            }

            /// Run this activity and deserialize the result into the function's response type.
            pub fn run(
                self,
            ) -> impl std::future::Future<
                Output = ::atb_temporal_ext::activities::ActivityResult<#inner_ty>
            > + 'a {
                async move {
                    let resolution = self.resolution().await;
                    ::atb_temporal_ext::activities::resolution_value::<#inner_ty>(resolution)
                }
            }
        }
    };

    let impl_builder_block = quote! {
        impl<'a> #struct_name<'a> {
            #new_fn

            /// Replace options wholesale (while retaining type + payload).
            pub fn with_options(mut self, options: #options_ty) -> Self {
                let base = self.options;
                self.options = #options_ty {
                    activity_type: base.activity_type,
                    input: base.input,
                    ..options
                };
                self
            }

            #run_methods
        }
    };

    // 8+9. Inherent handler + bind (Temporal worker side)

    let handler_sig = if let Some(req) = &request_arg {
        let req_ident = &req.binding_ident;
        let handler_ty = &req.handler_ty;
        quote! {
            #ctx_ident: temporalio_sdk::ActContext,
            #req_ident: #handler_ty
        }
    } else {
        let phantom_req = format_ident!("__bbc_activity_req");
        quote! {
            #ctx_ident: temporalio_sdk::ActContext,
            #phantom_req: ()
        }
    };

    let handler_ret_ty = quote! { #return_ty };

    let base_block = if let Some(req) = &request_arg {
        let destructure = &req.destructure;
        quote! {{
            #destructure
            #user_block
        }}
    } else {
        quote! { #user_block }
    };

    let impl_handler_and_bind_block = quote! {
        impl #struct_name<'static> {
            #(#attrs)*
            pub async fn handler #generics (
                #handler_sig
            ) -> #handler_ret_ty {
                #base_block
            }

            /// Register this activity handler with a Temporal worker.
            pub fn bind(worker: &mut temporalio_sdk::Worker) {
                worker.register_activity(#act_id_literal, Self::handler);
            }
        }
    };

    // 10. Workflow-facing builder ctor with original name

    let builder_fn = if let Some(req) = &request_arg {
        let req_ty = &req.ty;
        quote! {
            #vis fn #fn_name<'a>(
                ctx: &'a temporalio_sdk::WfContext,
                request: &'a #req_ty,
            ) -> ::atb_temporal_ext::activities::ActivityResult<#struct_name<'a>> {
                #struct_name::new(ctx, request)
            }
        }
    } else {
        quote! {
            #vis fn #fn_name<'a>(
                ctx: &'a temporalio_sdk::WfContext,
            ) -> ::atb_temporal_ext::activities::ActivityResult<#struct_name<'a>> {
                #struct_name::new(ctx)
            }
        }
    };

    let expanded = quote! {
        #struct_def
        #impl_builder_block
        #impl_handler_and_bind_block
        #builder_fn
    };

    Ok(expanded)
}

fn extract_arg(arg: &FnArg) -> syn::Result<(Pat, Type)> {
    match arg {
        FnArg::Typed(pat_type) => Ok(((*pat_type.pat).clone(), (*pat_type.ty).clone())),
        FnArg::Receiver(_) => Err(Error::new_spanned(
            arg,
            "#[activity] does not support methods with self receivers",
        )),
    }
}

struct RequestArg {
    ty: Type,
    handler_ty: Type,
    binding_ident: proc_macro2::Ident,
    destructure: proc_macro2::TokenStream,
}

impl RequestArg {
    fn new(pat: Pat, ty: Type) -> Self {
        let binding_ident = format_ident!("__bbc_activity_req");
        let (handler_ty, needs_ref) = strip_leading_reference(&ty);
        let binding_expr = if needs_ref {
            quote! { &#binding_ident }
        } else {
            quote! { #binding_ident }
        };
        let destructure = quote! {
            let #pat = #binding_expr;
        };

        Self {
            ty,
            handler_ty,
            binding_ident,
            destructure,
        }
    }
}

fn strip_leading_reference(ty: &Type) -> (Type, bool) {
    if let Type::Reference(r) = ty {
        ((*r.elem).clone(), true)
    } else {
        (ty.clone(), false)
    }
}

/// Expect a Result-like type and return `(Ok, Option<Err>)`.
///
/// Supports syntactic forms like:
/// - `Result<T, E>`
/// - `ActivityResult<T>` where `type ActivityResult<T> = Result<..., ...>;`
///
/// We treat the first generic argument as the "Ok" type.
fn parse_result_ok_type(ty: &Type) -> syn::Result<(Type, Option<Type>)> {
    let Type::Path(type_path) = ty else {
        return Err(Error::new_spanned(
            ty,
            "expected Result-like return type (e.g. Result<T, E> or ActivityResult<T>)",
        ));
    };

    let last_segment = type_path.path.segments.last().ok_or_else(|| {
        Error::new_spanned(
            ty,
            "expected Result-like return type (e.g. Result<T, E> or ActivityResult<T>)",
        )
    })?;

    let PathArguments::AngleBracketed(args) = &last_segment.arguments else {
        return Err(Error::new_spanned(
            ty,
            "expected Result-like return type with generic arguments (e.g. Result<T, E> or ActivityResult<T>)",
        ));
    };

    let mut args_iter = args.args.iter();

    let first = args_iter.next().ok_or_else(|| {
        Error::new_spanned(
            ty,
            "Result-like type requires an Ok type (e.g. Result<T, E> or ActivityResult<T>)",
        )
    })?;

    let GenericArgument::Type(ok_ty) = first else {
        return Err(Error::new_spanned(
            first,
            "expected type for Ok variant of Result-like return type",
        ));
    };

    let second = args_iter.next();

    let err_ty = match second {
        Some(GenericArgument::Type(err_ty)) => Some(err_ty.clone()),
        Some(other) => {
            return Err(Error::new_spanned(
                other,
                "expected type for Err variant of Result-like return type",
            ));
        }
        None => None,
    };

    Ok((ok_ty.clone(), err_ty))
}

/// Extract the inner `T` from `ActExitValue<T>`.
fn extract_act_exit_inner(ok_ty: &Type) -> syn::Result<Type> {
    if let Type::Path(type_path) = ok_ty
        && let Some(last_segment) = type_path.path.segments.last()
        && last_segment.ident == "ActExitValue"
    {
        let PathArguments::AngleBracketed(args) = &last_segment.arguments else {
            return Err(Error::new_spanned(
                ok_ty,
                "ActExitValue must specify an inner type",
            ));
        };

        let mut args_iter = args.args.iter();

        let first = args_iter
            .next()
            .ok_or_else(|| Error::new_spanned(ok_ty, "ActExitValue must specify an inner type"))?;

        let GenericArgument::Type(inner_ty) = first else {
            return Err(Error::new_spanned(
                first,
                "ActExitValue inner generic must be a type",
            ));
        };

        return Ok(inner_ty.clone());
    }

    Err(Error::new_spanned(
        ok_ty,
        "#[activity2] requires Result<ActExitValue<_>, _> return type",
    ))
}

fn is_unit_type(ty: &Type) -> bool {
    matches!(ty, Type::Tuple(t) if t.elems.is_empty())
}
