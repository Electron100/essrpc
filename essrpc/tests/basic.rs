extern crate essrpc;
extern crate serde;
extern crate serde_derive;

use std::fmt;
use std::os::unix::net::UnixStream;
use std::result::Result;
use std::thread;

use serde_derive::{Deserialize, Serialize};

use essrpc::essrpc;
use essrpc::transports::{BincodeTransport, JSONTransport};
use essrpc::{RPCClient, RPCServer};

#[derive(Debug, Deserialize, Serialize)]
pub struct TestError {
    msg: String,
}

impl fmt::Display for TestError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "error: {}", self.msg)
    }
}

impl std::error::Error for TestError {}
impl From<essrpc::RPCError> for TestError {
    fn from(error: essrpc::RPCError) -> Self {
        TestError {
            msg: format!("{}", error),
        }
    }
}

#[essrpc]
pub trait Foo {
    fn bar(&self, a: String, b: i32) -> Result<String, TestError>;
    fn expect_error(&self) -> Result<String, TestError>;
}

struct FooImpl;

impl FooImpl {
    fn new() -> Self {
        FooImpl {}
    }
}

impl Foo for FooImpl {
    fn bar(&self, a: String, b: i32) -> Result<String, TestError> {
        Ok(format!("{} is {}", a, b))
    }
    fn expect_error(&self) -> Result<String, TestError> {
        Err(TestError {
            msg: "iamerror".to_string(),
        })
    }
}

#[test]
fn basic_bincode() {
    let foo = bincode_foo();
    match foo.bar("the answer".to_string(), 42) {
        Ok(result) => assert_eq!("the answer is 42", result),
        Err(e) => panic!("error: {:?}", e),
    }
}

#[test]
fn basic_json() {
    let foo = json_foo();
    match foo.bar("the answer".to_string(), 42) {
        Ok(result) => assert_eq!("the answer is 42", result),
        Err(e) => panic!("error: {:?}", e),
    }
}

#[test]
fn propagates_error() {
    let foo = json_foo();
    match foo.expect_error() {
        Ok(_) => panic!("Should have generated an error"),
        Err(e) => assert_eq!(&e.msg, "iamerror"),
    }
}

#[test]
fn serve_multiple() {
    let (s1, s2) = UnixStream::pair().unwrap();
    thread::spawn(move || {
        let mut serve = FooRPCServer::new(FooImpl::new(), BincodeTransport::new(s2));
        serve.serve()
    });
    let foo = FooRPCClient::new(BincodeTransport::new(s1));
    match foo.bar("the answer".to_string(), 42) {
        Ok(result) => assert_eq!("the answer is 42", result),
        Err(e) => panic!("error: {:?}", e),
    }
    match foo.bar("the answer".to_string(), 43) {
        Ok(result) => assert_eq!("the answer is 43", result),
        Err(e) => panic!("error: {:?}", e),
    }
}

fn json_foo() -> impl Foo {
    let (s1, s2) = UnixStream::pair().unwrap();;
    thread::spawn(move || {
        let mut serve = FooRPCServer::new(FooImpl::new(), JSONTransport::new(s2));
        serve.serve_single_call()
    });
    FooRPCClient::new(JSONTransport::new(s1))
}

fn bincode_foo() -> impl Foo {
    let (s1, s2) = UnixStream::pair().unwrap();
    thread::spawn(move || {
        let mut serve = FooRPCServer::new(FooImpl::new(), BincodeTransport::new(s2));
        serve.serve_single_call()
    });
    FooRPCClient::new(BincodeTransport::new(s1))
}
