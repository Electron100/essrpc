use essrpc::essrpc;
use essrpc::transports::{
    BincodeAsyncClientTransport, BincodeTransport, JSONAsyncClientTransport, JSONTransport,
};
use essrpc::{AsyncRPCClient, RPCServer};
use serde::{Deserialize, Serialize};
use std::result::Result;
use std::{fmt, thread};
use tokio;
use tokio_util::compat::TokioAsyncReadCompatExt;

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

#[essrpc(async, sync)]
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

#[tokio::test]
async fn basic_json_async() {
    let foo = json_foo();
    match foo.bar("the answer".to_string(), 42).await {
        Ok(result) => assert_eq!("the answer is 42", result),
        Err(e) => panic!("error: {:?}", e),
    }
}

#[tokio::test]
async fn basic_bincode_async() {
    let foo = bincode_foo();
    match foo.bar("the answer".to_string(), 42).await {
        Ok(result) => assert_eq!("the answer is 42", result),
        Err(e) => panic!("error: {:?}", e),
    }
}

fn json_foo() -> impl FooAsync {
    let (s1, s2) = tokio::net::UnixStream::pair().unwrap();
    // The server isn't actually async, so convert into a non-asyn Unix stream
    let s2 = s2.into_std().unwrap();
    s2.set_nonblocking(false).unwrap();
    thread::spawn(move || {
        let mut serve = FooRPCServer::new(FooImpl::new(), JSONTransport::new(s2));
        serve.serve_single_call()
    });
    FooAsyncRPCClient::new(JSONAsyncClientTransport::new(s1.compat()))
}

fn bincode_foo() -> impl FooAsync {
    let (s1, s2) = tokio::net::UnixStream::pair().unwrap();
    // The server isn't actually async, so convert into a non-asyn Unix stream
    let s2 = s2.into_std().unwrap();
    s2.set_nonblocking(false).unwrap();
    thread::spawn(move || {
        let mut serve = FooRPCServer::new(FooImpl::new(), BincodeTransport::new(s2));
        serve.serve_single_call()
    });
    FooAsyncRPCClient::new(BincodeAsyncClientTransport::new(s1.compat()))
}
