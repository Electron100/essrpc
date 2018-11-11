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
            let name_literal = make_pat_literal_str(name);
            add_param_tokens.extend(
                quote!(self.tr.tx_add_param(#name_literal, #name, &mut state)?;));
        }
    }
    let rettype = &method.sig.decl.output;
    let ident_literal = make_ident_literal_str(ident);

    quote!(
        fn #ident(#param_tokens) #rettype {
            println!("Client method");
            let mut tx = self.tx.borrow_mut();
            let mut state = self.tr.tx_begin(#ident_literal)?;
            #add_param_tokens
            let data_in = self.tr.tx_finalize(&mut state)?;
            println!("Client sending request");
            tx.send(data_in)?;
            println!("Client reading request response");
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

fn create_server(trait_ident: &Ident, methods: &[TraitItemMethod]) -> TokenStream2 {
    let server_ident = server_ident(trait_ident);

    let mut server_method_matches = TokenStream2::new();
        
    for method in methods {
        server_method_matches.extend(create_server_match(method))
    }
    
    quote!(
        struct #server_ident<T, TR, TP> where
            T: #trait_ident,
            TR: essrpc::Transform,
            TP: essrpc::Transport {
            
            tr: TR,
            tx: std::cell::RefCell<TP>,
            imp: T
        }

        impl <T, TR, TP> #server_ident<T, TR, TP> where
            T: #trait_ident,
            TR: essrpc::Transform,
            TP: essrpc::Transport {

            fn new(imp: T, transform: TR, transport: TP) -> Self {
                #server_ident{tr: transform,
                              tx: std::cell::RefCell::new(transport),
                              imp: imp}
            }
            
        }

        impl <TR, TP, W, T> essrpc::RPCServer for #server_ident<T, TR, TP> where
            TR: essrpc::Transform<Wire=W>,
            TP: essrpc::Transport<Wire=W>,
            T: #trait_ident
        {
            fn handle_single_call(&mut self) -> std::result::Result<(), failure::Error> {
                let mut tx = self.tx.borrow_mut();
                println!("Server trying to receive");
                let callw: W = tx.receive()?;
                let (method, mut rxstate) = self.tr.rx_begin(callw)?;
                let replyw: W = match method.as_str() {
                    #server_method_matches
                    _ => bail!("Unknown rpc method {}", method)
                }?;
                println!("Server sending reply");
                tx.send(replyw)
            }
        }
    )
}

fn create_server_match(method: &TraitItemMethod) -> TokenStream2 {
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

    let ident_literal = make_ident_literal_str(&ident);
    quote!(
        #ident_literal => {
            println!("Server dispatching method");
            #param_retrieve_tokens
            let ret = self.imp.#ident(#param_call_tokens)?;
            self.tr.to_wire(ret)
        },
    )
}
