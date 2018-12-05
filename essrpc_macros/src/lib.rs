//The quote macro can require a high recursion limit
#![recursion_limit="256"]

extern crate proc_macro;
extern crate proc_macro2;
extern crate quote;
extern crate syn;

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use proc_macro2::{Ident, Span};
use quote::quote;
use syn::{FnArg, ItemTrait, LitStr, Pat, TraitItem, TraitItemMethod};

#[proc_macro_attribute]
pub fn essrpc(args: TokenStream, input: TokenStream) -> TokenStream {
    // We don't handle any arguments today, perhaps we will in the future.
    let _args = args.to_string();

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

    result.extend(create_client(&trait_ident, &methods));
    result.extend(create_server(&trait_ident, &methods));

    result.into()
}

fn client_ident(trait_ident: &Ident) -> Ident {
    Ident::new(&format!("{}RPCClient", trait_ident), Span::call_site())
}

fn server_ident(trait_ident: &Ident) -> Ident {
    Ident::new(&format!("{}RPCServer", trait_ident), Span::call_site())
}

fn make_pat_literal_str(pat: &Pat) -> LitStr {
    match pat {
        Pat::Ident(p) => make_ident_literal_str(&p.ident),
        _ => panic!("Unhandled PAT type {:?}", pat)
    }
}

fn make_ident_literal_str(ident: &Ident) -> LitStr {
    let as_str = format!("{}", ident);
    LitStr::new(&as_str, Span::call_site())
}
    

fn impl_client_method(method: &TraitItemMethod, id: u32) -> TokenStream2 {
    let ident = &method.sig.ident;
    let param_tokens = &method.sig.decl.inputs;

    let first = param_tokens.first();
    if !first.is_some() || (match first.unwrap().value() {
        FnArg::SelfRef(_) => false,
        _ => true
    }) {
        if method.default.is_some() {
            // this method is not needed for the RPC client
            return TokenStream2::new();
        }
        panic!("RPC methods must take '&self' as the first parameter, {} does not", ident);
    }

    let mut add_param_tokens = TokenStream2::new();

    for p in param_tokens.iter() {
        if let FnArg::Captured(arg) = p {
            let name = &arg.pat;
            let name_literal = make_pat_literal_str(name);
            add_param_tokens.extend(
                quote!(tr.tx_add_param(#name_literal, #name, &mut state)?;));
        }
    }
    let rettype = match method.sig.decl.output {
        syn::ReturnType::Default => panic!("RPC methods must have a return type, {} does not ", ident),
        syn::ReturnType::Type(_arrow, ref t) => t
    };
    let ident_literal = make_ident_literal_str(ident);

    quote!(
        fn #ident(#param_tokens) -> #rettype {
            let mut tr = self.tr.borrow_mut();
            let mut state = tr.tx_begin_call(essrpc::MethodId{name: #ident_literal, num: #id})?;
            #add_param_tokens
            tr.tx_finalize(&mut state)?;
            let ret: std::result::Result<#rettype, essrpc::RPCError> =
                tr.rx_response();
            match ret {
                Ok(v) => v,
                Err(e) => Err(e.into())
            }
        })
}

fn create_client(trait_ident: &Ident, methods: &[TraitItemMethod]) -> TokenStream2 {
    let client_ident = client_ident(trait_ident);

    let mut method_impl_tokens = TokenStream2::new();

    let mut mcnt = 0;
    for method in methods {
        method_impl_tokens.extend(impl_client_method(method, mcnt));
        mcnt += 1;
    }

    quote!(
        struct #client_ident<TR: essrpc::Transport> {
            tr: std::cell::RefCell<TR>,
        }

        impl <TR> essrpc::RPCClient for #client_ident<TR> where
            TR: essrpc::Transport {

            type TR = TR;
            
            fn new(transport: TR) -> Self {
                #client_ident{tr: std::cell::RefCell::new(transport)}
            }
        }

        impl <TR> #trait_ident for #client_ident<TR> where
            TR: essrpc::Transport {
            
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
        struct #server_ident<T, TR> where
            T: #trait_ident,
            TR: essrpc::Transport {
            
            tr: TR,
            imp: T
        }

        impl <T, TR> #server_ident<T, TR> where
            T: #trait_ident,
            TR: essrpc::Transport {

            fn new(imp: T, transport: TR) -> Self {
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
            TR: essrpc::Transport,
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
    let param_tokens = &method.sig.decl.inputs;

    let mut param_retrieve_tokens = TokenStream2::new();
    let mut param_call_tokens = TokenStream2::new();
    let mut first = true;
    
    for p in param_tokens.iter() {
        if let FnArg::Captured(arg) = p {
            let name = &arg.pat;
            let name_literal = make_pat_literal_str(name);
            let ty = &arg.ty;
            param_retrieve_tokens.extend(
                quote!(let #name: #ty = self.tr.rx_read_param(#name_literal, &mut rxstate)?;));
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
