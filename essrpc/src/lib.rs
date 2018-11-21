extern crate bincode;
extern crate erased_serde;
extern crate failure;
extern crate serde;
extern crate serde_derive;
extern crate serde_json;
extern crate uuid;

use std::fmt;

use serde::{Deserialize, Serialize};
use serde_derive::{Deserialize, Serialize};

pub mod transports;

type Result<T> = std::result::Result<T, RPCError>;

#[derive(Debug)]
pub struct MethodId {
    pub name: &'static str,
    pub num: u32
}

#[derive(Debug)]
pub enum PartialMethodId {
    Name(String),
    Num(u32)
}

pub trait Transport {
    type TXState;
    type RXState;
    
    fn tx_begin_call(&mut self, method: MethodId) -> Result<Self::TXState>;
    fn tx_add_param(&mut self, name: &'static str, value: impl Serialize,
                        state: &mut Self::TXState) -> Result<()>;
    fn tx_finalize(&mut self, state: &mut Self::TXState) -> Result<()>;
    fn tx_response(&mut self, value: impl Serialize) -> Result<()>;

    /// Begin reading a method call from the server. Returns
    /// the method name and internal state
    fn rx_begin_call(&mut self) -> Result<(PartialMethodId, Self::RXState)>;
    fn rx_read_param<T>(&mut self, name: &'static str, state: &mut Self::RXState) -> Result<T> where
        for<'de> T: serde::Deserialize<'de>;
    fn rx_response<T>(&mut self) -> Result<T> where
        for<'de> T: Deserialize<'de>;
    
}

pub trait RPCClient {
    type TR: Transport;
     fn new(transform: Self::TR) -> Self;
}

pub trait RPCServer {
    fn handle_single_call(&mut self) -> Result<()>;
}

#[derive(Debug, Deserialize,Serialize)]
pub struct GenericSerializableError {
    description: String,
    cause: Option<Box<GenericSerializableError>>
}
impl GenericSerializableError {
    pub fn new(e: impl std::error::Error) -> Self {
        let cause = match e.source() {
            None => None,
            Some(ec) => Some(Box::new(GenericSerializableError::from_dyn(ec)))
        };
        GenericSerializableError{description: e.to_string(), cause: cause}
    }

    pub fn from_dyn(e: &dyn std::error::Error) -> Self {
        let cause = match e.source() {
            None => None,
            Some(ec) => Some(Box::new(GenericSerializableError::from_dyn(ec)))
        };
        GenericSerializableError{description: e.to_string(), cause: cause}
    }
}
impl std::error::Error for GenericSerializableError {
    fn source(&self) -> Option<&(std::error::Error + 'static)> {
        match self.cause {
            Some(ref e) => Some(e),
            None => None
        }
    }
}
impl fmt::Display for GenericSerializableError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.cause {
            Some(ref e) => write!(f, "{} caused by:\n {}", self.description, e),
            None => write!(f, "{}", self.description)
        }
    }
}

#[derive(Debug, Deserialize,Serialize)]
pub struct RPCError {
    pub kind: RPCErrorKind,
    msg: String,
    cause: Option<Box<GenericSerializableError>>
}

impl RPCError {
    pub fn new(kind: RPCErrorKind, msg: impl Into<String>) -> Self {
        RPCError{kind: kind, msg: msg.into(), cause: None} 
    }
    pub fn with_cause(kind: RPCErrorKind, msg: impl Into<String>, cause: impl std::error::Error) -> Self {
        RPCError{kind: kind, msg: msg.into(), cause: Some(Box::new(GenericSerializableError::new(cause)))} 
    }
    pub fn cause<'a>(&'a self) -> Option<&'a GenericSerializableError> {
        match self.cause {
            None => None,
            Some(ref e) => Some(&e)
        }
    }
}

impl fmt::Display for RPCError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.cause {
            Some(ref e) => write!(f, "{} caused by:\n {}", self.msg, e),
            None => write!(f, "{}", self.msg)
        }
    }
}

impl std::error::Error for RPCError {
}

#[derive(Debug, Deserialize, Serialize)]
pub enum RPCErrorKind {
    SerializationError,
    UnknownMethod,
    Other,
}
