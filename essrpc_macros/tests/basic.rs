extern crate essrpc;
extern crate essrpc_macros;
extern crate failure;

use std::os::unix::net::UnixStream;
use std::thread;
use std::result::Result;

use failure::bail;
use failure::Error;

use essrpc::{RPCClient, RPCServer};
use essrpc::transports::{BincodeTransport};
use essrpc_macros::essrpc;


#[essrpc]
pub trait Foo {
    fn bar(&self, a: String, b: i32) -> Result<String, Error>;
}

struct FooImpl;

impl FooImpl {
    fn new() -> Self {
        FooImpl{}
    }
}

impl Foo for FooImpl {
    fn bar(&self, a: String, b: i32) -> Result<String, Error> {
        Ok(format!("{} is {}", a, b))
    }
}

#[test]
fn basic_rpc() {
    let (s1, s2) = UnixStream::pair().unwrap();;
    let foo = FooRPCClient::new(BincodeTransport::new(s1));

    thread::spawn(move || {
        let mut serve = FooRPCServer::new(FooImpl::new(), BincodeTransport::new(s2));
        serve.handle_single_call()
    });
    println!("Calling client bar");
    match foo.bar("the answer".to_string(), 42) {
        Ok(result) => assert_eq!("the answer is 42", result),
        Err(e) => println!("error: {:?}", e)
    }
}
