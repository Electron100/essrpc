//The quote macro can require a high recursion limit
#![recursion_limit="128"]

extern crate proc_macro;
extern crate proc_macro2;
extern crate quote;
extern crate syn;

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use proc_macro2::{Ident, Span};
use quote::quote;
use syn::{FnArg, ItemTrait, MethodSig, TraitItem, TraitItemMethod};

#[proc_macro_attribute]
pub fn essrpc(args: TokenStream, input: TokenStream) -> TokenStream {
    let args = args.to_string();

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

    result.into()
}

fn new_server_method() -> TokenStream2 {
    TokenStream2::new()
}

fn params_ident(trait_ident: &Ident, sig: &MethodSig) -> Ident {
    Ident::new(&format!("{}_{}_RPCParams", trait_ident, sig.ident), Span::call_site())
}

fn client_ident(trait_ident: &Ident) -> Ident {
    Ident::new(&format!("{}RPCClient", trait_ident), Span::call_site())
}

fn impl_client_method(method: &TraitItemMethod) -> TokenStream2 {
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
        panic!("RPC methods must take '&self' as the first parameter");
    }

    let mut add_param_tokens = TokenStream2::new();

    for p in param_tokens.iter() {
        if let FnArg::Captured(arg) = p {
            let name = &arg.pat;
            add_param_tokens.extend(
                quote!(self.tr.tx_add_param("#name", #name, &mut state)?;));
        }
    }
    let rettype = &method.sig.decl.output;
    quote!(
        fn #ident(#param_tokens) #rettype {
            let mut tx = self.tx.borrow_mut();
            let mut state = self.tr.tx_begin("#ident")?;
            #add_param_tokens
            let data_in = self.tr.tx_finalize(&mut state)?;
            tx.send(data_in)?;
            let data_result = tx.receive()?;
            self.tr.from_wire(&data_result)
        })
}

fn create_client(trait_ident: &Ident, methods: &[TraitItemMethod]) -> TokenStream2 {
    let client_ident = client_ident(trait_ident);

    let mut method_impl_tokens = TokenStream2::new();
        
    for method in methods {
        method_impl_tokens.extend(impl_client_method(method))
    }

    quote!(
        struct #client_ident<TR: essrpc::Transform, TP: essrpc::Transport> {
            tr: TR,
            tx: std::cell::RefCell<TP>
        }

        impl <TR, TP, W> essrpc::RPCClient for #client_ident<TR, TP> where
            TR: essrpc::Transform<Wire=W>,
            TP: essrpc::Transport<Wire=W> {

            type TR = TR;
            type CTP = TP;
            
            fn new(transform: TR, transport: TP) -> Self {
                #client_ident{tr: transform, tx: std::cell::RefCell::new(transport)}
            }
        }

        impl <TR, TP, W> #trait_ident for #client_ident<TR, TP> where
            TR: essrpc::Transform<Wire=W>,
            TP: essrpc::Transport<Wire=W> {
            
            #method_impl_tokens
        }
    )
}
