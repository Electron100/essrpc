extern crate essrpc;
extern crate essrpc_macros;
extern crate failure;

use std::result::Result;

use failure::Error;

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

pub fn main() {
}
