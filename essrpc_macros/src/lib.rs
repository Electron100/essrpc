extern crate proc_macro;
extern crate proc_macro2;
extern crate quote;
extern crate syn;

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use proc_macro2::{Ident, Span};
use quote::quote;
use syn::{FnArg, ItemTrait, MethodSig, TraitItem};

#[proc_macro_attribute]
pub fn essrpc(args: TokenStream, input: TokenStream) -> TokenStream {
    let args = args.to_string();

    // we return the original trait tokens unchanged, but we append other items to it
    let mut result = input.clone();
    
    // TODO better error handling
    let ast_trait: ItemTrait = syn::parse(input).unwrap();

    let trait_name = ast_trait.ident;

    let mut methods: Vec<MethodSig> = Vec::new();
    
    // Look at each method
    for item in ast_trait.items {
        if let TraitItem::Method(m) = item {
             methods.push(m.sig.clone());
        }
    }

    result.extend(create_client(&trait_name, &methods));

    result
}

fn params_ident(trait_name: &Ident, sig: &MethodSig) -> Ident {
    Ident::new(&format!("{}_{}_RPCParams", trait_name, sig.ident), Span::call_site())
}

fn impl_client_method(method: &MethodSig) -> TokenStream2 {
    let ident = &method.ident;
    let param_tokens = &method.decl.inputs;
    let mut add_param_tokens = TokenStream2::new();
    for p in method.decl.inputs.iter() {
        if let FnArg::Captured(arg) = p {
            let name = &arg.pat;
            add_param_tokens.extend(
                quote!(self.tr.tx_add_param("#name", #name, &mut state)?;));
        }
    }
    let rettype = &method.decl.output;
    quote!(
        fn #ident(#param_tokens) #rettype {
            let mut state = self.tr.tx_begin("#ident")?;
            #add_param_tokens
            let data_in = self.tr.tx_finalize(&mut state)?;
            let data_result = self.tx.send(data_in)?;
            self.tr.from_wire(&data_result)
        })
}

fn create_client(trait_name: &Ident, methods: &[MethodSig]) -> TokenStream {
    let client_ident =
        Ident::new(&format!("{}Client", trait_name), Span::call_site());

    let mut method_impl_tokens = TokenStream2::new();
        
    for method in methods {
        method_impl_tokens.extend(impl_client_method(method))
    }

    quote!(
        struct #client_ident<TR: essrpc::Transform, TX: essrpc::ClientTransport> {
            tr: TR,
            tx: TX
        }

        impl <TR, TX, W> #trait_name for #client_ident<TR, TX> where
            TR: essrpc::Transform<Wire=W>,
            TX: essrpc::ClientTransport<Wire=W> {
            
            #method_impl_tokens
        }
    ).into()

        
        
}
