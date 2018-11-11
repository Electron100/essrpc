extern crate failure;
extern crate serde;
extern crate serde_json;
extern crate uuid;

use failure::Error;
use failure::Fail;
use serde::{Deserialize, Serialize};

pub mod transforms;

type Result<T> = std::result::Result<T, Error>;

pub trait Transform {
    type TXState;
    type RXState;
    
    fn tx_begin_call(&mut self, method: &'static str) -> Result<Self::TXState>;
    fn tx_add_param(&mut self, name: &'static str, value: impl Serialize,
                        state: &mut Self::TXState) -> Result<()>;
    fn tx_finalize(&mut self, state: &mut Self::TXState) -> Result<()>;
    fn tx_response(&mut self, value: impl Serialize) -> Result<()>;

    /// Begin reading a method call from the server. Returns
    /// the method name and internal state
    fn rx_begin_call(&mut self) -> Result<(String, Self::RXState)>;
    fn rx_read_param<T>(&mut self, name: &'static str, state: &mut Self::RXState) -> Result<T> where
        for<'de> T: serde::Deserialize<'de>;
    fn rx_response<T>(&mut self) -> Result<T> where
        for<'de> T: Deserialize<'de>;
    
}

pub trait RPCClient {
    type TR: Transform;
     fn new(transform: Self::TR) -> Self;
}

pub trait RPCServer {
    fn handle_single_call(&mut self) -> Result<()>;
}

#[derive(Debug, Fail)]
pub enum RPCError {
    #[fail(display = "unexpected input: {}", detail)]
    UnexpectedInput {
        detail: String,
    },
}
