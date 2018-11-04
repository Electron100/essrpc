extern crate essrpc_macros;
extern crate failure;

use std::result::Result;

use failure::Error;

use essrpc_macros::essrpc;

#[essrpc]
pub trait Foo {
    fn bar(a: String, b: i32) -> Result<String, Error>;
}
