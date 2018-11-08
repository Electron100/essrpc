extern crate essrpc;
extern crate essrpc_macros;
extern crate failure;

use std::os::unix::net::UnixStream;
use std::result::Result;

use failure::Error;

use essrpc::RPCClient;
use essrpc::transforms::JSONTransform;
use essrpc::transports::ReadWriteTransport;
use essrpc_macros::essrpc;

#[essrpc]
pub trait Foo {
    fn bar(&self, a: String, b: i32) -> Result<String, Error>;
}

pub trait Baz {
    fn baz() -> String {
        String::from("baz")
    }
}

struct BazT;

impl Baz for BazT {
}
    

pub fn main() {
    let (s1, s2) = UnixStream::pair().unwrap();;
    let foo = FooRPCClient::new(JSONTransform::new(), ReadWriteTransport::new(s1));
    //let s: String = BazT::baz();
    match foo.bar("hello".to_string(), 42) {
        Ok(result) => println!("ok {}", result),
        Err(e) => println!("error: {:?}", e)
    }
}
