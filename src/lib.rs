extern crate failure;
extern crate serde;
extern crate serde_json;
extern crate uuid;

use failure::Error;
use failure::Fail;
use serde::{Deserialize, Serialize};

pub mod transforms;
pub mod transports;

type Result<T> = std::result::Result<T, Error>;

pub trait Transform {
    type TXState;
    type RXState;
    type Wire;
    
    fn tx_begin(&self, method: &'static str) -> Result<Self::TXState>;
    fn tx_add_param(&self, name: &'static str, value: impl Serialize,
                        state: &mut Self::TXState) -> Result<()>;
    fn tx_finalize(&self, state: &mut Self::TXState) -> Result<Self::Wire>;

    /// Begin reading a method call from the server. Returns
    /// the method name and internal state
    fn rx_begin(&self, data: Self::Wire) -> Result<(String, Self::RXState)>;
    fn rx_read_param<T>(&self, name: &'static str, state: &mut Self::RXState) -> Result<T> where
        for<'de> T: serde::Deserialize<'de>;

    fn from_wire<'a, T>(&self, data: &'a Self::Wire) -> Result<T> where
        T: Deserialize<'a>;
}

pub trait RPCClient {
    type TR: Transform;
    type CTP: Transport;
    fn new(transform: Self::TR, transport: Self::CTP) -> Self;
}

pub trait Transport {
    type Wire;
    fn send(&mut self, data: Self::Wire) -> Result<()>;
    fn receive(&mut self) -> Result<Self::Wire>;
}


#[derive(Debug, Fail)]
pub enum RPCError {
    #[fail(display = "unexpected input: {}", detail)]
    UnexpectedInput {
        detail: String,
    },
}

// client implementation of Foo.bar
// fn bar(a: A, b: B) -> Result<C> {
//   let mut state = tr.tx_begin("bar")?;
//   tr.tx_add_param("a", a, &mut state)?;
//   tr.tx_add_param("b", b, &mut state)?;
//   let data_in = tr.tx_finalize(&state)?;
//   let data_result = tx.send(data_in)?;
//   tr.from_wire(data_result)

//
// FooClient::new(JsonTransform::new(), StdOutTransport::new()).bar()

// Server side
// bytes=read from stdin
// result = FooServer::new(JsonTransform::new(), FooImpl::new()).process(bytes)

// let call: Foo_Call = tb.from_bytes(bytes)?
// switch on call.method
// let p: Foo_bar_Params = tb.from_bytes(call.params)
// Result<C, E> = impl.bar(call.
