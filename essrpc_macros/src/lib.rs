//The quote macro can require a high recursion limit
#![recursion_limit = "256"]
// Clippy's suggestions for these don't compile
#![allow(clippy::explicit_counter_loop)]

extern crate proc_macro;
extern crate proc_macro2;
extern crate quote;
extern crate syn;

use core::convert::AsRef;
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use proc_macro2::{Ident, Span, TokenTree};
use quote::{quote, ToTokens};
use std::ops::Deref;
use syn::parse_quote;
use syn::{
    punctuated::Punctuated, token::Comma, /*spanned::Spanned,*/ FnArg, ItemTrait, LitStr, Pat,
    TraitItem, TraitItemMethod,
};

/// The main macro which does the magic. When applied to a trait `Foo`
/// generates a `FooRPCClient` type implementing
/// [RPCClient](../essrpc/trait.RPCClient.html) (and `Foo`).  as well as
/// `FooRPCServer` implementing [RPCServer](../essrpc/trait.RPCServer.html).
///
/// For an asynchronous client, the argument `async` can be used
/// (`#[essrpc(async)]`) to generate a `FooAsync` trait, which is like
/// `Foo` except every method returns a boxed `Future` instead of a
/// `Result` and a `FooAsyncRPCClient` type implementing `FooAsync`
/// and [AsyncRPCClient](../essrpc/trait.AsyncRPCClient.html).
///
/// See the crate-level documentation for examples.
#[proc_macro_attribute]
pub fn essrpc(args: TokenStream, input: TokenStream) -> TokenStream {
    let args: TokenStream2 = args.into();
    let mut sync_client = false;
    let mut async_client = false;
    for tok in args {
        if let TokenTree::Ident(ident) = tok {
            match ident.to_string().as_ref() {
                "sync" => sync_client = true,
                "async" => async_client = true,
                _ => (),
            }
        }
    }

    if !sync_client && !async_client {
        sync_client = true
    }

    let mut result: TokenStream2 = input.clone().into();

    // TODO better error handling
    let ast_trait: ItemTrait = syn::parse(input).unwrap();

    let trait_ident = ast_trait.ident;

    let mut methods: Vec<TraitItemMethod> = Vec::new();

    // Look at each method
    for item in ast_trait.items {
        if let TraitItem::Method(m) = item {
            methods.push(m.clone());
        }
    }

    if async_client {
        result.extend(create_async_client_trait(&trait_ident, &methods));
        result.extend(create_client(
            &async_client_trait_ident(&trait_ident),
            &methods,
            true,
        ));
    }
    if sync_client {
        result.extend(create_client(&trait_ident, &methods, false));
    }
    result.extend(create_server(&trait_ident, &methods));

    result.into()
}

fn client_ident(trait_ident: &Ident) -> Ident {
    Ident::new(&format!("{}RPCClient", trait_ident), Span::call_site())
}

fn client_transport_ident(async_client: bool) -> Ident {
    Ident::new(
        if async_client {
            "AsyncClientTransport"
        } else {
            "ClientTransport"
        },
        Span::call_site(),
    )
}

fn async_client_trait_ident(trait_ident: &Ident) -> Ident {
    Ident::new(&format!("{}Async", trait_ident), Span::call_site())
}

fn rpcclient_ident(async_client: bool) -> Ident {
    Ident::new(
        if async_client {
            "AsyncRPCClient"
        } else {
            "RPCClient"
        },
        Span::call_site(),
    )
}

fn server_ident(trait_ident: &Ident) -> Ident {
    Ident::new(&format!("{}RPCServer", trait_ident), Span::call_site())
}

fn make_pat_literal_str(pat: &Pat) -> LitStr {
    match pat {
        Pat::Ident(p) => make_ident_literal_str(&p.ident),
        _ => panic!("Unhandled PAT type {:?}", pat),
    }
}

fn make_ident_literal_str(ident: &Ident) -> LitStr {
    let as_str = format!("{}", ident);
    LitStr::new(&as_str, Span::call_site())
}

// True if has self param, false if has default implementation. Panics
// if no self and no default.
fn verify_self_param_or_unneeded(method: &TraitItemMethod) -> bool {
    if has_self_param(method) {
        return true;
    }
    if method.default.is_some() {
        // this method is not needed for the RPC client
        return false;
    }
    panic!(
        "RPC trait method {:?} has no self param and no default implementation",
        method
    );
}

fn has_self_param(method: &TraitItemMethod) -> bool {
    let param_tokens = &method.sig.inputs;
    let first = param_tokens.first();
    first.is_some() && (matches!(first.unwrap(), FnArg::Receiver(_)))
}

// Client method implementation for the call to tx_begin_call through
// tx_finalize. This portion is shared between sync and async.
fn client_method_tx_send(method: &TraitItemMethod, id: u32) -> TokenStream2 {
    let ident = &method.sig.ident;
    let param_tokens = &method.sig.inputs;

    let mut add_param_tokens = TokenStream2::new();

    for p in param_tokens.iter() {
        if let FnArg::Typed(arg) = p {
            let name = &arg.pat;
            let name_literal = make_pat_literal_str(name);
            add_param_tokens.extend(quote!(tr.tx_add_param(#name_literal, #name, &mut state)?;));
        }
    }

    let ident_literal = make_ident_literal_str(ident);
    quote!(
        let mut tr = self.tr.borrow_mut();
        let mut state = tr.tx_begin_call(essrpc::MethodId{name: #ident_literal, num: #id})?;
        #add_param_tokens
        let state = tr.tx_finalize(state)?;
    )
}

fn impl_client_method(method: &TraitItemMethod, id: u32) -> TokenStream2 {
    let ident = &method.sig.ident;
    let param_tokens = &method.sig.inputs;

    if !verify_self_param_or_unneeded(method) {
        return TokenStream2::new();
    }

    let rettype = get_return_type(method);

    let tx_send = client_method_tx_send(method, id);

    quote!(
    fn #ident(#param_tokens) -> #rettype {
        #tx_send
        let ret: std::result::Result<#rettype, essrpc::RPCError> =
            tr.rx_response(state);
        match ret {
            Ok(v) => v,
            Err(e) => Err(e.into())
        }
    })
}

fn get_return_type(method: &TraitItemMethod) -> &syn::Type {
    match method.sig.output {
        syn::ReturnType::Default => panic!(
            "RPC methods must have a return type, {} does not ",
            &method.sig.ident
        ),
        syn::ReturnType::Type(_arrow, ref t) => t,
    }
}

fn param_tokens_after_this(method: &TraitItemMethod) -> Punctuated<FnArg, Comma> {
    method.sig.inputs.clone().into_pairs().skip(1).collect()
}

fn impl_async_client_method(method: &TraitItemMethod, id: u32) -> TokenStream2 {
    let ident = &method.sig.ident;

    // get the parameters without the &self as we want to add a lifetime to that
    let param_tokens = param_tokens_after_this(method);

    if !verify_self_param_or_unneeded(method) {
        return TokenStream2::new();
    }

    let orig_rettype = get_return_type(method);
    let rettype = get_future_return_type(method);
    let tx_send = client_method_tx_send(method, id);

    quote!(
    fn #ident<'a>(&'a self, #param_tokens) -> #rettype {
        use std::future::Future;
        use futures;
        use futures::future::FutureExt;
        use futures::future::TryFutureExt;
        futures::future::lazy(move |_| {
            #tx_send
            Ok(state)
        }).and_then(move |state| -> essrpc::BoxFuture<#orig_rettype, essrpc::RPCError> {
            self.tr.borrow_mut().rx_response(state)
        }).map_err(|e: essrpc::RPCError| e.into())
            .and_then(|ret| futures::future::ready(ret))
            .boxed_local()
    })
}

fn create_async_client_trait(trait_ident: &Ident, methods: &[TraitItemMethod]) -> TokenStream2 {
    let ident = async_client_trait_ident(trait_ident);
    let mut method_decls: Vec<TokenStream2> = Vec::new();

    for method in methods {
        let rettype = get_future_return_type(method);
        let ident = &method.sig.ident;
        let param_tokens = param_tokens_after_this(method);
        method_decls.push(quote!(
            fn #ident<'a>(&'a self, #param_tokens) -> #rettype;));
    }

    quote!(
        pub trait #ident {
           #(#method_decls)*
        }
    )
}

fn get_future_return_type(method: &TraitItemMethod) -> syn::Type {
    match get_result_types(&method.sig.output) {
        Some((ok_type, err_type)) => parse_quote!(
            std::pin::Pin<Box<dyn std::future::Future<Output = std::result::Result<#ok_type, #err_type>> + 'a>>
        ),
        None => panic!(
            "return {} type is not of expected form Result<T, E>",
            method.sig.output.clone().into_token_stream()
        ),
    }
}

/// For a return type of the form Result<T, E>, figure out what T and E are.
fn get_result_types(result_type: &syn::ReturnType) -> Option<(syn::Type, syn::Type)> {
    // unfortunately, there's no "if not let", meaning we end up with
    // nesting instead of early return
    if let syn::ReturnType::Type(_, b) = result_type {
        if let syn::Type::Path(tp) = b.deref() {
            let result_seg: syn::PathSegment = (*tp.path.segments.last()?).clone();
            if let syn::PathArguments::AngleBracketed(args) = result_seg.arguments {
                if args.args.len() != 2 {
                    panic!(
                        "Expected Result to have two type parameters, found {}: {}",
                        args.args.len(),
                        result_type.clone().into_token_stream()
                    )
                }
                let ok_type_generic = args.args.first()?;
                let err_type_generic = args.args.last()?;
                if let syn::GenericArgument::Type(ok_type) = ok_type_generic {
                    if let syn::GenericArgument::Type(err_type) = err_type_generic {
                        return Some((ok_type.clone(), err_type.clone()));
                    }
                }
            }
        }
    }
    None
}

fn create_client(
    trait_ident: &Ident,
    methods: &[TraitItemMethod],
    async_client: bool,
) -> TokenStream2 {
    let client_ident = client_ident(trait_ident);
    let transport_ident = client_transport_ident(async_client);
    let rpcclient_ident = rpcclient_ident(async_client);

    let mut method_impl_tokens = TokenStream2::new();

    let mut mcnt = 0;
    for method in methods {
        method_impl_tokens.extend(if async_client {
            impl_async_client_method(method, mcnt)
        } else {
            impl_client_method(method, mcnt)
        });
        mcnt += 1;
    }

    quote!(
        pub struct #client_ident<TR: essrpc::#transport_ident> {
            tr: std::cell::RefCell<TR>,
        }

        impl <TR> essrpc::#rpcclient_ident for #client_ident<TR> where
            TR: essrpc::#transport_ident {

            type TR = TR;

            fn new(transport: TR) -> Self {
                #client_ident{tr: std::cell::RefCell::new(transport)}
            }
        }

        impl <TR> #trait_ident for #client_ident<TR> where
            TR: essrpc::#transport_ident {

            #method_impl_tokens
        }
    )
}

fn create_server(trait_ident: &Ident, methods: &[TraitItemMethod]) -> TokenStream2 {
    let server_ident = server_ident(trait_ident);

    let mut server_method_matches = TokenStream2::new();
    let mut server_by_name_matches = TokenStream2::new();

    let mut mcnt = 0;
    for method in methods {
        server_method_matches.extend(create_server_match(method, mcnt));
        let ident_literal = make_ident_literal_str(&method.sig.ident);
        server_by_name_matches.extend(quote!(#ident_literal => #mcnt,));
        mcnt += 1;
    }

    quote!(
        pub struct #server_ident<T, TR> where
            T: #trait_ident,
            TR: essrpc::ServerTransport {

            tr: TR,
            imp: T
        }

        impl <T, TR> #server_ident<T, TR> where
            T: #trait_ident,
            TR: essrpc::ServerTransport {

            pub fn new(imp: T, transport: TR) -> Self {
                #server_ident{tr: transport,
                              imp: imp}
            }

            fn method_num_from_name(name: &str) -> u32 {
                match name {
                    #server_by_name_matches
                    _ => std::u32::MAX
                }
            }

        }

        impl <TR, T> essrpc::RPCServer for #server_ident<T, TR> where
            TR: essrpc::ServerTransport,
            T: #trait_ident
        {
            fn serve_single_call(&mut self) -> std::result::Result<(), essrpc::RPCError> {
                let (method, mut rxstate) = self.tr.rx_begin_call()?;
                let id = match &method {
                    essrpc::PartialMethodId::Num(num) => *num,
                    essrpc::PartialMethodId::Name(name) => Self::method_num_from_name(&name),
                };
                match id {
                    #server_method_matches
                    _ => {
                        Err(essrpc::RPCError::new(
                            essrpc::RPCErrorKind::UnknownMethod, format!("Unknown rpc method {:?}", method)))
                    }
                }
            }
        }
    )
}

fn create_server_match(method: &TraitItemMethod, id: u32) -> TokenStream2 {
    let ident = &method.sig.ident;
    let param_tokens = &method.sig.inputs;

    let mut param_retrieve_tokens = TokenStream2::new();
    let mut param_call_tokens = TokenStream2::new();
    let mut first = true;

    for p in param_tokens.iter() {
        if let FnArg::Typed(arg) = p {
            let name = &arg.pat;
            let name_literal = make_pat_literal_str(name);
            let ty = &arg.ty;
            param_retrieve_tokens.extend(
                quote!(let #name: #ty = self.tr.rx_read_param(#name_literal, &mut rxstate)?;),
            );
            if first {
                first = false;
            } else {
                param_call_tokens.extend(quote!(,))
            }
            param_call_tokens.extend(quote!(#name));
        }
    }

    quote!(
        #id => {
            #param_retrieve_tokens
            let ret = self.imp.#ident(#param_call_tokens);
            self.tr.tx_response(ret)
        },
    )
}
